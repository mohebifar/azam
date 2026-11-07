//! Building a simulation from a project: meshes, particles, constraints, sewing, pins and initial
//! positions.

mod anchors;
pub mod coloring;
mod constraints;
mod particles;
mod pieces;
mod remap;
mod sewing;

use std::sync::Arc;

use azam_avatar::Avatar;
use azam_doc::{Piece, PieceId, Project};

pub use anchors::nearest_particle;
pub(crate) use pieces::PieceKey;
pub use pieces::{estimate_particles, piece_spacing};

use self::coloring::Coloring;
use self::particles::Material;
use crate::collide::body_tri::BodyTriangles;
use crate::collide::particle::ParticleContacts;
use crate::params::SimParams;
use crate::state::{BendConstraints, PieceRecord, Pins, Simulation, StretchConstraints};

/// How to place pieces when building.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildOptions {
    /// Place every edited piece at its arrangement instead of mapping the previous drape onto it.
    pub rearrange: bool,
    /// Pieces placed at their arrangement whatever the previous simulation holds.
    pub rearrange_ids: Vec<PieceId>,
    /// Push particles that start inside the body out of it.
    pub project_out: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            rearrange: false,
            rearrange_ids: Vec::new(),
            project_out: true,
        }
    }
}

/// The arrangement of `piece` as the avatar crate reads it.
fn arrangement(piece: &Piece) -> azam_avatar::Arrangement {
    serde_json::to_value(&piece.arr)
        .and_then(serde_json::from_value)
        .unwrap_or_else(|_| azam_avatar::Arrangement::in_slot("front", azam_avatar::DEFAULT_OFFSET_MM))
}

/// Records of every simulated piece, reusing the previous meshes of unchanged pieces.
fn records<'p>(
    project: &'p Project,
    previous: Option<&Simulation>,
    pd_scale: f64,
) -> Vec<(PieceRecord, &'p Piece)> {
    let mut out = Vec::new();
    let mut offset = 0;
    for piece in pieces::simulated_pieces(project) {
        let graded = pieces::graded(project, piece);
        let key = PieceKey::new(piece, &graded, pd_scale);
        let reused = previous
            .and_then(|p| p.records.iter().find(|r| r.id == piece.id))
            .filter(|r| r.key == key)
            .map(|r| (Arc::clone(&r.mesh), Arc::clone(&r.topology)));
        let (mesh, topology) = if let Some(m) = reused {
            m
        } else {
            let Some(mesh) = pieces::mesh(piece, &graded, pd_scale) else {
                continue;
            };
            let topology = mesh.topology();
            (Arc::new(mesh), Arc::new(topology))
        };
        let count = mesh.vertex_count();
        out.push((
            PieceRecord {
                id: piece.id.clone(),
                offset,
                count,
                mesh,
                topology,
                state: piece.state,
                fabric: piece.fabric.clone(),
                arrangement: arrangement(piece),
                graded,
                key,
            },
            piece,
        ));
        offset += count;
    }
    out
}

impl Simulation {
    /// Builds the simulation of `project` on `avatar` with default options. With `previous`,
    /// unchanged pieces keep their drape and edited pieces get it mapped onto their new mesh.
    #[must_use]
    pub fn build(
        project: &Project,
        avatar: &Avatar,
        previous: Option<&Simulation>,
        params: &SimParams,
    ) -> Self {
        Self::build_with(project, avatar, previous, params, &BuildOptions::default())
    }

