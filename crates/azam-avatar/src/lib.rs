//! Avatars for draping.
//!
//! Units are millimetres. In 3D, y is up, the avatar faces +z and stands with its feet on y = 0;
//! its left side is +x.
//!
//! Two body models are available:
//!
//! - the **parametric** mannequin ([`ParametricBody`]): an elliptical torso through the measured
//!   girths, blended with round-cone limbs and ellipsoid hands, feet and head. It needs no assets
//!   and is the default.
//! - the **morph-target** body ([`makehuman`]): a CC0 body model pack fitted to the tailor
//!   measurements. It is used when a pack has been loaded ([`MhModel::load_bundled`] finds the
//!   one shipped at `assets/makehuman/makehuman-cc0.bin`; [`MhModel::parse`] reads any).
//!
//! [`Avatar::build`] picks the model. What the cloth solver needs from an [`Avatar`]:
//!
//! - [`Avatar::grid`]: an [`SdfGrid`] of signed distances, queried with [`SdfGrid::sample`]
//!   (distance and gradient) or [`SdfGrid::distance`];
//! - [`Avatar::body_collision_mesh`]: skin vertices and normals for cloth-surface collision
//!   (morph-target avatars only);
//! - [`arrange_positions`]: the initial 3D placement of each piece's mesh vertices.
//!
//! The scene draws [`Avatar::mesh`]. Pattern drafting reads a [`TorsoProfile`]
//! ([`drafting_profile`]).
//!
//! Motion: a [`Motion`] gives an [`AvatarPose`] at every instant ([`pose_at`]);
//! [`Avatar::posed_in`] re-poses the mesh, joints and collision grid (in one grid layout per
//! motion, [`Avatar::motion_frame`]), and [`Avatar::with_velocity_from`] adds the body surface
//! velocity ([`BodyVelocity`]) the solver uses for friction against a moving body;
//! [`Avatar::posed_after`] does both at once, the velocity alongside the grid (a few
//! milliseconds in release, fast enough for every frame).
//!
//! Golden tests in `tests/` check the results against recorded reference data.

mod arrange;
mod avatar;
mod grid;
mod joints;
pub mod makehuman;
mod math;
mod measurements;
mod mesh_sdf;
mod motion;
mod parametric;
mod params;
mod posed_sdf;
mod profile;
mod rig;
mod sdf;
mod surface;
mod velocity;

pub use arrange::{
    Arrangement, ArrangementPoint, CHADOR_DRAPE, ChainFrame, DEFAULT_OFFSET_MM, SLOTS, Slot, SlotFrame,
    SlotKind, arrange_positions, arrangement_points, chain_frames, chain_position, slot_by_id, slot_frame,
};
pub use avatar::{Avatar, AvatarKind, BodyCollisionMesh, drafting_profile};
pub use grid::{DISTANCE_CLAMP_MM, FAR_DISTANCE_MM, GridFrame, SdfGrid, SdfSample};
pub use joints::{Joints, LimbJoints, Side};
pub use makehuman::MhModel;
pub use math::{DEG, Pchip, ellipse_axes, ellipse_perimeter, smooth_min};
pub use measurements::{
    FIT_TARGET_COUNT, FIT_TARGETS, MEASUREMENT_COUNT, MEASUREMENTS, MeasureKind, MeasurementDef,
    Measurements, measurement_index,
};
pub use mesh_sdf::{mesh_sdf, vertex_normals};
pub use motion::{AvatarPose, MOTION_RAMP_S, Motion, pose_at};
pub use parametric::ParametricBody;
pub use params::{
    AvatarParams, DEFAULT_ARM_ANGLE_DEG, DEFAULT_SKIN, FEMALE_SIZES, MALE_SIZES, Sex, SizeChartEntry,
    size_chart,
};
pub use posed_sdf::mesh_sdf_in;
pub use profile::{BodyLevels, REFERENCE_HEIGHT_MM, TorsoProfile, TorsoSlice, TorsoTable};
pub use sdf::{BodyPrimitive, Ellipsoid, RoundCone, Shape};
pub use surface::{AvatarMesh, surface_mesh};
pub use velocity::BodyVelocity;
