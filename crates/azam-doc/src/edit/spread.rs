//! Slash and spread, and pleats: a line dragged across a piece splits its outline, and the half
//! past the line moves to add fullness there. A pleat moves it out parallel to the line, leaving
//! a band twice the pleat's depth with two fold lines (a knife pleat); a spread turns it about
//! the line's far end, opening a wedge at the near end (flare).

use azam_geom::{OutlinePoint, PointId, Vec2, contains};
use serde_json::Value;

use super::cut::{CutError, crossings};
use super::refs::remap_refs;
use super::segment::split_segment;
use crate::ids::{PartId, PieceId, prefix};
use crate::parts::LINE_FOLD_KEY;
use crate::piece::{InternalLine, LinePoint};
use crate::project::Project;

/// Fold angles of a knife pleat's two lines (180 is flat): folded back nearly flat, one each way.
const PLEAT_FOLDS: [f64; 2] = [350.0, 10.0];

/// How the half past the line moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spread {
    /// A knife pleat `depth` mm deep: the half moves out parallel to the line by twice that.
    Pleat {
        /// Depth of the pleat, mm.
        depth: f64,
    },
    /// The half turns about the crossing nearer the line's end so the outline opens `amount` mm
    /// at the crossing nearer its start.
    Fan {
        /// Opening at the start of the line, mm.
        amount: f64,
    },
}

/// Spreads `piece` along the line from `a` to `b` (see [`Spread`]). The half that moves is the one
/// the line's left-hand normal points into, so dragging the other way spreads the other half.
///
/// # Errors
/// [`CutError`] when there is no such piece, the line does not cross the outline exactly twice,
/// or a side has nothing to move.
pub fn spread_piece(
    project: &mut Project,
    piece: &PieceId,
    a: Vec2,
    b: Vec2,
    how: Spread,
) -> Result<(), CutError> {
    let pc = project.pieces.get(piece).ok_or(CutError::NoPiece)?;
    let found = crossings(&pc.pts, a, b);
    if found.len() != 2 {
        return Err(CutError::Crossings(found.len()));
    }
    let old = pc.pts.clone();
    let mut pc = pc.clone();
    // corners where the line crosses: the one nearer `a` opens, the one nearer `b` is the hinge
    let (near_a, near_b) = if found[0].2.distance(a) <= found[1].2.distance(a) {
        (found[0].clone(), found[1].clone())
    } else {
        (found[1].clone(), found[0].clone())
    };
    let open_id = split_segment(
        &mut pc,
        &near_a.0,
        near_a.1.clamp(1e-6, 1.0 - 1e-6),
        project.id_gen(),
    )
    .ok_or(CutError::TooSmall)?;
    let ol = pc.outline();
    let near = ol.nearest(near_b.2).ok_or(CutError::TooSmall)?;
    let hinge_seg = ol.segments[near.segment].id.clone();
    let hinge_id = split_segment(
        &mut pc,
        &hinge_seg,
        near.t.clamp(1e-6, 1.0 - 1e-6),
        project.id_gen(),
    )
    .ok_or(CutError::TooSmall)?;
    project.pieces.insert(piece.clone(), pc);
    // seams and notches follow the new corners while the outline is still in place
    remap_refs(project, piece, &old);

    let (open_pos, hinge_pos) = (near_a.2, near_b.2);
    let normal = {
        let d = (hinge_pos - open_pos).normalize_or_zero();
        Vec2::new(-d.y, d.x)
    };
    let mut pc = project.pieces.get(piece).cloned().ok_or(CutError::NoPiece)?;
    let n = pc.pts.len();
    let (Some(o), Some(h)) = (pc.point_index(&open_id), pc.point_index(&hinge_id)) else {
        return Err(CutError::TooSmall);
    };
    // the side the normal points into, as a run of indices from the open corner to the hinge
    let forward: Vec<usize> = (1..n).map(|k| (o + k) % n).take_while(|&i| i != h).collect();
    let backward: Vec<usize> = (1..n).map(|k| (o + n - k) % n).take_while(|&i| i != h).collect();
    let side_of = |run: &[usize]| {
        run.iter()
            .map(|&i| (pc.pts[i].pos - open_pos).dot(normal))
            .sum::<f64>()
            / run.len().max(1) as f64
    };
    let forward_moves = side_of(&forward) >= side_of(&backward);
    // reorder so the open corner is first and the moving side follows it, then the hinge
    let mut pts: Vec<OutlinePoint> = if forward_moves {
        (0..n).map(|k| pc.pts[(o + k) % n].clone()).collect()
    } else {
        (0..n).map(|k| pc.pts[(o + n - k) % n].clone()).collect()
    };
    let moving = if forward_moves {
        forward.len()
    } else {
        backward.len()
    };
    if moving == 0 {
        return Err(CutError::TooSmall);
    }
    let h_at = moving + 1;
    let side_poly: Vec<Vec2> = pts[..=h_at].iter().map(|p| p.pos).collect();
    let map = spread_map(how, open_pos, hinge_pos, normal);
    let fresh = |project: &mut Project| PointId::new(project.fresh_id(prefix::POINT));
    // the open corner stays (under a new id) and its copy, keeping the id, moves with the side
    let open_fixed = OutlinePoint {
        id: fresh(project),
        ..pts[0].clone()
    };
    for p in &mut pts[..h_at] {
        p.pos = map(p.pos);
    }
    pts.insert(0, open_fixed);
    let h_at = h_at + 1;
    if let Spread::Pleat { .. } = how {
        // the hinge too opens: its moved copy (a new id) ends the side, the corner itself stays
        let moved = OutlinePoint {
            id: fresh(project),
            pos: map(pts[h_at].pos),
            ..pts[h_at].clone()
        };
        pts.insert(h_at, moved);
    }
    if !forward_moves {
        pts.reverse();
    }
    pc.pts = pts;
    move_contents(project, piece, &mut pc, &side_poly, &*map);
    if let Spread::Pleat { depth } = how {
        add_pleat_folds(project, &mut pc, open_pos, hinge_pos, normal, depth);
    }
    project.pieces.insert(piece.clone(), pc);
    Ok(())
}

