//! Editing seams: free sewing sides, flipping side B and swapping the two groups.

use azam_geom::{Outline, PointId, SewingSide, SideDirection, SideEnd};
use serde_json::Map;

use crate::ids::{PieceId, SeamId};
use crate::project::{Project, SeamSide};

/// A point on a piece outline: the segment (id of its start corner) and the fraction along it.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineSpot {
    /// The segment.
    pub segment: PointId,
    /// Fraction of the segment's arc length.
    pub t: f64,
}

/// The sewing side of `piece` from `a` to `b` along `outline`, running the shorter way round (the
/// longer way when `long_way` is set). Returns `None` if either segment is missing.
#[must_use]
pub fn free_side(
    outline: &Outline,
    piece: &PieceId,
    a: &OutlineSpot,
    b: &OutlineSpot,
    long_way: bool,
) -> Option<SeamSide> {
    let pa = outline.perimeter_param(outline.segment_index(&a.segment)?, a.t);
    let pb = outline.perimeter_param(outline.segment_index(&b.segment)?, b.t);
    let total = outline.total;
    let forward = (pb - pa).rem_euclid(total.max(f64::MIN_POSITIVE));
    let short_forward = forward <= total / 2.0;
    let direction = if short_forward == long_way {
        SideDirection::Backward
    } else {
        SideDirection::Forward
    };
    let end = |s: &OutlineSpot| SideEnd {
        segment: s.segment.clone(),
        t: s.t,
    };
    Some(SeamSide {
        piece: piece.clone(),
        side: SewingSide {
            a: end(a),
            b: end(b),
            direction,
        },
        extra: Map::new(),
    })
}

/// The side covering the whole segment `segment` of `piece`, starting from the end nearer to
/// fraction `t` (where the segment was clicked).
#[must_use]
pub fn segment_side_from(piece: &PieceId, segment: &PointId, t: f64) -> SeamSide {
    SeamSide::segment(piece, &segment.0, t > 0.5)
}

/// Reverses side B of `seam`: its sides in reverse order, each walked the other way, so the seam
/// joins the other ends together. Returns `false` if there is no such seam.
pub fn flip_seam(project: &mut Project, seam: &SeamId) -> bool {
    let Some(sw) = project.sewing.get_mut(seam) else {
        return false;
    };
    sw.b.reverse();
    for sd in &mut sw.b {
        std::mem::swap(&mut sd.side.a, &mut sd.side.b);
        sd.side.direction = match sd.side.direction {
            SideDirection::Forward => SideDirection::Backward,
            SideDirection::Backward => SideDirection::Forward,
        };
    }
    true
}

/// Swaps the two groups of sides of `seam`. Returns `false` if there is no such seam.
pub fn swap_seam_groups(project: &mut Project, seam: &SeamId) -> bool {
    let Some(sw) = project.sewing.get_mut(seam) else {
        return false;
    };
    std::mem::swap(&mut sw.a, &mut sw.b);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;
    use azam_geom::{DEFAULT_SIDE_STEP_MM, Vec2};

    fn square(p: &mut Project) -> PieceId {
        let pts =
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)].map(|(x, y)| (Vec2::new(x, y), false));
        crate::create_piece(p, &pts).expect("piece")
    }

    fn spot(p: &Project, id: &PieceId, k: usize, t: f64) -> OutlineSpot {
        OutlineSpot {
            segment: p.pieces[id].pts[k].id.clone(),
            t,
        }
    }

    #[test]
    fn free_sides_take_the_short_way_unless_asked() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let id = square(&mut p);
        let ol = p.pieces[&id].outline();
        // From the middle of the top edge to the middle of the left edge: 100 mm backwards,
        // 300 mm forwards.
        let (a, b) = (spot(&p, &id, 0, 0.5), spot(&p, &id, 3, 0.5));
        let short = free_side(&ol, &id, &a, &b, false).expect("side");
        assert_eq!(short.side.direction, SideDirection::Backward);
        let range = short.side.range(&ol).expect("range");
        assert!((range.length - 100.0).abs() < 1e-9);
        let long = free_side(&ol, &id, &a, &b, true).expect("side");
        assert!((long.side.range(&ol).expect("range").length - 300.0).abs() < 1e-9);
    }

    #[test]
    fn flipping_reverses_side_b_and_swapping_exchanges_groups() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let id = square(&mut p);
        let top = p.pieces[&id].pts[0].id.clone();
        let bottom = p.pieces[&id].pts[2].id.clone();
        let seam = p.add_seam(
            vec![segment_side_from(&id, &top, 0.1)],
            vec![segment_side_from(&id, &bottom, 0.9)],
        );
        let ol = p.pieces[&id].outline();
        let start_of_b = |p: &Project| {
            p.sewing[&seam].b[0]
                .side
                .polyline(&ol, DEFAULT_SIDE_STEP_MM)
                .first()
                .map(|q| q.pos)
        };
        assert_eq!(
            start_of_b(&p),
            Some(Vec2::new(0.0, 100.0)),
            "clicked near its end"
        );
        assert!(flip_seam(&mut p, &seam));
        assert_eq!(start_of_b(&p), Some(Vec2::new(100.0, 100.0)));
        assert!(swap_seam_groups(&mut p, &seam));
        assert_eq!(p.sewing[&seam].a[0].side.a.segment, bottom);
        assert!(!flip_seam(&mut p, &SeamId::new("none")));
    }
}
