//! Avatar settings as stored in a project: sex, tailor measurements and pose, with presets and
//! size charts.

use serde::{Deserialize, Serialize};

/// Body type of the avatar. It picks the preset that fills in missing measurements and the bust
/// shape of the parametric body.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sex {
    /// Female body (the default).
    #[default]
    Female,
    /// Male body.
    Male,
}

impl Sex {
    /// The lowercase name used in project files.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Female => "female",
            Self::Male => "male",
        }
    }
}

/// Default arm angle from vertical, in degrees.
pub const DEFAULT_ARM_ANGLE_DEG: f64 = 45.0;
/// Default skin colour of a new project's avatar.
pub const DEFAULT_SKIN: &str = "#b4b1ac";

/// Avatar settings of a project. Lengths are millimetres, angles degrees.
///
/// The field names match the project file. Measurements that are missing from a file are filled
/// in from the preset of the file's [`Sex`] when it is deserialised.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "RawAvatarParams")]
pub struct AvatarParams {
    /// Body type.
    pub sex: Sex,
    /// Standing height, floor to crown.
    pub height: f64,
    /// Bust (female) or chest (male) circumference.
    pub bust: f64,
    /// Underbust circumference. Zero or less means "bust minus 110 mm".
    pub underbust: f64,
    /// Waist circumference.
    pub waist: f64,
    /// Hip circumference.
    pub hip: f64,
    /// Shoulder width, shoulder point to shoulder point.
    pub shoulder: f64,
    /// Arm length, shoulder to wrist.
    pub arm: f64,
    /// Crotch height (inseam), floor to crotch.
    pub crotch: f64,
    /// Neck circumference.
    pub neck: f64,
    /// Thigh circumference.
    pub thigh: f64,
    /// Upper arm circumference.
    pub upper_arm: f64,
    /// Arm angle from vertical (0 = arms down).
    pub arm_angle: f64,
    /// Leg spread angle from vertical.
    pub leg_spread: f64,
    /// Body fat 0..1 for the morph-target body; `None` lets the fit choose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fat: Option<f64>,
    /// Muscle 0..1 for the morph-target body; `None` means 0.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muscle: Option<f64>,
    /// Across-front width (morph-target body only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub across_front: Option<f64>,
    /// Back length, nape to waist (morph-target body only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_length: Option<f64>,
    /// Waist-to-hip length (morph-target body only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waist_to_hip: Option<f64>,
    /// Wrist circumference (morph-target body only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrist: Option<f64>,
    /// Knee height, knee to floor (morph-target body only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knee_height: Option<f64>,
    /// Knee circumference (morph-target body only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knee: Option<f64>,
    /// Calf circumference (morph-target body only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calf: Option<f64>,
    /// Ankle circumference (morph-target body only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ankle: Option<f64>,
    /// Skin colour as a CSS hex string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
    /// Whether the avatar is shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<bool>,
}

impl Default for AvatarParams {
    fn default() -> Self {
        Self::preset(Sex::Female)
    }
}

impl AvatarParams {
    /// The preset body for `sex` (the medium size of its size chart, arms at 45°, legs at 6°).
    #[must_use]
    pub fn preset(sex: Sex) -> Self {
        let (height, bust, underbust, waist, hip, shoulder, arm, crotch, neck, thigh, upper_arm) = match sex {
            Sex::Female => (
                1700.0, 880.0, 760.0, 700.0, 960.0, 365.0, 560.0, 780.0, 330.0, 540.0, 270.0,
            ),
            Sex::Male => (
                1800.0, 980.0, 920.0, 840.0, 980.0, 410.0, 610.0, 840.0, 390.0, 560.0, 310.0,
            ),
        };
        Self {
            sex,
            height,
            bust,
            underbust,
            waist,
            hip,
            shoulder,
            arm,
            crotch,
            neck,
            thigh,
            upper_arm,
            arm_angle: DEFAULT_ARM_ANGLE_DEG,
            leg_spread: 6.0,
            fat: None,
            muscle: None,
            across_front: None,
            back_length: None,
            waist_to_hip: None,
            wrist: None,
            knee_height: None,
            knee: None,
            calf: None,
            ankle: None,
            skin: None,
            show: None,
        }
    }

