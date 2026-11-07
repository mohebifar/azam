//! Triangle meshes of pattern pieces for the cloth solver.
//!
//! Units are millimetres. The 2D frame has x to the right and y down, as in the pattern window.
//!
//! [`mesh_piece`] samples the piece outline at a target spacing, fills the inside with a lattice of
//! near-equilateral triangles aligned with the grain, adds samples along internal lines (folds,
//! darts) and triangulates everything with a Delaunay triangulation. Boundary samples come first in
//! the vertex list, in outline order, so the solver can find seam vertices by index.
//!
//! [`PieceMesh::topology`] lists the unique edges with their opposite vertices (for bending
//! constraints) and [`PieceMesh::locate`] maps a 2D point to barycentric coordinates in the mesh.
//!
//! Golden tests in `tests/` check the results against recorded reference data for every garment
//! template.

mod delaunay;
mod locate;
mod mesh;
mod numeric;
mod spatial;
mod topology;

pub use locate::Location;
pub use mesh::{
    DEFAULT_SPACING_MM, InternalLine, InternalSamples, MAX_SPACING_MM, MIN_SPACING_MM, MeshOptions,
    PieceMesh, mesh_piece,
};
pub use topology::{Edge, Topology};
