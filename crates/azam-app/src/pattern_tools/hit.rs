//! What lies under the pointer in the pattern editor: parts, points, segments, internal lines,
//! pieces, graphics, seams and topstitches, all at once so each tool picks what it needs.

use azam_doc::{Extras, ItemId, ItemKind, PieceId, PointId, Project, SeamId, StitchId, StitchStyle};
use azam_geom::{DEFAULT_SIDE_STEP_MM, Outline, Vec2, contains, distance_to_segment};

use crate::scene::{internal_polyline, inward_sign, offset_polyline};

/// Pick distance, in logical pixels.
pub const PICK_PX: f64 = 7.0;
/// Height of a comment pin's head centre above the point it marks, in logical pixels.
const PIN_HEAD_UP_PX: f64 = 10.0;
/// Pick radius of a comment pin's head, in logical pixels.
const PIN_PICK_PX: f64 = 11.0;

/// An outline point under the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct PointHit {
    /// The piece.
    pub piece: PieceId,
    /// Index of the point.
    pub index: usize,
    /// Whether it is a curve point.
    pub curve: bool,
    /// Its position (in the shown size).
    pub pos: Vec2,
}

/// A segment under the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentHit {
    /// The piece.
    pub piece: PieceId,
    /// The segment's id.
    pub segment: PointId,
    /// Index of the segment in the outline.
    pub index: usize,
    /// Fraction along it nearest to the pointer.
    pub t: f64,
    /// The nearest point on it.
    pub pos: Vec2,
}

/// Everything under the pointer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hit {
    /// Where the pointer is, in pattern space.
    pub pos: Vec2,
    /// A comment pin.
    pub comment: Option<ItemId>,
    /// A button.
    pub button: Option<ItemId>,
    /// A buttonhole.
    pub hole: Option<ItemId>,
    /// An outline point.
    pub point: Option<PointHit>,
    /// A segment.
    pub segment: Option<SegmentHit>,
    /// An internal line: piece and index.
    pub internal: Option<(PieceId, usize)>,
    /// The piece the pointer is inside.
    pub piece: Option<PieceId>,
    /// A graphic: piece and index.
    pub graphic: Option<(PieceId, usize)>,
    /// A seam.
    pub seam: Option<SeamId>,
    /// A topstitch.
    pub stitch: Option<StitchId>,
}

/// Pieces in drawing order (the order the editor paints and lists them).
#[must_use]
pub fn piece_order(project: &Project) -> Vec<PieceId> {
    let mut pieces: Vec<_> = project.pieces.values().filter(|p| p.pts.len() >= 3).collect();
    pieces.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    pieces.into_iter().map(|p| p.id.clone()).collect()
}

/// The outline of `piece` in the size shown.
#[must_use]
pub fn shown_outline(project: &Project, piece: &PieceId) -> Option<Outline> {
    let pc = project.pieces.get(piece)?;
    Some(pc.graded_outline(&project.active_size, &project.base_size))
}

/// Points of `piece` in the size shown: position and whether it is a curve point.
#[must_use]
pub fn shown_points(project: &Project, piece: &PieceId) -> Vec<(Vec2, bool)> {
    let Some(pc) = project.pieces.get(piece) else {
        return Vec::new();
    };
    match &pc.grade {
        Some(rules) => azam_geom::graded_points(&pc.pts, rules, &project.active_size, &project.base_size)
            .iter()
            .map(|p| (p.pos, p.curve))
            .collect(),
        None => pc.pts.iter().map(|p| (p.pos, p.curve)).collect(),
    }
}

/// Distance from `q` to the polyline `pts`.
#[must_use]
pub fn polyline_distance(q: Vec2, pts: &[Vec2]) -> f64 {
    pts.windows(2)
        .map(|w| distance_to_segment(q, w[0], w[1]).distance)
        .fold(f64::INFINITY, f64::min)
}

fn item_pos(it: &azam_doc::Item) -> Option<Vec2> {
    Some(Vec2::new(it.x?, it.y?))
}

