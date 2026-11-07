//! Parametric body: an elliptical torso blended with round-cone limbs, ellipsoid hands, feet and
//! head, sized from tailor measurements and sampled into a signed distance grid.

use std::f64::consts::PI;

use glam::DVec3;
use rayon::prelude::*;

use crate::grid::{DISTANCE_CLAMP_MM, GridFrame, SdfGrid};
use crate::joints::{Joints, LimbJoints, Side};
use crate::math::{DEG, clamp, round_half_up};
use crate::params::AvatarParams;
use crate::profile::TorsoProfile;
use crate::sdf::{BodyPrimitive, Ellipsoid, RoundCone};

/// Distance reported far above or below the torso, in millimetres.
const TORSO_FAR_MM: f64 = 1e3;
/// The torso distance is evaluated this far beyond the ends of the torso table, in millimetres.
const TORSO_END_MARGIN_MM: f64 = 40.0;
/// Grid cell size at the reference height, in millimetres.
const GRID_CELL_AT_REFERENCE_MM: f64 = 7.5;
/// Smallest grid cell size, in millimetres.
const MIN_GRID_CELL_MM: f64 = 6.0;
/// The coarse pre-pass samples every this many fine cells.
const COARSE_STRIDE: usize = 4;

/// The parametric body: torso profile, blended primitives and joints.
#[derive(Clone, Debug, PartialEq)]
pub struct ParametricBody {
    /// Torso cross-sections and landmark heights.
    pub profile: TorsoProfile,
    /// Limbs, hands, feet, neck and head, in blending order.
    pub primitives: Vec<BodyPrimitive>,
    /// Limb joints.
    pub joints: Joints,
}

impl ParametricBody {
    /// Sizes the body from `params`.
    #[must_use]
    pub fn new(params: &AvatarParams) -> Self {
        let profile = TorsoProfile::parametric(params);
        let m = &profile.params;
        let (h, s) = (profile.height, profile.scale);
        let ys = profile.levels;
        let neck_radius = profile.neck_radius;
        let beta = m.arm_angle * DEG;
        let ua_r = m.upper_arm / (2.0 * PI);
        let mut primitives = Vec::with_capacity(14);
        let mut limb = |sd: f64| {
            let shoulder = DVec3::new(
                sd * (profile.half_shoulder - ua_r * 0.55),
                ys.shoulder - ua_r * 0.62,
                -6.0,
            );
            let a = DVec3::new(sd * beta.sin(), -beta.cos(), 0.0);
            let (lu, lf) = (m.arm * 0.54, m.arm * 0.46);
            let elbow = DVec3::new(shoulder.x + a.x * lu, shoulder.y + a.y * lu, shoulder.z + 6.0);
            let wrist = DVec3::new(elbow.x + a.x * lf, elbow.y + a.y * lf, elbow.z + 4.0);
            let cone = |a, b, r1, r2| RoundCone { a, b, r1, r2 };
            primitives.push(BodyPrimitive::round_cone(
                cone(shoulder, elbow, ua_r, ua_r * 0.74),
                16.0 * s,
            ));
            primitives.push(BodyPrimitive::round_cone(
                cone(elbow, wrist, ua_r * 0.74, ua_r * 0.52),
                12.0 * s,
            ));
            let hand_len = 92.0 * s;
            let hand = Ellipsoid {
                center: DVec3::new(
                    wrist.x + a.x * hand_len * 0.55,
                    wrist.y + a.y * hand_len * 0.55,
                    wrist.z + 6.0,
                ),
                radii: DVec3::new(
                    26.0 * s + a.x.abs() * 22.0 * s,
                    38.0 * s + a.y.abs() * 20.0 * s,
                    44.0 * s,
                ),
            };
            primitives.push(BodyPrimitive::ellipsoid(hand, 14.0 * s));
            let th_r = m.thigh / (2.0 * PI);
            let gam = m.leg_spread * DEG;
            let hip = DVec3::new(sd * profile.hip_axes[0] * 0.47, ys.crotch + 44.0 * s, -8.0);
            let la = DVec3::new(sd * gam.sin(), -gam.cos(), 0.0);
            let (knee_y, ankle_y) = (h * 0.285, h * 0.048);
            let lt = (hip.y - knee_y) / gam.cos();
            let ls = (knee_y - ankle_y) / gam.cos();
            let knee = DVec3::new(hip.x + la.x * lt, knee_y, hip.z + 6.0);
            let ankle = DVec3::new(knee.x + la.x * ls, ankle_y, knee.z - 6.0);
            primitives.push(BodyPrimitive::round_cone(
                cone(hip, knee, th_r, th_r * 0.62),
                40.0 * s,
            ));
            primitives.push(BodyPrimitive::round_cone(
                cone(knee, ankle, th_r * 0.62, th_r * 0.40),
                16.0 * s,
            ));
            let foot = Ellipsoid {
                center: DVec3::new(ankle.x, 34.0 * s, ankle.z + 48.0 * s),
                radii: DVec3::new(44.0 * s, 34.0 * s, 122.0 * s),
            };
            primitives.push(BodyPrimitive::ellipsoid(foot, 18.0 * s));
            LimbJoints {
                shoulder,
                elbow,
                wrist,
                arm_axis: a,
                upper_arm_radius: ua_r,
                hip,
                knee,
                ankle,
                leg_axis: la,
                thigh_radius: th_r,
            }
        };
        let left = limb(Side::Left.sign());
        let right = limb(Side::Right.sign());
        let neck = RoundCone {
            a: DVec3::new(0.0, ys.neck - 34.0 * s, -14.0),
            b: DVec3::new(0.0, ys.chin + 8.0 * s, -4.0),
            r1: neck_radius * 1.02,
            r2: neck_radius * 0.95,
        };
        primitives.push(BodyPrimitive::round_cone(neck, 30.0 * s));
        let head = Ellipsoid {
            center: DVec3::new(0.0, ys.chin + 0.064 * h, 8.0 * s),
            radii: DVec3::new(0.058 * h, 0.074 * h, 0.068 * h),
        };
        primitives.push(BodyPrimitive::ellipsoid(head, 22.0 * s));
        Self {
            profile,
            primitives,
            joints: Joints { left, right },
        }
    }

