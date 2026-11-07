//! Avatar motions for checking garments while the body moves: a pose per instant of a motion.
//!
//! A [`Motion`] gives an [`AvatarPose`] for every time ([`pose_at`]); [`crate::Avatar::posed`]
//! re-poses the avatar's mesh, joints and collision grid. Every motion starts from the rest pose
//! (the avatar as built) and eases in, so cloth that has settled is not jolted.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

/// Seconds over which a cyclic motion (walk) reaches full amplitude, and a turntable full speed.
pub const MOTION_RAMP_S: f64 = 1.0;

/// A predefined avatar motion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Motion {
    /// Standing still in the rest pose.
    #[default]
    None,
    /// Raising both arms sideways above the shoulders and lowering them again.
    RaiseArms,
    /// Walking on the spot: hips, knees and arms swing.
    Walk,
    /// Bending into a sitting position (hips and knees at about 90°) and standing up again.
    Sit,
    /// Turning about the vertical axis, one revolution per period.
    Turntable,
}

impl Motion {
    /// Every motion, in menu order.
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::RaiseArms,
        Self::Walk,
        Self::Sit,
        Self::Turntable,
    ];

    /// Display name.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::RaiseArms => "Raise Arms",
            Self::Walk => "Walk",
            Self::Sit => "Sit",
            Self::Turntable => "Turntable",
        }
    }

    /// Length of one cycle, in seconds (0 for [`Motion::None`]).
    #[must_use]
    pub fn period(self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::RaiseArms => 4.0,
            Self::Walk => 1.2,
            Self::Sit => 6.0,
            Self::Turntable => 8.0,
        }
    }
}

/// Joint angles of a pose relative to the rest pose, in degrees. Pairs are `[left, right]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AvatarPose {
    /// Sideways arm raise (abduction, in the frontal plane); positive lifts the hand.
    pub arm_raise: [f64; 2],
    /// Forward arm swing (shoulder flexion); positive brings the hand forwards.
    pub arm_swing: [f64; 2],
    /// Elbow bend; positive brings the hand forwards and up.
    pub elbow: [f64; 2],
    /// Hip bend (flexion); positive brings the knee forwards.
    pub hip: [f64; 2],
    /// Knee bend; positive brings the foot backwards.
    pub knee: [f64; 2],
    /// Forward lean of the upper body at the lower back.
    pub lean: f64,
    /// Turn of the whole body about the vertical axis; positive turns the front towards +x.
    pub turn: f64,
}

impl AvatarPose {
    /// The rest pose: the avatar as built.
    pub const REST: Self = Self {
        arm_raise: [0.0; 2],
        arm_swing: [0.0; 2],
        elbow: [0.0; 2],
        hip: [0.0; 2],
        knee: [0.0; 2],
        lean: 0.0,
        turn: 0.0,
    };

    /// Whether this is the rest pose.
    #[must_use]
    pub fn is_rest(&self) -> bool {
        *self == Self::REST
    }
}

/// Smooth 0→1 ease (zero slope at both ends).
fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// 0 at the start of a cycle, 1 halfway, back to 0 at the end, with zero velocity at the turns.
fn rise_and_fall(t: f64, period: f64) -> f64 {
    0.5 - 0.5 * (TAU * t / period).cos()
}

/// The pose of `motion` at `t` seconds after it started (`t` ≤ 0 gives the rest pose).
#[must_use]
pub fn pose_at(motion: Motion, t: f64) -> AvatarPose {
    let mut p = AvatarPose::REST;
    if t <= 0.0 || !t.is_finite() {
        return p;
    }
    let period = motion.period();
    match motion {
        Motion::None => {}
        Motion::RaiseArms => {
            let s = rise_and_fall(t, period);
            p.arm_raise = [75.0 * s; 2];
            p.elbow = [10.0 * s; 2];
        }
        Motion::Walk => {
            let a = smoothstep(t / MOTION_RAMP_S);
            let ph = TAU * t / period;
            let (s, s_late) = (ph.sin(), (ph + 0.6).sin());
            p.hip = [22.0 * a * s, -22.0 * a * s];
            p.knee = [
                a * (6.0 + 34.0 * s_late.max(0.0)),
                a * (6.0 + 34.0 * (-s_late).max(0.0)),
            ];
            p.arm_swing = [-18.0 * a * s, 18.0 * a * s];
            p.elbow = [a * (14.0 - 6.0 * s), a * (14.0 + 6.0 * s)];
            p.lean = 3.0 * a;
        }
        Motion::Sit => {
            let s = rise_and_fall(t, period);
            p.hip = [82.0 * s; 2];
            p.knee = [86.0 * s; 2];
            p.lean = 14.0 * s;
            p.arm_swing = [22.0 * s; 2];
            p.elbow = [24.0 * s; 2];
        }
        Motion::Turntable => {
            // constant angular acceleration over the ramp, then constant speed
            let w = 360.0 / period;
            let r = MOTION_RAMP_S;
            p.turn = if t < r {
                w * t * t / (2.0 * r)
            } else {
                w * (t - r / 2.0)
            };
            p.turn = p.turn.rem_euclid(360.0);
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_motion_starts_at_rest_and_moves_smoothly() {
        for m in Motion::ALL {
            assert!(pose_at(m, 0.0).is_rest(), "{m:?}");
            assert!(pose_at(m, -1.0).is_rest(), "{m:?}");
            let dt = 1.0 / 60.0;
            let mut prev = pose_at(m, 0.0);
            for i in 1..600 {
                let p = pose_at(m, f64::from(i) * dt);
                let jump = |a: f64, b: f64| {
                    let d = (a - b).abs();
                    d.min(360.0 - d)
                };
                let max = [
                    jump(p.arm_raise[0], prev.arm_raise[0]),
                    jump(p.hip[0], prev.hip[0]),
                    jump(p.knee[1], prev.knee[1]),
                    jump(p.turn, prev.turn),
                    jump(p.lean, prev.lean),
                ]
                .into_iter()
                .fold(0.0, f64::max);
                assert!(max < 4.0, "{m:?} jumps {max}° at frame {i}");
                prev = p;
            }
        }
        assert!(pose_at(Motion::RaiseArms, 2.0).arm_raise[0] > 70.0);
        assert!(pose_at(Motion::Sit, 3.0).knee[0] > 80.0);
        assert!((pose_at(Motion::Turntable, 5.0).turn - 360.0 / 8.0 * 4.5).abs() < 1e-9);
    }
}
