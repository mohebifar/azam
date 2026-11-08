//! Cloth simulation for azam: an XPBD solver on the CPU.
//!
//! Units are millimetres, grams and seconds. In 3D, y is up, the avatar faces +z and stands with
//! its feet on y = 0. Gravity is 9810 mm/s².
//!
//! # Use
//!
//! 1. Resolve the project's stored settings with [`resolve_settings`] and derive [`SimParams`].
//! 2. [`Simulation::build`] meshes every piece, places it around the avatar (or keeps the drape of
//!    a previous simulation for unchanged pieces), and builds the constraints.
//! 3. Call [`Simulation::advance`] once per screen frame; read [`Simulation::piece_views`] (after
//!    [`Simulation::compute_normals`]) to draw. [`Simulation::grab_at`],
//!    [`Simulation::set_grab_target`] and [`Simulation::release_grab`] drag cloth;
//!    [`AutoStop`] decides when a run has settled.
//! 4. [`Simulation::fit_values`] and [`Simulation::fit_summary`] give fit maps.
//!
//! # Solver
//!
//! Each frame runs `substeps` substeps. A substep integrates gravity (held back while seams are
//! still open) and damping, pushes particles out of the body's distance grid and above the floor,
//! then runs `iterations` Gauss–Seidel passes of XPBD stretch constraints (grain-dependent
//! compliance, softer in compression) and sewing constraints (rate-limited while seams close),
//! followed by one pass of bending (resisting folding only, two-way on fold lines) and button
//! fasteners. Cloth-cloth contacts come next: particle spheres through a spatial hash by default,
//! or vertex-triangle contacts ([`SimParams::cpu_triangle_collision`]); between layers the
//! higher layer is kept outside along the body normal. Then the grab, a final penetration guard,
//! skin-vertex vs cloth-triangle collision (morph-target avatars), pins and the velocity update.
//!
//! # Data layout
//!
//! Particle state ([`Particles`]) and constraints ([`StretchConstraints`], [`BendConstraints`],
//! [`SewingConstraints`], [`Fasteners`], [`Pins`]) are flat arrays, positions and velocities as
//! `x, y, z` triples in `f32`, with arithmetic done in `f64` and rounded on every store. Each
//! constraint kind also has graph-colour batches ([`Coloring`]) so a parallel solver can run a
//! colour at once. The untangler ([`untangle`]) finds cloth that crossed cloth, on a thread or
//! synchronously.
//!
//! Golden tests in `tests/` check the results against recorded reference data.

mod build;
mod collide;
mod fit;
mod flatten;
mod numeric;
mod output;
mod params;
mod settings;
mod settle;
mod state;
mod step;
pub mod untangle;

pub use build::coloring::{ColorBatches, Coloring, colorize};
pub use build::{BuildOptions, estimate_particles, nearest_particle, piece_spacing};
pub use collide::body_tri::{BODY_HASH_BUCKETS, BODY_HASH_CELL_MM, BodyTriangles};
pub use collide::triangle::{TRIANGLE_HASH_BUCKETS, TriangleContacts, sewn_piece_mask};
pub use fit::{FitMode, FitSummary};
pub use flatten::flatten_piece;
pub use output::PieceView;
pub use params::{STANDARD_GRAVITY_MM_S2, SimParams};
pub use settings::{
    FrictionModel, ResolvedSettings, SIM_DEFAULTS, SettingKey, SimPreset, SimValues, preset_settings,
    resolve_settings,
};
pub use settle::{AutoStop, SettleReason, SettleTracker, StopReason};
pub use state::{
    BendConstraints, Fasteners, Grab, Particles, PieceRecord, Pins, SewingConstraints, Simulation,
    StretchConstraints,
};
pub use step::{DEFAULT_DT_S, GRAB_RADIUS_MM};
