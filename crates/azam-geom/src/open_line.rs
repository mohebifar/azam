//! Open polylines with curve points, used for internal lines (darts, folds, placement marks).

use crate::curve::flatten_bezier;
use crate::{OutlinePoint, Vec2};

/// Flattens an open line. Curve points are Bézier controls between the corners around them; the
/// first and last points are always corners.
#[must_use]
pub fn flatten_open_line(points: &[OutlinePoint]) -> Vec<Vec2> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut out: Vec<Vec2> = Vec::new();
    let mut span_start = 0;
    for i in 1..points.len() {
        if !points[i].curve || i == points.len() - 1 {
            let ctrl: Vec<Vec2> = points[span_start..=i].iter().map(|p| p.pos).collect();
            let skip = usize::from(!out.is_empty());
            out.extend(flatten_bezier(&ctrl).into_iter().skip(skip));
            span_start = i;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_line_keeps_its_corners() {
        let pts = [
            OutlinePoint::corner("a", 0.0, 0.0),
            OutlinePoint::corner("b", 10.0, 0.0),
            OutlinePoint::corner("c", 10.0, 10.0),
        ];
        assert_eq!(
            flatten_open_line(&pts),
            vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0)]
        );
    }

    #[test]
    fn too_short_is_empty() {
        assert!(flatten_open_line(&[OutlinePoint::corner("a", 0.0, 0.0)]).is_empty());
    }
}
