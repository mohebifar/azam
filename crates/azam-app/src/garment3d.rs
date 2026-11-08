//! The 3D scene: the avatar and the simulated garment, uploaded to the renderer.
//!
//! The avatar is rebuilt on a worker thread whenever its measurements change, so dragging a
//! measurement never stalls the window. The garment is a cloth [`Simulation`]: it is rebuilt after
//! every project edit (unchanged pieces keep their drape), stepped once per frame while playing,
//! and drawn piece by piece, optionally coloured by a fit map.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use azam_avatar::{Avatar, AvatarKind, AvatarParams, MEASUREMENTS, Motion};
use azam_doc::{PieceId, Project};
use azam_render::{DecalId, Draw, MeshData, MeshId, Renderer, Weave, srgb_to_linear};
use azam_sim::{AutoStop, FitMode, FitSummary, SimParams, Simulation, StopReason, resolve_settings};
use azam_sim_gpu::{GpuSim, GpuSolver};
use glam::Vec3;

use crate::avatar_motion::{MotionState, bundled_model, makehuman_available};
use crate::operators::Shading;
use crate::scene::{parse_hex, piece_color};
use crate::selection::Selection;

/// Mesh id of the avatar.
const AVATAR_MESH: MeshId = MeshId(0);
/// First mesh id used for pieces.
pub(crate) const FIRST_PIECE_MESH: u32 = 1;
/// Default skin colour.
const SKIN: u32 = 0xc9a58e;
/// The fit-map ramp, from relaxed to over the limit.
const FIT_COLORS: [u32; 8] = [
    0x2b4bd9, 0x2f8de0, 0x2fc2c9, 0x3fcf7a, 0xc9d93a, 0xf2b33a, 0xf06a32, 0xd9262c,
];
/// Weave repeat at fabric scale 1, in mm (about two threads of a shirting).
const WEAVE_REPEAT_MM: f32 = 1.6;
/// Fit values are recomputed every this many frames while playing.
const FIT_EVERY_FRAMES: u64 = 6;

/// A cloth vertex held by the grab tool.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrabState {
    /// Distance from the eye to the grab point along the view ray, in mm.
    pub depth: f32,
}

/// The avatar and garment as uploaded to the renderer.
#[derive(Default)]
pub struct Scene3d {
    avatar_key: String,
    /// The current avatar, once built.
    pub avatar: Option<Arc<Avatar>>,
    pending: Option<(String, Receiver<Avatar>)>,
    avatar_uploaded: bool,
    avatar_generation: usize,
    built_for: Option<(u64, usize)>,
    /// The cloth simulation, once the avatar exists.
    pub sim: Option<Simulation>,
    piece_meshes: Vec<MeshId>,
    auto_stop: AutoStop,
    fit_mode: Option<FitMode>,
    fit_frame: u64,
    /// Fit summary as of the last fit map.
    pub fit_summary: Option<FitSummary>,
    /// The grab in progress.
    pub grab: Option<GrabState>,
    /// Milliseconds the last simulation frame took.
    pub step_ms: f32,
    /// Keep the cloth on the CPU solver even when a GPU is available.
    pub cpu_only: bool,
    gpu: Option<GpuSolver>,
    gpu_sim: Option<GpuSim>,
    /// Why the GPU solver is not in use, if it failed.
    pub gpu_error: Option<String>,
    /// The avatar motion played with the simulation.
    pub motion: MotionState,
    /// Each piece's composited graphics: what they were made from, the renderer's id, the image.
    decals: std::collections::HashMap<PieceId, (String, DecalId, Arc<crate::decals::PieceDecal>)>,
    decal_images: crate::decals::ImageCache,
    next_decal: u32,
    /// Use the parametric mannequin even when the morph-target pack is available.
    pub mannequin: bool,
}

/// The avatar settings of `project` as the avatar builder reads them.
fn avatar_params(project: &Project) -> Option<AvatarParams> {
    serde_json::to_value(&project.avatar)
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
}

/// The solver parameters of `project`.
fn sim_params(project: &Project) -> SimParams {
    SimParams::from(&resolve_settings(&project.sim))
}

