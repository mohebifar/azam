//! Builds the morph-target avatar pack read by [`azam_avatar::MhModel::parse`] from the CC0 system
//! assets of MakeHuman 1.1+ (base mesh, morph targets, skeleton and skin weights).
//!
//! The pack holds, in millimetres with y up, the avatar facing +z and its left side on +x:
//!
//! - the body vertices of `3dobjs/base.obj` (group `body`), then the eye helper vertices
//!   (`helper-l-eye`, `helper-r-eye`), then the joint helper cubes the pose needs (`joint-*`);
//! - body and eye triangles (the quads split in two);
//! - the morph targets the fit composes: `g_female` / `g_male` (MakeHuman's default ethnic mix,
//!   the mean of its three `*-young` ethnic targets), the eighteen
//!   `universal-{female,male}-young-*` muscle/weight targets, the `measure-*-incr/decr` targets
//!   and the two nipple smoothing targets;
//! - limb skin weights summed over each limb's bone subtree of `rigs/default_weights.mhw`
//!   (normalised per vertex), plus optional forearm, shin and spine weights for posing;
//! - measuring loops and pairs, landmarks and joint vertex groups, derived from the CC0 mesh and
//!   targets as described in [`derive`].
//!
//! See `assets/makehuman/README.md` for the sources and how to rebuild.

pub mod derive;
pub mod pack;
pub mod source;

use std::fmt;

/// Why the pack could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildError(pub String);

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BuildError {}

/// Shorthand for an error with a message.
pub(crate) fn err<T>(msg: impl Into<String>) -> Result<T, BuildError> {
    Err(BuildError(msg.into()))
}

pub use pack::{BuildOptions, build_pack};
