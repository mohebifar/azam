//! Floating-point helpers that reproduce the reference arithmetic bit for bit.

use azam_geom::Vec2;

pub(crate) use azam_geom::hypot;

/// Running arc length at each vertex of a polyline (the first entry is 0), summing edge lengths
/// with [`hypot`].
pub(crate) fn arc_lengths(poly: &[Vec2]) -> Vec<f64> {
    azam_geom::cumulative_lengths(poly)
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "these tests check exact results")]
mod tests {
    use super::*;

    #[test]
    fn arc_lengths_accumulate() {
        let poly = [Vec2::ZERO, Vec2::new(3.0, 4.0), Vec2::new(3.0, 10.0)];
        assert_eq!(arc_lengths(&poly), vec![0.0, 5.0, 11.0]);
        assert_eq!(arc_lengths(&[]), vec![0.0]);
    }
}