/// Moves by `map` what lies on the moving side (`side_poly`) of piece `pc`: its internal lines,
/// pins and graphics, and the project's buttons, buttonholes and comments on it.
fn move_contents(
    project: &mut Project,
    piece: &PieceId,
    pc: &mut crate::piece::Piece,
    side_poly: &[Vec2],
    map: &dyn Fn(Vec2) -> Vec2,
) {
    let inside = |q: Vec2| contains(side_poly, q);
    for il in &mut pc.internals {
        if il.pts.first().is_some_and(|p| inside(p.pos)) {
            for p in &mut il.pts {
                p.pos = map(p.pos);
            }
        }
    }
    for pin in &mut pc.pins {
        let q = Vec2::new(pin.x, pin.y);
        if inside(q) {
            let m = map(q);
            (pin.x, pin.y) = (m.x, m.y);
        }
    }
    for g in &mut pc.graphics {
        let q = Vec2::new(g.x, g.y);
        if inside(q) {
            let m = map(q);
            (g.x, g.y) = (m.x, m.y);
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
            let m = map(Vec2::new(x, y));
            (it.x, it.y) = (Some(m.x), Some(m.y));
        }
    }
}

/// The two fold lines of a knife pleat `depth` mm deep, along the line from `open_pos` to
/// `hinge_pos`, the second `depth` further along `normal`.
fn add_pleat_folds(
    project: &mut Project,
    pc: &mut crate::piece::Piece,
    open_pos: Vec2,
    hinge_pos: Vec2,
    normal: Vec2,
    depth: f64,
) {
    for (k, fold) in PLEAT_FOLDS.into_iter().enumerate() {
        let shift = normal * (depth * k as f64);
        let mut extra = serde_json::Map::new();
        extra.insert("closed".to_owned(), Value::Bool(false));
        extra.insert(LINE_FOLD_KEY.to_owned(), Value::from(fold));
        pc.internals.push(InternalLine {
            id: Some(PartId::new(project.fresh_id(prefix::INTERNAL))),
            pts: [open_pos + shift, hinge_pos + shift]
                .into_iter()
                .map(|pos| LinePoint {
                    id: None,
                    pos,
                    curve: false,
                })
                .collect(),
            extra,
        });
    }
}

