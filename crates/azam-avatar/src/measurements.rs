//! Tailor measurements of the morph-target body and how each is read off the mesh.

/// How a measurement is read off the body mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasureKind {
    /// Body height, floor to crown.
    Height,
    /// Tape around the named vertex loop (convex hull perimeter in the loop's plane).
    Circumference(&'static str),
    /// Straight distance between the two vertices of the named measure.
    Distance(&'static str),
    /// Distance between the two shoulder landmarks.
    Shoulder,
    /// Upper arm length plus lower arm length.
    Arm,
    /// Height of the crotch landmark above the floor.
    Crotch,
}

/// One tailor measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasurementDef {
    /// Key, the same as the field name in the project file.
    pub key: &'static str,
    /// Display label.
    pub label: &'static str,
    /// Display group.
    pub group: &'static str,
    /// How it is measured.
    pub kind: MeasureKind,
}

const fn def(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    kind: MeasureKind,
) -> MeasurementDef {
    MeasurementDef {
        key,
        label,
        group,
        kind,
    }
}

/// Number of tailor measurements.
pub const MEASUREMENT_COUNT: usize = 19;

/// Every tailor measurement, in display order.
pub const MEASUREMENTS: [MeasurementDef; MEASUREMENT_COUNT] = {
    use MeasureKind::{Arm, Circumference, Crotch, Distance, Height, Shoulder};
    [
        def("height", "Height", "Body", Height),
        def("bust", "Bust / chest", "Body", Circumference("measure-bust-circ")),
        def(
            "underbust",
            "Underbust",
            "Body",
            Circumference("measure-underbust-circ"),
        ),
        def("waist", "Waist", "Body", Circumference("measure-waist-circ")),
        def("hip", "Hip", "Body", Circumference("measure-hips-circ")),
        def("neck", "Neck", "Neck & torso", Circumference("measure-neck-circ")),
        def("shoulder", "Shoulder width", "Neck & torso", Shoulder),
        def(
            "acrossFront",
            "Across front",
            "Neck & torso",
            Distance("measure-frontchest-dist"),
        ),
        def(
            "backLength",
            "Back length (nape–waist)",
            "Neck & torso",
            Distance("measure-napetowaist-dist"),
        ),
        def(
            "waistToHip",
            "Waist to hip",
            "Neck & torso",
            Distance("measure-waisttohip-dist"),
        ),
        def("arm", "Arm length (shoulder–wrist)", "Arms", Arm),
        def(
            "upperArm",
            "Upper arm",
            "Arms",
            Circumference("measure-upperarm-circ"),
        ),
        def("wrist", "Wrist", "Arms", Circumference("measure-wrist-circ")),
        def("crotch", "Crotch height (inseam)", "Legs", Crotch),
        def(
            "kneeHeight",
            "Knee to floor",
            "Legs",
            Distance("measure-lowerleg-height"),
        ),
        def("thigh", "Thigh", "Legs", Circumference("measure-thigh-circ")),
        def("knee", "Knee", "Legs", Circumference("measure-knee-circ")),
        def("calf", "Calf", "Legs", Circumference("measure-calf-circ")),
        def("ankle", "Ankle", "Legs", Circumference("measure-ankle-circ")),
    ]
};

/// Index of the measurement with `key` in [`MEASUREMENTS`].
#[must_use]
pub fn measurement_index(key: &str) -> Option<usize> {
    MEASUREMENTS.iter().position(|d| d.key == key)
}

/// Named measure of the upper arm length.
pub(crate) const UPPER_ARM_LENGTH: &str = "measure-upperarm-length";
/// Named measure of the lower arm length.
pub(crate) const LOWER_ARM_LENGTH: &str = "measure-lowerarm-length";

/// Number of morph targets the fit adjusts.
pub const FIT_TARGET_COUNT: usize = 19;

/// Morph targets the fit adjusts, in parameter order. Each has `-incr` and `-decr` variants.
pub const FIT_TARGETS: [&str; FIT_TARGET_COUNT] = [
    "measure-neck-circ",
    "measure-upperarm-circ",
    UPPER_ARM_LENGTH,
    LOWER_ARM_LENGTH,
    "measure-wrist-circ",
    "measure-frontchest-dist",
    "measure-bust-circ",
    "measure-underbust-circ",
    "measure-waist-circ",
    "measure-napetowaist-dist",
    "measure-waisttohip-dist",
    "measure-shoulder-dist",
    "measure-hips-circ",
    "measure-upperleg-height",
    "measure-thigh-circ",
    "measure-lowerleg-height",
    "measure-calf-circ",
    "measure-ankle-circ",
    "measure-knee-circ",
];

/// One value per tailor measurement, in [`MEASUREMENTS`] order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measurements(pub [f64; MEASUREMENT_COUNT]);

impl Measurements {
    /// The value of the measurement with `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<f64> {
        measurement_index(key).map(|i| self.0[i])
    }

    /// Pairs of key and value, in display order.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, f64)> + '_ {
        MEASUREMENTS.iter().zip(self.0).map(|(d, v)| (d.key, v))
    }
}
