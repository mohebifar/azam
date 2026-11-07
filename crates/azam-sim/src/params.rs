//! Solver parameters derived from resolved settings.

use crate::numeric::round_half_up;
use crate::settings::{FrictionModel, ResolvedSettings, SimPreset};

/// Standard gravity in mm/s².
pub const STANDARD_GRAVITY_MM_S2: f64 = 9810.0;
/// Cap on the cloth-on-cloth friction coefficient with the grip model.
const GRIP_MU_CAP: f64 = 0.8;
/// Cap on the cloth-on-cloth friction coefficient with the sliding model.
const SLIDE_MU_CAP: f64 = 1.5;
/// Share of the friction cone a sliding contact may remove once it slips.
const KINETIC_FRACTION: f64 = 0.8;
/// Default cloth-contact re-detection interval of the triangle collision path, in substeps.
const DEFAULT_TRIANGLE_DETECT_EVERY: u32 = 4;
/// Default extra search distance of the triangle collision path, in mm.
const DEFAULT_CONTACT_SLACK_MM: f64 = 10.0;

/// Parameters of one simulation.
#[allow(clippy::struct_excessive_bools, reason = "independent solver switches")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimParams {
    /// Multiplier of every piece's particle distance.
    pub pd_scale: f64,
    /// Solver substeps per frame.
    pub substeps: u32,
    /// Stretch and sewing iterations per substep.
    pub iterations: u32,
    /// Simulated frames per call of [`crate::Simulation::advance`] (clamped to 1–6 there).
    pub frames_per_step: u32,
    /// Gravity in mm/s².
    pub gravity: f64,
    /// Air damping, per second.
    pub air: f64,
    /// Speed at which open seams close, in mm/s.
    pub sew_speed: f64,
    /// Fastest the grab point moves, in mm/s.
    pub grab_speed: f64,
    /// Cloth-cloth collision.
    pub self_collision: bool,
    /// Contact re-detection interval of the accelerated solver, in substeps.
    pub cc_every_gpu: u32,
    /// Contact passes per substep of the accelerated solver.
    pub cc_iters: u32,
    /// Distance kept between cloth surfaces, in mm.
    pub cloth_thickness: f64,
    /// Cloth-on-cloth friction as a multiple of the fabric friction.
    pub cc_fric: f64,
    /// Pull crossed layers back to their own side.
    pub cc_repair: bool,
    /// Run the untangler.
    pub untangle: bool,
    /// Cloth-on-cloth friction model.
    pub friction_model: FrictionModel,
    /// Cap on the cloth-on-cloth friction coefficient.
    pub cc_mu_cap: f64,
    /// Use vertex-triangle cloth contacts on the CPU instead of the particle-sphere contacts.
    pub cpu_triangle_collision: bool,
    /// Re-detect vertex-triangle contacts every this many substeps.
    pub triangle_detect_every: u32,
    /// Extra search distance of vertex-triangle contact detection, in mm.
    pub contact_slack: f64,
}

impl Default for SimParams {
    fn default() -> Self {
        Self::from(&crate::settings::ResolvedSettings {
            preset: SimPreset::Normal,
            values: SimPreset::Normal.values(),
            base: SimPreset::Normal.values(),
            overrides: serde_json::Map::new(),
            modified: false,
        })
    }
}

/// Rounds a setting to a count (NaN and negatives give 0).
fn count(x: f64) -> u32 {
    round_half_up(x) as u32
}

impl From<&ResolvedSettings> for SimParams {
    fn from(st: &ResolvedSettings) -> Self {
        let v = &st.values;
        let slide = v.fric_model == FrictionModel::Slide;
        Self {
            pd_scale: v.pd_scale,
            substeps: count(v.substeps),
            iterations: count(v.iterations),
            frames_per_step: count(v.frames_per_step),
            gravity: STANDARD_GRAVITY_MM_S2 * v.gravity,
            air: v.air,
            sew_speed: v.sew_speed,
            grab_speed: v.grab_speed,
            self_collision: v.self_collision,
            cc_every_gpu: count(v.cc_every_gpu),
            cc_iters: count(v.cc_iters),
            cloth_thickness: v.cloth_thickness,
            cc_fric: v.cc_fric,
            cc_repair: v.cc_repair,
            untangle: v.untangle,
            friction_model: v.fric_model,
            cc_mu_cap: if slide { SLIDE_MU_CAP } else { GRIP_MU_CAP },
            cpu_triangle_collision: false,
            triangle_detect_every: DEFAULT_TRIANGLE_DETECT_EVERY,
            contact_slack: DEFAULT_CONTACT_SLACK_MM,
        }
    }
}

impl SimParams {
    /// Fraction of a touching contact's tangential slip removed this substep.
    ///
    /// `mu0` is the mean fabric friction of the two contacts, `dn` the normal correction and `tl`
    /// the tangential slip length. With [`FrictionModel::Grip`] it is the capped coefficient; with
    /// [`FrictionModel::Slide`] the whole slip is removed inside the friction cone and a bounded
    /// part of it outside.
    #[must_use]
    pub fn friction_fraction(&self, mu0: f64, dn: f64, tl: f64) -> f64 {
        let mu = self.cc_mu_cap.min(mu0 * self.cc_fric);
        if self.friction_model != FrictionModel::Slide {
            return mu;
        }
        if tl > mu * dn {
            (KINETIC_FRACTION * mu * dn / tl).min(1.0)
        } else {
            1.0
        }
    }

    /// Simulated frames per [`crate::Simulation::advance`] call, clamped to 1–6.
    #[must_use]
    pub fn frames_per_advance(&self) -> u32 {
        if self.frames_per_step == 0 {
            1
        } else {
            self.frames_per_step.min(6)
        }
    }
}
