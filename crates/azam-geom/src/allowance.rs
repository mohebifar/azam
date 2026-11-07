//! Seam and hem allowance: the cut line around a piece's sewing line.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::polygon::signed_area;
use crate::{Outline, PointId, Vec2};

/// Directions closer to parallel than this (cross product) do not intersect.
const PARALLEL_EPSILON: f64 = 1e-9;
/// A mitre may reach this many allowance widths from its corner (plus [`MITER_SLACK_MM`]) before it
/// is cut off with a bevel.
const MITER_LIMIT_WIDTHS: f64 = 3.0;
/// Extra reach allowed for a mitre, in mm.
const MITER_SLACK_MM: f64 = 1.0;
/// A folded hem corner may reach this many allowance widths from its corner (plus
/// [`HEM_CORNER_SLACK_MM`]) before falling back to a plain mitre.
const HEM_CORNER_LIMIT_WIDTHS: f64 = 4.0;
/// Extra reach allowed for a folded hem corner, in mm.
const HEM_CORNER_SLACK_MM: f64 = 5.0;

/// How one edge is finished outside its sewing line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeFinish {
    /// Width outside the sewing line, in mm.
    pub allowance: f64,
    /// `true` for a hem that is turned up, which folds the corners where it meets other edges.
    pub hem: bool,
}

/// Allowance settings of a piece: a default, per-edge overrides, and hems.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EdgeAllowances {
    /// Seam allowance of every edge without an override, in mm.
    pub piece: f64,
    /// Seam allowance overrides by segment id, in mm.
    #[serde(default)]
    pub per_segment: BTreeMap<PointId, f64>,
    /// Hem allowances by segment id, in mm. A hem replaces the seam allowance of its edge.
    #[serde(default)]
    pub hems: BTreeMap<PointId, f64>,
}

impl EdgeAllowances {
    /// The finish of the segment that starts at `segment`.
    #[must_use]
    pub fn finish(&self, segment: &PointId) -> EdgeFinish {
        match self.hems.get(segment) {
            Some(&hem) if hem > 0.0 => EdgeFinish {
                allowance: hem,
                hem: true,
            },
            _ => EdgeFinish {
                allowance: self.per_segment.get(segment).copied().unwrap_or(self.piece),
                hem: false,
            },
        }
    }
}

/// Offsets an outline outwards by a width per segment, joining edges with limited mitres.
#[must_use]
pub fn offset_outline(outline: &Outline, width_for: impl Fn(&PointId) -> f64) -> Vec<Vec2> {
    cut_outline(outline, |id| EdgeFinish {
        allowance: width_for(id),
        hem: false,
    })
}

/// The cut line of a piece.
///
/// Each edge is offset by its allowance and joined with limited mitres. Where a hem meets another
/// edge, the corner is folded: below the hem fold, the side's cut edge mirrors the part above it,
/// so the allowance matches the garment once the hem is turned up.
#[must_use]
pub fn cut_outline(outline: &Outline, finish_for: impl Fn(&PointId) -> EdgeFinish) -> Vec<Vec2> {
    let edges = offset_edges(outline, finish_for);
    let mut cut = Vec::with_capacity(edges.len() + 8);
    for (i, e2) in edges.iter().enumerate() {
        let e1 = &edges[(i + edges.len() - 1) % edges.len()];
        if e1.segment != e2.segment
            && e1.hem != e2.hem
            && let Some([first, second]) = folded_hem_corner(e1, e2)
        {
            cut.push(first);
            cut.push(second);
            continue;
        }
        mitre(e1, e2, &mut cut);
    }
    cut
}

/// One flattened outline edge moved outwards by its allowance.
struct OffsetEdge<'a> {
    a: Vec2,
    b: Vec2,
    width: f64,
    segment: &'a PointId,
    hem: bool,
    /// Start of the edge on the sewing line.
    net_start: Vec2,
    /// Unit direction of the edge.
    dir: Vec2,
}

fn offset_edges(outline: &Outline, finish_for: impl Fn(&PointId) -> EdgeFinish) -> Vec<OffsetEdge<'_>> {
    // y-down: positive area means the outline runs clockwise on screen.
    let clockwise = signed_area(&outline.poly) > 0.0;
    let mut edges = Vec::with_capacity(outline.poly.len());
    for seg in &outline.segments {
        let finish = finish_for(&seg.id);
        let w = finish.allowance;
        for pair in seg.poly.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let len = crate::hypot(b.x - a.x, b.y - a.y);
            let len = if len == 0.0 { 1.0 } else { len };
            let dir = Vec2::new((b.x - a.x) / len, (b.y - a.y) / len);
            let normal = if clockwise {
                Vec2::new(dir.y, -dir.x)
            } else {
                Vec2::new(-dir.y, dir.x)
            };
            edges.push(OffsetEdge {
                a: a + normal * w,
                b: b + normal * w,
                width: w,
                segment: &seg.id,
                hem: finish.hem,
                net_start: a,
                dir,
            });
        }
    }
    edges
}