    /// Signed distance of the torso alone (an elliptical cylinder per height, capped at the ends).
    #[must_use]
    #[allow(clippy::float_cmp)] // beyond the table ends exactly
    pub fn torso_distance(&self, p: DVec3) -> f64 {
        let t = &self.profile.torso;
        if p.y < t.y0 - TORSO_END_MARGIN_MM || p.y > t.y1 + TORSO_END_MARGIN_MM {
            return TORSO_FAR_MM;
        }
        let yc = clamp(p.y, t.y0, t.y1);
        let e = t.eval(yc);
        let (x, z) = (p.x, p.z - e.center_z);
        let sx = x / e.half_width;
        let sz = z / e.half_depth;
        let sn = (sx * sx + sz * sz).sqrt();
        let r = (x * x + z * z).sqrt();
        let mut d = if sn > 1e-6 {
            r * (1.0 - 1.0 / sn)
        } else {
            -e.half_width.min(e.half_depth)
        };
        if p.y != yc {
            let beyond = (p.y - yc).abs();
            d = d.max(beyond) + if d < 0.0 { beyond } else { 0.0 };
        }
        d
    }

    /// Exact signed distance to the body surface (negative inside).
    #[must_use]
    pub fn distance(&self, p: DVec3) -> f64 {
        self.primitives
            .iter()
            .fold(self.torso_distance(p), |d, prim| prim.blend_into(d, p))
    }

    /// Samples the body into a signed distance grid.
    ///
    /// The grid covers the reach of the arms and the full height. Distances are first sampled on
    /// a grid four times coarser; fine cells whose coarse cell is far from the surface are
    /// interpolated (the distance is 1-Lipschitz), the rest are evaluated exactly. Values are
    /// clamped to [`DISTANCE_CLAMP_MM`].
    #[must_use]
    pub fn sdf_grid(&self) -> SdfGrid {
        let p = &self.profile;
        let s = p.scale;
        let g = MIN_GRID_CELL_MM.max(round_half_up(GRID_CELL_AT_REFERENCE_MM * s));
        let reach_x = (self.joints.left.wrist.x.abs() + 140.0 * s)
            .max(p.hip_axes[0] + 120.0)
            .max(p.half_shoulder + 120.0);
        let (bx0, bx1) = (-reach_x, reach_x);
        let (by0, by1) = (-10.0, p.height + 40.0);
        let (bz0, bz1) = (-260.0 * s, 260.0 * s);
        let count = |lo: f64, hi: f64| ((hi - lo) / g).ceil() as usize + 1;
        let frame = GridFrame {
            origin: DVec3::new(bx0, by0, bz0),
            cell: g,
            dims: [count(bx0, bx1), count(by0, by1), count(bz0, bz1)],
        };
        SdfGrid {
            origin: frame.origin,
            cell: g,
            dims: frame.dims,
            data: fill_grid(&frame, |q| self.distance(q)),
        }
    }
}

