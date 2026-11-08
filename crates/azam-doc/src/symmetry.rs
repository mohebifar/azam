//! Symmetric editing of one piece, as CLO's unfolded symmetric editing works: a piece can carry
//! a vertical axis, and moving an outline point on one side moves its mirror partner on the
//! other (a point on the axis stays on it). Partners are the points that mirror each other across
//! the axis before an edit, so a piece drafted symmetric stays symmetric however it is reshaped.

use azam_geom::Vec2;
use serde_json::Value;

use crate::{PieceId, Project};

/// [`crate::Piece::extra`] key holding a piece's symmetry axis (its x, in mm).
pub const SYMMETRY_KEY: &str = "symmetryAxis";
/// Points this close to each other's mirror image (or to the axis), in mm, are partners.
const PARTNER_TOLERANCE_MM: f64 = 0.5;
/// A point that moved less than this, in mm, counts as unmoved.
const MOVED_MM: f64 = 1e-6;

/// The symmetry axis of `piece` in `project`, if it is edited symmetrically.
#[must_use]
pub fn symmetry_axis(project: &Project, piece: &PieceId) -> Option<f64> {
    project
        .pieces
        .get(piece)?
        .extra
        .get(SYMMETRY_KEY)?
        .as_f64()
        .filter(|x| x.is_finite())
}

/// Turns symmetric editing of `piece` on (about the vertical line through the middle of its
/// outline) or off. Returns the axis when turned on; `None` when turned off or when the piece is
/// missing.
pub fn toggle_symmetry(project: &mut Project, piece: &PieceId) -> Option<f64> {
    let pc = project.pieces.get_mut(piece)?;
    if pc.extra.remove(SYMMETRY_KEY).is_some() {
        return None;
    }
    let bb = azam_geom::bbox(pc.pts.iter().map(|p| p.pos))?;
    let axis = bb.center().x;
    pc.extra.insert(SYMMETRY_KEY.to_owned(), Value::from(axis));
    Some(axis)
}

/// Each symmetric piece's outline points, taken before an edit so [`sync_symmetry`] can tell
/// which side moved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SymmetrySnapshot(Vec<PieceSnapshot>);

/// One symmetric piece before an edit: its id, its axis and its outline points.
type PieceSnapshot = (PieceId, f64, Vec<(azam_geom::PointId, Vec2)>);

/// The outline points of every symmetric piece now.
#[must_use]
pub fn symmetry_snapshot(project: &Project) -> SymmetrySnapshot {
    SymmetrySnapshot(
        project
            .pieces
            .values()
            .filter_map(|pc| {
                let axis = symmetry_axis(project, &pc.id)?;
                Some((
                    pc.id.clone(),
                    axis,
                    pc.pts.iter().map(|p| (p.id.clone(), p.pos)).collect(),
                ))
            })
            .collect(),
    )
}

