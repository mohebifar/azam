//! Limb joints of an avatar.

use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Body side, from the avatar's point of view. The avatar faces +z, so its left is +x.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    /// The avatar's left (+x).
    #[serde(rename = "L")]
    Left,
    /// The avatar's right (-x).
    #[serde(rename = "R")]
    Right,
}

impl Side {
    /// +1 for the left side, -1 for the right.
    #[must_use]
    pub fn sign(self) -> f64 {
        match self {
            Self::Left => 1.0,
            Self::Right => -1.0,
        }
    }
}

/// Joint positions and limb radii of one side of the body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimbJoints {
    /// Shoulder joint.
    pub shoulder: DVec3,
    /// Elbow joint.
    pub elbow: DVec3,
    /// Wrist joint.
    pub wrist: DVec3,
    /// Unit direction of the arm (shoulder towards wrist).
    pub arm_axis: DVec3,
    /// Upper arm radius.
    pub upper_arm_radius: f64,
    /// Hip joint.
    pub hip: DVec3,
    /// Knee joint.
    pub knee: DVec3,
    /// Ankle joint.
    pub ankle: DVec3,
    /// Unit direction of the leg (hip towards ankle).
    pub leg_axis: DVec3,
    /// Thigh radius.
    pub thigh_radius: f64,
}

/// Limb joints of both sides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joints {
    /// The avatar's left side (+x).
    pub left: LimbJoints,
    /// The avatar's right side (-x).
    pub right: LimbJoints,
}

impl Joints {
    /// Joints of one side.
    #[must_use]
    pub fn side(&self, side: Side) -> &LimbJoints {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }
}
