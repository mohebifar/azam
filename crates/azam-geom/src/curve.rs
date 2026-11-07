//! Bézier evaluation, flattening and arc-length lookup on polylines.

use crate::Vec2;

/// Fewest line steps used to flatten a curved segment.
const MIN_FLATTEN_STEPS: usize = 10;
/// Most line steps used to flatten a curved segment.
const MAX_FLATTEN_STEPS: usize = 240;
/// Target length of one flattening step, in mm of control-polygon length.
const FLATTEN_STEP_MM: f64 = 3.0;
/// Lengths below this are treated as zero.
const LENGTH_EPSILON: f64 = 1e-9;

/// Evaluates a Bézier curve of any degree at `t` in `[0, 1]` (de Casteljau).
///
/// # Panics
/// Panics if `ctrl` is empty.
#[must_use]
pub fn bezier_point(ctrl: &[Vec2], t: f64) -> Vec2 {
    let mut p = ctrl.to_vec();
    for round in 1..p.len() {
        for i in 0..p.len() - round {
            let step = (p[i + 1] - p[i]) * t;
            p[i] += step;
        }
    }
    p[0]
}

/// Flattens a Bézier segment into a polyline that starts and ends on its corners.
///
/// A straight segment (two points) stays two points. A curved one gets one step per 3 mm of
/// control-polygon length, clamped to 10–240 steps.
#[must_use]
pub fn flatten_bezier(ctrl: &[Vec2]) -> Vec<Vec2> {
    if ctrl.len() <= 2 {
        return ctrl.to_vec();
    }
    let polygon_len: f64 = ctrl.windows(2).map(|w| distance(w[0], w[1])).sum();
    let steps = ((polygon_len / FLATTEN_STEP_MM).ceil() as usize).clamp(MIN_FLATTEN_STEPS, MAX_FLATTEN_STEPS);
    (0..=steps)
        .map(|i| bezier_point(ctrl, i as f64 / steps as f64))
        .collect()
}

/// Running arc length at each vertex of a polyline; the first entry is 0.
#[must_use]
pub fn cumulative_lengths(poly: &[Vec2]) -> Vec<f64> {
    let mut cum = Vec::with_capacity(poly.len());
    let mut total = 0.0;
    cum.push(0.0);
    for w in poly.windows(2) {
        total += distance(w[0], w[1]);
        cum.push(total);
    }
    cum
}

/// A point on a polyline with the direction of the edge it lies on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolyPoint {
    /// Position in mm.
    pub pos: Vec2,
    /// Unit direction of the edge the point lies on (zero for a degenerate edge).
    pub tangent: Vec2,
}

/// The point at arc length `len` along a polyline, given its [`cumulative_lengths`].
///
/// Lengths outside the polyline are clamped to its ends.
///
/// # Panics
/// Panics if the polyline has fewer than two points.
#[must_use]
pub fn point_at_length(poly: &[Vec2], cum: &[f64], len: f64) -> PolyPoint {
    let last = cum.len() - 1;
    let (lo, hi) = if len <= 0.0 {
        (0, 1)
    } else if len >= cum[last] {
        (last - 1, last)
    } else {
        // Largest `lo` with cum[lo] <= len.
        let lo = cum.partition_point(|&c| c <= len) - 1;
        (lo, lo + 1)
    };
    let edge_len = cum[hi] - cum[lo];
    let u = if edge_len > LENGTH_EPSILON {
        ((len - cum[lo]) / edge_len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (a, b) = (poly[lo], poly[hi]);
    let divisor = if edge_len == 0.0 { 1.0 } else { edge_len };
    PolyPoint {
        pos: a + (b - a) * u,
        tangent: (b - a) / divisor,
    }
}

/// Euclidean distance, using `hypot` so results match the golden data.
pub(crate) fn distance(a: Vec2, b: Vec2) -> f64 {
    crate::hypot(b.x - a.x, b.y - a.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_segment_is_not_subdivided() {
        let line = [Vec2::new(0.0, 0.0), Vec2::new(30.0, 40.0)];
        assert_eq!(flatten_bezier(&line), line.to_vec());
        assert_eq!(cumulative_lengths(&line), vec![0.0, 50.0]);
    }

    #[test]
    fn quadratic_midpoint() {
        let ctrl = [Vec2::new(0.0, 0.0), Vec2::new(50.0, 100.0), Vec2::new(100.0, 0.0)];
        assert_eq!(bezier_point(&ctrl, 0.5), Vec2::new(50.0, 50.0));
    }

    #[test]
    fn point_at_length_clamps_and_interpolates() {
        let poly = [Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0)];
        let cum = cumulative_lengths(&poly);
        assert_eq!(point_at_length(&poly, &cum, -5.0).pos, Vec2::ZERO);
        assert_eq!(point_at_length(&poly, &cum, 15.0).pos, Vec2::new(10.0, 5.0));
        assert_eq!(point_at_length(&poly, &cum, 15.0).tangent, Vec2::new(0.0, 1.0));
        assert_eq!(point_at_length(&poly, &cum, 99.0).pos, Vec2::new(10.0, 10.0));
    }
}