/// After an edit: for each symmetric piece, every point that moved while its partner (as of
/// `before`) did not carries its partner along, mirrored; a point on the axis that moved slides
/// back onto it vertically.
pub fn sync_symmetry(project: &mut Project, before: &SymmetrySnapshot) {
    for (piece, axis, old) in &before.0 {
        let Some(pc) = project.pieces.get_mut(piece) else {
            continue;
        };
        if !pc.extra.contains_key(SYMMETRY_KEY) {
            continue;
        }
        let mirror = |p: Vec2| Vec2::new(2.0 * axis - p.x, p.y);
        let now = |pc: &crate::Piece, id: &azam_geom::PointId| pc.pts.iter().position(|p| &p.id == id);
        let mut updates: Vec<(usize, Vec2)> = Vec::new();
        for (i, (id, was)) in old.iter().enumerate() {
            let Some(k) = now(pc, id) else { continue };
            let pos = pc.pts[k].pos;
            if (pos - *was).length() <= MOVED_MM {
                continue;
            }
            if (was.x - axis).abs() <= PARTNER_TOLERANCE_MM {
                updates.push((k, Vec2::new(*axis, pos.y)));
                continue;
            }
            // its partner: the point that mirrored it before
            let partner = old
                .iter()
                .enumerate()
                .find(|&(j, (_, q))| j != i && (mirror(*q) - *was).length() <= PARTNER_TOLERANCE_MM);
            let Some((_, (pid, pwas))) = partner else { continue };
            let Some(pk) = now(pc, pid) else { continue };
            // both sides moved (a move of the whole piece, say): leave them be
            if (pc.pts[pk].pos - *pwas).length() > MOVED_MM {
                continue;
            }
            updates.push((pk, mirror(pos)));
        }
        let moved_whole = !old.is_empty()
            && old
                .iter()
                .all(|(id, was)| now(pc, id).is_some_and(|k| (pc.pts[k].pos - *was).length() > MOVED_MM));
        for (k, pos) in updates {
            pc.pts[k].pos = pos;
        }
        // a piece moved or turned as a whole takes its axis along
        if moved_whole && let Some(bb) = azam_geom::bbox(pc.pts.iter().map(|p| p.pos)) {
            pc.extra
                .insert(SYMMETRY_KEY.to_owned(), Value::from(bb.center().x));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tee_front() -> (Project, PieceId) {
        let p = crate::from_template(crate::Template::Tee, crate::Sex::Female, None, None, 0.0);
        let id = p
            .pieces
            .values()
            .find(|pc| pc.name == "Front")
            .expect("front")
            .id
            .clone();
        (p, id)
    }

    fn pos(p: &Project, piece: &PieceId, id: &str) -> Vec2 {
        let pc = &p.pieces[piece];
        pc.pts[pc.point_index(&azam_geom::PointId::new(id)).expect("point")].pos
    }

    #[test]
    fn moving_one_side_moves_its_mirror() {
        let (mut p, f) = tee_front();
        let axis = toggle_symmetry(&mut p, &f).expect("on");
        let before = symmetry_snapshot(&p);
        let pc = p.pieces.get_mut(&f).expect("front");
        let k = pc
            .point_index(&azam_geom::PointId::new("shL"))
            .expect("left shoulder");
        pc.pts[k].pos += Vec2::new(-12.0, 8.0);
        sync_symmetry(&mut p, &before);
        let (l, r) = (pos(&p, &f, "shL"), pos(&p, &f, "shR"));
        assert!((l.x + r.x - 2.0 * axis).abs() < 1e-9, "mirrored across the axis");
        assert!((l.y - r.y).abs() < 1e-9, "at the same height");
        // off again: one side moves alone
        assert!(toggle_symmetry(&mut p, &f).is_none());
        let before = symmetry_snapshot(&p);
        let pc = p.pieces.get_mut(&f).expect("front");
        let k = pc
            .point_index(&azam_geom::PointId::new("shL"))
            .expect("left shoulder");
        pc.pts[k].pos.x -= 5.0;
        sync_symmetry(&mut p, &before);
        assert!((pos(&p, &f, "shL").x + pos(&p, &f, "shR").x - 2.0 * axis).abs() > 4.0);
    }

    #[test]
    fn a_point_on_the_axis_stays_on_it_and_moving_the_piece_takes_the_axis_along() {
        let (mut p, f) = tee_front();
        let axis = toggle_symmetry(&mut p, &f).expect("on");
        // a point on the axis (a hem or neck centre curve point), if the draft has one
        let on_axis = p.pieces[&f].pts.iter().position(|q| (q.pos.x - axis).abs() < 0.5);
        if let Some(k) = on_axis {
            let before = symmetry_snapshot(&p);
            let pc = p.pieces.get_mut(&f).expect("front");
            pc.pts[k].pos += Vec2::new(9.0, 4.0);
            sync_symmetry(&mut p, &before);
            assert!((p.pieces[&f].pts[k].pos.x - axis).abs() < 1e-9);
        }
        let before = symmetry_snapshot(&p);
        let pc = p.pieces.get_mut(&f).expect("front");
        for q in &mut pc.pts {
            q.pos.x += 100.0;
        }
        sync_symmetry(&mut p, &before);
        let moved = symmetry_axis(&p, &f).expect("still on");
        assert!(
            (moved - axis - 100.0).abs() < 1e-6,
            "the axis came along: {moved} vs {axis}"
        );
    }
}
