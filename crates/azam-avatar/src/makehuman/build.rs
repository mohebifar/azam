//! Building the morph-target avatar: fit, compose, scale, ground, pose, then measure the
//! landmarks, torso sections and collision grid.

use std::f64::consts::PI;

use glam::DVec3;

use crate::grid::SdfGrid;
use crate::joints::{Joints, LimbJoints};
use crate::math::{ellipse_axes, round_half_up};
use crate::measurements::{LOWER_ARM_LENGTH, MEASUREMENTS, MeasureKind, Measurements, UPPER_ARM_LENGTH};
use crate::mesh_sdf::{mesh_sdf, vertex_normals};
use crate::params::AvatarParams;
use crate::profile::{BodyLevels, TorsoProfile, TorsoTable};
use crate::surface::AvatarMesh;

use super::fit::MhFit;
use super::measure::{Positions, normalize, tape_measure};
use super::parse::MhModel;
use super::pose::PoseInfo;

/// Default cell size of the collision grid, in millimetres.
pub const DEFAULT_MH_GRID_CELL_MM: f64 = 7.5;
/// Padding of the collision grid around the body, in millimetres.
const GRID_PAD_MM: f64 = 90.0;
/// Height step of the measured torso table, in millimetres.
const TORSO_STEP_MM: f64 = 4.0;
/// The torso table starts this far below the crotch, in millimetres.
const TORSO_BELOW_CROTCH_MM: f64 = 40.0;
/// The torso table ends this far above the neck, in millimetres.
const TORSO_ABOVE_NECK_MM: f64 = 10.0;
/// Vertices this far outside the table still count for its end rows, in millimetres.
const TORSO_ROW_SLACK_MM: f64 = 6.0;
/// Skin weights at or above these belong to an arm / a leg rather than the torso.
const ARM_WEIGHT_LIMIT: u8 = 70;
const LEG_WEIGHT_LIMIT: u8 = 90;
/// Arm weight above which a vertex is ignored for the shoulder line.
const SHOULDER_ARM_WEIGHT_LIMIT: u8 = 150;
/// Shoulder samples are taken up to this far above the neck…
const SHOULDER_CAP_ABOVE_NECK_MM: f64 = 25.0;
/// …and down to this far below the shoulder point, in millimetres.
const SHOULDER_SAMPLE_DEPTH_MM: f64 = 120.0;
/// The chin is this far below the head joint, in millimetres.
const CHIN_BELOW_HEAD_JOINT_MM: f64 = 40.0;
/// Re-ground the posed body when its lowest point moved more than this, in millimetres.
const REGROUND_TOLERANCE_MM: f64 = 0.01;
/// X of the height tape, in millimetres.
const HEIGHT_TAPE_X_MM: f64 = -260.0;

/// A measuring tape drawn on the avatar.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasureTape {
    /// Measurement key.
    pub key: &'static str,
    /// Polyline points.
    pub points: Vec<DVec3>,
    /// Whether the polyline is a closed loop.
    pub closed: bool,
}

/// How the morph-target body was fitted, for display.
#[derive(Clone, Debug, PartialEq)]
pub struct MhReport {
    /// Measurements of the fitted body (before posing).
    pub measured: Measurements,
    /// The fit result.
    pub fit: MhFit,
    /// Natural limb angles before posing.
    pub pose: PoseInfo,
    /// Measuring tapes for display.
    pub tapes: Vec<MeasureTape>,
}

/// A built morph-target body.
#[derive(Clone, Debug, PartialEq)]
pub struct MhBody {
    /// Drafting and arrangement profile measured from the body.
    pub profile: TorsoProfile,
    /// Limb joints.
    pub joints: Joints,
    /// Posed body and eye mesh with vertex normals; body vertices come first.
    pub mesh: AvatarMesh,
    /// Number of body vertices at the start of the mesh.
    pub body_vertex_count: usize,
    /// Fit details.
    pub report: MhReport,
}

impl MhBody {
    /// Collision grid of the posed mesh with cell size `cell` (see [`DEFAULT_MH_GRID_CELL_MM`]).
    #[must_use]
    pub fn collision_grid(&self, cell: f64) -> SdfGrid {
        mesh_sdf(
            &self.mesh.positions,
            &self.mesh.indices,
            cell,
            GRID_PAD_MM,
            &self.mesh.normals,
        )
    }
}

