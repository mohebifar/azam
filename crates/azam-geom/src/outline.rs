//! Closed piece outlines: segments between corners, flattened for arc-length queries.

use std::collections::BTreeMap;

use crate::curve::{PolyPoint, cumulative_lengths, flatten_bezier, point_at_length};
use crate::polygon::distance_to_segment;
use crate::{OutlinePoint, PointId, Vec2};

/// Perimeter positions closer than this to a vertex are treated as on it, in mm.
const PATH_EPSILON: f64 = 1e-6;

/// One edge of an outline, from a corner to the next corner.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    /// Id of the start corner. Sewing and per-edge settings refer to the segment by this id.
    pub id: PointId,
    /// Index of the start corner in the piece's point list.
    pub start_point: usize,
    /// Index of the end corner in the piece's point list.
    pub end_point: usize,
    /// Bézier control polygon: start corner, curve points, end corner.
    pub ctrl: Vec<Vec2>,
    /// Flattened polyline, from start corner to end corner.
    pub poly: Vec<Vec2>,
    /// Running arc length at each vertex of `poly`.
    pub cum: Vec<f64>,
    /// Arc length of the segment in mm.
    pub len: f64,
}

impl Segment {
    /// The point at fraction `t` of this segment's arc length.
    #[must_use]
    pub fn point_at(&self, t: f64) -> PolyPoint {
        point_at_length(&self.poly, &self.cum, self.len * t)
    }
}

/// A point on the outline, with the segment it falls on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerimeterPoint {
    /// Position in mm.
    pub pos: Vec2,
    /// Unit direction of the outline at this point.
    pub tangent: Vec2,
    /// Index of the segment in [`Outline::segments`].
    pub segment: usize,
    /// Fraction of that segment's length (0 at its start corner).
    pub t: f64,
}

/// The closest point on an outline to a query point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NearestPoint {
    /// Index of the segment in [`Outline::segments`].
    pub segment: usize,
    /// Fraction of that segment's length.
    pub t: f64,
    /// Closest position in mm.
    pub pos: Vec2,
    /// Distance from the query point in mm.
    pub distance: f64,
}

/// A closed outline split into segments at its corners.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    /// Segments in outline order.
    pub segments: Vec<Segment>,
    /// Perimeter position where each segment starts.
    pub starts: Vec<f64>,
    /// Total perimeter in mm.
    pub total: f64,
    /// The whole outline as one closed polyline (the closing vertex is not repeated).
    pub poly: Vec<Vec2>,
    by_id: BTreeMap<PointId, usize>,
}

impl Outline {
    /// Builds the outline of a piece from its points.
    #[must_use]
    pub fn new(points: &[OutlinePoint]) -> Self {
        let mut outline = Self::default();
        for (start_point, end_point, ctrl) in segment_spans(points) {
            let poly = flatten_bezier(&ctrl);
            let cum = cumulative_lengths(&poly);
            let len = cum.last().copied().unwrap_or(0.0);
            let id = points[start_point].id.clone();
            outline.by_id.insert(id.clone(), outline.segments.len());
            outline.starts.push(outline.total);
            outline.total += len;
            outline.poly.extend_from_slice(&poly[..poly.len() - 1]);
            outline.segments.push(Segment {
                id,
                start_point,
                end_point,
                ctrl,
                poly,
                cum,
                len,
            });
        }
        outline
    }

    /// Index of the segment that starts at corner `id`.
    #[must_use]
    pub fn segment_index(&self, id: &PointId) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    /// Perimeter position of fraction `t` along segment `segment`.
    #[must_use]
    pub fn perimeter_param(&self, segment: usize, t: f64) -> f64 {
        self.starts[segment] + self.segments[segment].len * t
    }

    /// The point at perimeter position `p`. Positions wrap around the closed outline.
    ///
    /// # Panics
    /// Panics if the outline is empty.
    #[must_use]
    pub fn point_at_perimeter(&self, p: f64) -> PerimeterPoint {
        let p = wrap(p, self.total);
        let last = self.segments.len() - 1;
        let segment = (0..self.segments.len())
            .find(|&i| p < self.starts[i] + self.segments[i].len)
            .unwrap_or(last);
        let seg = &self.segments[segment];
        let t = if seg.len > 0.0 {
            (p - self.starts[segment]) / seg.len
        } else {
            0.0
        };
        let point = seg.point_at(t.clamp(0.0, 1.0));
        PerimeterPoint {
            pos: point.pos,
            tangent: point.tangent,
            segment,
            t,
        }
    }

