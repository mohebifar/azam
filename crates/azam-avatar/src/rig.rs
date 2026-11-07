//! Re-posing a built avatar: rigid segment transforms from an [`AvatarPose`], linear-blend
//! skinning of the display mesh and a fresh collision grid.
//!
//! The body has ten segments: the root (pelvis, legs' parent, turned by the turntable), the upper
//! body above the lower back, and for each side the upper arm, forearm, thigh and shin. The
//! morph-target body skins its mesh with the pack's limb, forearm, shin and spine weights; the
//! parametric mannequin moves its limb primitives rigidly with their segments (its torso turns
//! with the root and bends at the lower back) and skins its display mesh by nearness to each part.

use std::sync::{Arc, OnceLock};

use glam::{DMat3, DVec3};

use crate::avatar::Avatar;
use crate::grid::{GridFrame, SdfGrid};
use crate::joints::{Joints, LimbJoints};
use crate::mesh_sdf::vertex_normals;
use crate::motion::{AvatarPose, Motion, pose_at};
use crate::parametric::{ParametricBody, fill_grid};
use crate::posed_sdf::mesh_sdf_in;
use crate::surface::AvatarMesh;
use crate::velocity::BodyVelocity;

/// Number of rigid segments.
pub(crate) const SEGMENTS: usize = 10;
const ROOT: usize = 0;
const SPINE: usize = 1;
/// Upper arm, forearm, thigh and shin of the left side; each right-side twin is at +2.
const UPPER_ARM: usize = 2;
const FOREARM: usize = 3;
const THIGH: usize = 6;
const SHIN: usize = 7;
/// Offset from a left-side segment to the same side's segment (`[left, right]`).
const SIDE_OFFSET: [usize; 2] = [0, 2];

/// Padding of a posed collision grid around the body, in millimetres.
const GRID_PAD_MM: f64 = 90.0;
/// The grid always reaches at least this far below the floor, in millimetres.
const FLOOR_MARGIN_MM: f64 = 10.0;
/// The upper body's lean fades in over this distance either side of the lower-back pivot, mm.
const LEAN_BAND_MM: f64 = 60.0;
/// Parametric display vertices blend the parts within this distance of the nearest one, mm.
const PART_BLEND_MM: f64 = 25.0;
/// Poses sampled per cycle when finding the grid frame of a motion.
const FRAME_SAMPLES: usize = 32;
/// The velocity grid's cells are this many collision grid cells.
const VELOCITY_CELLS: f64 = 3.0;

/// A rigid transform `p ↦ m p + t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rigid {
    m: DMat3,
    t: DVec3,
}

impl Rigid {
    const IDENTITY: Self = Self {
        m: DMat3::IDENTITY,
        t: DVec3::ZERO,
    };

    /// Rotation by `deg` degrees about the line through `pivot` along `axis`.
    fn rotation(pivot: DVec3, axis: DVec3, deg: f64) -> Self {
        let m = DMat3::from_axis_angle(axis.normalize(), deg.to_radians());
        Self {
            m,
            t: pivot - m * pivot,
        }
    }

    /// `self` after `inner`.
    fn then(self, inner: Self) -> Self {
        Self {
            m: self.m * inner.m,
            t: self.m * inner.t + self.t,
        }
    }

    fn apply(self, p: DVec3) -> DVec3 {
        self.m * p + self.t
    }

    fn inverse(self) -> Self {
        let m = self.m.transpose();
        Self { m, t: -(m * self.t) }
    }
}

/// Rest-pose joints the segments rotate about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RigJoints {
    /// Shoulder, elbow, wrist, hip, knee and ankle, `[left, right]`.
    pub limbs: [LimbJoints; 2],
    /// Lower-back pivot of the upper body.
    pub spine: DVec3,
    /// A point on the vertical turn axis.
    pub axis: DVec3,
}

impl RigJoints {
    pub(crate) fn new(joints: &Joints, spine: DVec3, axis: DVec3) -> Self {
        Self {
            limbs: [joints.left, joints.right],
            spine,
            axis,
        }
    }
}