    /// Builds the simulation of `project` on `avatar`.
    ///
    /// Pieces are simulated in pattern order. Each gets a mesh at its particle distance (scaled by
    /// [`SimParams::pd_scale`]), lumped masses from its fabric's density, stretch and shear
    /// constraints along every mesh edge and bending constraints across every interior edge
    /// (grain-dependent compliances from the fabric), fold lines, sewing constraints along every
    /// seam, button fasteners and pins.
    ///
    /// When `previous` is given and pieces are not re-arranged, the previous gravity hold-back
    /// carries over, so a rebuild during sewing does not drop the garment.
    #[must_use]
    pub fn build_with(
        project: &Project,
        avatar: &Avatar,
        previous: Option<&Simulation>,
        params: &SimParams,
        options: &BuildOptions,
    ) -> Self {
        let recs = records(project, previous, params.pd_scale);
        let n: usize = recs.iter().map(|(r, _)| r.count).sum();
        let mut particles = particles::allocate(n);
        let records: Vec<PieceRecord> = recs.iter().map(|(r, _)| r.clone()).collect();
        remap::initial_positions(
            avatar,
            &records,
            previous,
            options.rearrange,
            &options.rearrange_ids,
            &mut particles.x,
        );
        particles.prev.clone_from(&particles.x);

        let mut stretch = StretchConstraints::default();
        let mut bend = BendConstraints::default();
        for (ri, (r, piece)) in recs.iter().enumerate() {
            let m = Material::new(project, piece);
            particles::fill(&mut particles, ri, r, &m);
            if r.state != azam_doc::PieceState::Off {
                constraints::add_piece(&mut stretch, &mut bend, piece, r, &m);
            }
        }
        let sewing = sewing::sewing(project, &records);
        for c in 0..sewing.len() {
            for q in [sewing.a[c], sewing.b0[c], sewing.b1[c]] {
                particles.sewn[q as usize] = true;
            }
        }
        let fasteners = anchors::fasteners(project, &records);
        let pinned = anchors::pinned(project, &records);
        let mut pins = Pins::default();
        for &q in &pinned {
            pins.vertex.push(q);
            pins.pos
                .extend_from_slice(&particles.x[q as usize * 3..q as usize * 3 + 3]);
            particles.inv_mass[q as usize] = 0.0;
        }
        let coloring = Coloring::new(n, &stretch, &bend, &fasteners, &sewing);
        let keep_gravity = !options.rearrange && options.rearrange_ids.is_empty();
        let good = particles.x.clone();
        let body_triangles = BodyTriangles::new(avatar, &records, &particles.margin);
        let mut sim = Self {
            records,
            particles,
            stretch,
            bend,
            sewing,
            fasteners,
            pins,
            coloring,
            params: *params,
            grab: None,
            time: 0.0,
            energy: 0.0,
            frames: 0,
            sew_gap: 0.0,
            gravity_scale: previous.filter(|_| keep_gravity).and_then(|p| p.gravity_scale),
            piece_gravity: Vec::new(),
            normals: vec![0.0; n * 3],
            body_fixes: 0,
            good,
            particle_contacts: ParticleContacts::default(),
            triangle_contacts: None,
            body_triangles,
        };
        if options.project_out {
            sim.project_out(avatar);
        }
        sim
    }
}

impl Simulation {
    /// Puts every piece back at its arrangement around the avatar, as a fresh build would: no
    /// velocity, clocks and gravity hold-back reset, pins re-taken, contacts and grab dropped.
    pub fn reset(&mut self, avatar: &Avatar) {
        for r in &self.records {
            remap::arrange(avatar, r, &mut self.particles.x);
        }
        let p = &mut self.particles;
        p.prev.clone_from(&p.x);
        p.v.fill(0.0);
        for (e, &q) in self.pins.vertex.iter().enumerate() {
            let k = q as usize * 3;
            self.pins.pos[e * 3..e * 3 + 3].copy_from_slice(&p.x[k..k + 3]);
        }
        self.good.clone_from(&p.x);
        self.time = 0.0;
        self.energy = 0.0;
        self.frames = 0;
        self.sew_gap = 0.0;
        self.gravity_scale = None;
        self.piece_gravity.clear();
        self.grab = None;
        self.body_fixes = 0;
        self.particle_contacts = ParticleContacts::default();
        self.triangle_contacts = None;
        self.project_out(avatar);
    }

    /// Replaces every particle position (`x, y, z` per particle), for example after an edit made
    /// elsewhere; velocities are cleared and the positions become the substep start and the
    /// last good state.
    ///
    /// # Panics
    ///
    /// Panics if `x` does not hold three values per particle.
    pub fn set_positions(&mut self, x: &[f32]) {
        let p = &mut self.particles;
        assert_eq!(x.len(), p.x.len(), "expected three values per particle");
        p.x.copy_from_slice(x);
        p.prev.copy_from_slice(x);
        p.v.fill(0.0);
        self.good.copy_from_slice(x);
    }
}