    /// The closest point on the outline to `q`, or `None` for an empty outline.
    #[must_use]
    pub fn nearest(&self, q: Vec2) -> Option<NearestPoint> {
        let mut best: Option<NearestPoint> = None;
        for (segment, seg) in self.segments.iter().enumerate() {
            for (i, w) in seg.poly.windows(2).enumerate() {
                let hit = distance_to_segment(q, w[0], w[1]);
                if best.is_none_or(|b| hit.distance < b.distance) {
                    let len = seg.cum[i] + (seg.cum[i + 1] - seg.cum[i]) * hit.t;
                    let t = if seg.len > 0.0 { len / seg.len } else { 0.0 };
                    best = Some(NearestPoint {
                        segment,
                        t,
                        pos: hit.closest,
                        distance: hit.distance,
                    });
                }
            }
        }
        best
    }

    /// The part of the outline from perimeter position `p0` to `p1` (`p1 > p0`; it may pass the
    /// start of the outline).
    #[must_use]
    pub fn path_between(&self, p0: f64, p1: f64) -> Vec<Vec2> {
        let vertices: Vec<(f64, Vec2)> = self
            .segments
            .iter()
            .zip(&self.starts)
            .flat_map(|(seg, &start)| {
                seg.poly[..seg.poly.len() - 1]
                    .iter()
                    .zip(&seg.cum)
                    .map(move |(&v, &c)| (start + c, v))
            })
            .collect();
        let mut out = vec![self.point_at_perimeter(p0).pos];
        let mut base = (p0 / self.total).floor() * self.total;
        while base < p1 {
            out.extend(
                vertices
                    .iter()
                    .filter(|(p, _)| p + base > p0 + PATH_EPSILON && p + base < p1 - PATH_EPSILON)
                    .map(|&(_, v)| v),
            );
            base += self.total;
        }
        out.push(self.point_at_perimeter(p1).pos);
        out
    }
}

/// Splits a point list into segments: `(start corner, end corner, control polygon)`.
///
/// Curve points belong to the segment of the corner before them. An outline with no corners is
/// treated as one closed curve starting at its first point.
fn segment_spans(points: &[OutlinePoint]) -> Vec<(usize, usize, Vec<Vec2>)> {
    let n = points.len();
    let mut corners: Vec<usize> = (0..n).filter(|&i| !points[i].curve).collect();
    if corners.is_empty() && n > 0 {
        corners.push(0);
    }
    (0..corners.len())
        .map(|k| {
            let (i0, i1) = (corners[k], corners[(k + 1) % corners.len()]);
            let mut ctrl = vec![points[i0].pos];
            let mut j = (i0 + 1) % n;
            while j != i1 && ctrl.len() <= n {
                ctrl.push(points[j].pos);
                j = (j + 1) % n;
            }
            ctrl.push(points[i1].pos);
            (i0, i1, ctrl)
        })
        .collect()
}

/// Wraps a perimeter position into `[0, total)`.
///
/// Written as `((v % t) + t) % t` rather than `rem_euclid` so results match the golden data bit
/// for bit.
pub(crate) fn wrap(v: f64, total: f64) -> f64 {
    ((v % total) + total) % total
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "these tests use exactly representable values")]
mod tests {
    use super::*;

    fn square() -> Vec<OutlinePoint> {
        vec![
            OutlinePoint::corner("a", 0.0, 0.0),
            OutlinePoint::corner("b", 100.0, 0.0),
            OutlinePoint::corner("c", 100.0, 100.0),
            OutlinePoint::corner("d", 0.0, 100.0),
        ]
    }

    #[test]
    fn square_has_four_segments() {
        let ol = Outline::new(&square());
        assert_eq!(ol.segments.len(), 4);
        assert_eq!(ol.total, 400.0);
        assert_eq!(ol.starts, vec![0.0, 100.0, 200.0, 300.0]);
        assert_eq!(ol.poly.len(), 4);
        assert_eq!(ol.segment_index(&"c".into()), Some(2));
    }

    #[test]
    fn perimeter_wraps() {
        let ol = Outline::new(&square());
        let p = ol.point_at_perimeter(450.0);
        assert_eq!(p.pos, Vec2::new(50.0, 0.0));
        assert_eq!(p.segment, 0);
    }

    #[test]
    fn curve_points_join_their_segment() {
        let mut pts = square();
        pts.insert(1, OutlinePoint::curve("ab", 50.0, -40.0));
        let ol = Outline::new(&pts);
        assert_eq!(ol.segments.len(), 4);
        assert_eq!(ol.segments[0].ctrl.len(), 3);
        assert!(ol.segments[0].len > 100.0);
    }

    #[test]
    fn nearest_point_on_edge() {
        let ol = Outline::new(&square());
        let hit = ol.nearest(Vec2::new(130.0, 25.0)).expect("outline is not empty");
        assert_eq!(hit.segment, 1);
        assert_eq!(hit.pos, Vec2::new(100.0, 25.0));
        assert_eq!(hit.distance, 30.0);
    }
}
