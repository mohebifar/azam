//! Darts: a wedge folded out of a piece's edge and sewn shut, as pattern makers shape a bodice
//! or a skirt waist.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use azam_geom::{GradeDelta, Outline, OutlinePoint, PointId, SideDirection, Vec2};

use super::{remap_refs, split_segment};
use crate::Piece;
use crate::ids::{PieceId, SeamId, prefix};
use crate::project::{Project, SeamSide};

/// Width of a dart the Dart tool makes, in mm.
pub const DEFAULT_DART_WIDTH_MM: f64 = 20.0;
/// Depth (edge to apex) of a dart the Dart tool makes, in mm.
pub const DEFAULT_DART_DEPTH_MM: f64 = 80.0;
/// Darts keep at least this fraction of the segment on either side.
const DART_MARGIN: f64 = 0.02;
/// The apex stays at most this share of the way to the far side of the piece.
const MAX_DEPTH_SHARE: f64 = 0.8;

/// Why a dart could not be cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DartError {
    /// The piece or the segment does not exist.
    NoSuchSegment,
    /// The width or depth is not positive, or the segment is too short for the width.
    TooSmall,
    /// A seam runs along where the dart would go; a dart there would fold the seam into its legs.
    Sewn,
}

impl fmt::Display for DartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoSuchSegment => "there is no such segment",
            Self::TooSmall => "the segment is too short for a dart",
            Self::Sewn => "that edge is sewn: unsew it, add the dart, then sew it again",
        })
    }
}

impl std::error::Error for DartError {}

/// Cuts a dart `width` mm wide and up to `depth` mm deep into the segment of `piece` that starts
/// at `segment`, centred at fraction `t` along it (moved inwards if the dart would run off an
/// end), and sews its two legs together. The apex points into the piece, short of its far side.
/// Seams and parts on the piece stay attached; on a graded piece the new points grade with the
/// edge. Returns the dart's seam.
///
/// # Errors
/// [`DartError`] when there is no such segment, the dart does not fit, or a seam runs there.
pub fn add_dart(
    project: &mut Project,
    piece: &PieceId,
    segment: &PointId,
    t: f64,
    width: f64,
    depth: f64,
) -> Result<SeamId, DartError> {
    let old = project
        .pieces
        .get(piece)
        .ok_or(DartError::NoSuchSegment)?
        .pts
        .clone();
    let outline = Outline::new(&old);
    let si = outline.segment_index(segment).ok_or(DartError::NoSuchSegment)?;
    let seg = &outline.segments[si];
    let len = seg.len;
    let half = width / 2.0 / len;
    if width <= 0.0 || depth <= 0.0 || !len.is_finite() || half >= 0.5 - DART_MARGIN {
        return Err(DartError::TooSmall);
    }
    let t = t.clamp(half + DART_MARGIN, 1.0 - half - DART_MARGIN);
    let (t1, t2) = (t - half, t + half);
    if sewn_at(project, piece, &outline, si, [t1, t, t2]) {
        return Err(DartError::Sewn);
    }
    let ends = [old[seg.start_point].id.clone(), old[seg.end_point].id.clone()];

    project.ensure_ids_seeded();
    let mut ids = std::mem::take(&mut project.ids);
    let legs = project.pieces.get_mut(piece).and_then(|pc| {
        let p2 = split_segment(pc, segment, t2, &mut ids)?;
        // the segment now ends at p2, so t1 is a share of its first t2
        let p1 = split_segment(pc, segment, t1 / t2, &mut ids)?;
        let (i1, i2) = (pc.point_index(&p1)?, pc.point_index(&p2)?);
        let (a, b) = (pc.pts[i1].pos, pc.pts[i2].pos);
        let mid = (a + b) * 0.5;
        let along = (b - a).normalize();
        let normal = Vec2::new(-along.y, along.x);
        let poly = Outline::new(&pc.pts).poly;
        let inward = if inside(&poly, mid + normal) {
            normal
        } else {
            -normal
        };
        let room = ray_exit(&poly, mid, inward).unwrap_or(f64::INFINITY);
        let apex = OutlinePoint {
            id: PointId::new(ids.fresh(prefix::POINT)),
            pos: mid + inward * depth.min(room * MAX_DEPTH_SHARE),
            curve: false,
        };
        let apex_id = apex.id.clone();
        // the legs are straight: curve points between p1 and p2 give way to the apex
        let n = pc.pts.len();
        let mut between = Vec::new();
        let mut j = (i1 + 1) % n;
        while j != i2 {
            between.push(j);
            j = (j + 1) % n;
        }
        let mut out = Vec::with_capacity(n + 1);
        for (i, p) in pc.pts.iter().enumerate() {
            if between.contains(&i) {
                continue;
            }
            out.push(p.clone());
            if i == i1 {
                out.push(apex.clone());
            }
        }
        pc.pts = out;
        grade_dart(pc, &ends, [&p1, &apex_id, &p2], [t1, t2]);
        Some((p1, apex_id))
    });
    project.ids = ids;
    let (p1, apex) = legs.ok_or(DartError::NoSuchSegment)?;
    remap_refs(project, piece, &old);
    // leg one runs edge → apex, leg two (walked backwards) also edge → apex
    Ok(project.add_seam(
        vec![SeamSide::segment(piece, &p1.0, false)],
        vec![SeamSide::segment(piece, &apex.0, true)],
    ))
}

