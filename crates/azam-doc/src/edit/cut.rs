//! Cutting a piece in two along a line (CLO's Cut tool): corners are added where the line crosses
//! the outline (curves keep their exact shape), and the far half becomes a new piece that takes
//! the seams, topstitches, notches, internal lines, graphics, pins and trims lying on it.

use std::collections::BTreeSet;

use azam_geom::{OutlinePoint, PointId, Vec2, contains};

use super::refs::remap_refs;
use super::segment::split_segment;
use crate::ids::{PieceId, prefix};
use crate::project::Project;

/// Why a piece could not be cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutError {
    /// There is no such piece.
    NoPiece,
    /// The line does not cross the outline exactly twice.
    Crossings(usize),
    /// One side of the cut would have fewer than three corners.
    TooSmall,
}

impl std::fmt::Display for CutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPiece => write!(f, "there is no such piece"),
            Self::Crossings(n) => write!(f, "the line must cross the outline twice (it crosses {n} times)"),
            Self::TooSmall => write!(f, "one side of the cut would be too small"),
        }
    }
}

/// Where the line through `a` and `b` crosses the outline of `pts`: segment id and arc fraction.
fn crossings(pts: &[OutlinePoint], a: Vec2, b: Vec2) -> Vec<(PointId, f64, Vec2)> {
    let ol = azam_geom::Outline::new(pts);
    let dir = b - a;
    let side = |p: Vec2| dir.perp_dot(p - a);
    let mut out = Vec::new();
    for seg in &ol.segments {
        for (i, w) in seg.poly.windows(2).enumerate() {
            let (s0, s1) = (side(w[0]), side(w[1]));
            // a crossing counts at the start of a polyline step, not at its end, so a vertex on
            // the line is counted once
            if (s0 <= 0.0 && s1 > 0.0) || (s0 >= 0.0 && s1 < 0.0) {
                let u = s0 / (s0 - s1);
                let len = seg.cum[i] + (seg.cum[i + 1] - seg.cum[i]) * u;
                let t = if seg.len > 0.0 { len / seg.len } else { 0.0 };
                out.push((seg.id.clone(), t, w[0] + (w[1] - w[0]) * u));
            }
        }
    }
    out
}