/// The transform of every segment for `pose`.
fn transforms(j: &RigJoints, pose: &AvatarPose) -> [Rigid; SEGMENTS] {
    let mut out = [Rigid::IDENTITY; SEGMENTS];
    let root = Rigid::rotation(j.axis, DVec3::Y, pose.turn);
    let spine = root.then(Rigid::rotation(j.spine, DVec3::X, pose.lean));
    out[ROOT] = root;
    out[SPINE] = spine;
    for (s, limb) in j.limbs.iter().enumerate() {
        let side = if s == 0 { 1.0 } else { -1.0 };
        let off = SIDE_OFFSET[s];
        let shoulder = Rigid::rotation(limb.shoulder, DVec3::X, -pose.arm_swing[s]).then(Rigid::rotation(
            limb.shoulder,
            DVec3::Z,
            side * pose.arm_raise[s],
        ));
        let upper = spine.then(shoulder);
        let d = (limb.elbow - limb.shoulder).normalize_or_zero();
        let flex_axis = d.cross(DVec3::Z).try_normalize().unwrap_or(DVec3::NEG_X);
        let fore = upper.then(Rigid::rotation(limb.elbow, flex_axis, pose.elbow[s]));
        let thigh = root.then(Rigid::rotation(limb.hip, DVec3::X, -pose.hip[s]));
        let shin = thigh.then(Rigid::rotation(limb.knee, DVec3::X, pose.knee[s]));
        out[UPPER_ARM + off] = upper;
        out[FOREARM + off] = fore;
        out[THIGH + off] = thigh;
        out[SHIN + off] = shin;
    }
    out
}

/// How the body's collision surface is rebuilt.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RigKind {
    /// From the skinned mesh.
    Mesh,
    /// From the parametric primitives, moved rigidly; `segments` holds each primitive's segment.
    Parametric {
        /// The body as built.
        body: Box<ParametricBody>,
        /// Segment of each primitive.
        segments: Vec<usize>,
    },
}

/// What re-posing needs from the avatar as built.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Rig {
    pub joints: RigJoints,
    /// Display mesh of the rest pose.
    pub rest: AvatarMesh,
    /// Skin vertices at the start of the mesh (all vertices for the parametric body).
    pub body_vertex_count: usize,
    /// Segment weights, [`SEGMENTS`] per vertex (computed on first use for the parametric body).
    pub weights: OnceLock<Vec<f32>>,
    pub kind: RigKind,
    /// Collision grid cell size.
    pub cell: f64,
}

