//! Edits to one segment of an outline: splitting, adding curve points and reshaping.

use azam_geom::{Outline, OutlinePoint, PointId, Segment, Vec2};

use crate::ids::{IdGen, prefix};
use crate::num::js_first_key;
use crate::piece::Piece;

/// Bézier parameter at fraction `t_arc` of a segment's arc length.
///
/// The flattened polyline samples the curve at evenly spaced parameters, so the parameter is read
/// off the polyline vertex the arc length falls in.
#[must_use]
pub fn seg_bez_param(seg: &Segment, t_arc: f64) -> f64 {
    if seg.ctrl.len() == 2 {
        return t_arc;
    }
    let target = seg.len * t_arc;
    let cum = &seg.cum;
    let n = cum.len() - 1;
    let mut i = 0;
    while i < n - 1 && cum[i + 1] < target {
        i += 1;
    }
    let u = if cum[i + 1] > cum[i] {
        (target - cum[i]) / (cum[i + 1] - cum[i])
    } else {
        0.0
    };
    (i as f64 + u) / n as f64
}

/// Splits a Bézier control polygon at parameter `t` (de Casteljau). Returns the control polygons
/// of the two halves; both include the split point.
#[must_use]
pub fn de_casteljau_split(ctrl: &[Vec2], t: f64) -> (Vec<Vec2>, Vec<Vec2>) {
    let mut left = Vec::with_capacity(ctrl.len());
    let mut right = Vec::with_capacity(ctrl.len());
    let mut pts = ctrl.to_vec();
    while let (Some(&first), Some(&last)) = (pts.first(), pts.last()) {
        left.push(first);
        right.push(last);
        pts = pts
            .windows(2)
            .map(|w| Vec2::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t))
            .collect();
    }
    right.reverse();
    (left, right)
}

/// Indices of the curve points strictly between a segment's corners, in outline order.
pub(crate) fn interior_indices(seg: &Segment, n: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut j = (seg.start_point + 1) % n;
    while j != seg.end_point && out.len() < n {
        out.push(j);
        j = (j + 1) % n;
    }
    out
}

/// Inserts a corner point at fraction `t_arc` along segment `segment`, keeping the curve's shape
/// exactly (the segment's control points are replaced by those of the two halves).
///
/// Returns the id of the new corner, or `None` if the piece has no such segment.
pub fn split_segment(piece: &mut Piece, segment: &PointId, t_arc: f64, ids: &mut IdGen) -> Option<PointId> {
    let ol = Outline::new(&piece.pts);
    let seg = &ol.segments[ol.segment_index(segment)?];
    let tb = seg_bez_param(seg, t_arc);
    let (left, right) = de_casteljau_split(&seg.ctrl, tb);
    let corner_id = PointId::new(ids.fresh(prefix::POINT));
    let mid = *left.last()?;
    let mut new_ctrl: Vec<OutlinePoint> = left[1..left.len() - 1]
        .iter()
        .map(|p| OutlinePoint::curve(ids.fresh(prefix::POINT), p.x, p.y))
        .collect();
    new_ctrl.push(OutlinePoint {
        id: corner_id.clone(),
        pos: mid,
        curve: false,
    });
    new_ctrl.extend(
        right[1..right.len() - 1]
            .iter()
            .map(|p| OutlinePoint::curve(ids.fresh(prefix::POINT), p.x, p.y)),
    );

    let n = piece.pts.len();
    let interior = interior_indices(seg, n);
    let mut out = Vec::with_capacity(n + new_ctrl.len());
    for (i, p) in piece.pts.iter().enumerate() {
        if interior.contains(&i) {
            continue;
        }
        out.push(p.clone());
        if i == seg.start_point {
            out.append(&mut new_ctrl);
        }
    }
    piece.pts = out;
    Some(corner_id)
}

