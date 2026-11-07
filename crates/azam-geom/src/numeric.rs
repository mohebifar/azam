//! Floating-point helpers that reproduce the reference arithmetic bit for bit.

/// Length of the vector `(x, y)`, computed as the square root of the Kahan-compensated sum of the
/// squares normalised by the larger magnitude, times that magnitude.
///
/// This is the formula of the reference implementation the golden data was recorded with.
/// `f64::hypot` is more accurate but differs from it in the last bit for about a third of inputs,
/// which shifts lattice points by an ulp and can reorder the Delaunay sweep.
#[must_use]
pub fn hypot(x: f64, y: f64) -> f64 {
    let (x, y) = (x.abs(), y.abs());
    if x.is_nan() || y.is_nan() {
        return if x == f64::INFINITY || y == f64::INFINITY {
            f64::INFINITY
        } else {
            f64::NAN
        };
    }
    let max = x.max(y);
    if max == f64::INFINITY || max == 0.0 {
        return max;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for v in [x, y] {
        let n = v / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "these tests check exact results")]
mod tests {
    use super::*;

    #[test]
    fn exact_cases() {
        assert_eq!(hypot(3.0, -4.0), 5.0);
        assert_eq!(hypot(0.0, 0.0), 0.0);
        assert_eq!(hypot(f64::NEG_INFINITY, f64::NAN), f64::INFINITY);
        assert!(hypot(1.0, f64::NAN).is_nan());
    }

    #[test]
    fn close_to_the_correctly_rounded_length() {
        let (x, y) = (123.456, -789.012);
        assert!((hypot(x, y) - x.hypot(y)).abs() <= 2.0 * f64::EPSILON * x.hypot(y));
    }
}
