//! Posing the morph-target body: arms about the shoulders and legs about the hips, in the frontal
//! plane, with linear-blend skinning.

use glam::DVec3;

use crate::math::DEG;

use super::parse::MhModel;

/// Rotations smaller than this (radians) are skipped.
const MIN_ROTATION_RAD: f64 = 1e-4;

/// Natural limb angles of the unposed body, in degrees from vertical.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PoseInfo {
    /// Left arm.
    pub left_arm: f64,
    /// Left leg.
    pub left_leg: f64,
    /// Right arm.
    pub right_arm: f64,
    /// Right leg.
    pub right_leg: f64,
}

impl MhModel {
    /// Centroid of a joint's vertex group in `positions`.
    pub(crate) fn joint(&self, positions: &[f32], name: &str) -> DVec3 {
        let group = self.joint_vertices(name);
        let mut c = [0.0_f64; 3];
        for &v in group {
            for (a, ci) in c.iter_mut().enumerate() {
                *ci += f64::from(positions[v as usize * 3 + a]);
            }
        }
        let n = group.len() as f64;
        DVec3::new(c[0] / n, c[1] / n, c[2] / n)
    }

    /// Rotates the vertices weighted by `weights` about `pivot` by `angle` radians in the x-y plane.
    fn rotate(&self, positions: &mut [f32], weights: &[u8], pivot: DVec3, angle: f64) {
        if angle.abs() < MIN_ROTATION_RAD {
            return;
        }
        let (c, s) = (angle.cos(), angle.sin());
        for (v, &w) in weights.iter().enumerate().take(self.vertex_count) {
            if w == 0 {
                continue;
            }
            let wt = f64::from(w) / 255.0;
            let k = v * 3;
            let (x, y) = (f64::from(positions[k]), f64::from(positions[k + 1]));
            let (dx, dy) = (x - pivot.x, y - pivot.y);
            let rx = pivot.x + dx * c - dy * s;
            let ry = pivot.y + dx * s + dy * c;
            positions[k] = (x + (rx - x) * wt) as f32;
            positions[k + 1] = (y + (ry - y) * wt) as f32;
        }
    }

    /// Poses `positions` in place: arms at `arm_angle` and legs at `leg_spread` degrees from
    /// vertical (`None` keeps the natural angle). Returns the natural angles.
    ///
    /// # Panics
    ///
    /// Panics if `positions` holds fewer than `3 * vertex_count` values.
    pub fn pose(&self, positions: &mut [f32], arm_angle: Option<f64>, leg_spread: Option<f64>) -> PoseInfo {
        assert!(
            positions.len() >= self.vertex_count * 3,
            "positions do not cover the model"
        );
        let mut info = PoseInfo::default();
        let w = &self.weights;
        for (sd, arm_w, leg_w) in [("l", &w.arm_left, &w.leg_left), ("r", &w.arm_right, &w.leg_right)] {
            let dir = if sd == "l" { 1.0 } else { -1.0 };
            let shoulder = self.joint(positions, &format!("joint-{sd}-shoulder"));
            let hand = self.joint(positions, &format!("joint-{sd}-hand"));
            let natural_arm = (hand.x - shoulder.x).abs().atan2(shoulder.y - hand.y);
            if let Some(a) = arm_angle {
                self.rotate(positions, arm_w, shoulder, dir * (a * DEG - natural_arm));
            }
            let hip = self.joint(positions, &format!("joint-{sd}-upper-leg"));
            let ankle = self.joint(positions, &format!("joint-{sd}-ankle"));
            let natural_leg = (ankle.x - hip.x).abs().atan2(hip.y - ankle.y);
            if let Some(a) = leg_spread {
                self.rotate(positions, leg_w, hip, dir * (a * DEG - natural_leg));
            }
            let (arm, leg) = (natural_arm / DEG, natural_leg / DEG);
            if sd == "l" {
                (info.left_arm, info.left_leg) = (arm, leg);
            } else {
                (info.right_arm, info.right_leg) = (arm, leg);
            }
        }
        info
    }
}