impl Rig {
    /// Rig of a morph-target body: per-vertex segment weights from the limb, forearm, shin and
    /// spine skin weights (`0..=255`; empty optional arrays count as zero).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mesh(
        joints: &RigJoints,
        rest: AvatarMesh,
        body_vertex_count: usize,
        limb: [&[u8]; 4],
        lower: [&[u8]; 4],
        spine: &[u8],
        cell: f64,
    ) -> Self {
        let n = rest.vertex_count();
        let w = |a: &[u8], v: usize| a.get(v).map_or(0.0, |&x| f32::from(x) / 255.0);
        let mut weights = vec![0.0_f32; n * SEGMENTS];
        for v in 0..n {
            let row = &mut weights[v * SEGMENTS..(v + 1) * SEGMENTS];
            let [arm_l, arm_r, leg_l, leg_r] = limb.map(|a| w(a, v));
            let [fa_l, fa_r, sh_l, sh_r] = lower.map(|a| w(a, v));
            row[FOREARM] = fa_l.min(arm_l);
            row[UPPER_ARM] = arm_l - row[FOREARM];
            row[FOREARM + 2] = fa_r.min(arm_r);
            row[UPPER_ARM + 2] = arm_r - row[FOREARM + 2];
            row[SHIN] = sh_l.min(leg_l);
            row[THIGH] = leg_l - row[SHIN];
            row[SHIN + 2] = sh_r.min(leg_r);
            row[THIGH + 2] = leg_r - row[SHIN + 2];
            row[SPINE] = (w(spine, v) - arm_l - arm_r).max(0.0);
            let used: f32 = row.iter().sum();
            if used > 1.0 {
                for x in row.iter_mut() {
                    *x /= used;
                }
            } else {
                row[ROOT] = 1.0 - used;
            }
        }
        Self {
            joints: *joints,
            rest,
            body_vertex_count,
            weights: OnceLock::from(weights),
            kind: RigKind::Mesh,
            cell,
        }
    }

    /// Rig of the parametric mannequin (limbs, hands and feet in build order, then neck and head).
    pub(crate) fn parametric(body: &ParametricBody, rest: AvatarMesh, cell: f64) -> Self {
        let n_prims = body.primitives.len();
        let segments = if n_prims == 14 {
            let side = |off: usize| [UPPER_ARM, FOREARM, FOREARM, THIGH, SHIN, SHIN].map(|s| s + off);
            let mut s = side(0).to_vec();
            s.extend(side(2));
            s.extend([SPINE, SPINE]);
            s
        } else {
            vec![ROOT; n_prims]
        };
        let lv = body.profile.levels;
        let cz = body.profile.torso.eval(lv.waist).center_z;
        let joints = RigJoints::new(
            &body.joints,
            DVec3::new(0.0, lv.waist, cz),
            DVec3::new(0.0, 0.0, cz),
        );
        Self {
            joints,
            body_vertex_count: rest.vertex_count(),
            rest,
            weights: OnceLock::new(),
            kind: RigKind::Parametric {
                body: Box::new(body.clone()),
                segments,
            },
            cell,
        }
    }

    /// Weight of the upper body for a torso point at height `y` (fades in around the pivot).
    fn spine_share(&self, y: f64) -> f64 {
        let x = ((y - self.joints.spine.y + LEAN_BAND_MM) / (2.0 * LEAN_BAND_MM)).clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    }

    fn weights(&self) -> &[f32] {
        self.weights.get_or_init(|| {
            let RigKind::Parametric { body, segments } = &self.kind else {
                return Vec::new();
            };
            let n = self.rest.vertex_count();
            let mut out = vec![0.0_f32; n * SEGMENTS];
            for v in 0..n {
                let p = DVec3::new(
                    f64::from(self.rest.positions[v * 3]),
                    f64::from(self.rest.positions[v * 3 + 1]),
                    f64::from(self.rest.positions[v * 3 + 2]),
                );
                let torso = body.torso_distance(p);
                let dists: Vec<f64> = body.primitives.iter().map(|q| q.distance(p)).collect();
                let best = dists.iter().copied().fold(torso, f64::min);
                let share = |d: f64| (1.0 - (d - best) / PART_BLEND_MM).max(0.0);
                let row = &mut out[v * SEGMENTS..(v + 1) * SEGMENTS];
                let st = share(torso);
                let up = self.spine_share(p.y);
                row[SPINE] += (st * up) as f32;
                row[ROOT] += (st * (1.0 - up)) as f32;
                for (d, &s) in dists.iter().zip(segments) {
                    row[s] += share(*d) as f32;
                }
                let sum: f32 = row.iter().sum();
                for x in row.iter_mut() {
                    *x /= sum.max(1e-6);
                }
            }
            out
        })
    }

    /// Skinned rest positions for `pose`, grounded, and the segment transforms including the
    /// grounding shift.
    fn pose_positions(&self, pose: &AvatarPose) -> (Vec<f32>, [Rigid; SEGMENTS]) {
        let mut tr = transforms(&self.joints, pose);
        let weights = self.weights();
        let n = self.rest.vertex_count();
        let mut out = vec![0.0_f32; n * 3];
        let mut lowest = f64::INFINITY;
        for v in 0..n {
            let p = DVec3::new(
                f64::from(self.rest.positions[v * 3]),
                f64::from(self.rest.positions[v * 3 + 1]),
                f64::from(self.rest.positions[v * 3 + 2]),
            );
            let row = &weights[v * SEGMENTS..(v + 1) * SEGMENTS];
            let mut q = DVec3::ZERO;
            for (s, &w) in row.iter().enumerate() {
                if w > 0.0 {
                    q += tr[s].apply(p) * f64::from(w);
                }
            }
            if v < self.body_vertex_count {
                lowest = lowest.min(q.y);
            }
            out[v * 3..v * 3 + 3].copy_from_slice(&q.to_array().map(|c| c as f32));
        }
        let rest_lowest = (0..self.body_vertex_count)
            .map(|v| f64::from(self.rest.positions[v * 3 + 1]))
            .fold(f64::INFINITY, f64::min);
        let shift = if lowest.is_finite() && rest_lowest.is_finite() {
            rest_lowest - lowest
        } else {
            0.0
        };
        if shift != 0.0 {
            for v in 0..n {
                out[v * 3 + 1] = (f64::from(out[v * 3 + 1]) + shift) as f32;
            }
            let lift = Rigid {
                m: DMat3::IDENTITY,
                t: DVec3::new(0.0, shift, 0.0),
            };
            for t in &mut tr {
                *t = lift.then(*t);
            }
        }
        (out, tr)
    }

    /// The collision grid of the body skinned to `positions` with segment transforms `tr`.
    fn grid(
        &self,
        pose: &AvatarPose,
        positions: &[f32],
        normals: &[f32],
        tr: &[Rigid; SEGMENTS],
        frame: &GridFrame,
    ) -> SdfGrid {
        match &self.kind {
            RigKind::Mesh => mesh_sdf_in(positions, &self.rest.indices, normals, frame),
            RigKind::Parametric { body, segments } => {
                let inv: Vec<Rigid> = tr.iter().map(|t| t.inverse()).collect();
                let pivot = self.joints.spine;
                let lean = pose.lean;
                let torso_at = |p: DVec3| {
                    let q = inv[ROOT].apply(p);
                    if lean == 0.0 {
                        return q;
                    }
                    let a = -lean * self.spine_share(q.y);
                    Rigid::rotation(pivot, DVec3::X, a).apply(q)
                };
                let distance = |p: DVec3| {
                    body.primitives
                        .iter()
                        .zip(segments)
                        .fold(body.torso_distance(torso_at(p)), |d, (prim, &s)| {
                            prim.blend_into(d, inv[s].apply(p))
                        })
                };
                SdfGrid {
                    origin: frame.origin,
                    cell: frame.cell,
                    dims: frame.dims,
                    data: fill_grid(frame, distance),
                }
            }
        }
    }

    /// The grid frame around `positions` (body vertices) plus padding.
    fn frame_around(&self, boxes: impl IntoIterator<Item = (DVec3, DVec3)>) -> GridFrame {
        let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        for (a, b) in boxes {
            lo = lo.min(a);
            hi = hi.max(b);
        }
        let pad = DVec3::splat(GRID_PAD_MM);
        let mut lo = lo - pad;
        lo.y = lo.y.min(-FLOOR_MARGIN_MM);
        GridFrame::covering(lo, hi + pad, self.cell)
    }

    fn bounds(&self, positions: &[f32]) -> (DVec3, DVec3) {
        let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        for p in positions[..self.body_vertex_count * 3].chunks_exact(3) {
            let p = DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
            lo = lo.min(p);
            hi = hi.max(p);
        }
        (lo, hi)
    }
}