/// Whether a seam side on `piece` covers any of fractions `ts` of segment `si`.
fn sewn_at(project: &Project, piece: &PieceId, outline: &Outline, si: usize, ts: [f64; 3]) -> bool {
    project
        .sewing
        .values()
        .flat_map(|sw| sw.a.iter().chain(&sw.b))
        .filter(|sd| sd.piece == *piece)
        .filter_map(|sd| sd.side.range(outline))
        .any(|r| {
            let sign = if r.direction == SideDirection::Forward {
                1.0
            } else {
                -1.0
            };
            ts.iter().any(|&t| {
                let along = (sign * (outline.perimeter_param(si, t) - r.start)).rem_euclid(outline.total);
                along <= r.length
            })
        })
}

/// Gives a dart's new points grade rules from the edge they were cut into (whose corners are
/// `ends`): the legs at fractions `ts` along it, the apex halfway between the legs.
fn grade_dart(pc: &mut Piece, ends: &[PointId; 2], [p1, apex, p2]: [&PointId; 3], ts: [f64; 2]) {
    let Some(rules) = pc.grade.as_mut() else {
        return;
    };
    let (a, b) = (
        rules.get(&ends[0]).cloned().unwrap_or_default(),
        rules.get(&ends[1]).cloned().unwrap_or_default(),
    );
    let sizes: BTreeSet<_> = a.keys().chain(b.keys()).cloned().collect();
    if sizes.is_empty() {
        return;
    }
    let lerp = |t: f64| -> BTreeMap<_, _> {
        sizes
            .iter()
            .map(|s| {
                let (da, db) = (
                    a.get(s).copied().unwrap_or_default(),
                    b.get(s).copied().unwrap_or_default(),
                );
                let d = GradeDelta {
                    dx: da.dx + (db.dx - da.dx) * t,
                    dy: da.dy + (db.dy - da.dy) * t,
                };
                (s.clone(), d)
            })
            .collect()
    };
    rules.insert(p1.clone(), lerp(ts[0]));
    rules.insert(p2.clone(), lerp(ts[1]));
    rules.insert(apex.clone(), lerp(f64::midpoint(ts[0], ts[1])));
}

/// Distance from `from` along unit `dir` to the nearest edge of the polygon `poly` ahead of it.
fn ray_exit(poly: &[Vec2], from: Vec2, dir: Vec2) -> Option<f64> {
    let n = poly.len();
    (0..n)
        .filter_map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let e = b - a;
            let denom = dir.x * e.y - dir.y * e.x;
            if denom.abs() < 1e-12 {
                return None;
            }
            let w = a - from;
            let t = (w.x * e.y - w.y * e.x) / denom;
            let u = (w.x * dir.y - w.y * dir.x) / denom;
            (t > 1e-6 && (0.0..=1.0).contains(&u)).then_some(t)
        })
        .min_by(f64::total_cmp)
}