    /// The avatar of a new project: the preset with the default skin, shown.
    #[must_use]
    pub fn new_project(sex: Sex) -> Self {
        Self {
            skin: Some(DEFAULT_SKIN.to_owned()),
            show: Some(true),
            ..Self::preset(sex)
        }
    }

    /// Whether this is a female body.
    #[must_use]
    pub fn is_female(&self) -> bool {
        self.sex == Sex::Female
    }

    /// Underbust circumference, falling back to bust minus 110 mm when unset.
    #[must_use]
    pub fn underbust_or_default(&self) -> f64 {
        if self.underbust > 0.0 {
            self.underbust
        } else {
            self.bust - UNDERBUST_FALLBACK_MM
        }
    }

    /// The value of a tailor measurement by its key (see [`crate::MEASUREMENTS`]), or `None`
    /// when it is unset or the key is unknown.
    #[must_use]
    pub fn measurement(&self, key: &str) -> Option<f64> {
        Some(match key {
            "height" => self.height,
            "bust" => self.bust,
            "underbust" => self.underbust,
            "waist" => self.waist,
            "hip" => self.hip,
            "neck" => self.neck,
            "shoulder" => self.shoulder,
            "arm" => self.arm,
            "upperArm" => self.upper_arm,
            "crotch" => self.crotch,
            "thigh" => self.thigh,
            "acrossFront" => return self.across_front,
            "backLength" => return self.back_length,
            "waistToHip" => return self.waist_to_hip,
            "wrist" => return self.wrist,
            "kneeHeight" => return self.knee_height,
            "knee" => return self.knee,
            "calf" => return self.calf,
            "ankle" => return self.ankle,
            _ => return None,
        })
    }

    /// Copies every measurement of a size-chart row onto these settings.
    pub fn apply_size(&mut self, size: &SizeChartEntry) {
        self.height = size.height;
        self.bust = size.bust;
        self.underbust = size.underbust;
        self.waist = size.waist;
        self.hip = size.hip;
        self.neck = size.neck;
        self.shoulder = size.shoulder;
        self.arm = size.arm;
        self.crotch = size.crotch;
        self.thigh = size.thigh;
        self.upper_arm = size.upper_arm;
    }
}

/// Underbust used when none is set: this much less than the bust, in millimetres.
const UNDERBUST_FALLBACK_MM: f64 = 110.0;

/// Avatar settings as they appear in a file: every field optional.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAvatarParams {
    sex: Option<Sex>,
    height: Option<f64>,
    bust: Option<f64>,
    underbust: Option<f64>,
    waist: Option<f64>,
    hip: Option<f64>,
    shoulder: Option<f64>,
    arm: Option<f64>,
    crotch: Option<f64>,
    neck: Option<f64>,
    thigh: Option<f64>,
    upper_arm: Option<f64>,
    arm_angle: Option<f64>,
    leg_spread: Option<f64>,
    fat: Option<f64>,
    muscle: Option<f64>,
    across_front: Option<f64>,
    back_length: Option<f64>,
    waist_to_hip: Option<f64>,
    wrist: Option<f64>,
    knee_height: Option<f64>,
    knee: Option<f64>,
    calf: Option<f64>,
    ankle: Option<f64>,
    skin: Option<String>,
    show: Option<bool>,
}

