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

/// Room the collision grid of a custom body leaves around it, in millimetres.
const CUSTOM_GRID_PAD_MM: f64 = 90.0;

/// Which body model an avatar was built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvatarKind {
    /// The built-in parametric mannequin (no assets needed).
    Parametric,
    /// The morph-target body fitted to measurements.
    MakeHuman,
    /// A body mesh brought in from a file (it does not move with motions).
    Custom,
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

    /// A body from a mesh (`positions` `x, y, z` per vertex, y up; `indices` three per triangle),
    /// for an avatar brought in from another program. The mesh's units are guessed from its
    /// height (m, cm or mm), it is scaled to the height in `params`, centred on the vertical axis
    /// and stood on the floor. Torso sections, landmarks and joints (used to arrange pieces) come
    /// from the measurements in `params`, so they should describe the same body. Returns `None`
    /// for a mesh without triangles or height.
    #[must_use]
    pub fn custom(positions: &[f32], indices: &[u32], params: &AvatarParams, grid_cell: f64) -> Option<Self> {
        let n = positions.len() / 3;
        if n < 3 || indices.len() < 3 || indices.iter().any(|&i| i as usize >= n) {
            return None;
        }
        let axis = |k: usize| positions.iter().skip(k).step_by(3).map(|&v| f64::from(v));
        let (lo_y, hi_y) = axis(1).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(v), b.max(v))
        });
        let mid = |k: usize| {
            let (lo, hi) = axis(k).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                (a.min(v), b.max(v))
            });
            f64::midpoint(lo, hi)
        };
        let (cx, cz) = (mid(0), mid(2));
        let height = hi_y - lo_y;
        if !(height > 0.0 && height.is_finite()) {
            return None;
        }
        let base = Self::parametric(params);
        let scale = base.height() / height;
        let placed: Vec<f32> = positions
            .chunks_exact(3)
            .flat_map(|v| {
                [
                    ((f64::from(v[0]) - cx) * scale) as f32,
                    ((f64::from(v[1]) - lo_y) * scale) as f32,
                    ((f64::from(v[2]) - cz) * scale) as f32,
                ]
            })
            .collect();
        let normals = crate::mesh_sdf::vertex_normals(&placed, indices, n);
        let grid = crate::mesh_sdf::mesh_sdf(&placed, indices, grid_cell, CUSTOM_GRID_PAD_MM, &normals);
        Some(Self {
            kind: AvatarKind::Custom,
            profile: base.profile,
            joints: base.joints,
            grid,
            mesh: AvatarMesh {
                positions: placed,
                normals,
                indices: indices.to_vec(),
            },
            makehuman: None,
            velocity: None,
            body_vertex_count: n,
            rig: None,
            pose: AvatarPose::REST,
        })
    }

    /// Body height.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.profile.height
    }

    /// Skin vertices for cloth-surface collision. Morph-target and custom bodies provide them;
    /// the parametric mannequin relies on the distance grid alone.
    #[must_use]
    pub fn body_collision_mesh(&self) -> Option<BodyCollisionMesh<'_>> {
        matches!(self.kind, AvatarKind::MakeHuman | AvatarKind::Custom).then_some(BodyCollisionMesh {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mesh_in_metres_becomes_a_body_of_the_measured_height() {
        let params = AvatarParams::preset(crate::Sex::Female);
        let source = Avatar::parametric(&params);
        // the same body exported in metres, off to the side
        let metres: Vec<f32> = source
            .mesh
            .positions
            .chunks_exact(3)
            .flat_map(|v| [v[0] / 1000.0 + 2.0, v[1] / 1000.0, v[2] / 1000.0 - 1.0])
            .collect();
        let custom = Avatar::custom(&metres, &source.mesh.indices, &params, 10.0).expect("a body");
        assert_eq!(custom.kind, AvatarKind::Custom);
        let top = custom
            .mesh
            .positions
            .iter()
            .skip(1)
            .step_by(3)
            .fold(f32::MIN, |a, &v| a.max(v));
        assert!(
            (f64::from(top) - source.height()).abs() < 1.0,
            "scaled to {top} mm"
        );
        let waist = custom.profile.levels.waist;
        assert!(
            custom.grid.distance(DVec3::new(0.0, waist, 0.0)) < 0.0,
            "the waist's middle is inside the body"
        );
        assert!(
            custom.grid.distance(DVec3::new(0.0, waist, 400.0)) > 0.0,
            "and 40 cm in front is not"
        );
        assert!(custom.body_collision_mesh().is_some());
        assert!(
            Avatar::custom(&[0.0; 6], &[0, 1, 0], &params, 10.0).is_none(),
            "no height, no body"
        );
    }
}