/// Whether `q` lies inside the closed polygon `poly` (even-odd rule).
fn inside(poly: &[Vec2], q: Vec2) -> bool {
    let mut odd = false;
    let mut j = poly.len().wrapping_sub(1);
    for (i, a) in poly.iter().enumerate() {
        let b = poly[j];
        if (a.y > q.y) != (b.y > q.y) && q.x < (b.x - a.x) * (q.y - a.y) / (b.y - a.y) + a.x {
            odd = !odd;
        }
        j = i;
    }
    odd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;
    use azam_geom::SizeName;

    fn square(p: &mut Project, w: f64, h: f64) -> PieceId {
        let pts = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(x, y)| (Vec2::new(x, y), false));
        crate::create_piece(p, &pts).expect("piece")
    }

    fn project() -> Project {
        Project::new(None, Sex::Female, 0.0)
    }

    #[test]
    fn a_dart_cuts_a_sewn_wedge_into_the_edge() {
        let mut p = project();
        let piece = square(&mut p, 200.0, 300.0);
        let top = p.pieces[&piece].pts[0].id.clone();
        let seams_before = p.sewing.len();
        let seam = add_dart(&mut p, &piece, &top, 0.5, 20.0, 80.0).expect("dart");

        let pts = &p.pieces[&piece].pts;
        assert_eq!(pts.len(), 7, "two legs and an apex join the four corners");
        let pos: Vec<Vec2> = pts.iter().map(|q| q.pos).collect();
        assert!(
            pos.iter().any(|q| (q.x - 90.0).abs() < 1e-6 && q.y.abs() < 1e-6),
            "leg at x = 90"
        );
        assert!(
            pos.iter().any(|q| (q.x - 110.0).abs() < 1e-6 && q.y.abs() < 1e-6),
            "leg at x = 110"
        );
        assert!(
            pos.iter()
                .any(|q| (q.x - 100.0).abs() < 1e-6 && (q.y - 80.0).abs() < 1e-6),
            "the apex points into the piece"
        );
        assert_eq!(p.sewing.len(), seams_before + 1);
        let sw = &p.sewing[&seam];
        let leg_len = |sd: &SeamSide| {
            let ol = p.pieces[&piece].outline();
            ol.segments[ol.segment_index(&sd.side.a.segment).expect("leg")].len
        };
        assert!(
            (leg_len(&sw.a[0]) - leg_len(&sw.b[0])).abs() < 1e-9,
            "the legs are equal"
        );
    }

    #[test]
    fn a_dart_needs_room() {
        let mut p = project();
        let piece = square(&mut p, 30.0, 30.0);
        let top = p.pieces[&piece].pts[0].id.clone();
        assert_eq!(
            add_dart(&mut p, &piece, &top, 0.5, 40.0, 10.0),
            Err(DartError::TooSmall),
            "wider than the edge"
        );
        assert_eq!(
            add_dart(&mut p, &piece, &top, 0.5, 10.0, 0.0),
            Err(DartError::TooSmall),
            "no depth"
        );
        assert_eq!(
            p.pieces[&piece].pts.len(),
            4,
            "a refused dart leaves the piece alone"
        );
    }

    #[test]
    fn a_sewn_edge_takes_no_dart() {
        let mut p = project();
        let a = square(&mut p, 200.0, 300.0);
        let b = square(&mut p, 200.0, 300.0);
        let (top_a, top_b) = (p.pieces[&a].pts[0].id.clone(), p.pieces[&b].pts[0].id.clone());
        p.add_seam(
            vec![SeamSide::segment(&a, &top_a.0, false)],
            vec![SeamSide::segment(&b, &top_b.0, true)],
        );
        assert_eq!(
            add_dart(&mut p, &a, &top_a, 0.5, 20.0, 80.0),
            Err(DartError::Sewn)
        );
        let side = p.pieces[&a].pts[1].id.clone();
        assert!(
            add_dart(&mut p, &a, &side, 0.5, 20.0, 80.0).is_ok(),
            "an unsewn edge still can"
        );
    }

    #[test]
    fn a_deep_dart_stops_short_of_the_far_side() {
        let mut p = project();
        let piece = square(&mut p, 200.0, 50.0);
        let top = p.pieces[&piece].pts[0].id.clone();
        add_dart(&mut p, &piece, &top, 0.5, 20.0, 80.0).expect("dart");
        let apex = p.pieces[&piece]
            .pts
            .iter()
            .find(|q| (q.pos.x - 100.0).abs() < 1e-6)
            .expect("apex")
            .pos;
        assert!(
            (apex.y - 40.0).abs() < 1e-6,
            "the apex stops at 80% of the 50 mm: {}",
            apex.y
        );
    }

    #[test]
    fn dart_points_grade_with_their_edge() {
        let mut p = project();
        let piece = square(&mut p, 200.0, 300.0);
        let pts: Vec<PointId> = p.pieces[&piece].pts.iter().map(|q| q.id.clone()).collect();
        crate::set_sizes(&mut p, &["M", "L"].map(SizeName::from));
        p.base_size = SizeName::from("M");
        // the top edge's right corner grows 10 mm in L
        crate::grade_points_linear(&mut p, &piece, &pts[1..2], Vec2::new(10.0, 0.0));
        add_dart(&mut p, &piece, &pts[0], 0.5, 20.0, 80.0).expect("dart");
        let pc = &p.pieces[&piece];
        let l = pc.graded_outline(&SizeName::from("L"), &p.base_size);
        let base = pc.graded_outline(&p.base_size, &p.base_size);
        let moved: Vec<f64> = l.poly.iter().zip(&base.poly).map(|(a, b)| a.x - b.x).collect();
        assert!(
            moved.iter().any(|&d| (d - 4.5).abs() < 1e-6),
            "the near leg moves 45% of 10 mm"
        );
        assert!(
            moved.iter().any(|&d| (d - 5.5).abs() < 1e-6),
            "the far leg moves 55% of 10 mm"
        );
    }
}
