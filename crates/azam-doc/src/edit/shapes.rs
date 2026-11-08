//! Drawing pieces and internal shapes from a dragged box: rectangles and ellipses.

use azam_geom::{OutlinePoint, Vec2};
use serde_json::Value;

use crate::ids::{PartId, PieceId};
use crate::parts::{Extras, LINE_CLOSED_KEY, LINE_FOLD_KEY};
use crate::piece::{InternalLine, LinePoint};
use crate::project::Project;

/// Distance of a cubic Bézier control point from its end, as a fraction of the radius, for a
/// quarter ellipse (4/3·(√2 − 1), rounded as stored drafts use it).
pub const ELLIPSE_KAPPA: f64 = 0.5523;
/// Boxes thinner than this make no shape, in mm.
pub const MIN_SHAPE_MM: f64 = 3.0;

/// A shape drawn from a box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// The box itself.
    Rectangle,
    /// The ellipse inscribed in the box.
    Ellipse,
}

impl Shape {
    /// Name prefix of a piece drawn with this shape.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
        }
    }

    /// Outline points `(position, curve point)` of the shape in the box with corners `a` and `b`
    /// (in any order), clockwise on the y-down page starting at the right; `None` if the box is
    /// thinner than [`MIN_SHAPE_MM`].
    #[must_use]
    pub fn points(self, a: Vec2, b: Vec2) -> Option<Vec<(Vec2, bool)>> {
        let (lo, hi) = (a.min(b), a.max(b));
        if hi.x - lo.x < MIN_SHAPE_MM || hi.y - lo.y < MIN_SHAPE_MM {
            return None;
        }
        Some(match self {
            Self::Rectangle => vec![
                (lo, false),
                (Vec2::new(hi.x, lo.y), false),
                (hi, false),
                (Vec2::new(lo.x, hi.y), false),
            ],
            Self::Ellipse => {
                let c = (lo + hi) / 2.0;
                let r = (hi - lo) / 2.0;
                let k = ELLIPSE_KAPPA;
                [
                    (1.0, 0.0, false),
                    (1.0, k, true),
                    (k, 1.0, true),
                    (0.0, 1.0, false),
                    (-k, 1.0, true),
                    (-1.0, k, true),
                    (-1.0, 0.0, false),
                    (-1.0, -k, true),
                    (-k, -1.0, true),
                    (0.0, -1.0, false),
                    (k, -1.0, true),
                    (1.0, -k, true),
                ]
                .into_iter()
                .map(|(x, y, curve)| (Vec2::new(c.x + r.x * x, c.y + r.y * y), curve))
                .collect()
            }
        })
    }
}

/// Adds a piece shaped as `shape` in the box from `a` to `b`, named after the shape and numbered.
/// Returns its id, or `None` if the box is too thin.
pub fn create_shape_piece(project: &mut Project, shape: Shape, a: Vec2, b: Vec2) -> Option<PieceId> {
    let points = shape.points(a, b)?;
    let number = project.pieces.len() + 1;
    let id = super::create_piece(project, &points)?;
    if let Some(pc) = project.pieces.get_mut(&id) {
        pc.name = format!("{} {number}", shape.name());
    }
    Some(id)
}

/// Adds an elliptical internal line inscribed in the box from `a` to `b` to `piece` (stored as an
/// open line that returns to its first point). Returns its id, or `None` if there is no such piece
/// or the box is too thin.
pub fn add_internal_ellipse(project: &mut Project, piece: &PieceId, a: Vec2, b: Vec2) -> Option<PartId> {
    let mut points: Vec<LinePoint> = Shape::Ellipse
        .points(a, b)?
        .into_iter()
        .map(|(pos, curve)| LinePoint { id: None, pos, curve })
        .collect();
    points.push(points[0].clone());
    super::add_internal_line(project, piece, &points, false)
}

/// The internal line at `index` of `piece`, to change.
fn line_mut<'a>(project: &'a mut Project, piece: &PieceId, index: usize) -> Option<&'a mut InternalLine> {
    project.pieces.get_mut(piece)?.internals.get_mut(index)
}

/// Moves internal line `index` of `piece` by `delta`. Returns `false` if there is no such line.
pub fn translate_internal_line(project: &mut Project, piece: &PieceId, index: usize, delta: Vec2) -> bool {
    let Some(il) = line_mut(project, piece, index) else {
        return false;
    };
    for p in &mut il.pts {
        p.pos += delta;
    }
    true
}

/// Deletes internal line `index` of `piece`. Returns `false` if there is no such line.
pub fn delete_internal_line(project: &mut Project, piece: &PieceId, index: usize) -> bool {
    match project.pieces.get_mut(piece) {
        Some(pc) if index < pc.internals.len() => {
            pc.internals.remove(index);
            true
        }
        _ => false,
    }
}