/// What lies under pattern point `q` with the view at `zoom` logical pixels per mm. With
/// `outline_only`, parts, seams and topstitches are left out.
#[must_use]
pub fn hit_test(project: &Project, q: Vec2, zoom: f64, outline_only: bool) -> Hit {
    let tol = PICK_PX / zoom;
    let mut hit = Hit {
        pos: q,
        ..Hit::default()
    };
    if !outline_only {
        let head = Vec2::new(0.0, PIN_HEAD_UP_PX / zoom);
        hit.comment = project
            .comments
            .values()
            .find(|c| item_pos(c).is_some_and(|p| (p - head).distance(q) < PIN_PICK_PX / zoom))
            .map(|c| c.id.clone());
        if hit.comment.is_some() {
            return hit;
        }
        let near = |kind: ItemKind, key: &str, fallback: f64| {
            project
                .items(kind)
                .values()
                .find(|it| {
                    item_pos(it).is_some_and(|p| p.distance(q) < it.num(key, fallback) / 2.0 + tol * 0.5)
                })
                .map(|it| it.id.clone())
        };
        hit.button = near(
            ItemKind::Button,
            azam_doc::BUTTON_SIZE_KEY,
            azam_doc::DEFAULT_BUTTON_SIZE_MM,
        );
        hit.hole = near(
            ItemKind::Hole,
            azam_doc::HOLE_LENGTH_KEY,
            azam_doc::DEFAULT_HOLE_LENGTH_MM,
        );
    }
    for id in piece_order(project).iter().rev() {
        let (Some(pc), Some(ol)) = (project.pieces.get(id), shown_outline(project, id)) else {
            continue;
        };
        if hit.point.is_none() {
            let mut best = tol;
            for (index, (p, curve)) in shown_points(project, id).into_iter().enumerate() {
                let d = p.distance(q);
                if d < best {
                    best = d;
                    hit.point = Some(PointHit {
                        piece: id.clone(),
                        index,
                        curve,
                        pos: p,
                    });
                }
            }
        }
        if hit.segment.is_none()
            && let Some(n) = ol.nearest(q)
            && n.distance < tol
        {
            hit.segment = Some(SegmentHit {
                piece: id.clone(),
                segment: ol.segments[n.segment].id.clone(),
                index: n.segment,
                t: n.t,
                pos: n.pos,
            });
        }
        if hit.internal.is_none() {
            hit.internal = pc
                .internals
                .iter()
                .position(|il| polyline_distance(q, &internal_polyline(il)) < tol)
                .map(|i| (id.clone(), i));
        }
        if hit.piece.is_none() && contains(&ol.poly, q) {
            hit.piece = Some(id.clone());
        }
        if hit.graphic.is_none() {
            hit.graphic = pc
                .graphics
                .iter()
                .position(|g| {
                    let size = azam_doc::graphic_size(g);
                    let (s, c) = (-g.num(azam_doc::GRAPHIC_ROTATION_KEY, 0.0))
                        .to_radians()
                        .sin_cos();
                    let d = q - Vec2::new(g.x, g.y);
                    let local = Vec2::new(d.x * c - d.y * s, d.x * s + d.y * c);
                    local.x.abs() < size.x / 2.0 && local.y.abs() < size.y / 2.0
                })
                .map(|i| (id.clone(), i));
        }
        if hit.point.is_some() && hit.segment.is_some() && hit.piece.is_some() {
            break;
        }
    }
    if !outline_only {
        hit.seam = seam_at(project, q, tol);
        hit.stitch = stitch_at(project, q, tol);
    }
    hit
}

fn seam_at(project: &Project, q: Vec2, tol: f64) -> Option<SeamId> {
    project
        .sewing
        .values()
        .find(|sw| {
            sw.a.iter().chain(&sw.b).any(|sd| {
                shown_outline(project, &sd.piece).is_some_and(|ol| {
                    let pl: Vec<Vec2> = sd
                        .side
                        .polyline(&ol, DEFAULT_SIDE_STEP_MM)
                        .iter()
                        .map(|p| p.pos)
                        .collect();
                    polyline_distance(q, &pl) < tol
                })
            })
        })
        .map(|sw| sw.id.clone())
}

fn stitch_at(project: &Project, q: Vec2, tol: f64) -> Option<StitchId> {
    project
        .stitches
        .values()
        .find(|st| {
            let Some(ol) = st.p.as_ref().and_then(|p| shown_outline(project, p)) else {
                return false;
            };
            let style = StitchStyle::of(st);
            let inward = inward_sign(&ol.poly);
            let whole = st.seg.0 == azam_doc::WHOLE_OUTLINE;
            ol.segments.iter().filter(|s| whole || s.id == st.seg).any(|s| {
                (0..style.rows).any(|r| {
                    polyline_distance(q, &offset_polyline(&s.poly, style.row_offset(r) * inward)) < tol
                })
            })
        })
        .map(|st| st.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use azam_doc::Sex;

    fn square() -> (Project, PieceId) {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let pts =
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let id = azam_doc::create_piece(&mut p, &pts).expect("piece");
        (p, id)
    }

    #[test]
    fn hits_points_segments_and_insides() {
        let (p, id) = square();
        let corner = hit_test(&p, Vec2::new(101.0, 1.0), 1.0, false);
        assert_eq!(corner.point.as_ref().map(|h| h.index), Some(1));
        let edge = hit_test(&p, Vec2::new(50.0, 2.0), 1.0, false);
        assert!(edge.point.is_none());
        let seg = edge.segment.expect("top edge");
        assert_eq!(seg.segment, p.pieces[&id].pts[0].id);
        assert!((seg.t - 0.5).abs() < 1e-9);
        assert_eq!(edge.piece, Some(id));
        assert_eq!(
            hit_test(&p, Vec2::new(200.0, 200.0), 1.0, false),
            Hit {
                pos: Vec2::new(200.0, 200.0),
                ..Hit::default()
            }
        );
    }

    #[test]
    fn hits_topstitches_inside_the_edge() {
        let (mut p, id) = square();
        let seg = p.pieces[&id].pts[0].id.clone();
        let st = azam_doc::add_stitch(&mut p, &id, Some(&seg)).expect("stitch");
        let on = hit_test(
            &p,
            Vec2::new(50.0, azam_doc::DEFAULT_STITCH_OFFSET_MM),
            1.0,
            false,
        );
        assert_eq!(on.stitch, Some(st));
        let off = hit_test(&p, Vec2::new(50.0, 50.0), 1.0, false);
        assert_eq!(off.stitch, None);
    }

    #[test]
    fn buttons_and_comments_take_precedence() {
        let (mut p, id) = square();
        let b = azam_doc::add_button(&mut p, &id, Vec2::new(50.0, 50.0)).expect("button");
        assert_eq!(hit_test(&p, Vec2::new(52.0, 50.0), 1.0, false).button, Some(b));
        let c = azam_doc::add_comment(&mut p, &id, Vec2::new(20.0, 80.0), "hi", 0.0).expect("comment");
        let h = hit_test(&p, Vec2::new(20.0, 70.0), 1.0, false);
        assert_eq!(h.comment, Some(c));
        assert!(h.piece.is_none(), "a comment hides what is under it");
    }
}