impl Scene3d {
    /// Which solver steps the cloth: `"GPU"` or `"CPU"`.
    #[must_use]
    pub fn solver_name(&self) -> &'static str {
        if self.gpu_sim.is_some() { "GPU" } else { "CPU" }
    }

    /// Switches between the GPU and CPU solvers (the next rebuild uses the choice).
    pub fn set_use_gpu(&mut self, on: bool) {
        self.cpu_only = !on;
        self.built_for = None;
        if !on {
            self.gpu_sim = None;
        }
    }

    /// Uploads the current simulation to the GPU solver (creating it on the renderer's device the
    /// first time). Any failure leaves the CPU solver in charge and records why.
    fn attach_gpu(&mut self, renderer: &Renderer) {
        self.gpu_sim = None;
        if self.cpu_only || self.gpu_error.is_some() {
            return;
        }
        if self.gpu.is_none() {
            match GpuSolver::new(renderer.device().clone(), renderer.queue().clone()) {
                Ok(solver) => self.gpu = Some(solver),
                Err(e) => {
                    self.gpu_error = Some(e.to_string());
                    return;
                }
            }
        }
        if let (Some(solver), Some(sim), Some(avatar)) = (&self.gpu, &self.sim, &self.avatar) {
            match solver.upload(sim, avatar) {
                Ok(gpu_sim) => self.gpu_sim = Some(gpu_sim),
                Err(e) => self.gpu_error = Some(e.to_string()),
            }
        }
    }

    /// Chooses the avatar motion played with the simulation.
    pub fn set_motion(&mut self, motion: Motion) {
        self.motion.set(motion);
    }

    /// Chooses the parametric mannequin (`true`) or the morph-target body (rebuilds the avatar).
    pub fn set_mannequin(&mut self, on: bool) {
        self.mannequin = on;
    }

    /// The avatar as the solver currently sees it: posed while a motion plays.
    #[must_use]
    pub fn body(&self) -> Option<&Arc<Avatar>> {
        self.motion.body().or(self.avatar.as_ref())
    }

    /// Which body model the avatar uses, for the avatar panel.
    #[must_use]
    pub fn avatar_status(&self) -> String {
        let Some(avatar) = &self.avatar else {
            return "Building the avatar…".into();
        };
        match (&avatar.kind, &avatar.makehuman) {
            (AvatarKind::MakeHuman, Some(r)) => {
                let worst = MEASUREMENTS
                    .iter()
                    .enumerate()
                    .filter_map(|(i, _)| r.fit.wanted[i].map(|w| (r.measured.0[i] - w).abs()))
                    .fold(0.0, f64::max);
                format!("MakeHuman (CC0) · fits within {worst:.1} mm")
            }
            _ if makehuman_available() && !self.mannequin => "Mannequin (MakeHuman pack unreadable)".into(),
            _ => "Built-in mannequin".into(),
        }
    }

    /// Whether an avatar is being built.
    #[must_use]
    pub fn building(&self) -> bool {
        self.pending.is_some()
    }

    /// Particles in the simulation.
    #[must_use]
    pub fn particle_count(&self) -> usize {
        self.sim.as_ref().map_or(0, |s| s.particles.x.len() / 3)
    }

    /// Brings the renderer up to date with `project` at `revision`. Returns `true` while work is
    /// still pending (the caller should keep rendering frames).
    pub fn sync(&mut self, project: &Project, revision: u64, renderer: &mut Renderer) -> bool {
        self.sync_avatar(project, renderer);
        if let Some(avatar) = self.avatar.clone()
            && self.built_for != Some((revision, self.avatar_generation))
        {
            self.built_for = Some((revision, self.avatar_generation));
            let previous = self.sim.take();
            let mut sim = Simulation::build(project, &avatar, previous.as_ref(), &sim_params(project));
            sim.compute_normals();
            self.sim = Some(sim);
            self.attach_gpu(renderer);
            self.upload_pieces(renderer);
            self.sync_decals(project, renderer);
            self.fit_frame = 0;
            // the new simulation starts on the resting avatar
            self.motion.stop();
            if self.motion.take_back_to_rest() {
                upload_avatar_vertices(renderer, &avatar);
            }
        }
        if self.motion.take_back_to_rest()
            && let Some(avatar) = self.avatar.clone()
        {
            self.show_body(renderer, &avatar);
        }
        self.pending.is_some()
    }

    /// Composites what is printed or sewn onto each piece whose prints changed and uploads it;
    /// forgets the images of pieces that are gone or have nothing printed any more.
    fn sync_decals(&mut self, project: &Project, renderer: &mut Renderer) {
        self.decals.retain(|id, (_, decal, _)| {
            let keep = project.pieces.contains_key(id);
            if !keep {
                renderer.remove_decal(*decal);
            }
            keep
        });
        for id in project.pieces.keys() {
            let key = crate::decals::decal_key(project, id);
            if self.decals.get(id).is_some_and(|(k, _, _)| *k == key) {
                continue;
            }
            let Some(image) = crate::decals::piece_decal(project, id, &mut self.decal_images) else {
                if let Some((_, decal, _)) = self.decals.remove(id) {
                    renderer.remove_decal(decal);
                }
                continue;
            };
            let decal = self.decals.get(id).map_or_else(
                || {
                    self.next_decal += 1;
                    DecalId(self.next_decal)
                },
                |(_, d, _)| *d,
            );
            renderer.set_decal(decal, image.image.width, image.image.height, &image.image.data);
            self.decals.insert(id.clone(), (key, decal, Arc::new(image)));
        }
    }

    /// Gives the solver and the renderer `body` (a posed or the resting avatar).
    fn show_body(&mut self, renderer: &mut Renderer, body: &Avatar) {
        if let Some(gpu) = self.gpu_sim.as_mut()
            && let Err(e) = gpu.update_avatar(body)
        {
            self.gpu_error = Some(e.to_string());
            self.gpu_sim = None;
        }
        upload_avatar_vertices(renderer, body);
    }

    fn sync_avatar(&mut self, project: &Project, renderer: &mut Renderer) {
        let Some(params) = avatar_params(project) else {
            return;
        };
        let use_pack = !self.mannequin && makehuman_available();
        let key = format!(
            "{}|{}",
            serde_json::to_string(&params).unwrap_or_default(),
            if use_pack { "makehuman" } else { "mannequin" }
        );
        if key != self.avatar_key && self.pending.as_ref().is_none_or(|(k, _)| *k != key) {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let model = if use_pack { bundled_model() } else { None };
                let _ = tx.send(Avatar::build(&params, model.as_deref()));
            });
            self.pending = Some((key, rx));
        }
        if let Some((key, rx)) = &self.pending
            && let Ok(avatar) = rx.try_recv()
        {
            self.avatar_key.clone_from(key);
            self.pending = None;
            self.avatar = Some(Arc::new(avatar));
            self.avatar_uploaded = false;
            self.avatar_generation += 1;
        }
        if !self.avatar_uploaded
            && let Some(avatar) = &self.avatar
        {
            let mesh = &avatar.mesh;
            renderer.set_mesh(
                AVATAR_MESH,
                &MeshData {
                    positions: &triples(&mesh.positions),
                    normals: &triples(&mesh.normals),
                    colors: None,
                    uvs: None,
                    indices: &mesh.indices,
                },
            );
            self.avatar_uploaded = true;
        }
    }

    /// Uploads every piece's mesh (after a rebuild: vertex counts may have changed).
    fn upload_pieces(&mut self, renderer: &mut Renderer) {
        let Some(sim) = &self.sim else { return };
        let count = sim.records.len();
        for id in self.piece_meshes.drain(count.min(self.piece_meshes.len())..) {
            renderer.remove_mesh(id);
        }
        for (ri, view) in sim.piece_views().enumerate() {
            let id = MeshId(FIRST_PIECE_MESH + ri as u32);
            if ri >= self.piece_meshes.len() {
                self.piece_meshes.push(id);
            }
            renderer.set_mesh(
                id,
                &MeshData {
                    positions: &triples(view.positions),
                    normals: &triples(view.normals),
                    colors: None,
                    uvs: Some(&pattern_uvs(&view.record.mesh.xy)),
                    indices: view.triangles,
                },
            );
        }
    }

    /// Advances the simulation by one screen frame and uploads the new vertices. Returns why the
    /// run stopped, if auto-stop ended it.
    pub fn step(&mut self, project: &Project, renderer: &mut Renderer) -> Option<StopReason> {
        let values = resolve_settings(&project.sim).values;
        if let (Some(sim), Some(rest)) = (&self.sim, self.avatar.clone()) {
            let frame_dt = values.dt * f64::from(sim.params.frames_per_advance());
            if let Some(body) = self.motion.advance(&rest, frame_dt) {
                self.show_body(renderer, &body);
            }
        }
        let body = self.body().cloned();
        let (Some(sim), Some(avatar)) = (self.sim.as_mut(), body.as_ref()) else {
            return None;
        };
        let start = std::time::Instant::now();
        let on_gpu = match self.gpu_sim.as_mut().map(|g| g.step_frame(sim, values.dt)) {
            Some(Ok(_)) => true,
            Some(Err(e)) => {
                // The device failed: carry on with the CPU solver from the last read-back state.
                self.gpu_error = Some(e.to_string());
                self.gpu_sim = None;
                false
            }
            None => false,
        };
        if !on_gpu {
            sim.advance(avatar, values.dt);
        }
        sim.compute_normals();
        self.step_ms = start.elapsed().as_secs_f32() * 1000.0;
        for (ri, view) in sim.piece_views().enumerate() {
            renderer.update_vertices(
                MeshId(FIRST_PIECE_MESH + ri as u32),
                &triples(view.positions),
                &triples(view.normals),
            );
        }
        self.fit_frame += 1;
        // a moving avatar keeps the cloth moving: auto-stop waits for the motion to end
        if values.auto_stop && self.motion.motion() == Motion::None {
            self.auto_stop.update(sim.time, sim.energy, sim.sew_gap, &values)
        } else {
            None
        }
    }

    /// Moves piece record `ri` rigidly by `delta` mm, or turns it by `turn` radians around the
    /// body's vertical axis, between simulation frames (placing pieces before draping): the CPU
    /// state, the GPU solver's copy and the drawn mesh all follow.
    pub fn place_piece(&mut self, ri: usize, delta: Vec3, turn: f64, renderer: &mut Renderer) {
        let Some(sim) = self.sim.as_mut() else { return };
        if delta != Vec3::ZERO {
            sim.translate_piece(ri, [f64::from(delta.x), f64::from(delta.y), f64::from(delta.z)]);
        }
        if turn != 0.0 {
            sim.turn_piece(ri, turn, [0.0, 0.0]);
        }
        sim.compute_normals();
        if let Some(gpu) = self.gpu_sim.as_mut() {
            gpu.upload_positions(true);
        }
        if let Some(view) = sim.piece_views().nth(ri) {
            renderer.update_vertices(
                MeshId(FIRST_PIECE_MESH + ri as u32),
                &triples(view.positions),
                &triples(view.normals),
            );
        }
    }

    /// Starts a fresh auto-stop window (when play starts).
    pub fn restart_auto_stop(&mut self) {
        self.auto_stop = AutoStop::new();
    }

    /// Puts the garment back in its arranged pose (and the avatar in its rest pose).
    pub fn reset(&mut self, renderer: &mut Renderer) {
        self.motion.stop();
        if self.motion.take_back_to_rest()
            && let Some(avatar) = self.avatar.clone()
        {
            self.show_body(renderer, &avatar);
        }
        if let (Some(sim), Some(avatar)) = (self.sim.as_mut(), self.avatar.as_ref()) {
            match self.gpu_sim.as_mut() {
                Some(gpu) => {
                    if let Err(e) = gpu.reset(sim, avatar) {
                        self.gpu_error = Some(e.to_string());
                        self.gpu_sim = None;
                        sim.reset(avatar);
                    }
                }
                None => sim.reset(avatar),
            }
            sim.compute_normals();
        }
        self.upload_pieces(renderer);
        self.fit_frame = 0;
    }

    /// Recolours the garment with fit mode `mode` (or clears the colours), recomputing the values
    /// when they are stale.
    pub fn update_fit(&mut self, project: &Project, mode: Option<FitMode>, renderer: &mut Renderer) {
        let stale = self.fit_mode != mode || self.fit_frame % FIT_EVERY_FRAMES == 0;
        if !stale {
            return;
        }
        self.fit_mode = mode;
        self.fit_frame = self.fit_frame.max(1);
        let (Some(sim), Some(avatar), Some(mode)) = (self.sim.as_ref(), self.body(), mode) else {
            self.fit_summary = None;
            return;
        };
        let values = sim.fit_values(project, avatar, mode);
        self.fit_summary = (mode == FitMode::Fit).then(|| sim.fit_summary(&values));
        for (ri, view) in sim.piece_views().enumerate() {
            let range = view.record.offset..view.record.offset + view.record.count;
            let colors: Vec<[f32; 4]> = values[range].iter().map(|&v| fit_color(mode, v)).collect();
            renderer.update_colors(MeshId(FIRST_PIECE_MESH + ri as u32), &colors);
        }
    }
    /// The piece particle `q` belongs to and where it lies on the flat pattern, in mm.
    #[must_use]
    pub fn particle_pattern_pos(&self, q: usize) -> Option<(PieceId, f64, f64)> {
        let sim = self.sim.as_ref()?;
        let r = sim
            .records
            .iter()
            .find(|r| (r.offset..r.offset + r.count).contains(&q))?;
        let k = 2 * (q - r.offset);
        Some((r.id.clone(), *r.mesh.xy.get(k)?, *r.mesh.xy.get(k + 1)?))
    }

    /// The nearest garment hit along a ray: piece record, global particle nearest the hit, distance.
    #[must_use]
    pub fn hit(&self, origin: Vec3, dir: Vec3) -> Option<(usize, usize, f32)> {
        let sim = self.sim.as_ref()?;
        let mut best: Option<(usize, usize, f32)> = None;
        for (ri, view) in sim.piece_views().enumerate() {
            let p = view.positions;
            let v = |i: u32| Vec3::new(p[i as usize * 3], p[i as usize * 3 + 1], p[i as usize * 3 + 2]);
            for t in view.triangles.chunks_exact(3) {
                let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
                if let Some(d) = crate::viewport::ray_triangle(origin, dir, a, b, c)
                    && best.is_none_or(|(_, _, bd)| d < bd)
                {
                    let point = origin + dir * d;
                    let nearest = [t[0], t[1], t[2]]
                        .into_iter()
                        .min_by(|&i, &j| v(i).distance(point).total_cmp(&v(j).distance(point)))
                        .unwrap_or(t[0]);
                    best = Some((ri, view.record.offset + nearest as usize, d));
                }
            }
        }
        best
    }

    /// The piece whose surface the ray from `origin` along `dir` meets first.
    #[must_use]
    pub fn pick(&self, origin: Vec3, dir: Vec3) -> Option<PieceId> {
        let (ri, _, _) = self.hit(origin, dir)?;
        self.sim
            .as_ref()
            .and_then(|s| s.records.get(ri))
            .map(|r| r.id.clone())
    }

    /// Every vertex position of the garment and avatar, for framing.
    pub fn points(&self) -> impl Iterator<Item = Vec3> + '_ {
        let avatar = self
            .body()
            .map(|a| a.mesh.positions.as_slice())
            .unwrap_or_default();
        let cloth = self
            .sim
            .as_ref()
            .map(|s| s.particles.x.as_slice())
            .unwrap_or_default();
        avatar
            .chunks_exact(3)
            .chain(cloth.chunks_exact(3))
            .map(|p| Vec3::new(p[0], p[1], p[2]))
    }

    /// What to draw.
    #[must_use]
    pub fn draws(&self, project: &Project, selection: &Selection, shading: Shading) -> Vec<Draw> {
        let mut draws = Vec::new();
        if project.avatar.show && self.avatar.is_some() {
            let skin = parse_hex(&project.avatar.skin).unwrap_or(SKIN);
            let mut avatar = Draw::new(AVATAR_MESH, srgb(skin));
            avatar.specular = 0.12;
            draws.push(avatar);
        }
        let Some(sim) = &self.sim else { return draws };
        let fit = shading == Shading::FitMap;
        for (ri, record) in sim.records.iter().enumerate() {
            let color = if fit {
                [1.0; 3]
            } else {
                srgb(piece_color(project, &record.id))
            };
            let mut draw = Draw::new(MeshId(FIRST_PIECE_MESH + ri as u32), color);
            draw.inside = 0.62;
            if let Some(fabric) = project.effective_fabric(&record.fabric) {
                draw.specular = (fabric.sheen as f32).clamp(0.0, 1.0) * 0.6 + 0.04;
                draw.roughness = (fabric.rough as f32).clamp(0.0, 1.0);
                draw.weave = if fit {
                    Weave::None
                } else {
                    Weave::from_name(&fabric.weave)
                };
                draw.weave_mm = (fabric.scale as f32).clamp(0.2, 20.0) * WEAVE_REPEAT_MM;
                draw.color[3] = (fabric.opacity as f32).clamp(0.2, 1.0);
            } else {
                draw.specular = 0.08;
            }
            draw.vertex_colors = if fit { 1.0 } else { 0.0 };
            if !fit && let Some((_, decal, image)) = self.decals.get(&record.id) {
                draw.decal = Some((*decal, image.rect()));
            }
            if selection.pieces.contains(&record.id) && !fit {
                draw.highlight = if selection.active.as_ref() == Some(&record.id) {
                    0.5
                } else {
                    0.3
                };
            }
            draws.push(draw);
        }
        draws
    }

    /// The avatar and garment as meshes with materials for the path tracer (F12), mirroring
    /// [`Scene3d::draws`] in solid shading.
    #[must_use]
    pub fn trace_meshes(&self, project: &Project) -> Vec<azam_trace::Mesh> {
        let linear = |c: u32| srgb(c).map(srgb_to_linear);
        let mut meshes = Vec::new();
        if project.avatar.show
            && let Some(avatar) = self.body()
        {
            let skin = parse_hex(&project.avatar.skin).unwrap_or(SKIN);
            meshes.push(azam_trace::Mesh {
                positions: triples(&avatar.mesh.positions),
                normals: triples(&avatar.mesh.normals),
                uvs: Vec::new(),
                indices: avatar.mesh.indices.clone(),
                material: azam_trace::Material::skin(linear(skin)),
                decal: None,
            });
        }
        let Some(sim) = &self.sim else { return meshes };
        for view in sim.piece_views() {
            let color = linear(piece_color(project, &view.record.id));
            let mut material = azam_trace::Material::cloth(color);
            if let Some(fabric) = project.effective_fabric(&view.record.fabric) {
                material = azam_trace::Material::fabric(color, fabric.rough as f32, fabric.sheen as f32);
                material.weave = Weave::from_name(&fabric.weave);
                material.weave_mm = (fabric.scale as f32).clamp(0.2, 20.0) * WEAVE_REPEAT_MM;
                material.opacity = (fabric.opacity as f32).clamp(0.2, 1.0);
            }
            material.inside = 0.7;
            meshes.push(azam_trace::Mesh {
                positions: triples(view.positions),
                normals: triples(view.normals),
                uvs: pattern_uvs(&view.record.mesh.xy),
                indices: view.triangles.to_vec(),
                material,
                decal: self.decals.get(&view.record.id).map(|(_, _, d)| {
                    Arc::new(azam_trace::Decal {
                        width: d.image.width,
                        height: d.image.height,
                        rgba: d.image.data.clone(),
                        min: [d.min.x as f32, d.min.y as f32],
                        size: [d.size.x as f32, d.size.y as f32],
                    })
                }),
            });
        }
        meshes
    }
}

