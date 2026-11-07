//! Torso profile of the parametric avatar, as far as garment drafting needs it: landmark heights
//! and the elliptical torso cross-section at each height.
//!
//! Heights are measured up from the floor, in mm.

use crate::avatar::{Measurements, Sex};

/// Reference height the profile's absolute offsets are designed for, in mm.
const REFERENCE_HEIGHT_MM: f64 = 1700.0;
/// Spacing of the tabulated cross-sections, in mm.
const TABLE_STEP_MM: f64 = 2.0;
/// How far below the shoulder line the shoulder search runs before giving up, in mm.
const SHOULDER_SEARCH_DEPTH_MM: f64 = 60.0;
/// Underbust assumed below the bust when a body gives none, in mm.
const DEFAULT_UNDERBUST_DROP_MM: f64 = 110.0;

/// Landmark heights of a body, from the floor, in mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landmarks {
    /// Shoulder line.
    pub shoulder: f64,
    /// Base of the neck.
    pub neck: f64,
    /// Chin.
    pub chin: f64,
    /// Bust line.
    pub bust: f64,
    /// Underbust line.
    pub under: f64,
    /// Natural waist.
    pub waist: f64,
    /// High hip.
    pub high: f64,
    /// Hip line.
    pub hip: f64,
    /// Crotch.
    pub crotch: f64,
    /// Knee.
    pub knee: f64,
}

/// What garment templates need to know about the body they are drafted on.
///
/// The parametric [`TorsoProfile`] implements it; a scanned or imported avatar can supply its own.
pub trait BodyProfile {
    /// Height of the shoulder surface at horizontal distance `x` from the centre line, in mm.
    fn shoulder_y(&self, x: f64) -> f64;
    /// Landmark heights.
    fn landmarks(&self) -> Landmarks;
    /// Torso cross-section at height `y`: half width, half depth and forward offset, in mm.
    fn torso_at(&self, y: f64) -> [f64; 3];
    /// Lowest and highest height covered by [`BodyProfile::torso_at`].
    fn torso_range(&self) -> (f64, f64);
}

/// Torso of the parametric avatar, tabulated from smooth curves through key cross-sections.
#[derive(Clone, Debug, PartialEq)]
pub struct TorsoProfile {
    landmarks: Landmarks,
    y0: f64,
    y1: f64,
    /// Tabulated half width, half depth and forward offset. Stored in single precision, as the
    /// avatar's GPU tables are, so drafted patterns match those built from the same tables.
    table: Vec<[f32; 3]>,
}

/// Ellipse axes `[a, b]` with `b = k·a` whose perimeter (Ramanujan) is `girth`.
#[must_use]
pub fn ellipse_axes(girth: f64, k: f64) -> [f64; 2] {
    let p = std::f64::consts::PI * (3.0 * (1.0 + k) - ((3.0 + k) * (1.0 + 3.0 * k)).sqrt());
    let a = girth / p;
    [a, a * k]
}

impl TorsoProfile {
    /// The torso of a body with measurements `m`.
    #[must_use]
    pub fn new(m: &Measurements) -> Self {
        let h = m.height;
        let fem = m.sex == Sex::Female;
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
        let bust = ellipse_axes(m.bust, if fem { 0.70 } else { 0.66 });
        let underbust = if m.underbust == 0.0 {
            m.bust - DEFAULT_UNDERBUST_DROP_MM
        } else {
            m.underbust
        };
        let under = ellipse_axes(underbust, 0.72);
        let high = ellipse_axes(f64::midpoint(m.hip, m.waist), 0.74);
        let neck_r = m.neck / (2.0 * std::f64::consts::PI);
        let half_shoulder = m.shoulder / 2.0;
        // (height, half width, half depth, forward offset) of the key cross-sections.
        let sections: [[f64; 4]; 13] = [
            [y_crotch - 52.0 * s, hip[0] * 0.18, hip[1] * 0.2, -6.0],
            [y_crotch - 30.0 * s, hip[0] * 0.74, hip[1] * 0.62, -6.0],
            [y_crotch, hip[0] * 0.93, hip[1] * 0.86, -10.0],
            [y_hip, hip[0], hip[1], if fem { -20.0 } else { -14.0 }],
            [y_high, high[0], high[1], -10.0],
            [y_waist, waist[0], waist[1], -2.0],
            [y_under, under[0], under[1], 2.0],
            [y_bust, bust[0], bust[1], if fem { 14.0 } else { 6.0 }],
            [h * 0.768, bust[0] * 0.96, bust[1] * 0.92, 4.0],
            [y_shoulder - 32.0 * s, half_shoulder * 0.86, bust[1] * 0.78, -2.0],
            [y_shoulder, half_shoulder * 0.80, 74.0 * s, -10.0],
            [y_shoulder + 16.0 * s, neck_r * 2.25, 58.0 * s, -14.0],
            [y_neck, neck_r * 1.15, neck_r * 1.05, -14.0],
        ];
        let ys: Vec<f64> = sections.iter().map(|r| r[0]).collect();
        let curves: [Pchip; 3] = std::array::from_fn(|c| {
            let vs: Vec<f64> = sections.iter().map(|r| r[c + 1]).collect();
            Pchip::new(&ys, &vs)
        });
        let (y0, y1) = (ys[0], ys[ys.len() - 1]);
        let n = ((y1 - y0) / TABLE_STEP_MM).ceil() as usize + 1;
        let table = (0..n)
            .map(|i| {
                let y = y0 + i as f64 * TABLE_STEP_MM;
                curves.each_ref().map(|c| c.eval(y) as f32)
            })
            .collect();
        Self {
            landmarks: Landmarks {
                shoulder: y_shoulder,
                neck: y_neck,
                chin: y_chin,
                bust: y_bust,
                under: y_under,
                waist: y_waist,
                high: y_high,
                hip: y_hip,
                crotch: y_crotch,
                knee: h * 0.285,
            },
            y0,
            y1,
            table,
        }
    }
}