/// Adds a curve point at `pos` to segment `segment`, among its control points at the position
/// that matches fraction `t_arc` along the segment.
///
/// Returns the id of the new point, or `None` if the piece has no such segment.
pub fn add_curve_point(
    piece: &mut Piece,
    segment: &PointId,
    t_arc: f64,
    pos: Vec2,
    ids: &mut IdGen,
) -> Option<PointId> {
    let ol = Outline::new(&piece.pts);
    let seg = &ol.segments[ol.segment_index(segment)?];
    let n = piece.pts.len();
    let mut ctrl_idx = vec![seg.start_point];
    ctrl_idx.extend(interior_indices(seg, n));
    ctrl_idx.push(seg.end_point);
    let m = ctrl_idx.len() - 1;
    let slot = (t_arc * m as f64).ceil().min(m as f64).max(1.0) as usize;
    let insert_after = ctrl_idx[slot - 1];
    let id = PointId::new(ids.fresh(prefix::POINT));
    piece.pts.insert(
        insert_after + 1,
        OutlinePoint {
            id: id.clone(),
            pos,
            curve: true,
        },
    );
    Some(id)
}

/// Reshapes segment `segment` as a quadratic curve through `through`: its curve points are replaced
/// by one control point (keeping the first curve point's id, if it had any).
///
/// Returns `false` if the piece has no such segment.
pub fn set_curvature(piece: &mut Piece, segment: &PointId, through: Vec2, ids: &mut IdGen) -> bool {
    let ol = Outline::new(&piece.pts);
    let Some(k) = ol.segment_index(segment) else {
        return false;
    };
    let seg = &ol.segments[k];
    let n = piece.pts.len();
    let p0 = piece.pts[seg.start_point].pos;
    let p1 = piece.pts[seg.end_point].pos;
    let interior: Vec<PointId> = interior_indices(seg, n)
        .into_iter()
        .map(|j| piece.pts[j].id.clone())
        .collect();
    let keep = js_first_key(interior.iter().map(|id| id.0.as_str()))
        .map_or_else(|| PointId::new(ids.fresh(prefix::POINT)), PointId::new);
    let control = OutlinePoint {
        id: keep,
        pos: Vec2::new(
            2.0 * through.x - f64::midpoint(p0.x, p1.x),
            2.0 * through.y - f64::midpoint(p0.y, p1.y),
        ),
        curve: true,
    };
    let mut out = Vec::with_capacity(n);
    for (i, p) in piece.pts.iter().enumerate() {
        if interior.contains(&p.id) {
            continue;
        }
        out.push(p.clone());
        if i == seg.start_point {
            out.push(control.clone());
        }
    }
    piece.pts = out;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PieceId;

    fn square() -> Piece {
        Piece::new(
            PieceId::new("pc"),
            "Square",
            vec![
                OutlinePoint::corner("a", 0.0, 0.0),
                OutlinePoint::corner("b", 100.0, 0.0),
                OutlinePoint::corner("c", 100.0, 100.0),
                OutlinePoint::corner("d", 0.0, 100.0),
            ],
        )
    }

    #[test]
    fn splitting_a_straight_segment_adds_one_corner() {
        let mut pc = square();
        let mut ids = IdGen::after(0);
        let id = split_segment(&mut pc, &"b".into(), 0.25, &mut ids).expect("segment exists");
        assert_eq!(pc.pts.len(), 5);
        assert_eq!(pc.pts[2].id, id);
        assert!((pc.pts[2].pos - Vec2::new(100.0, 25.0)).length() < 1e-12);
    }

    #[test]
    fn curvature_passes_through_the_handle() {
        let mut pc = square();
        let mut ids = IdGen::after(0);
        assert!(set_curvature(
            &mut pc,
            &"a".into(),
            Vec2::new(50.0, -20.0),
            &mut ids
        ));
        let ol = pc.outline();
        let mid = ol.segments[0].ctrl.clone();
        let at_half = azam_geom::bezier_point(&mid, 0.5);
        assert!((at_half - Vec2::new(50.0, -20.0)).length() < 1e-9);
    }
}
