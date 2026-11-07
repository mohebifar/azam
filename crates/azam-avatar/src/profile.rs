//! Torso profile: elliptical torso cross-sections by height, body landmark heights and the
//! shoulder line. Pattern drafting and piece arrangement both read it.

use crate::math::{Pchip, clamp, ellipse_axes};
use crate::params::AvatarParams;

/// Reference height the parametric proportions are designed for, in millimetres.
pub const REFERENCE_HEIGHT_MM: f64 = 1700.0;
/// Height step of the parametric torso table, in millimetres.
const PARAMETRIC_TORSO_STEP_MM: f64 = 2.0;
/// Distance below the shoulder line where the shoulder search gives up, in millimetres.
const SHOULDER_SEARCH_DEPTH_MM: f64 = 60.0;

/// Horizontal cross-section of the torso at one height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TorsoSlice {
    /// Half width (x semi-axis).
    pub half_width: f64,
    /// Half depth (z semi-axis).
    pub half_depth: f64,
    /// Z of the section centre.
    pub center_z: f64,
}

/// Torso cross-sections sampled at a fixed height step, linearly interpolated in between.
#[derive(Clone, Debug, PartialEq)]
pub struct TorsoTable {
    /// Height of the first row.
    pub y0: f64,
    /// Height of the topmost knot (the table may extend slightly past it).
    pub y1: f64,
    /// Height step between rows.
    pub step: f64,
    /// Half width per row.
    pub half_width: Vec<f32>,
    /// Half depth per row.
    pub half_depth: Vec<f32>,
    /// Section centre z per row.
    pub center_z: Vec<f32>,
}

impl TorsoTable {
    /// Number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.half_width.len()
    }

    /// Whether the table has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.half_width.is_empty()
    }

    /// Cross-section at height `y`, clamped to the table.
    #[must_use]
    pub fn eval(&self, y: f64) -> TorsoSlice {
        let n = self.len();
        if n < 2 {
            let at = |v: &[f32]| v.first().map_or(0.0, |&x| f64::from(x));
            return TorsoSlice {
                half_width: at(&self.half_width),
                half_depth: at(&self.half_depth),
                center_z: at(&self.center_z),
            };
        }
        let f = clamp((y - self.y0) / self.step, 0.0, n as f64 - 1.0001);
        let i = (f.floor() as usize).min(n - 2);
        let u = f - i as f64;
        let lerp = |v: &[f32]| {
            let (a, b) = (f64::from(v[i]), f64::from(v[i + 1]));
            a + (b - a) * u
        };
        TorsoSlice {
            half_width: lerp(&self.half_width),
            half_depth: lerp(&self.half_depth),
            center_z: lerp(&self.center_z),
        }
    }
}

/// Heights of body landmarks above the floor, in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyLevels {
    /// Shoulder point.
    pub shoulder: f64,
    /// Neck base.
    pub neck: f64,
    /// Chin.
    pub chin: f64,
    /// Bust line.
    pub bust: f64,
    /// Underbust line.
    pub underbust: f64,
    /// Waist line.
    pub waist: f64,
    /// High hip, between waist and hip.
    pub high_hip: f64,
    /// Hip line.
    pub hip: f64,
    /// Crotch.
    pub crotch: f64,
    /// Knee.
    pub knee: f64,
}

/// Where the shoulder line comes from.
#[derive(Clone, Debug, PartialEq)]
enum ShoulderLine {
    /// Search down the torso table from its top for the first section at least as wide.
    Table,
    /// Highest of these `(|x|, y)` surface samples within a horizontal window.
    Samples(Vec<(f64, f64)>),
}

/// Half width of the window used to pick shoulder samples, in millimetres.
const SHOULDER_SAMPLE_WINDOW_MM: f64 = 7.0;

/// The body shape seen by pattern drafting and piece arrangement.
#[derive(Clone, Debug, PartialEq)]
pub struct TorsoProfile {
    /// The resolved avatar settings.
    pub params: AvatarParams,
    /// Body height.
    pub height: f64,
    /// Height relative to [`REFERENCE_HEIGHT_MM`].
    pub scale: f64,
    /// Whether the body is female.
    pub female: bool,
    /// Torso cross-sections.
    pub torso: TorsoTable,
    /// Hip ellipse semi-axes `[x, z]`.
    pub hip_axes: [f64; 2],
    /// Neck radius.
    pub neck_radius: f64,
    /// Half the shoulder width.
    pub half_shoulder: f64,
    /// Landmark heights.
    pub levels: BodyLevels,
    shoulder_line: ShoulderLine,
}

/// One knot of the parametric torso: height, half width, half depth, centre z.
type TorsoKnot = [f64; 4];