/// Makes internal line `index` of `piece` a fold line at `degrees` (180 is flat), or an ordinary
/// line when `degrees` is `None`. Returns `false` if there is no such line.
pub fn set_internal_fold(project: &mut Project, piece: &PieceId, index: usize, degrees: Option<f64>) -> bool {
    let Some(il) = line_mut(project, piece, index) else {
        return false;
    };
    il.set(LINE_FOLD_KEY, degrees.map_or(Value::Null, Value::from));
    true
}

/// Sets whether internal line `index` of `piece` closes on itself. Returns `false` if there is no
/// such line.
pub fn set_internal_closed(project: &mut Project, piece: &PieceId, index: usize, closed: bool) -> bool {
    let Some(il) = line_mut(project, piece, index) else {
        return false;
    };
    il.set(LINE_CLOSED_KEY, closed);
    true
}

/// The fold angle of `line` in degrees, if it is a fold line.
#[must_use]
pub fn internal_fold(line: &InternalLine) -> Option<f64> {
    line.extra.get(LINE_FOLD_KEY).and_then(Value::as_f64)
}

/// Moves points `indices` of `piece` together so their median's `x` (`along_x`) or `y` is `value`
/// (Blender's sidebar Median). Returns `false` if there is no such piece or point.
pub fn set_points_median(
    project: &mut Project,
    piece: &PieceId,
    indices: &[usize],
    along_x: bool,
    value: f64,
) -> bool {
    let Some(pc) = project.pieces.get_mut(piece) else {
        return false;
    };
    if indices.is_empty() || indices.iter().any(|&i| i >= pc.pts.len()) {
        return false;
    }
    let now = indices.iter().map(|&i| pc.pts[i].pos).sum::<Vec2>() / indices.len() as f64;
    let d = if along_x {
        Vec2::new(value - now.x, 0.0)
    } else {
        Vec2::new(0.0, value - now.y)
    };
    for &i in indices {
        pc.pts[i].pos += d;
    }
    true
}

/// Makes the straight segment that starts at corner `index` of `piece` `length` mm long by moving
/// its far corner along it. Returns `false` if the segment is curved (either end is a curve
/// point), has no length, or there is no such point.
pub fn set_segment_length(project: &mut Project, piece: &PieceId, index: usize, length: f64) -> bool {
    let Some(pc) = project.pieces.get_mut(piece) else {
        return false;
    };
    let n = pc.pts.len();
    if index >= n || n < 2 || length.is_nan() || length <= 0.0 {
        return false;
    }
    let next = (index + 1) % n;
    if pc.pts[index].curve || pc.pts[next].curve {
        return false;
    }
    let a = pc.pts[index].pos;
    let dir = (pc.pts[next].pos - a).normalize_or_zero();
    if dir == Vec2::ZERO {
        return false;
    }
    pc.pts[next].pos = a + dir * length;
    true
}

