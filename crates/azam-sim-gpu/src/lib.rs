//! Cloth simulation for azam on the GPU: the frame of the XPBD solver as wgpu compute passes.
//!
//! The CPU builds the simulation with [`azam_sim`] (meshes, constraints, arrangement, start-up
//! push-out) and keeps it as the model the rest of the app reads; the GPU runs every frame and
//! hands the positions back.
//!
//! # Use
//!
//! 1. [`GpuSolver::new`] on the app's device (or [`GpuSolver::new_default`] for a device of its
//!    own) builds every pipeline once. Any error means: use the CPU solver.
//! 2. [`GpuSolver::upload`] attaches an [`azam_sim::Simulation`] (rebuild means upload again).
//! 3. Once per screen frame, [`GpuSim::step_frame`] runs
//!    [`azam_sim::SimParams::frames_per_advance`] frames on the GPU and writes the positions,
//!    gravity hold-back, seam gap, energy and clocks into the simulation. Normals, fit maps and
//!    picking keep using the CPU-side simulation. [`GpuSim::begin_frame`] /
//!    [`GpuSim::try_finish_frame`] split the frame for callers that do not want to wait.
//! 4. Grabbing works through the simulation as on the CPU ([`azam_sim::Simulation::grab_at`],
//!    `set_grab_target`, `release_grab`); pins likewise (`pin`, `unpin`). After CPU edits of
//!    positions call [`GpuSim::upload_positions`]; to restart call [`GpuSim::reset`].
//!
//! # Sharing a device
//!
//! The solver needs wgpu's default limits (eight storage buffers per stage, 256-thread
//! workgroups, a dynamic uniform offset of 256 bytes) and storage buffers large enough for the
//! simulation; create the shared device with [`GpuSolver::required_limits`] for the largest
//! garments, then pass clones of the device and queue to [`GpuSolver::new`].
//!
//! # Solver
//!
//! A frame computes the seam gap and gravity hold-back, then per substep: integration, body and
//! floor collision against the distance grid, `iterations` passes of graph-coloured (parallel
//! Gauss–Seidel) stretch and sewing constraints, bending and fasteners, vertex-triangle cloth
//! contacts through a GPU spatial hash (re-detected every few substeps, resolved with friction
//! through fixed-point atomic accumulators), grab, the penetration guard, skin-vertex vs
//! cloth-triangle collision for morph-target avatars, pins and the velocity update. Because a
//! colour of constraints is solved at once, results differ from the CPU solver's sequential
//! order; they agree in aggregate (see the crate's tests).
//!
//! Units are millimetres, grams and seconds; y is up.

mod buffers;
mod device;
mod error;
mod frame;
mod layout;
mod options;
mod pack;
mod passes;
mod pipelines;
mod readback;
mod sim;
mod untangle;

#[cfg(test)]
mod tests;

pub use device::{GpuSolver, STORAGE_BUFFERS_PER_STAGE};
pub use error::GpuError;
pub use layout::FIXED_POINT_SCALE;
pub use options::GpuOptions;
pub use sim::{ContactStats, FrameReport, GpuSim};