impl MhModel {
    /// Builds the body for `params`: fits the morph targets, scales to the wanted height, grounds
    /// and poses the body, then measures landmarks, joints and torso sections. The collision grid
    /// is computed separately with [`MhBody::collision_grid`].
    #[must_use]
    pub fn build_body(&self, params: &AvatarParams) -> MhBody {
        let fit = self.fit(params);
        let mut pos = self.compose(&fit.shape);
        for v in &mut pos {
            *v = (f64::from(*v) * fit.scale) as f32;
        }
        let ground = |pos: &mut [f32], y0: f64| {
            for v in pos.chunks_exact_mut(3) {
                v[1] = (f64::from(v[1]) - y0) as f32;
            }
        };
        let range = self.height_range(Positions::Full(&pos));
        ground(&mut pos, range[0]);
        let measured = self.measure_all_in(Positions::Full(&pos));
        let pose = self.pose(&mut pos, Some(params.arm_angle), Some(params.leg_spread));
        let range = self.height_range(Positions::Full(&pos));
        if range[0].abs() > REGROUND_TOLERANCE_MM {
            ground(&mut pos, range[0]);
        }
        let height = measured.0[0];
        let mut tris = self.body_triangles.clone();
        tris.extend_from_slice(&self.eye_triangles);
        let normals = vertex_normals(&pos, &tris, self.vertex_count);
        let levels = self.levels(&pos);
        let get = |key: &str| measured.get(key).unwrap_or_default();
        let joints = self.limb_joints(&pos, &measured);
        let torso = self.torso_table(&pos, &levels);
        let shoulder_cap = levels.neck + SHOULDER_CAP_ABOVE_NECK_MM;
        let w = &self.weights;
        let shoulder_samples = (0..self.body_vertex_count)
            .filter(|&v| {
                w.arm_left[v] <= SHOULDER_ARM_WEIGHT_LIMIT && w.arm_right[v] <= SHOULDER_ARM_WEIGHT_LIMIT
            })
            .map(|v| (f64::from(pos[v * 3]).abs(), f64::from(pos[v * 3 + 1])))
            .filter(|&(_, y)| y <= shoulder_cap && y >= levels.shoulder - SHOULDER_SAMPLE_DEPTH_MM)
            .collect();
        let shoulder = get("shoulder");
        let half_shoulder = (if shoulder == 0.0 {
            params.shoulder
        } else {
            shoulder
        }) / 2.0;
        let profile = TorsoProfile::measured(
            params.clone(),
            height,
            torso,
            ellipse_axes(get("hip"), 0.76),
            get("neck") / (2.0 * PI),
            half_shoulder,
            levels,
            shoulder_samples,
        );
        let tapes = self.tapes(&pos, height);
        MhBody {
            profile,
            joints,
            mesh: AvatarMesh {
                positions: pos,
                normals,
                indices: tris,
            },
            body_vertex_count: self.body_vertex_count,
            report: MhReport {
                measured,
                fit,
                pose,
                tapes,
            },
        }
    }

    /// Landmark heights of the posed body.
    fn levels(&self, pos: &[f32]) -> BodyLevels {
        let full = Positions::Full(pos);
        let loop_y = |name: &str| {
            let l = self.measure_vertices(name);
            l.iter().fold(0.0, |s, &v| s + f64::from(pos[v as usize * 3 + 1])) / l.len() as f64
        };
        let (waist, hip) = (loop_y("measure-waist-circ"), loop_y("measure-hips-circ"));
        BodyLevels {
            shoulder: self.vertex(full, self.landmark("shoulderL")).y,
            neck: loop_y("measure-neck-circ"),
            chin: self.joint(pos, "joint-head").y - CHIN_BELOW_HEAD_JOINT_MM,
            bust: loop_y("measure-bust-circ"),
            underbust: loop_y("measure-underbust-circ"),
            waist,
            high_hip: f64::midpoint(waist, hip),
            hip,
            crotch: self.vertex(full, self.landmark("crotch")).y,
            knee: loop_y("measure-knee-circ"),
        }
    }

    /// Limb joints of the posed body; limb radii come from the measured girths.
    fn limb_joints(&self, pos: &[f32], measured: &Measurements) -> Joints {
        let get = |key: &str| measured.get(key).unwrap_or_default();
        let limb = |s: &str| {
            let j = |n: &str| self.joint(pos, &format!("joint-{s}-{n}"));
            let (shoulder, elbow, wrist) = (j("shoulder"), j("elbow"), j("hand"));
            let (hip, knee, ankle) = (j("upper-leg"), j("knee"), j("ankle"));
            LimbJoints {
                shoulder,
                elbow,
                wrist,
                arm_axis: normalize(wrist - shoulder),
                upper_arm_radius: get("upperArm") / (2.0 * PI),
                hip,
                knee,
                ankle,
                leg_axis: normalize(ankle - hip),
                thigh_radius: get("thigh") / (2.0 * PI),
            }
        };
        Joints {
            left: limb("l"),
            right: limb("r"),
        }
    }