impl BodyProfile for TorsoProfile {
    fn shoulder_y(&self, x: f64) -> f64 {
        // Search down from the neck for the first height where the torso is at least `x` wide.
        let stop = self.landmarks.shoulder - SHOULDER_SEARCH_DEPTH_MM;
        let mut y = self.y1;
        while y > stop {
            if self.torso_at(y)[0] >= x {
                return y;
            }
            y -= TABLE_STEP_MM;
        }
        self.landmarks.shoulder
    }

    fn landmarks(&self) -> Landmarks {
        self.landmarks
    }

    fn torso_at(&self, y: f64) -> [f64; 3] {
        let last = self.table.len() as f64 - 1.0001;
        let f = ((y - self.y0) / TABLE_STEP_MM).clamp(0.0, last);
        let i = f.floor() as usize;
        let u = f - i as f64;
        let (a, b) = (self.table[i], self.table[i + 1]);
        std::array::from_fn(|c| {
            let (va, vb) = (f64::from(a[c]), f64::from(b[c]));
            va + (vb - va) * u
        })
    }

    fn torso_range(&self) -> (f64, f64) {
        (self.y0, self.y1)
    }
}

/// Monotone cubic (PCHIP) interpolation through knots.
struct Pchip {
    xs: Vec<f64>,
    ys: Vec<f64>,
    h: Vec<f64>,
    m: Vec<f64>,
}

impl Pchip {
    fn new(xs: &[f64], ys: &[f64]) -> Self {
        let n = xs.len();
        let h: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
        let d: Vec<f64> = (0..n - 1).map(|i| (ys[i + 1] - ys[i]) / h[i]).collect();
        let mut m = vec![0.0; n];
        m[0] = d[0];
        m[n - 1] = d[n - 2];
        for i in 1..n - 1 {
            m[i] = if d[i - 1] * d[i] <= 0.0 {
                0.0
            } else {
                let w1 = 2.0 * h[i] + h[i - 1];
                let w2 = h[i] + 2.0 * h[i - 1];
                (w1 + w2) / (w1 / d[i - 1] + w2 / d[i])
            };
        }
        Self {
            xs: xs.to_vec(),
            ys: ys.to_vec(),
            h,
            m,
        }
    }

    fn eval(&self, x: f64) -> f64 {
        let (xs, ys, h, m) = (&self.xs, &self.ys, &self.h, &self.m);
        let n = xs.len();
        if x <= xs[0] {
            return ys[0];
        }
        if x >= xs[n - 1] {
            return ys[n - 1];
        }
        let mut i = 0;
        while i < n - 2 && x > xs[i + 1] {
            i += 1;
        }
        let t = (x - xs[i]) / h[i];
        let t2 = t * t;
        let t3 = t2 * t;
        (2.0 * t3 - 3.0 * t2 + 1.0) * ys[i]
            + (t3 - 2.0 * t2 + t) * h[i] * m[i]
            + (-2.0 * t3 + 3.0 * t2) * ys[i + 1]
            + (t3 - t2) * h[i] * m[i + 1]
    }
}
