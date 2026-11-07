//! The avatar the scene shows and the cloth solver collides with.

use std::sync::Arc;

use glam::DVec3;

use crate::grid::SdfGrid;
use crate::joints::Joints;
use crate::makehuman::{DEFAULT_MH_GRID_CELL_MM, MhBody, MhModel, MhReport};
use crate::motion::AvatarPose;
use crate::parametric::ParametricBody;
use crate::params::AvatarParams;
use crate::profile::TorsoProfile;
use crate::rig::{Rig, RigJoints};
use crate::surface::{AvatarMesh, surface_mesh};
use crate::velocity::BodyVelocity;

/// Which body model an avatar was built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvatarKind {
    /// The built-in parametric mannequin (no assets needed).
    Parametric,
    /// The morph-target body fitted to measurements.
    MakeHuman,
}

/// The skin vertices the solver keeps the cloth surface clear of (morph-target avatars only).
#[derive(Clone, Copy, Debug)]
pub struct BodyCollisionMesh<'a> {
    /// Vertex positions, `x, y, z` per vertex; the first `body_vertex_count` are skin.
    pub positions: &'a [f32],
    /// Unit vertex normals, `x, y, z` per vertex.
    pub normals: &'a [f32],
    /// Number of skin vertices at the start of the buffers.
    pub body_vertex_count: usize,
}

/// A built avatar.
#[derive(Clone, Debug, PartialEq)]
pub struct Avatar {
    /// The body model used.
    pub kind: AvatarKind,
    /// Torso sections, landmark heights and shoulder line (also used for drafting).
    pub profile: TorsoProfile,
    /// Limb joints.
    pub joints: Joints,
    /// Signed distance grid for cloth collision.
    pub grid: SdfGrid,
    /// Display mesh with normals.
    pub mesh: AvatarMesh,
    /// Fit details of a morph-target avatar.
    pub makehuman: Option<MhReport>,
    /// Velocity of the body surface while it moves (see [`Avatar::with_velocity_from`]); `None`
    /// for a body at rest.
    pub velocity: Option<BodyVelocity>,
    pub(crate) body_vertex_count: usize,
    pub(crate) rig: Option<Arc<Rig>>,
    pub(crate) pose: AvatarPose,
}

impl Avatar {
    /// Builds the avatar for `params`: the morph-target body when a model is loaded, otherwise the
    /// parametric mannequin.
    #[must_use]
    pub fn build(params: &AvatarParams, model: Option<&MhModel>) -> Self {
        match model {
            Some(m) => Self::makehuman(m, params, DEFAULT_MH_GRID_CELL_MM),
            None => Self::parametric(params),
        }
    }

    /// The parametric mannequin: distance grid plus a surface-nets display mesh.
    #[must_use]
    pub fn parametric(params: &AvatarParams) -> Self {
        Self::from_parametric(ParametricBody::new(params))
    }

    /// Wraps an already sized parametric body.
    #[must_use]
    pub fn from_parametric(body: ParametricBody) -> Self {
        let grid = body.sdf_grid();
        let mesh = surface_mesh(&grid);
        let rig = Rig::parametric(&body, mesh.clone(), grid.cell);
        Self {
            kind: AvatarKind::Parametric,
            profile: body.profile,
            joints: body.joints,
            grid,
            body_vertex_count: mesh.vertex_count(),
            mesh,
            makehuman: None,
            velocity: None,
            rig: Some(Arc::new(rig)),
            pose: AvatarPose::REST,
        }
    }

    /// The morph-target body fitted to `params`, with a collision grid of cell size `grid_cell`.
    #[must_use]
    pub fn makehuman(model: &MhModel, params: &AvatarParams, grid_cell: f64) -> Self {
        let body = model.build_body(params);
        let grid = body.collision_grid(grid_cell);
        let MhBody {
            profile,
            joints,
            mesh,
            body_vertex_count,
            report,
        } = body;
        let lv = profile.levels;
        let cz = profile.torso.eval(lv.waist).center_z;
        let spine = if model.has_joint("joint-spine-3") {
            model.joint(&mesh.positions, "joint-spine-3")
        } else {
            DVec3::new(0.0, lv.waist, cz)
        };
        let w = &model.weights;
        let pw = &model.pose_weights;
        let rig = Rig::mesh(
            &RigJoints::new(&joints, spine, DVec3::new(0.0, 0.0, cz)),
            mesh.clone(),
            body_vertex_count,
            [&w.arm_left, &w.arm_right, &w.leg_left, &w.leg_right],
            [
                &pw.lower_arm_left,
                &pw.lower_arm_right,
                &pw.lower_leg_left,
                &pw.lower_leg_right,
            ],
            &pw.spine,
            grid_cell,
        );
        Self {
            kind: AvatarKind::MakeHuman,
            profile,
            joints,
            grid,
            mesh,
            makehuman: Some(report),
            velocity: None,
            body_vertex_count,
            rig: Some(Arc::new(rig)),
            pose: AvatarPose::REST,
        }
    }

    /// Body height.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.profile.height
    }

    /// Skin vertices for cloth-surface collision. Only morph-target avatars provide them; the
    /// parametric mannequin relies on the distance grid alone.
    #[must_use]
    pub fn body_collision_mesh(&self) -> Option<BodyCollisionMesh<'_>> {
        (self.kind == AvatarKind::MakeHuman).then_some(BodyCollisionMesh {
            positions: &self.mesh.positions,
            normals: &self.mesh.normals,
            body_vertex_count: self.body_vertex_count,
        })
    }
}

/// The profile pattern drafting uses for `params`: measured from the morph-target body when a
/// model is loaded (no collision grid is built), otherwise the parametric torso.
#[must_use]
pub fn drafting_profile(params: &AvatarParams, model: Option<&MhModel>) -> TorsoProfile {
    match model {
        Some(m) => m.build_body(params).profile,
        None => TorsoProfile::parametric(params),
    }
}