/// The colour of fit value `v` in `mode`, as linear RGBA.
fn fit_color(mode: FitMode, v: f32) -> [f32; 4] {
    let c: [f32; 3] = match mode {
        FitMode::Strain => ramp((v - 1.0) / 0.2),
        FitMode::Stress => ramp(v / 100.0),
        FitMode::Pressure if v > 0.0 => [0.95, 0.25, 0.2],
        FitMode::Pressure => [0.93, 0.93, 0.93],
        FitMode::Fit if v >= 1.0 => [0.85, 0.15, 0.17],
        FitMode::Fit if v >= 0.6 => [0.98, 0.8, 0.2],
        FitMode::Fit => [0.95, 0.95, 0.95],
    };
    [
        srgb_to_linear(c[0]),
        srgb_to_linear(c[1]),
        srgb_to_linear(c[2]),
        1.0,
    ]
}

/// The fit ramp at `t` in 0..1, as sRGB.
fn ramp(t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0) * (FIT_COLORS.len() - 1) as f32;
    let i = (t.floor() as usize).min(FIT_COLORS.len() - 1);
    let f = t - i as f32;
    let (a, b) = (
        srgb(FIT_COLORS[i]),
        srgb(FIT_COLORS[(i + 1).min(FIT_COLORS.len() - 1)]),
    );
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
    ]
}