/// Joins two consecutive offset edges at their intersection, or bevels if the mitre is too long.
fn mitre(e1: &OffsetEdge, e2: &OffsetEdge, cut: &mut Vec<Vec2>) {
    let Some(corner) = intersect(e1.a, e1.b - e1.a, e2.a, e2.b - e2.a) else {
        cut.push(e2.a);
        return;
    };
    let limit = e1.width.max(e2.width) * MITER_LIMIT_WIDTHS + MITER_SLACK_MM;
    if dist(corner, e2.a) > limit {
        cut.push(e1.b);
        cut.push(e2.a);
    } else {
        cut.push(corner);
    }
}

/// The two cut-line points of a folded hem corner, in outline order, or `None` if the fold does
/// not fit (the caller then mitres).
fn folded_hem_corner(e1: &OffsetEdge, e2: &OffsetEdge) -> Option<[Vec2; 2]> {
    let (hem, side) = if e2.hem { (e2, e1) } else { (e1, e2) };
    let corner = e2.net_start;
    let fold_dir = hem.dir;
    let side_dir = side.b - side.a;
    // Where the side's cut edge crosses the hem fold line.
    let on_fold = intersect(side.a, side_dir, corner, fold_dir)?;
    // The side's cut edge mirrored across the fold line, run down to the hem's cut edge.
    let k = 2.0 * (side_dir.x * fold_dir.x + side_dir.y * fold_dir.y);
    let mirrored = Vec2::new(k * fold_dir.x - side_dir.x, k * fold_dir.y - side_dir.y);
    let on_hem = intersect(on_fold, mirrored, hem.a, hem.b - hem.a)?;
    let limit = hem.width.max(side.width) * HEM_CORNER_LIMIT_WIDTHS + HEM_CORNER_SLACK_MM;
    if dist(on_fold, corner) >= limit || dist(on_hem, corner) >= limit {
        return None;
    }
    Some(if e2.hem {
        [on_fold, on_hem]
    } else {
        [on_hem, on_fold]
    })
}

/// Intersection of the line through `p` along `d` with the line through `q` along `e`.
fn intersect(p: Vec2, d: Vec2, q: Vec2, e: Vec2) -> Option<Vec2> {
    let den = d.x * e.y - d.y * e.x;
    if den.abs() < PARALLEL_EPSILON {
        return None;
    }
    let t = ((q.x - p.x) * e.y - (q.y - p.y) * e.x) / den;
    Some(Vec2::new(p.x + d.x * t, p.y + d.y * t))
}

fn dist(a: Vec2, b: Vec2) -> f64 {
    crate::hypot(a.x - b.x, a.y - b.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutlinePoint;

    fn square() -> Outline {
        Outline::new(&[
            OutlinePoint::corner("a", 0.0, 0.0),
            OutlinePoint::corner("b", 100.0, 0.0),
            OutlinePoint::corner("c", 100.0, 100.0),
            OutlinePoint::corner("d", 0.0, 100.0),
        ])
    }

    fn assert_close(actual: &[Vec2], expected: &[Vec2]) {
        assert_eq!(actual.len(), expected.len(), "{actual:?}");
        for (a, e) in actual.iter().zip(expected) {
            assert!(a.distance(*e) < 1e-9, "{a:?} != {e:?}");
        }
    }

    #[test]
    fn square_offset_is_a_bigger_square() {
        let cut = offset_outline(&square(), |_| 10.0);
        assert_close(
            &cut,
            &[
                Vec2::new(-10.0, -10.0),
                Vec2::new(110.0, -10.0),
                Vec2::new(110.0, 110.0),
                Vec2::new(-10.0, 110.0),
            ],
        );
    }

    #[test]
    fn hem_overrides_seam_allowance() {
        let mut allowances = EdgeAllowances {
            piece: 10.0,
            ..Default::default()
        };
        allowances.per_segment.insert("b".into(), 15.0);
        allowances.hems.insert("c".into(), 30.0);
        assert_eq!(
            allowances.finish(&"a".into()),
            EdgeFinish {
                allowance: 10.0,
                hem: false
            }
        );
        assert_eq!(
            allowances.finish(&"b".into()),
            EdgeFinish {
                allowance: 15.0,
                hem: false
            }
        );
        assert_eq!(
            allowances.finish(&"c".into()),
            EdgeFinish {
                allowance: 30.0,
                hem: true
            }
        );
    }

    #[test]
    fn hem_corners_are_folded() {
        // Bottom edge (segment "c", from (100,100) to (0,100)) is a 30 mm hem; sides have 10 mm.
        let mut allowances = EdgeAllowances {
            piece: 10.0,
            ..Default::default()
        };
        allowances.hems.insert("c".into(), 30.0);
        let cut = cut_outline(&square(), |id| allowances.finish(id));
        // Square corners fold straight down: the side edge continues to the hem's cut edge.
        assert_eq!(cut.len(), 6, "{cut:?}");
        assert!(
            cut.iter().any(|p| p.distance(Vec2::new(110.0, 100.0)) < 1e-9),
            "{cut:?}"
        );
        assert!(
            cut.iter().any(|p| p.distance(Vec2::new(110.0, 130.0)) < 1e-9),
            "{cut:?}"
        );
    }
}
