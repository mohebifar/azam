//! Simulation settings: defaults, quality presets, limits and per-project overrides.
//!
//! A project stores a preset name plus overrides ([`azam_doc::SimSettings`]); [`resolve_settings`]
//! turns that into concrete [`SimValues`], and [`crate::SimParams`] derives the solver parameters
//! from them.

use azam_doc::SimSettings;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::numeric::clamp;

/// Cloth-on-cloth friction behaviour while two layers touch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrictionModel {
    /// Contacts hold: a fixed fraction of the slip is removed (capped at 0.8).
    #[default]
    Grip,
    /// Coulomb friction: layers slide once the slip exceeds the friction cone.
    Slide,
}

/// Every simulation setting, resolved to a value.
#[allow(
    clippy::struct_excessive_bools,
    reason = "mirrors the stored settings, which are independent switches"
)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimValues {
    /// Multiplies every piece's particle distance (mesh density); below 1 is finer.
    pub pd_scale: f64,
    /// Simulated seconds per frame.
    pub dt: f64,
    /// Solver substeps per frame.
    pub substeps: f64,
    /// Stretch and sewing constraint iterations per substep.
    pub iterations: f64,
    /// Simulated frames per screen frame.
    pub frames_per_step: f64,
    /// Cloth-cloth collision, within and between pieces and layers.
    pub self_collision: bool,
    /// Re-detect cloth contacts every this many substeps (accelerated solver).
    #[serde(rename = "ccEveryGPU")]
    pub cc_every_gpu: f64,
    /// Contact resolution passes per substep (accelerated solver).
    pub cc_iters: f64,
    /// Distance kept between cloth surfaces, in mm.
    pub cloth_thickness: f64,
    /// Cloth-on-cloth friction as a multiple of the fabric friction.
    pub cc_fric: f64,
    /// Pull layers that crossed back to their own side.
    pub cc_repair: bool,
    /// Find cloth that passed through cloth and push it back.
    pub untangle: bool,
    /// Cloth-on-cloth friction model.
    pub fric_model: FrictionModel,
    /// Gravity in g.
    pub gravity: f64,
    /// Air damping, per second.
    pub air: f64,
    /// Speed at which open seams are pulled closed, in mm/s.
    pub sew_speed: f64,
    /// Fastest the grab point follows the pointer, in mm/s.
    pub grab_speed: f64,
    /// Stop a run once the garment has settled.
    pub auto_stop: bool,
    /// Settle threshold on the motion energy (RMS particle speed, mm/s).
    pub settle: f64,
    /// Stop an auto-stopping run after this many simulated seconds.
    pub max_time: f64,
}

/// The defaults every preset starts from.
pub const SIM_DEFAULTS: SimValues = SimValues {
    pd_scale: 1.0,
    dt: 1.0 / 60.0,
    substeps: 10.0,
    iterations: 3.0,
    frames_per_step: 1.0,
    self_collision: true,
    cc_every_gpu: 2.0,
    cc_iters: 2.0,
    cloth_thickness: 2.5,
    cc_fric: 2.0,
    cc_repair: true,
    untangle: true,
    fric_model: FrictionModel::Grip,
    gravity: 1.0,
    air: 0.3,
    sew_speed: 1600.0,
    grab_speed: 700.0,
    auto_stop: false,
    settle: 14.0,
    max_time: 20.0,
};

/// One simulation setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingKey {
    /// [`SimValues::pd_scale`].
    PdScale,
    /// [`SimValues::dt`].
    Dt,
    /// [`SimValues::substeps`].
    Substeps,
    /// [`SimValues::iterations`].
    Iterations,
    /// [`SimValues::frames_per_step`].
    FramesPerStep,
    /// [`SimValues::self_collision`].
    SelfCollision,
    /// [`SimValues::cc_every_gpu`].
    CcEveryGpu,
    /// [`SimValues::cc_iters`].
    CcIters,
    /// [`SimValues::cloth_thickness`].
    ClothThickness,
    /// [`SimValues::cc_fric`].
    CcFric,
    /// [`SimValues::cc_repair`].
    CcRepair,
    /// [`SimValues::untangle`].
    Untangle,
    /// [`SimValues::fric_model`].
    FricModel,
    /// [`SimValues::gravity`].
    Gravity,
    /// [`SimValues::air`].
    Air,
    /// [`SimValues::sew_speed`].
    SewSpeed,
    /// [`SimValues::grab_speed`].
    GrabSpeed,
    /// [`SimValues::auto_stop`].
    AutoStop,
    /// [`SimValues::settle`].
    Settle,
    /// [`SimValues::max_time`].
    MaxTime,
}

