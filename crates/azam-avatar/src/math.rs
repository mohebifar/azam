//! Numeric helpers: ellipse sizing, monotone interpolation, smooth minimum and robust norms.

use std::f64::consts::PI;

/// Radians per degree.
pub const DEG: f64 = PI / 180.0;

/// Clamps `v` to `[lo, hi]` (`lo` wins if the range is empty).
pub(crate) fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Sign of `x`: -1, 1, or `x` itself for zero and NaN.
pub(crate) fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// Rounds to the nearest integer; halves round towards positive infinity.
pub(crate) fn round_half_up(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// Euclidean norm of `values`, computed as a scaled, compensated sum of squares so it neither
/// overflows nor loses precision for very different magnitudes.
pub(crate) fn norm_of(values: &[f64]) -> f64 {
    let mut max = 0.0_f64;
    let mut any_nan = false;
    for &v in values {
        if v.is_nan() {
            any_nan = true;
        } else if v.abs() > max {
            max = v.abs();
        }
    }
    if max.is_infinite() {
        return f64::INFINITY;
    }
    if any_nan {
        return f64::NAN;
    }
    if max <= 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for &v in values {
        let n = v.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// Length of the 2D vector `(a, b)`.
pub(crate) fn hypot2(a: f64, b: f64) -> f64 {
    norm_of(&[a, b])
}

/// Length of the 3D vector `(a, b, c)`.
pub(crate) fn hypot3(a: f64, b: f64, c: f64) -> f64 {
    norm_of(&[a, b, c])
}

/// Semi-axes `[a, b]` of an ellipse with circumference `circumference` and axis ratio
/// `ratio = b / a` (Ramanujan's perimeter approximation).
#[must_use]
pub fn ellipse_axes(circumference: f64, ratio: f64) -> [f64; 2] {
    let k = ratio;
    let p = PI * (3.0 * (1.0 + k) - ((3.0 + k) * (1.0 + 3.0 * k)).sqrt());
    let a = circumference / p;
    [a, a * k]
}

/// Perimeter of an ellipse with semi-axes `a` and `b` (Ramanujan's approximation).
#[must_use]
pub fn ellipse_perimeter(a: f64, b: f64) -> f64 {
    PI * (3.0 * (a + b) - ((3.0 * a + b) * (a + 3.0 * b)).sqrt())
}

/// Polynomial smooth minimum of two distances with blend radius `k` (> 0).
#[must_use]
pub fn smooth_min(a: f64, b: f64, k: f64) -> f64 {
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// Monotone piecewise cubic Hermite interpolation (PCHIP) through a set of knots.
///
/// Outside the knot range the end values are held.
#[derive(Clone, Debug, PartialEq)]
pub struct Pchip {
    xs: Vec<f64>,
    ys: Vec<f64>,
    h: Vec<f64>,
    m: Vec<f64>,
}

impl Pchip {
    /// Builds the interpolant through `(xs[i], ys[i])`. The `xs` must increase. Extra values in
    /// the longer slice are ignored; with no knots the interpolant is zero, with one it is constant.
    #[must_use]
    pub fn new(xs: &[f64], ys: &[f64]) -> Self {
        let n = xs.len().min(ys.len());
        let (xs, ys) = (xs[..n].to_vec(), ys[..n].to_vec());
        let mut h = Vec::with_capacity(n.saturating_sub(1));
        let mut d = Vec::with_capacity(n.saturating_sub(1));
        for i in 0..n.saturating_sub(1) {
            h.push(xs[i + 1] - xs[i]);
            d.push((ys[i + 1] - ys[i]) / h[i]);
        }
        let mut m = vec![0.0; n];
        if n >= 2 {
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
        }
        Self { xs, ys, h, m }
    }

    /// Interpolated value at `x`.
    #[must_use]
    pub fn eval(&self, x: f64) -> f64 {
        let n = self.xs.len();
        match n {
            0 => return 0.0,
            1 => return self.ys[0],
            _ => {}
        }
        let (xs, ys, h, m) = (&self.xs, &self.ys, &self.h, &self.m);
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

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn ellipse_axes_invert_the_perimeter() {
        let [a, b] = ellipse_axes(900.0, 0.7);
        assert!((ellipse_perimeter(a, b) - 900.0).abs() < 1e-9);
        assert!((b / a - 0.7).abs() < 1e-12);
    }

    #[test]
    fn pchip_is_monotone_and_holds_ends() {
        let p = Pchip::new(&[0.0, 1.0, 2.0, 4.0], &[0.0, 1.0, 1.0, 3.0]);
        assert_eq!(p.eval(-1.0), 0.0);
        assert_eq!(p.eval(9.0), 3.0);
        let mut last = p.eval(0.0);
        for i in 1..=400 {
            let v = p.eval(f64::from(i) * 0.01);
            assert!(v >= last - 1e-12);
            last = v;
        }
        assert_eq!(Pchip::new(&[], &[]).eval(1.0), 0.0);
        assert_eq!(Pchip::new(&[1.0], &[5.0]).eval(0.0), 5.0);
    }

    #[test]
    fn norm_matches_naive_length_and_handles_extremes() {
        assert!((hypot3(3.0, 4.0, 12.0) - 13.0).abs() < 1e-12);
        assert_eq!(hypot2(0.0, 0.0), 0.0);
        assert!(hypot2(1e300, 1e300).is_finite());
        assert!(hypot2(f64::NAN, 1.0).is_nan());
        assert_eq!(hypot2(f64::INFINITY, f64::NAN), f64::INFINITY);
    }

    #[test]
    fn rounding_and_sign_follow_their_contracts() {
        assert_eq!(round_half_up(-0.5), 0.0);
        assert_eq!(round_half_up(2.5), 3.0);
        assert_eq!(round_half_up(0.499_999_999_999_999_94), 0.0);
        assert_eq!(sign(0.0), 0.0);
        assert_eq!(sign(-3.0), -1.0);
        assert!((smooth_min(1.0, 1.0, 4.0) - 0.0).abs() < 1e-12);
    }
}