/// Cuts `piece` in two along the line through `a` and `b`. The piece keeps the half that runs
/// from the first new corner to the second in outline order; the other half becomes a new piece,
/// whose id is returned.
///
/// # Errors
/// [`CutError`] when there is no such piece, the line does not cross the outline exactly twice,
/// or a half would have fewer than three corners.
pub fn cut_piece(project: &mut Project, piece: &PieceId, a: Vec2, b: Vec2) -> Result<PieceId, CutError> {
    let pc = project.pieces.get(piece).ok_or(CutError::NoPiece)?;
    let found = crossings(&pc.pts, a, b);
    if found.len() != 2 {
        return Err(CutError::Crossings(found.len()));
    }
    let old = pc.pts.clone();
    let mut pc = project.pieces.get(piece).cloned().ok_or(CutError::NoPiece)?;
    // the first corner, then the second found again on the changed outline
    let (seg1, t1, _) = &found[0];
    let id1 = split_segment(&mut pc, seg1, t1.clamp(1e-6, 1.0 - 1e-6), project.id_gen())
        .ok_or(CutError::TooSmall)?;
    let at2 = found[1].2;
    let ol = pc.outline();
    let near = ol.nearest(at2).ok_or(CutError::TooSmall)?;
    let seg2 = ol.segments[near.segment].id.clone();
    let id2 = split_segment(&mut pc, &seg2, near.t.clamp(1e-6, 1.0 - 1e-6), project.id_gen())
        .ok_or(CutError::TooSmall)?;
    let (Some(mut i1), Some(mut i2)) = (pc.point_index(&id1), pc.point_index(&id2)) else {
        return Err(CutError::TooSmall);
    };
    if i1 > i2 {
        std::mem::swap(&mut i1, &mut i2);
    }
    let kept: Vec<OutlinePoint> = pc.pts[i1..=i2].to_vec();
    let moved: Vec<OutlinePoint> = pc.pts[i2..].iter().chain(&pc.pts[..=i1]).cloned().collect();
    let corners = |pts: &[OutlinePoint]| pts.iter().filter(|p| !p.curve).count();
    if corners(&kept) < 3 || corners(&moved) < 3 {
        return Err(CutError::TooSmall);
    }
    // segments (by start corner) of the new piece: all its corners but the last, whose segment is
    // the cut edge back to its first
    let moved_segs: BTreeSet<PointId> = moved[..moved.len() - 1]
        .iter()
        .filter(|p| !p.curve)
        .map(|p| p.id.clone())
        .collect();
    let moved_poly: Vec<Vec2> = azam_geom::Outline::new(&moved).poly;
    let inside = |q: Vec2| contains(&moved_poly, q);

    let new_id = PieceId::new(project.fresh_id(prefix::PIECE));
    let mut other = pc.clone();
    other.id = new_id.clone();
    other.name = format!("{} 2", pc.name);
    other.pts = moved;
    other.mirror = None;
    other
        .internals
        .retain(|il| il.pts.first().is_some_and(|p| inside(p.pos)));
    other.notches.retain(|n| moved_segs.contains(&n.s));
    other.pins.retain(|p| inside(Vec2::new(p.x, p.y)));
    other.graphics.retain(|g| inside(Vec2::new(g.x, g.y)));
    pc.pts = kept;
    pc.internals
        .retain(|il| !il.pts.first().is_some_and(|p| inside(p.pos)));
    pc.notches.retain(|n| !moved_segs.contains(&n.s));
    pc.pins.retain(|p| !inside(Vec2::new(p.x, p.y)));
    pc.graphics.retain(|g| !inside(Vec2::new(g.x, g.y)));
    project.pieces.insert(piece.clone(), pc);
    // seams and topstitches first follow the new corners, then the half they lie on
    remap_refs(project, piece, &old);
    for seam in project.sewing.values_mut() {
        for side in seam.a.iter_mut().chain(seam.b.iter_mut()) {
            if side.piece == *piece && moved_segs.contains(&side.side.a.segment) {
                side.piece = new_id.clone();
            }
        }
    }
    for st in project.stitches.values_mut() {
        if st.p.as_ref() == Some(piece) && moved_segs.contains(&st.seg) {
            st.p = Some(new_id.clone());
        }
    }
    let items = project
        .buttons
        .values_mut()
        .chain(project.holes.values_mut())
        .chain(project.comments.values_mut());
    for it in items.filter(|it| it.p.as_ref() == Some(piece)) {
        if let (Some(x), Some(y)) = (it.x, it.y)
            && inside(Vec2::new(x, y))
        {
            it.p = Some(new_id.clone());
        }
    }
    project.add_piece(other);
    Ok(new_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;
    use crate::piece::Piece;

    fn square(p: &mut Project) -> PieceId {
        p.add_piece(Piece::new(
            PieceId::new("pc"),
            "Panel",
            vec![
                OutlinePoint::corner("a", 0.0, 0.0),
                OutlinePoint::corner("b", 200.0, 0.0),
                OutlinePoint::corner("c", 200.0, 100.0),
                OutlinePoint::corner("d", 0.0, 100.0),
            ],
        ))
    }

    #[test]
    fn a_vertical_line_cuts_a_panel_into_two() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let pc = square(&mut p);
        let other = cut_piece(&mut p, &pc, Vec2::new(80.0, -50.0), Vec2::new(80.0, 150.0)).expect("cut");
        let area = |id: &PieceId| azam_geom::signed_area(&p.pieces[id].outline().poly).abs();
        let (a1, a2) = (area(&pc), area(&other));
        assert!(
            (a1 + a2 - 20000.0).abs() < 1.0,
            "the halves cover the panel: {a1} + {a2}"
        );
        assert!((a1.min(a2) - 8000.0).abs() < 1.0, "cut at x = 80");
        assert_eq!(p.pieces[&other].name, "Panel 2");
        assert_eq!(p.pieces[&pc].pts.len(), 4);
        assert_eq!(p.pieces[&other].pts.len(), 4);
        // a line that misses the panel cuts nothing
        assert_eq!(
            cut_piece(&mut p, &pc, Vec2::new(500.0, 0.0), Vec2::new(500.0, 10.0)),
            Err(CutError::Crossings(0))
        );
    }
}