/// A piece's pattern coordinates (mm) as texture coordinates.
fn pattern_uvs(xy: &[f64]) -> Vec<[f32; 2]> {
    xy.chunks_exact(2).map(|c| [c[0] as f32, c[1] as f32]).collect()
}

/// `0xRRGGBB` to sRGB channels 0..1.
fn srgb(c: u32) -> [f32; 3] {
    [
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
        (c & 0xff) as f32 / 255.0,
    ]
}

/// Uploads `avatar`'s vertex positions and normals to the avatar mesh (same triangles).
fn upload_avatar_vertices(renderer: &mut Renderer, avatar: &Avatar) {
    renderer.update_vertices(
        AVATAR_MESH,
        &triples(&avatar.mesh.positions),
        &triples(&avatar.mesh.normals),
    );
}

/// A flat xyz buffer as vertices.
fn triples(flat: &[f32]) -> Vec<[f32; 3]> {
    flat.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp, reason = "the ramp ends are exact table entries")]
    fn ramp_runs_blue_to_red() {
        assert_eq!(ramp(0.0), srgb(FIT_COLORS[0]));
        assert_eq!(ramp(1.0), srgb(FIT_COLORS[7]));
        assert_eq!(ramp(-1.0), srgb(FIT_COLORS[0]));
    }
}