/// Samples `distance` into the grid `frame`.
///
/// Distances are first sampled on a grid four times coarser; fine cells whose coarse cell is far
/// from the surface are interpolated (the distance is 1-Lipschitz), the rest are evaluated
/// exactly. Values are clamped to [`DISTANCE_CLAMP_MM`].
pub(crate) fn fill_grid(frame: &GridFrame, distance: impl Fn(DVec3) -> f64 + Sync) -> Vec<f32> {
    let (bx0, by0, bz0) = (frame.origin.x, frame.origin.y, frame.origin.z);
    let g = frame.cell;
    let [nx, ny, nz] = frame.dims;
    let cs = COARSE_STRIDE;
    let coarse_count = |n: usize| (n - 1).div_ceil(cs) + 1;
    let (cnx, cny, cnz) = (coarse_count(nx), coarse_count(ny), coarse_count(nz));
    let mut coarse = vec![0.0_f32; cnx * cny * cnz];
    coarse
        .par_chunks_mut(cnx * cny)
        .enumerate()
        .for_each(|(k, slab)| {
            let z = bz0 + (k * cs) as f64 * g;
            for j in 0..cny {
                let y = by0 + (j * cs) as f64 * g;
                for i in 0..cnx {
                    let x = bx0 + (i * cs) as f64 * g;
                    slab[i + cnx * j] = distance(DVec3::new(x, y, z)) as f32;
                }
            }
        });
    let diag = 3.0_f64.sqrt() * cs as f64 * g;
    let cell_and_weight = |n: usize, cn: usize| {
        let c = (n / cs).min(cn.saturating_sub(2));
        (c, n as f64 / cs as f64 - c as f64)
    };
    let mut data = vec![0.0_f32; nx * ny * nz];
    data.par_chunks_mut(nx * ny).enumerate().for_each(|(k, slab)| {
        let z = bz0 + k as f64 * g;
        let (ck, wz) = cell_and_weight(k, cnz);
        for j in 0..ny {
            let y = by0 + j as f64 * g;
            let (cj, wy) = cell_and_weight(j, cny);
            for i in 0..nx {
                let (ci, wx) = cell_and_weight(i, cnx);
                let o = ci + cnx * (cj + cny * ck);
                let nxy = cnx * cny;
                let c = |q: usize| f64::from(coarse[q]);
                let (c000, c100, c010, c110) = (c(o), c(o + 1), c(o + cnx), c(o + cnx + 1));
                let (c001, c101, c011, c111) =
                    (c(o + nxy), c(o + nxy + 1), c(o + nxy + cnx), c(o + nxy + cnx + 1));
                let mn = c000
                    .abs()
                    .min(c100.abs())
                    .min(c010.abs())
                    .min(c110.abs())
                    .min(c001.abs())
                    .min(c101.abs())
                    .min(c011.abs())
                    .min(c111.abs());
                let d = if mn > diag && (c000 > 0.0) == (c111 > 0.0) {
                    let x00 = c000 + (c100 - c000) * wx;
                    let x10 = c010 + (c110 - c010) * wx;
                    let x01 = c001 + (c101 - c001) * wx;
                    let x11 = c011 + (c111 - c011) * wx;
                    let y0 = x00 + (x10 - x00) * wy;
                    let y1 = x01 + (x11 - x01) * wy;
                    y0 + (y1 - y0) * wz
                } else {
                    distance(DVec3::new(bx0 + i as f64 * g, y, z))
                };
                slab[i + nx * j] = clamp(d, -DISTANCE_CLAMP_MM, DISTANCE_CLAMP_MM) as f32;
            }
        }
    });
    data
}