impl From<RawAvatarParams> for AvatarParams {
    fn from(raw: RawAvatarParams) -> Self {
        let base = Self::preset(raw.sex.unwrap_or_default());
        Self {
            sex: base.sex,
            height: raw.height.unwrap_or(base.height),
            bust: raw.bust.unwrap_or(base.bust),
            underbust: raw.underbust.unwrap_or(base.underbust),
            waist: raw.waist.unwrap_or(base.waist),
            hip: raw.hip.unwrap_or(base.hip),
            shoulder: raw.shoulder.unwrap_or(base.shoulder),
            arm: raw.arm.unwrap_or(base.arm),
            crotch: raw.crotch.unwrap_or(base.crotch),
            neck: raw.neck.unwrap_or(base.neck),
            thigh: raw.thigh.unwrap_or(base.thigh),
            upper_arm: raw.upper_arm.unwrap_or(base.upper_arm),
            arm_angle: raw.arm_angle.unwrap_or(base.arm_angle),
            leg_spread: raw.leg_spread.unwrap_or(base.leg_spread),
            fat: raw.fat,
            muscle: raw.muscle,
            across_front: raw.across_front,
            back_length: raw.back_length,
            waist_to_hip: raw.waist_to_hip,
            wrist: raw.wrist,
            knee_height: raw.knee_height,
            knee: raw.knee,
            calf: raw.calf,
            ankle: raw.ankle,
            skin: raw.skin,
            show: raw.show,
        }
    }
}

/// One row of a standard size chart. Lengths are millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeChartEntry {
    /// Size label, e.g. `"M"`.
    pub name: &'static str,
    /// Standing height.
    pub height: f64,
    /// Bust or chest circumference.
    pub bust: f64,
    /// Underbust circumference.
    pub underbust: f64,
    /// Waist circumference.
    pub waist: f64,
    /// Hip circumference.
    pub hip: f64,
    /// Neck circumference.
    pub neck: f64,
    /// Shoulder width.
    pub shoulder: f64,
    /// Arm length.
    pub arm: f64,
    /// Crotch height.
    pub crotch: f64,
    /// Thigh circumference.
    pub thigh: f64,
    /// Upper arm circumference.
    pub upper_arm: f64,
}

#[allow(clippy::too_many_arguments)]
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
) -> SizeChartEntry {
    SizeChartEntry {
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

/// Female size chart, XS to XL.
pub const FEMALE_SIZES: [SizeChartEntry; 5] = [
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

/// Male size chart, S to XL.
pub const MALE_SIZES: [SizeChartEntry; 4] = [
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

/// The size chart for `sex`.
#[must_use]
pub fn size_chart(sex: Sex) -> &'static [SizeChartEntry] {
    match sex {
        Sex::Female => &FEMALE_SIZES,
        Sex::Male => &MALE_SIZES,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_come_from_the_preset_of_the_files_sex() {
        let p: AvatarParams =
            serde_json::from_str(r#"{"sex":"male","height":1750,"armAngle":null}"#).unwrap();
        assert_eq!(p.sex, Sex::Male);
        assert_eq!(p.height, 1750.0);
        assert_eq!(p.bust, 980.0);
        assert_eq!(p.arm_angle, DEFAULT_ARM_ANGLE_DEG);
        let empty: AvatarParams = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, AvatarParams::preset(Sex::Female));
    }

    #[test]
    fn round_trips_through_json_with_project_field_names() {
        let mut p = AvatarParams::new_project(Sex::Female);
        p.knee_height = Some(480.0);
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["upperArm"], 270.0);
        assert_eq!(json["legSpread"], 6.0);
        assert_eq!(json["kneeHeight"], 480.0);
        assert_eq!(json["skin"], DEFAULT_SKIN);
        assert!(json.get("fat").is_none());
        let back: AvatarParams = serde_json::from_value(json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn size_rows_apply_every_measurement() {
        let mut p = AvatarParams::preset(Sex::Female);
        p.apply_size(&FEMALE_SIZES[4]);
        assert_eq!((p.height, p.upper_arm, p.neck), (1740.0, 310.0, 355.0));
        assert_eq!(p.measurement("upperArm"), Some(310.0));
        assert_eq!(p.measurement("calf"), None);
        assert_eq!(p.measurement("nope"), None);
    }
}