/// The velocity grid's layout for a collision grid laid out as `frame`.
fn velocity_frame(frame: &GridFrame) -> GridFrame {
    GridFrame::covering(frame.origin, frame.max(), frame.cell * VELOCITY_CELLS)
}

/// A limb's joints moved by its segments.
fn posed_limb(l: &LimbJoints, tr: &[Rigid; SEGMENTS], off: usize) -> LimbJoints {
    let (upper, fore) = (tr[UPPER_ARM + off], tr[FOREARM + off]);
    let (thigh, shin) = (tr[THIGH + off], tr[SHIN + off]);
    let shoulder = upper.apply(l.shoulder);
    let elbow = upper.apply(l.elbow);
    let wrist = fore.apply(l.wrist);
    let hip = thigh.apply(l.hip);
    let knee = thigh.apply(l.knee);
    let ankle = shin.apply(l.ankle);
    LimbJoints {
        shoulder,
        elbow,
        wrist,
        arm_axis: (wrist - shoulder).normalize_or_zero(),
        hip,
        knee,
        ankle,
        leg_axis: (ankle - hip).normalize_or_zero(),
        ..*l
    }
}

impl Avatar {
    /// The pose this avatar is in ([`AvatarPose::REST`] as built).
    #[must_use]
    pub fn pose(&self) -> &AvatarPose {
        &self.pose
    }

    /// Whether the avatar can be re-posed ([`Avatar::posed`]).
    #[must_use]
    pub fn can_pose(&self) -> bool {
        self.rig.is_some()
    }

    /// The avatar in `pose` (relative to the rest pose it was built in), with a collision grid
    /// fitted around the posed body. The drafting profile is the rest pose's.
    #[must_use]
    pub fn posed(&self, pose: &AvatarPose) -> Self {
        let Some(rig) = &self.rig else {
            return self.clone();
        };
        let (positions, _) = rig.pose_positions(pose);
        let frame = rig.frame_around([rig.bounds(&positions)]);
        self.posed_in(pose, &frame)
    }