    /// Torso sections from the torso vertices (arm and leg vertices excluded by skin weight),
    /// smoothed with a 1-2-1 filter.
    fn torso_table(&self, pos: &[f32], levels: &BodyLevels) -> TorsoTable {
        let w = &self.weights;
        let is_torso = |v: usize| {
            w.arm_left[v] < ARM_WEIGHT_LIMIT
                && w.arm_right[v] < ARM_WEIGHT_LIMIT
                && w.leg_left[v] < LEG_WEIGHT_LIMIT
                && w.leg_right[v] < LEG_WEIGHT_LIMIT
        };
        let y0 = levels.crotch - TORSO_BELOW_CROTCH_MM;
        let y1 = levels.neck + TORSO_ABOVE_NECK_MM;
        let step = TORSO_STEP_MM;
        let n = ((y1 - y0) / step).ceil().max(0.0) as usize + 1;
        let mut half_width = vec![0.0_f32; n];
        let mut zmin = vec![1e9_f32; n];
        let mut zmax = vec![-1e9_f32; n];
        for v in (0..self.body_vertex_count).filter(|&v| is_torso(v)) {
            let y = f64::from(pos[v * 3 + 1]);
            if y < y0 - TORSO_ROW_SLACK_MM || y > y1 + TORSO_ROW_SLACK_MM {
                continue;
            }
            let bi = round_half_up((y - y0) / step);
            if bi < 0.0 || bi >= n as f64 {
                continue;
            }
            let bi = bi as usize;
            let (x, z) = (pos[v * 3].abs(), pos[v * 3 + 2]);
            if x > half_width[bi] {
                half_width[bi] = x;
            }
            if z < zmin[bi] {
                zmin[bi] = z;
            }
            if z > zmax[bi] {
                zmax[bi] = z;
            }
        }
        let mut half_depth = vec![0.0_f32; n];
        let mut center_z = vec![0.0_f32; n];
        for i in 0..n {
            if zmax[i] < zmin[i] {
                let j = i.saturating_sub(1);
                half_width[i] = if half_width[j] == 0.0 {
                    100.0
                } else {
                    half_width[j]
                };
                zmin[i] = if zmin[j] < 1e8 { zmin[j] } else { -80.0 };
                zmax[i] = if zmax[j] > -1e8 { zmax[j] } else { 80.0 };
            }
            let (lo, hi) = (f64::from(zmin[i]), f64::from(zmax[i]));
            half_depth[i] = ((hi - lo) / 2.0) as f32;
            center_z[i] = f64::midpoint(hi, lo) as f32;
        }
        let smooth = |a: &[f32]| {
            let mut o = a.to_vec();
            for i in 1..a.len().saturating_sub(1) {
                o[i] = ((f64::from(a[i - 1]) + 2.0 * f64::from(a[i]) + f64::from(a[i + 1])) / 4.0) as f32;
            }
            o
        };
        TorsoTable {
            y0,
            y1,
            step,
            half_width: smooth(&half_width),
            half_depth: smooth(&half_depth),
            center_z: smooth(&center_z),
        }
    }

    /// Measuring tapes for display, one per measurement.
    fn tapes(&self, pos: &[f32], height: f64) -> Vec<MeasureTape> {
        let full = Positions::Full(pos);
        let p = |g: u32| self.vertex(full, g);
        let pair = |name: &str| {
            let l = self.measure_vertices(name);
            [p(l[0]), p(l[1])]
        };
        MEASUREMENTS
            .iter()
            .map(|d| {
                let (points, closed) = match d.kind {
                    MeasureKind::Circumference(name) => {
                        (tape_measure(&self.loop_points(full, name)).hull, true)
                    }
                    MeasureKind::Distance(name) => (pair(name).to_vec(), false),
                    MeasureKind::Shoulder => (
                        vec![p(self.landmark("shoulderL")), p(self.landmark("shoulderR"))],
                        false,
                    ),
                    MeasureKind::Arm => {
                        let ([a0, a1], [b0, b1]) = (pair(UPPER_ARM_LENGTH), pair(LOWER_ARM_LENGTH));
                        (vec![a0, a1, b0, b1], false)
                    }
                    MeasureKind::Crotch => {
                        let c = p(self.landmark("crotch"));
                        (vec![DVec3::new(c.x, 0.0, c.z), c], false)
                    }
                    MeasureKind::Height => (
                        vec![
                            DVec3::new(HEIGHT_TAPE_X_MM, 0.0, 0.0),
                            DVec3::new(HEIGHT_TAPE_X_MM, height, 0.0),
                        ],
                        false,
                    ),
                };
                MeasureTape {
                    key: d.key,
                    points,
                    closed,
                }
            })
            .collect()
    }
}
