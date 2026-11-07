//! A simulation on the GPU: the uploaded state, the per-frame cycle (upload pending CPU edits,
//! dispatch, read back) and the interactive hooks.

use std::time::Instant;

use azam_avatar::{Avatar, BodyCollisionMesh, BodyVelocity, SdfGrid};
use azam_sim::Simulation;
use azam_sim::untangle::UntangleTotals;

use crate::buffers::{Buf, Buffers};
use crate::device::GpuSolver;
use crate::error::{GpuError, scoped};
use crate::frame::{self, Dispatch, FramePlan};
use crate::layout::{
    CONTROL_LEN, FrameParams, GRAB_CAP, MAX_PIECES, STATE_PIECE_GRAVITY, TRIANGLE_HASH_BUCKETS,
};
use crate::options::GpuOptions;
use crate::pack::{self, Groups, Packed};
use crate::readback::{Readback, Results};
use crate::untangle::Untangler;

/// Frame time used when none is given, s.
const DEFAULT_DT_S: f64 = 1.0 / 60.0;
/// Grab speed used when the parameters give none, mm/s.
const DEFAULT_GRAB_SPEED_MM_S: f64 = 700.0;
/// Contact re-detection interval used when the parameters give none, substeps.
const DEFAULT_DETECT_EVERY: u32 = 2;
/// Contact passes per substep used when the parameters give none.
const DEFAULT_CONTACT_ITERS: u32 = 2;
/// Last-good positions are saved every this many frames (a power of two).
const GOOD_SAVE_INTERVAL: u64 = 16;
/// Weight of the newest frame in the smoothed frame time.
const FRAME_TIME_SMOOTHING: f64 = 0.1;

/// What one finished frame did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameReport {
    /// Simulated frames.
    pub frames: u32,
    /// RMS speed of the free particles at the end, mm/s.
    pub energy: f64,
    /// The solver blew up (NaN) and the last good positions were restored.
    pub restored: bool,
    /// Wall time from submission to read-back, ms.
    pub ms: f64,
}

/// Contact statistics of the last detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactStats {
    /// Contacts found (may exceed the capacity; the excess is dropped).
    pub contacts: u32,
    /// Contact slots.
    pub contact_capacity: u32,
    /// Triangle hash entries written (may exceed the capacity).
    pub hash_entries: u32,
    /// Triangle hash entry slots.
    pub hash_capacity: u32,
    /// Cloth triangles.
    pub triangles: u32,
    /// Particles.
    pub particles: u32,
}

/// A frame submitted and not yet read back.
#[derive(Clone, Copy, Debug)]
struct InFlight {
    dt: f64,
    frames: u32,
    started: Instant,
}

/// CPU edits waiting to be uploaded before the next frame.
#[derive(Clone, Copy, Debug, Default)]
struct PendingUpload {
    positions: bool,
    reset_velocities: bool,
}

/// One simulation's GPU state. Build it with [`GpuSolver::upload`]; drive it with
/// [`GpuSim::step_frame`] (or [`GpuSim::begin_frame`] and [`GpuSim::finish_frame`]) on the same
/// [`Simulation`], which keeps receiving the positions for drawing, picking and fit maps.
#[derive(Debug)]
pub struct GpuSim {
    solver: GpuSolver,
    buffers: Buffers,
    readback: Readback,
    params: FrameParams,
    groups: Groups,
    pd: Vec<[u32; 8]>,
    good: Vec<f32>,
    pins_uploaded: Vec<u32>,
    particles: usize,
    pieces: usize,
    frames: u64,
    in_flight: Option<InFlight>,
    pending: PendingUpload,
    untangler: Untangler,
    tangled: usize,
    frame_ms: f64,
}

impl GpuSim {
    pub(crate) fn new(
        solver: GpuSolver,
        sim: &Simulation,
        grid: &SdfGrid,
        skin: Option<&BodyCollisionMesh<'_>>,
        options: GpuOptions,
    ) -> Result<Self, GpuError> {
        let n = sim.particle_count();
        if n == 0 {
            return Err(GpuError::EmptySimulation);
        }
        let packed: Packed = pack::pack(sim, grid, skin, &options);
        let device = solver.device.clone();
        let (buffers, readback) = scoped(&device, GpuError::Device, || {
            let b = Buffers::new(&device, &solver.queue, &solver.pipelines, &packed, &grid.data)?;
            let r = Readback::new(&device, n, b.particle_workgroups);
            Ok::<_, GpuError>((b, r))
        })??;
        Ok(Self {
            buffers,
            readback,
            params: packed.params,
            groups: packed.groups,
            pd: packed.pd,
            good: sim.particles.x.clone(),
            pins_uploaded: sim.pins.vertex.clone(),
            particles: n,
            pieces: sim.records.len(),
            frames: 0,
            in_flight: None,
            pending: PendingUpload::default(),
            untangler: Untangler::new(),
            tangled: 0,
            frame_ms: 0.0,
            solver,
        })
    }