    /// The avatar in `pose` with its collision grid laid out as `frame` (use
    /// [`Avatar::motion_frame`] so every pose of a motion shares one layout).
    #[must_use]
    pub fn posed_in(&self, pose: &AvatarPose, frame: &GridFrame) -> Self {
        self.posed_impl(pose, frame, None)
    }

    /// [`Avatar::posed_in`] plus the body surface velocity of moving there from `previous` (the
    /// same avatar in an earlier pose) over `dt` seconds, as [`Avatar::with_velocity_from`] gives
    /// it; the velocity is computed alongside the collision grid.
    #[must_use]
    pub fn posed_after(&self, pose: &AvatarPose, frame: &GridFrame, previous: &Self, dt: f64) -> Self {
        self.posed_impl(pose, frame, Some((previous, dt)))
    }

    fn posed_impl(&self, pose: &AvatarPose, frame: &GridFrame, previous: Option<(&Self, f64)>) -> Self {
        let Some(rig) = self.rig.clone() else {
            return self.clone();
        };
        let (positions, tr) = rig.pose_positions(pose);
        let indices = &rig.rest.indices;
        let normals = vertex_normals(&positions, indices, rig.rest.vertex_count());
        let vframe = velocity_frame(frame);
        let (grid, velocity) = rayon::join(
            || rig.grid(pose, &positions, &normals, &tr, frame),
            || previous.and_then(|(p, dt)| self.velocity_to(p, &positions, dt, vframe)),
        );
        let [left, right] = rig.joints.limbs;
        Self {
            kind: self.kind,
            profile: self.profile.clone(),
            joints: Joints {
                left: posed_limb(&left, &tr, 0),
                right: posed_limb(&right, &tr, 2),
            },
            grid,
            mesh: AvatarMesh {
                positions,
                normals,
                indices: indices.clone(),
            },
            makehuman: self.makehuman.clone(),
            body_vertex_count: self.body_vertex_count,
            rig: Some(rig),
            pose: *pose,
            velocity,
        }
    }

    /// One grid layout that holds the body in every pose of `motion` (sampled over a cycle).
    #[must_use]
    pub fn motion_frame(&self, motion: Motion) -> GridFrame {
        let Some(rig) = &self.rig else {
            return self.grid.frame();
        };
        let period = motion.period();
        let start = crate::motion::MOTION_RAMP_S;
        let mut boxes = vec![rig.bounds(&rig.pose_positions(&AvatarPose::REST).0)];
        if period > 0.0 {
            for i in 0..=FRAME_SAMPLES {
                let t = start + period * i as f64 / FRAME_SAMPLES as f64;
                boxes.push(rig.bounds(&rig.pose_positions(&pose_at(motion, t)).0));
            }
        }
        rig.frame_around(boxes)
    }

    /// This avatar with the body surface velocity of moving here from `previous` (the same avatar
    /// in an earlier pose) over `dt` seconds, for moving-collider friction. Without a common rig
    /// the velocity is left unset.
    #[must_use]
    pub fn with_velocity_from(mut self, previous: &Self, dt: f64) -> Self {
        let frame = velocity_frame(&self.grid.frame());
        self.velocity = self.velocity_to(previous, &self.mesh.positions, dt, frame);
        self
    }

    /// Surface velocity of moving to the skinned `positions` from `previous` over `dt` seconds,
    /// or `None` without a common rig.
    fn velocity_to(
        &self,
        previous: &Self,
        positions: &[f32],
        dt: f64,
        frame: GridFrame,
    ) -> Option<BodyVelocity> {
        let same_rig = match (&self.rig, &previous.rig) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        let n = positions.len();
        if !same_rig || previous.mesh.positions.len() != n || dt.is_nan() || dt <= 0.0 {
            return None;
        }
        let count = self.body_vertex_count.min(n / 3);
        let before = &previous.mesh.positions;
        let points = (0..count).map(|v| {
            let p = |b: &[f32]| {
                DVec3::new(
                    f64::from(b[v * 3]),
                    f64::from(b[v * 3 + 1]),
                    f64::from(b[v * 3 + 2]),
                )
            };
            let (a, b) = (p(positions), p(before));
            (a, (a - b) / dt)
        });
        Some(BodyVelocity::from_points(frame, points))
    }
}
