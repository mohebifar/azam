//! Morph-target avatar built from a CC0 body model pack: the body is fitted to tailor
//! measurements by least squares over morph-target weights, posed by linear-blend skinning, and
//! given a collision grid computed from its mesh.
//!
//! The pack is built from the CC0 assets of `MakeHuman` by `tools/mhpack` and shipped at
//! `assets/makehuman/makehuman-cc0.bin`; load it with [`MhModel::load_bundled`], or parse any
//! pack's uncompressed bytes with [`MhModel::parse`]. Without a pack the parametric body is used
//! (see [`crate::Avatar::build`]).

mod build;
mod bundled;
mod compose;
mod fit;
mod measure;
mod parse;
mod pose;

pub use build::{DEFAULT_MH_GRID_CELL_MM, MeasureTape, MhBody, MhReport};
pub use bundled::{PACK_DIR, PACK_ENV, PACK_FILE, find_pack, pack_candidates};
pub use compose::MhShape;
pub use fit::{MhFit, solve_linear};
pub use measure::{Tape, tape_measure};
pub use parse::{LimbWeights, MhModel, MhParseError, MorphTarget, PoseWeights};
pub use pose::PoseInfo;