impl TorsoProfile {
    /// The parametric torso for `params`: elliptical sections through measured girths at standard
    /// landmark heights, interpolated monotonically.
    #[must_use]
    pub fn parametric(params: &AvatarParams) -> Self {
        let m = params;
        let h = m.height;
        let female = m.is_female();
        let s = h / REFERENCE_HEIGHT_MM;
        let y_shoulder = h * 0.818;
        let y_neck = h * 0.848;
        let y_chin = h * 0.872;
        let y_bust = h * 0.722;
        let y_under = h * 0.683;
        let y_waist = h * 0.625;
        let y_high = h * 0.578;
        let y_hip = h * 0.527;
        let y_crotch = m.crotch.min(h * 0.5);
        let hip = ellipse_axes(m.hip, 0.76);
        let waist = ellipse_axes(m.waist, 0.74);
        let bust = ellipse_axes(m.bust, if female { 0.70 } else { 0.66 });
        let under = ellipse_axes(m.underbust_or_default(), 0.72);
        let high = ellipse_axes(f64::midpoint(m.hip, m.waist), 0.74);
        let neck_radius = m.neck / (2.0 * std::f64::consts::PI);
        let half_shoulder = m.shoulder / 2.0;
        let knots: [TorsoKnot; 13] = [
            [y_crotch - 52.0 * s, hip[0] * 0.18, hip[1] * 0.2, -6.0],
            [y_crotch - 30.0 * s, hip[0] * 0.74, hip[1] * 0.62, -6.0],
            [y_crotch, hip[0] * 0.93, hip[1] * 0.86, -10.0],
            [y_hip, hip[0], hip[1], if female { -20.0 } else { -14.0 }],
            [y_high, high[0], high[1], -10.0],
            [y_waist, waist[0], waist[1], -2.0],
            [y_under, under[0], under[1], 2.0],
            [y_bust, bust[0], bust[1], if female { 14.0 } else { 6.0 }],
            [h * 0.768, bust[0] * 0.96, bust[1] * 0.92, 4.0],
            [y_shoulder - 32.0 * s, half_shoulder * 0.86, bust[1] * 0.78, -2.0],
            [y_shoulder, half_shoulder * 0.80, 74.0 * s, -10.0],
            [y_shoulder + 16.0 * s, neck_radius * 2.25, 58.0 * s, -14.0],
            [y_neck, neck_radius * 1.15, neck_radius * 1.05, -14.0],
        ];
        let ys = knots.map(|k| k[0]);
        let column = |c: usize| Pchip::new(&ys, &knots.map(|k| k[c]));
        let (fa, fb, fc) = (column(1), column(2), column(3));
        let (y0, y1) = (ys[0], ys[12]);
        let step = PARAMETRIC_TORSO_STEP_MM;
        let rows = ((y1 - y0) / step).ceil() as usize + 1;
        let mut torso = TorsoTable {
            y0,
            y1,
            step,
            half_width: Vec::with_capacity(rows),
            half_depth: Vec::with_capacity(rows),
            center_z: Vec::with_capacity(rows),
        };
        for i in 0..rows {
            let y = y0 + i as f64 * step;
            torso.half_width.push(fa.eval(y) as f32);
            torso.half_depth.push(fb.eval(y) as f32);
            torso.center_z.push(fc.eval(y) as f32);
        }
        Self {
            params: params.clone(),
            height: h,
            scale: s,
            female,
            torso,
            hip_axes: hip,
            neck_radius,
            half_shoulder,
            levels: BodyLevels {
                shoulder: y_shoulder,
                neck: y_neck,
                chin: y_chin,
                bust: y_bust,
                underbust: y_under,
                waist: y_waist,
                high_hip: y_high,
                hip: y_hip,
                crotch: y_crotch,
                knee: h * 0.285,
            },
            shoulder_line: ShoulderLine::Table,
        }
    }

    /// A profile measured from a morph-target body. `shoulder_samples` are `(|x|, y)` points of the
    /// upper torso surface used to find the shoulder line.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn measured(
        params: AvatarParams,
        height: f64,
        torso: TorsoTable,
        hip_axes: [f64; 2],
        neck_radius: f64,
        half_shoulder: f64,
        levels: BodyLevels,
        shoulder_samples: Vec<(f64, f64)>,
    ) -> Self {
        Self {
            female: params.is_female(),
            params,
            height,
            scale: height / REFERENCE_HEIGHT_MM,
            torso,
            hip_axes,
            neck_radius,
            half_shoulder,
            levels,
            shoulder_line: ShoulderLine::Samples(shoulder_samples),
        }
    }

    /// Height of the shoulder surface at horizontal distance `x` from the centre line.
    #[must_use]
    pub fn shoulder_y(&self, x: f64) -> f64 {
        match &self.shoulder_line {
            ShoulderLine::Table => {
                let stop = self.levels.shoulder - SHOULDER_SEARCH_DEPTH_MM;
                let mut y = self.torso.y1;
                while y > stop {
                    if self.torso.eval(y).half_width >= x {
                        return y;
                    }
                    y -= self.torso.step;
                }
                self.levels.shoulder
            }
            ShoulderLine::Samples(samples) => {
                let best = samples
                    .iter()
                    .filter(|(px, _)| (px - x).abs() <= SHOULDER_SAMPLE_WINDOW_MM)
                    .map(|&(_, y)| y)
                    .fold(f64::NEG_INFINITY, f64::max);
                if best > f64::NEG_INFINITY {
                    best
                } else {
                    self.levels.shoulder
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::params::Sex;

    #[test]
    fn torso_follows_the_measured_girths() {
        let p = TorsoProfile::parametric(&AvatarParams::preset(Sex::Female));
        let waist = p.torso.eval(p.levels.waist);
        let perimeter = crate::math::ellipse_perimeter(waist.half_width, waist.half_depth);
        assert!((perimeter - 700.0).abs() < 1.0, "{perimeter}");
        assert!(p.shoulder_y(0.0) >= p.levels.neck - 1.0);
        assert_eq!(p.shoulder_y(1e4), p.levels.shoulder);
    }
}
