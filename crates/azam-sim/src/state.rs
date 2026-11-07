//! The simulation's data: per-piece records, particle state and constraint arrays.
//!
//! Everything per particle or per constraint is a flat array (structure of arrays) so the arrays
//! can be uploaded as they are; positions and velocities are `x, y, z` triples in `f32`.

use std::sync::Arc;

use azam_doc::{FabricId, PieceId, PieceState};
use azam_geom::OutlinePoint;
use azam_mesh::{PieceMesh, Topology};

use crate::build::PieceKey;
use crate::build::coloring::Coloring;
use crate::collide::body_tri::BodyTriangles;
use crate::collide::particle::ParticleContacts;
use crate::collide::triangle::TriangleContacts;
use crate::params::SimParams;

/// One simulated piece.
#[derive(Clone, Debug)]
pub struct PieceRecord {
    /// The piece.
    pub id: PieceId,
    /// Index of the piece's first particle.
    pub offset: usize,
    /// Number of particles (mesh vertices) of the piece.
    pub count: usize,
    /// The piece's mesh in its 2D pattern frame (shared with the simulation it was reused from).
    pub mesh: Arc<PieceMesh>,
    /// Unique edges of the mesh.
    pub topology: Arc<Topology>,
    /// Simulation state of the piece.
    pub state: PieceState,
    /// Fabric of the piece.
    pub fabric: FabricId,
    /// Arrangement around the avatar, for resets.
    pub arrangement: azam_avatar::Arrangement,
    /// Outline points in the simulated size, for remapping a later rebuild.
    pub graded: Vec<OutlinePoint>,
    /// Everything that decides the mesh; equal keys mean the mesh can be reused.
    pub(crate) key: PieceKey,
}

impl PieceRecord {
    /// Range of the piece's particles.
    #[must_use]
    pub fn particles(&self) -> std::ops::Range<usize> {
        self.offset..self.offset + self.count
    }
}

/// Per-particle state and properties. Index `q` is particle `q`; `x, y, z` arrays hold `3 * q ..
/// 3 * q + 3`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Particles {
    /// Positions, mm.
    pub x: Vec<f32>,
    /// Positions at the start of the current substep, mm.
    pub prev: Vec<f32>,
    /// Velocities, mm/s.
    pub v: Vec<f32>,
    /// Inverse masses, 1/g (0 for pinned, frozen and switched-off particles).
    pub inv_mass: Vec<f32>,
    /// Inverse masses before pinning.
    pub inv_mass_free: Vec<f32>,
    /// Internal damping rate, per second.
    pub damping: Vec<f32>,
    /// Clearance kept from the body, mm.
    pub margin: Vec<f32>,
    /// Collision radius for particle-particle cloth contacts, mm.
    pub radius: Vec<f32>,
    /// Friction coefficient, 0–0.9.
    pub friction: Vec<f32>,
    /// Layer (higher is outside).
    pub layer: Vec<i8>,
    /// Particle distance of the piece's mesh, mm.
    pub spacing: Vec<f32>,
    /// Fabric thickness, mm.
    pub thickness: Vec<f32>,
    /// Index of the particle's piece record.
    pub piece: Vec<u32>,
    /// Whether the particle's piece takes part in the simulation.
    pub active: Vec<bool>,
    /// Inflation pressure of the particle's piece.
    pub pressure: Vec<f32>,
    /// Whether the particle is part of a sewing constraint.
    pub sewn: Vec<bool>,
}

impl Particles {
    /// Number of particles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inv_mass.len()
    }

    /// Whether there are no particles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inv_mass.is_empty()
    }

    /// Position of particle `q`.
    #[must_use]
    pub fn position(&self, q: usize) -> [f32; 3] {
        [self.x[3 * q], self.x[3 * q + 1], self.x[3 * q + 2]]
    }
}

/// XPBD distance constraints along mesh edges (stretch and shear).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StretchConstraints {
    /// First particle.
    pub i: Vec<u32>,
    /// Second particle.
    pub j: Vec<u32>,
    /// Rest length, mm.
    pub rest: Vec<f32>,
    /// Compliance in s²/g (mm, g, s units).
    pub compliance: Vec<f32>,
    /// Compliance multiplier while compressed (buckling).
    pub compression: Vec<f32>,
}

impl StretchConstraints {
    /// Number of constraints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.i.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.i.is_empty()
    }
}