    /// Number of particles uploaded.
    #[must_use]
    pub fn particle_count(&self) -> usize {
        self.particles
    }

    /// Number of cloth triangles (of the active pieces).
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.params.n_tri as usize
    }

    /// Simulated frames since the upload.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Smoothed wall time per screen frame (submission to read-back), ms.
    #[must_use]
    pub fn frame_ms(&self) -> f64 {
        self.frame_ms
    }

    /// Whether a frame is submitted and not yet read back.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.in_flight.is_some()
    }

    /// Particles currently marked by the untangler.
    #[must_use]
    pub fn tangled(&self) -> usize {
        self.tangled
    }

    /// Totals of the background untangler's runs.
    #[must_use]
    pub fn untangle_totals(&self) -> UntangleTotals {
        self.untangler.totals()
    }

    /// Switches the background untangler on or off (it also follows
    /// [`azam_sim::SimParams::untangle`]).
    pub fn set_untangler(&mut self, enabled: bool) {
        self.untangler.enabled = enabled;
    }

    fn check(&self, sim: &Simulation) -> Result<(), GpuError> {
        if sim.particle_count() == self.particles {
            Ok(())
        } else {
            Err(GpuError::Mismatch {
                expected: self.particles,
                found: sim.particle_count(),
            })
        }
    }

    /// Runs one screen frame — [`azam_sim::SimParams::frames_per_advance`] frames of `dt`
    /// seconds — on the GPU and reads the positions back into `sim`. A frame already in flight
    /// (from [`GpuSim::begin_frame`]) is finished first.
    ///
    /// # Errors
    /// [`GpuError::Mismatch`] for a different simulation, [`GpuError::Device`] or
    /// [`GpuError::Readback`] when the GPU fails; fall back to the CPU solver then.
    pub fn step_frame(&mut self, sim: &mut Simulation, dt: f64) -> Result<FrameReport, GpuError> {
        if self.in_flight.is_some() {
            self.finish_frame(sim)?;
        }
        self.begin_frame(sim, dt)?;
        self.finish_frame(sim)?
            .ok_or_else(|| GpuError::Readback("no frame was in flight".into()))
    }

    /// Uploads pending CPU edits and submits one screen frame without waiting for it. Returns
    /// `false` (and does nothing) while a previous frame is still in flight.
    ///
    /// # Errors
    /// As [`GpuSim::step_frame`].
    pub fn begin_frame(&mut self, sim: &mut Simulation, dt: f64) -> Result<bool, GpuError> {
        self.check(sim)?;
        if self.in_flight.is_some() {
            return Ok(false);
        }
        let dt = if dt == 0.0 || dt.is_nan() {
            DEFAULT_DT_S
        } else {
            dt
        };
        let frames = sim.params.frames_per_advance();
        let plan_groups = self.groups.clone();
        let device = self.solver.device.clone();
        scoped(&device, GpuError::Device, || {
            self.prepare(sim, dt, frames);
            let plan = self.plan(sim, frames, &plan_groups);
            self.submit(&plan.dispatches(), true);
        })?;
        self.in_flight = Some(InFlight {
            dt,
            frames,
            started: Instant::now(),
        });
        Ok(true)
    }

    /// Waits for the frame in flight and reads it back into `sim`; `Ok(None)` when none is in
    /// flight.
    ///
    /// # Errors
    /// As [`GpuSim::step_frame`].
    pub fn finish_frame(&mut self, sim: &mut Simulation) -> Result<Option<FrameReport>, GpuError> {
        self.complete(sim, true)
    }

    /// Reads the frame in flight back into `sim` if the GPU has finished it, without waiting.
    ///
    /// # Errors
    /// As [`GpuSim::step_frame`].
    pub fn try_finish_frame(&mut self, sim: &mut Simulation) -> Result<Option<FrameReport>, GpuError> {
        self.complete(sim, false)
    }

    fn complete(&mut self, sim: &mut Simulation, wait: bool) -> Result<Option<FrameReport>, GpuError> {
        let Some(fl) = self.in_flight else {
            return Ok(None);
        };
        self.check(sim)?;
        let Some(results) = self.readback.poll(&self.solver.device, wait)? else {
            return Ok(None);
        };
        self.in_flight = None;
        let report = self.apply_results(sim, &results, fl);
        Ok(Some(report))
    }

    /// Writes a frame's results into `sim`: positions (unless a CPU edit is waiting to be
    /// uploaded), hold-back state, energy and clocks; restores the last good positions after a
    /// blow-up; feeds the untangler.
    fn apply_results(&mut self, sim: &mut Simulation, r: &Results, fl: InFlight) -> FrameReport {
        let p = &mut sim.particles;
        if !self.pending.positions
            && let Some(x) = &r.positions
        {
            p.x.copy_from_slice(x);
        }
        let st = &r.state;
        sim.gravity_scale = Some(f64::from(st[0]));
        sim.sew_gap = f64::from(st[1]);
        sim.piece_gravity.resize(self.pieces, 1.0);
        for (i, g) in sim.piece_gravity.iter_mut().enumerate() {
            *g = st[STATE_PIECE_GRAVITY + i.min(MAX_PIECES - 1)];
        }
        let restored = r.positions.is_none();
        if restored {
            p.x.copy_from_slice(&self.good);
            p.prev.copy_from_slice(&self.good);
            p.v.fill(0.0);
            self.upload_positions(true);
            sim.energy = 0.0;
        } else {
            sim.energy = if r.energy_count > 0.0 {
                (r.energy_sum / r.energy_count).sqrt()
            } else {
                0.0
            };
            if sim.frames % GOOD_SAVE_INTERVAL == 0 {
                self.good.copy_from_slice(&sim.particles.x);
            }
        }
        sim.frames += u64::from(fl.frames);
        sim.time += fl.dt * f64::from(fl.frames);
        self.frames += u64::from(fl.frames);
        if !restored && let Some(tangles) = self.untangler.tick(sim) {
            self.set_tangles(&tangles);
        }
        let ms = fl.started.elapsed().as_secs_f64() * 1e3;
        self.frame_ms = if self.frame_ms == 0.0 {
            ms
        } else {
            self.frame_ms * (1.0 - FRAME_TIME_SMOOTHING) + ms * FRAME_TIME_SMOOTHING
        };
        FrameReport {
            frames: fl.frames,
            energy: sim.energy,
            restored,
            ms,
        }
    }

    /// Uploads pending edits and the frame parameters of a frame of `dt` seconds.
    fn prepare(&mut self, sim: &mut Simulation, dt: f64, frames: u32) {
        let q = &self.solver.queue;
        if sim.pins.vertex != self.pins_uploaded {
            self.buffers.write_pins(
                &self.solver.device,
                q,
                &self.solver.pipelines,
                &pack::pins(&sim.pins),
            );
            self.pins_uploaded.clone_from(&sim.pins.vertex);
            // pinning changes inverse masses, which live in X
            self.pending.positions = true;
        }
        if self.pending.positions {
            let (x, prev) = pack::positions(sim);
            q.write_buffer(self.buffers.get(Buf::X), 0, bytemuck::cast_slice(&x));
            q.write_buffer(self.buffers.get(Buf::Prev), 0, bytemuck::cast_slice(&prev));
            if self.pending.reset_velocities {
                let zero = vec![[0.0_f32; 4]; self.particles];
                q.write_buffer(self.buffers.get(Buf::Vel), 0, bytemuck::cast_slice(&zero));
            }
            if sim.particles.x.iter().all(|v| v.is_finite()) {
                self.good.copy_from_slice(&sim.particles.x);
            }
            self.pending = PendingUpload::default();
        }
        let sp = &sim.params;
        let h = dt / f64::from(sp.substeps);
        let f = &mut self.params;
        f.h = h as f32;
        f.ih2 = (1.0 / (h * h)) as f32;
        f.gravity = sp.gravity as f32;
        f.air = sp.air as f32;
        f.max_sew = (sp.sew_speed * h) as f32;
        f.n_pin = sim.pins.vertex.len() as u32;
        f.n_grab = 0;
        if let Some(grab) = sim.grab.as_mut().filter(|g| !g.vertices.is_empty()) {
            let rows = pack::grab_rows(grab, GRAB_CAP);
            q.write_buffer(self.buffers.get(Buf::Grab), 0, bytemuck::cast_slice(&rows));
            let speed = if sp.grab_speed == 0.0 || sp.grab_speed.is_nan() {
                DEFAULT_GRAB_SPEED_MM_S
            } else {
                sp.grab_speed
            };
            let anchor = grab.advance_anchor(speed, dt * f64::from(frames));
            f.n_grab = (rows.len() / 2) as u32;
            f.grab_point = anchor.map(|a| a as f32);
        }
        q.write_buffer(&self.buffers.uniform, 0, bytemuck::bytes_of(&self.params));
    }

    fn plan<'g>(&self, sim: &Simulation, frames: u32, groups: &'g Groups) -> FramePlan<'g> {
        let sp = &sim.params;
        let f = &self.params;
        FramePlan {
            frames,
            substeps: sp.substeps,
            iterations: sp.iterations,
            detect_every: if sp.cc_every_gpu == 0 {
                DEFAULT_DETECT_EVERY
            } else {
                sp.cc_every_gpu
            },
            contact_iters: if sp.cc_iters == 0 {
                DEFAULT_CONTACT_ITERS
            } else {
                sp.cc_iters
            },
            self_collision: sp.self_collision,
            first_frame: self.frames == 0,
            particles: f.n,
            triangles: f.n_tri,
            sewing: f.n_sew,
            skin: f.n_skin,
            pins: f.n_pin,
            grab: f.n_grab,
            pressure: f.has_pressure != 0,
            contact_cap: f.contact_cap,
            groups,
        }
    }

    /// Records and submits `dispatches`, plus the read-back copies when `read_back`.
    fn submit(&mut self, dispatches: &[Dispatch], read_back: bool) {
        let device = &self.solver.device;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("azam-sim-gpu frame"),
        });
        frame::record(&mut encoder, &self.solver.pipelines, &self.buffers, dispatches);
        if read_back {
            self.readback.encode(&mut encoder, &self.buffers);
        }
        self.solver.queue.submit([encoder.finish()]);
        if read_back {
            self.readback.request();
        }
    }

    /// Uploads the CPU-side positions before the next frame (after
    /// [`Simulation::set_positions`], [`Simulation::translate_piece`] or any other CPU edit);
    /// velocities are cleared when `reset_velocities`. A frame in flight does not overwrite the
    /// edited positions when it is read back.
    pub fn upload_positions(&mut self, reset_velocities: bool) {
        self.pending.positions = true;
        self.pending.reset_velocities |= reset_velocities;
    }

    /// Applies untangle codes (per particle: bit 1 the wanted side, bits 8 and up the crossed
    /// piece + 1; 0 for none), as [`azam_sim::untangle::UntangleResult::tangles`] gives them.
    /// Codes of a different length are ignored.
    pub fn set_tangles(&mut self, tangles: &[u32]) {
        if tangles.len() != self.particles {
            return;
        }
        self.tangled = pack::apply_tangles(&mut self.pd, tangles);
        self.solver
            .queue
            .write_buffer(self.buffers.get(Buf::Pd), 0, bytemuck::cast_slice(&self.pd));
    }

    /// Puts every piece back at its arrangement ([`Simulation::reset`]) and uploads the fresh
    /// state: no velocity, gravity hold-back restarted, untangle codes cleared, contacts detected
    /// on the next frame. A frame in flight is discarded.
    ///
    /// # Errors
    /// [`GpuError::Readback`] when the frame in flight cannot be finished.
    pub fn reset(&mut self, sim: &mut Simulation, avatar: &Avatar) -> Result<(), GpuError> {
        self.check(sim)?;
        if self.in_flight.take().is_some() {
            // let the GPU finish so the staging buffer is free; the results are stale
            let _ = self.readback.poll(&self.solver.device, true)?;
        }
        sim.reset(avatar);
        let q = &self.solver.queue;
        q.write_buffer(
            self.buffers.get(Buf::State),
            0,
            bytemuck::cast_slice(&pack::initial_state(sim)),
        );
        q.write_buffer(
            self.buffers.get(Buf::Ctrl),
            0,
            bytemuck::cast_slice(&[0_u32; CONTROL_LEN]),
        );
        self.set_tangles(&vec![0; self.particles]);
        self.untangler.restart();
        self.upload_positions(true);
        self.pins_uploaded.clear();
        self.frames = 0;
        self.good.clone_from(&sim.particles.x);
        Ok(())
    }

    /// Replaces the body the cloth collides with, between frames, for a moving avatar: the
    /// distance grid (any layout; keeping one layout per motion avoids reallocating), the skin
    /// for skin-vertex collision (`None` turns it off) and the surface velocity (`None` for a
    /// body at rest; with it, friction acts relative to the moving skin). Takes effect from the
    /// next frame; a frame in flight finishes with the old body.
    ///
    /// # Errors
    /// [`GpuError::TooLarge`] when a buffer would exceed the device's limits,
    /// [`GpuError::Device`] when the device rejects a buffer.
    pub fn update_body(
        &mut self,
        grid: &SdfGrid,
        skin: Option<&BodyCollisionMesh<'_>>,
        velocity: Option<&BodyVelocity>,
    ) -> Result<(), GpuError> {
        let device = self.solver.device.clone();
        let (queue, pipelines) = (&self.solver.queue, &self.solver.pipelines);
        let buffers = &mut self.buffers;
        let f = &mut self.params;
        scoped(&device, GpuError::Device, || {
            buffers.write_growing(
                &device,
                queue,
                pipelines,
                Buf::Grid,
                bytemuck::cast_slice(&grid.data),
            )?;
            f.grid_origin = [grid.origin.x as f32, grid.origin.y as f32, grid.origin.z as f32];
            f.grid_cell = grid.cell as f32;
            f.grid_dims = grid.dims.map(|d| d as u32);
            f.n_skin = 0;
            if let Some(s) = skin.filter(|s| s.body_vertex_count > 0) {
                let hash = pack::SkinHash::new(s.positions, s.normals, s.body_vertex_count);
                buffers.write_growing(
                    &device,
                    queue,
                    pipelines,
                    Buf::Skin,
                    bytemuck::cast_slice(&hash.rows),
                )?;
                buffers.write_growing(
                    &device,
                    queue,
                    pipelines,
                    Buf::SkinHead,
                    bytemuck::cast_slice(&hash.head),
                )?;
                buffers.write_growing(
                    &device,
                    queue,
                    pipelines,
                    Buf::SkinNext,
                    bytemuck::cast_slice(&hash.next),
                )?;
                f.n_skin = hash.len() as u32;
            }
            let rows = pack::velocity_rows(velocity);
            buffers.write_growing(
                &device,
                queue,
                pipelines,
                Buf::BodyVel,
                bytemuck::cast_slice(&rows),
            )?;
            f.body_motion = u32::from(velocity.is_some());
            Ok::<_, GpuError>(())
        })?
    }

    /// [`GpuSim::update_body`] with the grid, skin and surface velocity of `avatar` (a re-posed
    /// avatar, see [`Avatar::posed_in`]).
    ///
    /// # Errors
    /// See [`GpuSim::update_body`].
    pub fn update_avatar(&mut self, avatar: &Avatar) -> Result<(), GpuError> {
        self.update_body(
            &avatar.grid,
            avatar.body_collision_mesh().as_ref(),
            avatar.velocity.as_ref(),
        )
    }

    /// Reads `count` words of buffer `which` from word `offset` (blocking).
    pub(crate) fn read_words(&self, which: Buf, offset: u64, count: u64) -> Result<Vec<u32>, GpuError> {
        let device = &self.solver.device;
        let size = count * 4;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("azam-sim-gpu probe"),
            size: crate::layout::padded(size),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_buffer_to_buffer(self.buffers.get(which), offset * 4, &staging, 0, size);
        self.solver.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| GpuError::Readback(e.to_string()))?
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        let words = {
            let view = staging.slice(..size).get_mapped_range();
            bytemuck::cast_slice::<u8, u32>(&view).to_vec()
        };
        staging.unmap();
        Ok(words)
    }

    /// Contact statistics of the last detection (blocking read; for diagnostics).
    ///
    /// # Errors
    /// [`GpuError::Readback`] when the read fails.
    pub fn contact_stats(&self) -> Result<ContactStats, GpuError> {
        let contacts = self.read_words(Buf::Ctrl, 0, 1)?[0];
        let hash_entries = self.read_words(Buf::Hash, u64::from(TRIANGLE_HASH_BUCKETS), 1)?[0];
        Ok(ContactStats {
            contacts,
            contact_capacity: self.params.contact_cap,
            hash_entries,
            hash_capacity: self.params.entry_cap,
            triangles: self.params.n_tri,
            particles: self.params.n,
        })
    }
}

#[cfg(test)]
impl GpuSim {
    /// Uploads pending edits and frame parameters for `dt`, then runs `dispatches` (no read-back).
    pub(crate) fn run_passes(&mut self, sim: &mut Simulation, dt: f64, dispatches: &[Dispatch]) {
        let frames = sim.params.frames_per_advance();
        self.prepare(sim, dt, frames);
        self.submit(dispatches, false);
    }

    /// The frame parameters as last uploaded.
    pub(crate) fn frame_params(&self) -> &FrameParams {
        &self.params
    }

    /// Writes raw words into buffer `which` at word `offset`.
    pub(crate) fn write_words(&self, which: Buf, offset: u64, words: &[u32]) {
        self.solver
            .queue
            .write_buffer(self.buffers.get(which), offset * 4, bytemuck::cast_slice(words));
    }
}
