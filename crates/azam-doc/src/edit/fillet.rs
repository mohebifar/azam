//! Rounding a corner (a fillet): the corner gives way to a curve that meets both edges
//! tangentially, a quadratic Bézier whose control point is the old corner.

use azam_geom::{OutlinePoint, Vec2};

use super::refs::remap_refs;
use crate::ids::{PieceId, prefix};
use crate::project::Project;

/// Share of each neighbouring edge a fillet may take at most.
const MAX_EDGE_SHARE: f64 = 0.45;

/// Rounds corner `index` of `piece` with a fillet of about `radius` mm (smaller where the
/// neighbouring edges are short). The corner's id stays on the far end of the curve, so what is
/// sewn to the next edge stays sewn. Returns `false` for a curve point, a straight-through corner
/// or a piece without such a point.
pub fn fillet_corner(project: &mut Project, piece: &PieceId, index: usize, radius: f64) -> bool {
    let Some(pc) = project.pieces.get(piece) else {
        return false;
    };
    let n = pc.pts.len();
    if index >= n || n < 3 || pc.pts[index].curve || radius.is_nan() || radius <= 0.0 {
        return false;
    }
    let p = pc.pts[index].pos;
    let prev = pc.pts[(index + n - 1) % n].pos;
    let next = pc.pts[(index + 1) % n].pos;
    let (u, v) = ((prev - p).normalize_or_zero(), (next - p).normalize_or_zero());
    if u == Vec2::ZERO || v == Vec2::ZERO {
        return false;
    }
    // interior angle between the edges; nearly straight corners have nothing to round
    let theta = u.dot(v).clamp(-1.0, 1.0).acos();
    if !(1e-3..=std::f64::consts::PI - 1e-3).contains(&theta) {
        return false;
    }
    let along = (radius / (theta / 2.0).tan())
        .min(p.distance(prev) * MAX_EDGE_SHARE)
        .min(p.distance(next) * MAX_EDGE_SHARE);
    let old = pc.pts.clone();
    let start_id = project.fresh_id(prefix::POINT);
    let ctrl_id = project.fresh_id(prefix::POINT);
    let Some(pc) = project.pieces.get_mut(piece) else {
        return false;
    };
    let corner = pc.pts[index].clone();
    let start = OutlinePoint::corner(&start_id, p.x + u.x * along, p.y + u.y * along);
    let ctrl = OutlinePoint::curve(ctrl_id, p.x, p.y);
    let end = OutlinePoint {
        pos: p + v * along,
        ..corner
    };
    pc.pts.splice(index..=index, [start, ctrl, end]);
    remap_refs(project, piece, &old);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;
    use crate::piece::Piece;

    #[test]
    fn a_square_corner_rounds_tangentially() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let pc = p.add_piece(Piece::new(
            PieceId::new("pc"),
            "Panel",
            vec![
                OutlinePoint::corner("a", 0.0, 0.0),
                OutlinePoint::corner("b", 200.0, 0.0),
                OutlinePoint::corner("c", 200.0, 100.0),
                OutlinePoint::corner("d", 0.0, 100.0),
            ],
        ));
        assert!(fillet_corner(&mut p, &pc, 1, 20.0));
        let pts = &p.pieces[&pc].pts;
        assert_eq!(pts.len(), 6);
        // a right angle: 20 mm back along each edge, the old corner as control point
        assert!(pts[1].pos.distance(Vec2::new(180.0, 0.0)) < 1e-9);
        assert!(pts[2].curve && pts[2].pos == Vec2::new(200.0, 0.0));
        assert!(pts[3].pos.distance(Vec2::new(200.0, 20.0)) < 1e-9);
        assert_eq!(pts[3].id.0, "b", "the corner's id moves to the end of the curve");
        assert!(
            !fillet_corner(&mut p, &pc, 2, 20.0),
            "a curve point is not a corner"
        );
        // a huge radius is held to the short edges
        assert!(fillet_corner(&mut p, &pc, 4, 500.0));
        let pts = &p.pieces[&pc].pts;
        assert!(pts[4].pos.distance(Vec2::new(200.0, 100.0)) <= 80.0 * 0.45 + 1e-9);
    }
}
