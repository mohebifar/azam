//! Floating-point helpers that reproduce the reference arithmetic bit for bit.
//!
//! Particle state is stored in single precision while every intermediate is computed in double
//! precision; [`nudge`] performs one such read-modify-write.

/// `2^63`, the first magnitude an `f64 → i64` cast no longer holds exactly.
const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;
/// `2^32`, the modulus of the 32-bit integer conversion.
const TWO_POW_32: f64 = 4_294_967_296.0;

/// Length of `(x, y, z)`: the square root of the Kahan-compensated sum of squares normalised by the
/// largest magnitude, times that magnitude (the reference formula, as in `azam_geom::hypot`).
pub(crate) fn hypot3(x: f64, y: f64, z: f64) -> f64 {
    let vals = [x.abs(), y.abs(), z.abs()];
    let mut max = 0.0_f64;
    let mut any_nan = false;
    for v in vals {
        if v.is_nan() {
            any_nan = true;
        } else if v > max {
            max = v;
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if any_nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for v in vals {
        let n = v / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `x`, or `fallback` when `x` is zero or NaN (the reference's `x || fallback`).
pub(crate) fn or_else(x: f64, fallback: f64) -> f64 {
    if x == 0.0 || x.is_nan() { fallback } else { x }
}

/// Rounds half-way cases up, towards positive infinity (the reference rounding).
pub(crate) fn round_half_up(x: f64) -> f64 {
    let r = x.ceil();
    if r - 0.5 > x { r - 1.0 } else { r }
}

/// Adds `d` to a single-precision value, computing in double precision and rounding the result
/// to nearest.
#[inline]
pub(crate) fn nudge(v: &mut f32, d: f64) {
    *v = (f64::from(*v) + d) as f32;
}

/// Converts to a 32-bit integer modulo `2^32` (NaN and infinities give 0).
pub(crate) fn to_int32(v: f64) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    let t = v.trunc();
    if t.abs() < TWO_POW_63 {
        return t as i64 as i32;
    }
    (t % TWO_POW_32) as i64 as i32
}

/// Spatial hash of integer cell coordinates (given as floats), before masking.
pub(crate) fn cell_hash(i: f64, j: f64, k: f64) -> i32 {
    to_int32(i * 73_856_093.0) ^ to_int32(j * 19_349_663.0) ^ to_int32(k * 83_492_791.0)
}

/// Bucket of a spatial hash value in a table of `buckets` entries (a power of two): its low bits.
pub(crate) fn bucket(h: i32, buckets: usize) -> usize {
    (h as u32 as usize) & (buckets - 1)
}

/// End marker of the linked lists of the spatial hashes.
pub(crate) const NO_ENTRY: u32 = u32::MAX;

/// Clamps `v` to `[lo, hi]` without the NaN panic of `f64::clamp` (NaN stays NaN).
pub(crate) fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Smallest of three values.
pub(crate) fn min3(a: f64, b: f64, c: f64) -> f64 {
    a.min(b).min(c)
}

/// Largest of three values.
pub(crate) fn max3(a: f64, b: f64, c: f64) -> f64 {
    a.max(b).max(c)
}

/// The three coordinates of particle `q` in a flat `x, y, z` buffer, widened.
#[inline]
pub(crate) fn load3(x: &[f32], q: usize) -> [f64; 3] {
    let k = q * 3;
    [f64::from(x[k]), f64::from(x[k + 1]), f64::from(x[k + 2])]
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "these tests check exact results")]
mod tests {
    use super::*;

    #[test]
    fn hypot3_matches_simple_cases() {
        assert_eq!(hypot3(2.0, -3.0, 6.0), 7.0);
        assert_eq!(hypot3(0.0, 0.0, 0.0), 0.0);
        assert!(hypot3(1.0, f64::NAN, 0.0).is_nan());
        assert_eq!(hypot3(f64::NAN, f64::NEG_INFINITY, 0.0), f64::INFINITY);
    }

    #[test]
    fn rounding_and_int_conversion() {
        assert_eq!(round_half_up(2.5), 3.0);
        assert_eq!(round_half_up(-2.5), -2.0);
        assert_eq!(round_half_up(0.499_999_999_999_999_94), 0.0);
        assert_eq!(to_int32(4_294_967_297.0), 1);
        assert_eq!(to_int32(-1.0), -1);
        assert_eq!(to_int32(f64::NAN), 0);
        assert_eq!(cell_hash(1.0, 0.0, 0.0), 73_856_093);
    }

    #[test]
    fn nudge_rounds_once() {
        let mut v = 1.0_f32;
        nudge(&mut v, 1e-9);
        assert_eq!(v, 1.0);
        nudge(&mut v, 0.5);
        assert_eq!(v, 1.5);
    }
}