/// Turns point `index` of `piece` into a curve point (`curve`) or a corner, keeping seams, notches
/// and topstitches attached. The first point stays a corner, and at least [`super::MIN_CORNERS`]
/// corners remain. Returns `false` if nothing changed.
pub fn set_point_curve(project: &mut Project, piece: &PieceId, index: usize, curve: bool) -> bool {
    let Some(pc) = project.pieces.get_mut(piece) else {
        return false;
    };
    let corners = pc.pts.iter().filter(|p| !p.curve).count();
    match pc.pts.get(index) {
        Some(p) if p.curve == curve => return false,
        Some(_) if curve && (index == 0 || corners <= super::MIN_CORNERS) => return false,
        None => return false,
        Some(_) => {}
    }
    let old: Vec<OutlinePoint> = pc.pts.clone();
    pc.pts[index].curve = curve;
    super::remap_refs(project, piece, &old);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;

    #[test]
    fn points_move_by_their_median_and_straight_segments_take_a_length() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let pc = p.add_piece(crate::piece::Piece::new(
            PieceId::new("pc"),
            "Box",
            vec![
                OutlinePoint::corner("a", 0.0, 0.0),
                OutlinePoint::corner("b", 100.0, 0.0),
                OutlinePoint::corner("c", 100.0, 50.0),
                OutlinePoint::corner("d", 0.0, 50.0),
            ],
        ));
        assert!(set_points_median(&mut p, &pc, &[1, 2], true, 120.0));
        assert_eq!(p.pieces[&pc].pts[1].pos, Vec2::new(120.0, 0.0));
        assert_eq!(p.pieces[&pc].pts[2].pos, Vec2::new(120.0, 50.0));
        assert!(set_points_median(&mut p, &pc, &[0], false, -10.0));
        assert_eq!(p.pieces[&pc].pts[0].pos, Vec2::new(0.0, -10.0));
        assert!(!set_points_median(&mut p, &pc, &[9], true, 0.0), "no such point");
        // the top edge, from a to b, becomes 80 mm long along its own direction
        assert!(set_segment_length(&mut p, &pc, 0, 80.0));
        let (a, b) = (p.pieces[&pc].pts[0].pos, p.pieces[&pc].pts[1].pos);
        assert!((a.distance(b) - 80.0).abs() < 1e-9);
        p.pieces.get_mut(&pc).expect("piece").pts[2].curve = true;
        assert!(
            !set_segment_length(&mut p, &pc, 1, 40.0),
            "a curved segment keeps its shape"
        );
    }

    #[test]
    fn rectangles_are_four_corners_in_box_order() {
        let pts = Shape::Rectangle
            .points(Vec2::new(10.0, 50.0), Vec2::new(0.0, 0.0))
            .expect("box");
        let pos: Vec<Vec2> = pts.iter().map(|p| p.0).collect();
        assert_eq!(
            pos,
            vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(10.0, 50.0),
                Vec2::new(0.0, 50.0)
            ]
        );
        assert!(pts.iter().all(|p| !p.1));
        assert!(
            Shape::Rectangle
                .points(Vec2::ZERO, Vec2::new(2.0, 50.0))
                .is_none()
        );
    }

    #[test]
    fn ellipses_pass_through_the_box_midpoints() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let id = create_shape_piece(
            &mut p,
            Shape::Ellipse,
            Vec2::new(0.0, 0.0),
            Vec2::new(200.0, 100.0),
        )
        .expect("ellipse");
        let pc = &p.pieces[&id];
        assert_eq!(pc.name, "Ellipse 1");
        assert_eq!(pc.pts.len(), 12);
        assert_eq!(pc.pts.iter().filter(|q| !q.curve).count(), 4);
        let ol = pc.outline();
        // Cubic quarters with this kappa are within 0.1% of the true perimeter (Ramanujan).
        let perimeter =
            std::f64::consts::PI * (3.0 * 150.0 - ((3.0_f64 * 100.0 + 50.0) * (100.0 + 3.0 * 50.0)).sqrt());
        assert!(
            (ol.total - perimeter).abs() / perimeter < 1e-3,
            "{} vs {perimeter}",
            ol.total
        );
        assert!(ol.segments[0].poly[0].distance(Vec2::new(200.0, 50.0)) < 1e-9);
    }

    #[test]
    fn internal_ellipses_close_back_on_their_start() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let id =
            create_shape_piece(&mut p, Shape::Rectangle, Vec2::ZERO, Vec2::new(300.0, 300.0)).expect("piece");
        let il =
            add_internal_ellipse(&mut p, &id, Vec2::new(50.0, 50.0), Vec2::new(150.0, 100.0)).expect("line");
        let line = &p.pieces[&id].internals[0];
        assert_eq!(line.id.as_ref(), Some(&il));
        assert_eq!(line.pts.len(), 13);
        assert_eq!(line.pts.first().map(|q| q.pos), line.pts.last().map(|q| q.pos));
        assert!(translate_internal_line(&mut p, &id, 0, Vec2::new(5.0, 5.0)));
        assert!((p.pieces[&id].internals[0].pts[0].pos - Vec2::new(155.0, 80.0)).length() < 1e-9);
        assert!(set_internal_fold(&mut p, &id, 0, Some(90.0)));
        assert_eq!(internal_fold(&p.pieces[&id].internals[0]), Some(90.0));
        assert!(set_internal_fold(&mut p, &id, 0, None));
        assert_eq!(internal_fold(&p.pieces[&id].internals[0]), None);
        assert!(delete_internal_line(&mut p, &id, 0));
        assert!(!delete_internal_line(&mut p, &id, 0));
    }

    #[test]
    fn points_turn_into_curve_points_and_back() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let pts = [
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (50.0, 120.0),
            (0.0, 100.0),
        ]
        .map(|(x, y)| (Vec2::new(x, y), false));
        let id = crate::create_piece(&mut p, &pts).expect("piece");
        assert!(set_point_curve(&mut p, &id, 3, true));
        assert!(p.pieces[&id].pts[3].curve);
        assert!(!set_point_curve(&mut p, &id, 3, true), "already a curve point");
        assert!(set_point_curve(&mut p, &id, 2, true));
        assert!(
            !set_point_curve(&mut p, &id, 1, true),
            "three corners must remain"
        );
        assert!(
            !set_point_curve(&mut p, &id, 0, true),
            "the first point stays a corner"
        );
        assert!(set_point_curve(&mut p, &id, 3, false));
    }
}
