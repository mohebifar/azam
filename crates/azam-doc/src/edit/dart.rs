//! Darts: a wedge folded out of a piece's edge and sewn shut, as pattern makers shape a bodice
//! or a skirt waist.

use azam_geom::{Outline, OutlinePoint, PointId, Vec2};

use super::{remap_refs, split_segment};
use crate::ids::{PieceId, SeamId, prefix};
use crate::project::{Project, SeamSide};

/// Width of a dart the Dart tool makes, in mm.
pub const DEFAULT_DART_WIDTH_MM: f64 = 20.0;
/// Depth (edge to apex) of a dart the Dart tool makes, in mm.
pub const DEFAULT_DART_DEPTH_MM: f64 = 80.0;
/// Darts keep at least this fraction of the segment on either side.
const DART_MARGIN: f64 = 0.02;

/// Cuts a dart `width` mm wide and `depth` mm deep into the segment of `piece` that starts at
/// `segment`, centred at fraction `t` along it (moved inwards if the dart would run off an end),
/// and sews its two legs together. The apex points into the piece. Seams and parts on the piece
/// stay attached. Returns the dart's seam, or `None` if there is no such segment, the sizes are
/// not positive, or the segment is too short for the width.
pub fn add_dart(
    project: &mut Project,
    piece: &PieceId,
    segment: &PointId,
    t: f64,
    width: f64,
    depth: f64,
) -> Option<SeamId> {
    let old = project.pieces.get(piece)?.pts.clone();
    let outline = Outline::new(&old);
    let len = outline.segments[outline.segment_index(segment)?].len;
    let half = width / 2.0 / len;
    if width <= 0.0 || depth <= 0.0 || !len.is_finite() || half >= 0.5 - DART_MARGIN {
        return None;
    }
    let t = t.clamp(half + DART_MARGIN, 1.0 - half - DART_MARGIN);
    let (t1, t2) = (t - half, t + half);

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
        let apex = OutlinePoint {
            id: PointId::new(ids.fresh(prefix::POINT)),
            pos: mid + inward * depth,
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
        Some((p1, apex_id))
    });
    project.ids = ids;
    let (p1, apex) = legs?;
    remap_refs(project, piece, &old);
    // leg one runs edge → apex, leg two (walked backwards) also edge → apex
    Some(project.add_seam(
        vec![SeamSide::segment(piece, &p1.0, false)],
        vec![SeamSide::segment(piece, &apex.0, true)],
    ))
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

    #[test]
    fn a_dart_cuts_a_sewn_wedge_into_the_edge() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let square =
            [(0.0, 0.0), (200.0, 0.0), (200.0, 300.0), (0.0, 300.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let piece = crate::create_piece(&mut p, &square).expect("piece");
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
        let mut p = Project::new(None, Sex::Female, 0.0);
        let square =
            [(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let piece = crate::create_piece(&mut p, &square).expect("piece");
        let top = p.pieces[&piece].pts[0].id.clone();
        assert!(
            add_dart(&mut p, &piece, &top, 0.5, 40.0, 10.0).is_none(),
            "wider than the edge"
        );
        assert!(
            add_dart(&mut p, &piece, &top, 0.5, 10.0, 0.0).is_none(),
            "no depth"
        );
        assert_eq!(
            p.pieces[&piece].pts.len(),
            4,
            "a refused dart leaves the piece alone"
        );
    }
}
