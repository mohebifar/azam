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

    /// These measurements graded from size `from` to size `to` of a size chart: each girth and
    /// length moves by the chart's step between them, so a custom body keeps its proportions.
    #[must_use]
    pub fn graded(&self, from: &SizeSpec, to: &SizeSpec) -> Self {
        Self {
            height: self.height + to.height - from.height,
            bust: self.bust + to.bust - from.bust,
            underbust: self.underbust + to.underbust - from.underbust,
            waist: self.waist + to.waist - from.waist,
            hip: self.hip + to.hip - from.hip,
            neck: self.neck + to.neck - from.neck,
            shoulder: self.shoulder + to.shoulder - from.shoulder,
            arm: self.arm + to.arm - from.arm,
            crotch: self.crotch + to.crotch - from.crotch,
            thigh: self.thigh + to.thigh - from.thigh,
            upper_arm: self.upper_arm + to.upper_arm - from.upper_arm,
            ..*self
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

/// [`Avatar::extra`] key: `false` keeps the avatar at the project's body whatever size is shown.
pub const FOLLOW_SIZE_KEY: &str = "followSize";

impl Avatar {
    /// Whether the avatar takes the body of the size shown (the default).
    #[must_use]
    pub fn follows_size(&self) -> bool {
        self.extra
            .get(FOLLOW_SIZE_KEY)
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    /// The body to show with size `shown` of a run whose base size is `base`: the project's body
    /// graded by the size chart when the avatar follows the size, else the body as it is (also
    /// when either size is not in the chart).
    #[must_use]
    pub fn body_for_size(&self, base: &str, shown: &str) -> Measurements {
        if !self.follows_size() || base == shown {
            return self.body;
        }
        let chart = size_chart(self.body.sex);
        let find = |name: &str| chart.iter().find(|s| s.name == name);
        match (find(base), find(shown)) {
            (Some(from), Some(to)) => self.body.graded(from, to),
            _ => self.body,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_avatar_takes_the_body_of_the_size_shown() {
        let mut av = Avatar::preset(Sex::Female);
        av.body.waist += 30.0; // a custom body, not the chart's
        let chart = size_chart(Sex::Female);
        let (m, l) = (
            chart.iter().find(|s| s.name == "M").expect("M"),
            chart.iter().find(|s| s.name == "L").expect("L"),
        );
        let large = av.body_for_size("M", "L");
        assert!((large.waist - (av.body.waist + l.waist - m.waist)).abs() < 1e-9);
        assert!(large.hip > av.body.hip && large.height >= av.body.height);
        assert_eq!(
            av.body_for_size("M", "M"),
            av.body,
            "the base size is the body itself"
        );
        assert_eq!(av.body_for_size("M", "Q"), av.body, "a size outside the chart");
        av.extra.insert(FOLLOW_SIZE_KEY.to_owned(), Value::Bool(false));
        assert_eq!(av.body_for_size("M", "L"), av.body, "following can be turned off");
    }
}