impl SettingKey {
    /// Every setting, in stored order.
    pub const ALL: [Self; 20] = [
        Self::PdScale,
        Self::Dt,
        Self::Substeps,
        Self::Iterations,
        Self::FramesPerStep,
        Self::SelfCollision,
        Self::CcEveryGpu,
        Self::CcIters,
        Self::ClothThickness,
        Self::CcFric,
        Self::CcRepair,
        Self::Untangle,
        Self::FricModel,
        Self::Gravity,
        Self::Air,
        Self::SewSpeed,
        Self::GrabSpeed,
        Self::AutoStop,
        Self::Settle,
        Self::MaxTime,
    ];

    /// Stored name.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::PdScale => "pdScale",
            Self::Dt => "dt",
            Self::Substeps => "substeps",
            Self::Iterations => "iterations",
            Self::FramesPerStep => "framesPerStep",
            Self::SelfCollision => "selfCollision",
            Self::CcEveryGpu => "ccEveryGPU",
            Self::CcIters => "ccIters",
            Self::ClothThickness => "clothThickness",
            Self::CcFric => "ccFric",
            Self::CcRepair => "ccRepair",
            Self::Untangle => "untangle",
            Self::FricModel => "fricModel",
            Self::Gravity => "gravity",
            Self::Air => "air",
            Self::SewSpeed => "sewSpeed",
            Self::GrabSpeed => "grabSpeed",
            Self::AutoStop => "autoStop",
            Self::Settle => "settle",
            Self::MaxTime => "maxTime",
        }
    }

    /// The setting with stored name `key`.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// Allowed range of a numeric setting.
    #[must_use]
    pub fn limits(self) -> Option<(f64, f64)> {
        Some(match self {
            Self::PdScale => (0.4, 2.5),
            Self::Dt => (1.0 / 240.0, 1.0 / 24.0),
            Self::Substeps => (2.0, 40.0),
            Self::Iterations => (1.0, 12.0),
            Self::FramesPerStep | Self::CcEveryGpu => (1.0, 6.0),
            Self::CcIters => (1.0, 5.0),
            Self::ClothThickness => (0.5, 8.0),
            Self::CcFric => (0.0, 5.0),
            Self::Gravity => (0.0, 2.0),
            Self::Air => (0.0, 3.0),
            Self::SewSpeed => (200.0, 6000.0),
            Self::GrabSpeed => (100.0, 3000.0),
            Self::Settle => (2.0, 60.0),
            Self::MaxTime => (2.0, 120.0),
            Self::SelfCollision | Self::CcRepair | Self::Untangle | Self::FricModel | Self::AutoStop => {
                return None;
            }
        })
    }

    /// The value of this setting in `v`, as stored.
    #[must_use]
    pub fn get(self, v: &SimValues) -> Value {
        match self {
            Self::SelfCollision => Value::Bool(v.self_collision),
            Self::CcRepair => Value::Bool(v.cc_repair),
            Self::Untangle => Value::Bool(v.untangle),
            Self::AutoStop => Value::Bool(v.auto_stop),
            Self::FricModel => Value::String(
                match v.fric_model {
                    FrictionModel::Grip => "grip",
                    FrictionModel::Slide => "slide",
                }
                .to_owned(),
            ),
            _ => {
                let mut copy = *v;
                number(self.numeric_mut(&mut copy).map_or(f64::NAN, |x| *x))
            }
        }
    }

    /// Sets this setting in `v` from a stored value. Numbers are clamped to [`Self::limits`];
    /// other values are converted the way the stored format reads them.
    pub fn set(self, v: &mut SimValues, value: &Value) {
        match self {
            Self::SelfCollision => v.self_collision = not_false(value),
            Self::CcRepair => v.cc_repair = not_false(value),
            Self::Untangle => v.untangle = not_false(value),
            Self::AutoStop => v.auto_stop = truthy(value),
            Self::FricModel => {
                v.fric_model = if value.as_str() == Some("slide") {
                    FrictionModel::Slide
                } else {
                    FrictionModel::Grip
                };
            }
            _ => {
                let x = match (value.as_f64(), self.limits()) {
                    (Some(n), Some((lo, hi))) => clamp(n, lo, hi),
                    (Some(n), None) => n,
                    (None, _) => coerce_number(value),
                };
                if let Some(field) = self.numeric_mut(v) {
                    *field = x;
                }
            }
        }
    }

    fn numeric_mut(self, v: &mut SimValues) -> Option<&mut f64> {
        Some(match self {
            Self::PdScale => &mut v.pd_scale,
            Self::Dt => &mut v.dt,
            Self::Substeps => &mut v.substeps,
            Self::Iterations => &mut v.iterations,
            Self::FramesPerStep => &mut v.frames_per_step,
            Self::CcEveryGpu => &mut v.cc_every_gpu,
            Self::CcIters => &mut v.cc_iters,
            Self::ClothThickness => &mut v.cloth_thickness,
            Self::CcFric => &mut v.cc_fric,
            Self::Gravity => &mut v.gravity,
            Self::Air => &mut v.air,
            Self::SewSpeed => &mut v.sew_speed,
            Self::GrabSpeed => &mut v.grab_speed,
            Self::Settle => &mut v.settle,
            Self::MaxTime => &mut v.max_time,
            Self::SelfCollision | Self::CcRepair | Self::Untangle | Self::FricModel | Self::AutoStop => {
                return None;
            }
        })
    }
}