/// Where a point of the moving side goes: out along `normal` for a pleat, turned about
/// `hinge_pos` for a spread (so `open_pos` moves along `normal`).
fn spread_map(how: Spread, open_pos: Vec2, hinge_pos: Vec2, normal: Vec2) -> Box<dyn Fn(Vec2) -> Vec2> {
    match how {
        Spread::Pleat { depth } => {
            let d = normal * (2.0 * depth);
            Box::new(move |v| v + d)
        }
        Spread::Fan { amount } => {
            let r = open_pos.distance(hinge_pos).max(1e-9);
            let mut angle = 2.0 * (amount / (2.0 * r)).clamp(-1.0, 1.0).asin();
            // turn so the open corner moves along the normal
            let turned = rotate(open_pos, hinge_pos, angle);
            if (turned - open_pos).dot(normal) < 0.0 {
                angle = -angle;
            }
            Box::new(move |v| rotate(v, hinge_pos, angle))
        }
    }
}

/// `v` turned by `angle` radians about `c`.
fn rotate(v: Vec2, c: Vec2, angle: f64) -> Vec2 {
    let (s, k) = angle.sin_cos();
    let d = v - c;
    c + Vec2::new(d.x * k - d.y * s, d.x * s + d.y * k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;
    use crate::piece::Piece;

    fn panel(p: &mut Project) -> PieceId {
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

    fn area(p: &Project, id: &PieceId) -> f64 {
        azam_geom::signed_area(&p.pieces[id].outline().poly).abs()
    }

    #[test]
    fn a_pleat_adds_a_band_with_two_fold_lines() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let pc = panel(&mut p);
        spread_piece(
            &mut p,
            &pc,
            Vec2::new(80.0, -20.0),
            Vec2::new(80.0, 120.0),
            Spread::Pleat { depth: 15.0 },
        )
        .expect("pleat");
        assert!(
            (area(&p, &pc) - (20000.0 + 30.0 * 100.0)).abs() < 1.0,
            "a 30 mm band: {}",
            area(&p, &pc)
        );
        assert_eq!(p.pieces[&pc].pts.len(), 8);
        let folds: Vec<_> = p.pieces[&pc]
            .internals
            .iter()
            .filter_map(|il| il.extra.get(LINE_FOLD_KEY))
            .collect();
        assert_eq!(folds.len(), 2);
    }

    #[test]
    fn a_spread_opens_a_wedge_at_the_start_of_the_line() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let pc = panel(&mut p);
        // from the hem (y = 100) up to the top: the hem opens 40 mm, the top stays closed
        spread_piece(
            &mut p,
            &pc,
            Vec2::new(100.0, 110.0),
            Vec2::new(100.0, -10.0),
            Spread::Fan { amount: 40.0 },
        )
        .expect("spread");
        // the wedge: two 100 mm sides with a 40 mm chord between their ends
        let wedge = 0.5 * 40.0 * (100.0_f64 * 100.0 - 20.0 * 20.0).sqrt();
        assert!(
            (area(&p, &pc) - (20000.0 + wedge)).abs() < 1.0,
            "a wedge: {}",
            area(&p, &pc)
        );
        assert_eq!(
            p.pieces[&pc].pts.len(),
            7,
            "a corner at each crossing, and the open one doubled"
        );
    }
}