/// Bending constraints between the vertices opposite each interior edge.
///
/// A non-negative compliance resists folding only (the distance may grow freely); a negative
/// compliance marks a fold line, which holds its rest distance both ways with stiffness `1 / (1 +
/// |compliance|)`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BendConstraints {
    /// First particle.
    pub i: Vec<u32>,
    /// Second particle.
    pub j: Vec<u32>,
    /// Rest distance, mm.
    pub rest: Vec<f32>,
    /// Compliance (dimensionless relaxation); negative for fold lines.
    pub compliance: Vec<f32>,
}

impl BendConstraints {
    /// Number of constraints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.i.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.i.is_empty()
    }
}

/// Sewing constraints: particle `a` is pulled onto the point `(1 - u) * b0 + u * b1` of the other
/// side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SewingConstraints {
    /// Particle on one side.
    pub a: Vec<u32>,
    /// First particle of the matching edge on the other side.
    pub b0: Vec<u32>,
    /// Second particle of the matching edge.
    pub b1: Vec<u32>,
    /// Position along the edge `b0 → b1`, 0–1.
    pub u: Vec<f32>,
    /// Index of the seam in the project's seam list (sorted by id).
    pub seam: Vec<u32>,
}

impl SewingConstraints {
    /// Number of constraints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.a.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.a.is_empty()
    }
}

/// Buttons held to their buttonholes: particle pairs pulled together to a small gap.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fasteners {
    /// Button particle.
    pub a: Vec<u32>,
    /// Buttonhole particle.
    pub b: Vec<u32>,
}

/// Pinned particles and where they are held.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pins {
    /// Pinned particles.
    pub vertex: Vec<u32>,
    /// Held positions, `x, y, z` per pin, mm.
    pub pos: Vec<f32>,
    /// Per pin: whether its position is fixed in space (set on the draped garment) rather than
    /// taken from wherever the cloth is when the simulation starts or resets.
    pub fixed: Vec<bool>,
}

/// Cloth held by the pointer: particles near the grabbed point follow a target that chases the
/// pointer at a bounded speed.
#[derive(Clone, Debug, PartialEq)]
pub struct Grab {
    /// Grabbed particles.
    pub vertices: Vec<u32>,
    /// How firmly each particle follows (fraction of the gap closed per substep).
    pub weights: Vec<f64>,
    /// Offset of each particle from the grab point, `x, y, z` per particle, mm.
    pub offsets: Vec<f64>,
    /// Where the pointer wants the grab point, mm.
    pub target: [f64; 3],
    /// Where the grab point is now (set on the first substep after grabbing), mm.
    pub current: Option<[f64; 3]>,
}

/// A cloth simulation of one project on one avatar.
#[derive(Clone, Debug)]
pub struct Simulation {
    /// Simulated pieces, in simulation order.
    pub records: Vec<PieceRecord>,
    /// Particle state.
    pub particles: Particles,
    /// Stretch and shear constraints.
    pub stretch: StretchConstraints,
    /// Bending and fold constraints.
    pub bend: BendConstraints,
    /// Sewing constraints.
    pub sewing: SewingConstraints,
    /// Button-to-buttonhole constraints.
    pub fasteners: Fasteners,
    /// Pinned particles.
    pub pins: Pins,
    /// Parallel batches of the constraints (for solvers that run a batch at once).
    pub coloring: Coloring,
    /// Solver parameters.
    pub params: SimParams,
    /// Cloth held by the pointer.
    pub grab: Option<Grab>,
    /// Simulated time, s.
    pub time: f64,
    /// Motion energy after the last frame: RMS speed of the free particles, mm/s.
    pub energy: f64,
    /// Frames simulated.
    pub frames: u64,
    /// Mean distance between the two sides of the seams before the last frame, mm.
    pub sew_gap: f64,
    /// Gravity scale held back while seams are open (`None` before the first frame).
    pub gravity_scale: Option<f64>,
    /// Per-piece gravity scale (a garment added over a sewn one floats until its seams close).
    pub piece_gravity: Vec<f32>,
    /// Vertex normals from the last [`Simulation::compute_normals`], `x, y, z` per particle.
    pub normals: Vec<f32>,
    /// Number of cloth-triangle corrections made by the body-skin collision in the last frame.
    pub body_fixes: usize,
    /// Last NaN-free positions, restored if the solver blows up.
    pub(crate) good: Vec<f32>,
    pub(crate) particle_contacts: ParticleContacts,
    pub(crate) triangle_contacts: Option<TriangleContacts>,
    pub(crate) body_triangles: Option<BodyTriangles>,
}

impl Simulation {
    /// Number of particles.
    #[must_use]
    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    /// Index of the record of piece `id`.
    #[must_use]
    pub fn record_index(&self, id: &PieceId) -> Option<usize> {
        self.records.iter().position(|r| &r.id == id)
    }
}