fn number(x: f64) -> Value {
    serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
}

/// Reads a non-numeric stored value as a number: `true` is 1, `false` and `null` are 0, numeric
/// text is parsed (empty text is 0), anything else is NaN.
fn coerce_number(value: &Value) -> f64 {
    match value {
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Null => 0.0,
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        Value::Array(_) | Value::Object(_) => f64::NAN,
    }
}

/// Switches stored as "on unless explicitly `false`".
fn not_false(value: &Value) -> bool {
    *value != Value::Bool(false)
}

/// Whether a stored value counts as set.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// A quality preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SimPreset {
    /// Coarser mesh, two simulated frames per redraw.
    Draft,
    /// The balanced default.
    #[default]
    Normal,
    /// Finer mesh and more solver work.
    Fine,
    /// More substeps, damping and sliding friction for garments worn over each other.
    Layered,
    /// Finest mesh and most solver work; stops by itself once settled.
    Final,
}

impl SimPreset {
    /// Every preset, in menu order.
    pub const ALL: [Self; 5] = [Self::Draft, Self::Normal, Self::Fine, Self::Layered, Self::Final];

    /// Stored name.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Normal => "normal",
            Self::Fine => "fine",
            Self::Layered => "layered",
            Self::Final => "final",
        }
    }

    /// The preset with stored name `key`.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    /// Display name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Normal => "Normal",
            Self::Fine => "Fine",
            Self::Layered => "Layered",
            Self::Final => "Final",
        }
    }

    /// One-line description for the settings panel.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Draft => {
                "Coarser mesh, two simulated frames per redraw. About twice as fast as Normal; for blocking out fit while you edit."
            }
            Self::Normal => "Balanced default for working on a garment.",
            Self::Fine => {
                "Finer mesh and more solver work: smaller wrinkles, tighter seams. About 1.5× the cost of Normal."
            }
            Self::Layered => {
                "More substeps and damping, and sliding friction between cloth, for garments worn over each other (robe + aba, coat over shirt): far fewer cloth pass-throughs."
            }
            Self::Final => {
                "Finest mesh and most solver work, for presentation and fit sign-off. About 3× the cost of Normal; stops by itself once settled."
            }
        }
    }

    /// The settings this preset changes from [`SIM_DEFAULTS`].
    #[must_use]
    pub fn changes(self) -> Vec<(SettingKey, Value)> {
        use SettingKey as K;
        let n = |x: f64| number(x);
        match self {
            Self::Draft => vec![
                (K::PdScale, n(1.3)),
                (K::Substeps, n(8.0)),
                (K::Iterations, n(2.0)),
                (K::FramesPerStep, n(2.0)),
            ],
            Self::Normal => Vec::new(),
            Self::Fine => vec![
                (K::PdScale, n(0.8)),
                (K::Substeps, n(14.0)),
                (K::Iterations, n(4.0)),
                (K::Air, n(0.5)),
            ],
            Self::Layered => vec![
                (K::Substeps, n(16.0)),
                (K::Air, n(0.6)),
                (K::FricModel, Value::String("slide".to_owned())),
            ],
            Self::Final => vec![
                (K::PdScale, n(0.65)),
                (K::Substeps, n(20.0)),
                (K::Iterations, n(5.0)),
                (K::Air, n(0.8)),
                (K::AutoStop, Value::Bool(true)),
                (K::MaxTime, n(30.0)),
            ],
        }
    }

    /// [`SIM_DEFAULTS`] with this preset's changes.
    #[must_use]
    pub fn values(self) -> SimValues {
        let mut v = SIM_DEFAULTS;
        for (k, value) in self.changes() {
            k.set(&mut v, &value);
        }
        v
    }
}

