//! Avatar settings stored in the project: body measurements, presets and size charts.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Skin colour of a new avatar.
pub const DEFAULT_SKIN: &str = "#b4b1ac";

/// Body type of the avatar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sex {
    /// Female body.
    #[default]
    Female,
    /// Male body.
    Male,
}

/// Body measurements in mm (circumferences are full girths). Angles are in degrees.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurements {
    /// Body type.
    pub sex: Sex,
    /// Standing height.
    pub height: f64,
    /// Bust or chest girth.
    pub bust: f64,
    /// Underbust girth.
    pub underbust: f64,
    /// Waist girth.
    pub waist: f64,
    /// Hip girth.
    pub hip: f64,
    /// Shoulder width, point to point.
    pub shoulder: f64,
    /// Arm length.
    pub arm: f64,
    /// Crotch height from the floor.
    pub crotch: f64,
    /// Neck girth.
    pub neck: f64,
    /// Thigh girth.
    pub thigh: f64,
    /// Upper-arm girth.
    pub upper_arm: f64,
    /// Arm angle of the A-pose, in degrees.
    pub arm_angle: f64,
    /// Leg spread, in degrees.
    pub leg_spread: f64,
}

impl Measurements {
    /// The default body of `sex`.
    #[must_use]
    pub fn preset(sex: Sex) -> Self {
        match sex {
            Sex::Female => Self {
                sex,
                height: 1700.0,
                bust: 880.0,
                underbust: 760.0,
                waist: 700.0,
                hip: 960.0,
                shoulder: 365.0,
                arm: 560.0,
                crotch: 780.0,
                neck: 330.0,
                thigh: 540.0,
                upper_arm: 270.0,
                arm_angle: 45.0,
                leg_spread: 6.0,
            },
            Sex::Male => Self {
                sex,
                height: 1800.0,
                bust: 980.0,
                underbust: 920.0,
                waist: 840.0,
                hip: 980.0,
                shoulder: 410.0,
                arm: 610.0,
                crotch: 840.0,
                neck: 390.0,
                thigh: 560.0,
                upper_arm: 310.0,
                arm_angle: 45.0,
                leg_spread: 6.0,
            },
        }
    }

    /// These measurements with the girths and lengths of a size-chart entry.
    #[must_use]
    pub fn with_size(&self, size: &SizeSpec) -> Self {
        Self {
            height: size.height,
            bust: size.bust,
            underbust: size.underbust,
            waist: size.waist,
            hip: size.hip,
            neck: size.neck,
            shoulder: size.shoulder,
            arm: size.arm,
            crotch: size.crotch,
            thigh: size.thigh,
            upper_arm: size.upper_arm,
            ..*self
        }
    }
}

/// The avatar of a project: measurements plus display settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Avatar {
    /// Body measurements.
    #[serde(flatten)]
    pub body: Measurements,
    /// Skin colour, `#rrggbb`.
    pub skin: String,
    /// Whether the avatar is shown in 3D.
    pub show: bool,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Avatar {
    /// The default avatar of `sex`.
    #[must_use]
    pub fn preset(sex: Sex) -> Self {
        Self {
            body: Measurements::preset(sex),
            skin: DEFAULT_SKIN.to_owned(),
            show: true,
            extra: Map::new(),
        }
    }
}

/// One size of a size chart, in mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeSpec {
    /// Size name, such as `"M"`.
    pub name: &'static str,
    /// Standing height.
    pub height: f64,
    /// Bust or chest girth.
    pub bust: f64,
    /// Underbust girth.
    pub underbust: f64,
    /// Waist girth.
    pub waist: f64,
    /// Hip girth.
    pub hip: f64,
    /// Neck girth.
    pub neck: f64,
    /// Shoulder width.
    pub shoulder: f64,
    /// Arm length.
    pub arm: f64,
    /// Crotch height.
    pub crotch: f64,
    /// Thigh girth.
    pub thigh: f64,
    /// Upper-arm girth.
    pub upper_arm: f64,
}

#[allow(clippy::too_many_arguments, reason = "one row of a size chart")]
const fn size(
    name: &'static str,
    height: f64,
    bust: f64,
    underbust: f64,
    waist: f64,
    hip: f64,
    neck: f64,
    shoulder: f64,
    arm: f64,
    crotch: f64,
    thigh: f64,
    upper_arm: f64,
) -> SizeSpec {
    SizeSpec {
        name,
        height,
        bust,
        underbust,
        waist,
        hip,
        neck,
        shoulder,
        arm,
        crotch,
        thigh,
        upper_arm,
    }
}

/// Female size chart, smallest first.
const FEMALE_SIZES: [SizeSpec; 5] = [
    size(
        "XS", 1640.0, 800.0, 690.0, 620.0, 880.0, 310.0, 350.0, 545.0, 755.0, 500.0, 245.0,
    ),
    size(
        "S", 1670.0, 840.0, 725.0, 660.0, 920.0, 320.0, 357.0, 552.0, 768.0, 520.0, 257.0,
    ),
    size(
        "M", 1700.0, 880.0, 760.0, 700.0, 960.0, 330.0, 365.0, 560.0, 780.0, 540.0, 270.0,
    ),
    size(
        "L", 1720.0, 940.0, 810.0, 760.0, 1020.0, 342.0, 375.0, 566.0, 788.0, 575.0, 290.0,
    ),
    size(
        "XL", 1740.0, 1000.0, 860.0, 820.0, 1080.0, 355.0, 385.0, 572.0, 796.0, 610.0, 310.0,
    ),
];

/// Male size chart, smallest first.
const MALE_SIZES: [SizeSpec; 4] = [
    size(
        "S", 1740.0, 900.0, 850.0, 760.0, 920.0, 370.0, 395.0, 590.0, 815.0, 520.0, 285.0,
    ),
    size(
        "M", 1800.0, 980.0, 920.0, 840.0, 980.0, 390.0, 410.0, 610.0, 840.0, 560.0, 310.0,
    ),
    size(
        "L", 1840.0, 1060.0, 1000.0, 920.0, 1040.0, 410.0, 425.0, 622.0, 855.0, 595.0, 335.0,
    ),
    size(
        "XL", 1870.0, 1140.0, 1080.0, 1000.0, 1100.0, 430.0, 440.0, 632.0, 865.0, 630.0, 360.0,
    ),
];

/// The size chart of `sex`, smallest first.
#[must_use]
pub fn size_chart(sex: Sex) -> &'static [SizeSpec] {
    match sex {
        Sex::Female => &FEMALE_SIZES,
        Sex::Male => &MALE_SIZES,
    }
}