/// Settings resolved from a project.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSettings {
    /// The preset in use.
    pub preset: SimPreset,
    /// The values in effect: the preset's values with the overrides applied (and clamped).
    pub values: SimValues,
    /// The preset's values.
    pub base: SimValues,
    /// The settings that differ from the preset, by stored name, as stored.
    pub overrides: Map<String, Value>,
    /// Whether any setting differs from the preset.
    pub modified: bool,
}

/// Old quality names and the presets that replaced them.
const LEGACY_QUALITY: [(&str, SimPreset); 2] = [("fast", SimPreset::Draft), ("complete", SimPreset::Fine)];

/// Resolves a project's stored settings: picks the preset (also from the quality name older
/// versions wrote), then applies the overrides, including the loose `air`, `g` and
/// `selfCollision` fields older versions wrote.
#[must_use]
pub fn resolve_settings(s: &SimSettings) -> ResolvedSettings {
    let preset = s
        .preset
        .as_deref()
        .and_then(SimPreset::from_key)
        .or_else(|| {
            let q = s.quality.as_deref()?;
            LEGACY_QUALITY
                .iter()
                .find(|(k, _)| *k == q)
                .map(|(_, p)| *p)
                .or_else(|| SimPreset::from_key(q))
        })
        .unwrap_or_default();
    let base = preset.values();
    let mut over = Map::new();
    let differs = |k: SettingKey, value: &Value| !value.is_null() && !same_value(value, &k.get(&base));
    if let Some(air) = s.extra.get("air")
        && differs(SettingKey::Air, air)
    {
        over.insert(SettingKey::Air.key().to_owned(), air.clone());
    }
    if let Some(g) = s.extra.get("g")
        && differs(SettingKey::Gravity, g)
    {
        over.insert(SettingKey::Gravity.key().to_owned(), g.clone());
    }
    if s.self_collision == Some(false) {
        over.insert(SettingKey::SelfCollision.key().to_owned(), Value::Bool(false));
    }
    for (k, value) in s.over.iter().flatten() {
        if let Some(key) = SettingKey::from_key(k)
            && differs(key, value)
        {
            over.insert(k.clone(), value.clone());
        }
    }
    let mut values = base;
    for (k, value) in &over {
        if let Some(key) = SettingKey::from_key(k) {
            key.set(&mut values, value);
        }
    }
    ResolvedSettings {
        preset,
        modified: !over.is_empty(),
        values,
        base,
        overrides: over,
    }
}

/// Strict equality of stored values (numbers by value, no conversion between kinds).
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

impl ResolvedSettings {
    /// The stored settings after changing one setting: an override equal to the preset's value
    /// is dropped.
    #[must_use]
    pub fn with_override(&self, key: SettingKey, value: Value) -> SimSettings {
        let mut over = self.overrides.clone();
        if same_value(&value, &key.get(&self.base)) {
            over.remove(key.key());
        } else {
            over.insert(key.key().to_owned(), value);
        }
        stored(self.preset, over)
    }

    /// The stored settings with every override dropped.
    #[must_use]
    pub fn reset(&self) -> SimSettings {
        stored(self.preset, Map::new())
    }

    /// The stored settings after toggling cloth self-collision.
    #[must_use]
    pub fn toggle_self_collision(&self) -> SimSettings {
        self.with_override(
            SettingKey::SelfCollision,
            Value::Bool(!self.values.self_collision),
        )
    }
}

/// The stored settings selecting `preset` without overrides.
#[must_use]
pub fn preset_settings(preset: SimPreset) -> SimSettings {
    stored(preset, Map::new())
}

fn stored(preset: SimPreset, over: Map<String, Value>) -> SimSettings {
    SimSettings {
        preset: Some(preset.key().to_owned()),
        over: (!over.is_empty()).then_some(over),
        ..SimSettings::default()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "settings are copied, not computed")]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn overrides_are_clamped_and_equal_values_dropped() {
        let s: SimSettings = serde_json::from_value(json!({
            "preset": "draft",
            "over": { "substeps": 99, "pdScale": 1.3, "bogus": 1, "air": null }
        }))
        .expect("valid settings");
        let r = resolve_settings(&s);
        assert_eq!(r.preset, SimPreset::Draft);
        assert_eq!(r.values.substeps, 40.0);
        assert_eq!(r.overrides.len(), 1);
        assert!(r.modified);
        let back = r.with_override(SettingKey::Substeps, json!(8));
        assert!(back.over.is_none());
    }

    #[test]
    fn legacy_quality_maps_to_presets() {
        let s = SimSettings {
            quality: Some("complete".to_owned()),
            self_collision: Some(false),
            ..SimSettings::default()
        };
        let r = resolve_settings(&s);
        assert_eq!(r.preset, SimPreset::Fine);
        assert!(!r.values.self_collision);
    }
}
