//! Editing outlines: moving points and segments, splitting segments, bending them with the
//! curvature tool, adding and moving curve control points, and moving internal lines.

use azam_doc::PieceId;
use azam_geom::Vec2;

use super::Press;
use super::hit::shown_points;
use crate::app::AzamApp;
use crate::pattern_input::PatternDrag;
use crate::selection::Part;

/// Runs `f` on `piece` with the project's (seeded) id generator.
fn with_ids<R>(
    p: &mut azam_doc::Project,
    piece: &PieceId,
    f: impl FnOnce(&mut azam_doc::Piece, &mut azam_doc::IdGen) -> R,
) -> Option<R> {
    p.ensure_ids_seeded();
    let mut ids = std::mem::take(&mut p.ids);
    let out = p.pieces.get_mut(piece).map(|pc| f(pc, &mut ids));
    p.ids = ids;
    out
}

impl AzamApp {
    /// A drag of points `indices` of `piece` starting at `q`.
    fn start_point_drag(&mut self, piece: &PieceId, indices: Vec<usize>, q: Vec2, snapping: Option<usize>) {
        if self.refuse_mirror(piece) {
            return;
        }
        let Some(pc) = self.session.project.pieces.get(piece) else {
            return;
        };
        let orig = indices
            .iter()
            .filter_map(|&i| pc.pts.get(i).map(|p| p.pos))
            .collect();
        self.pattern_drag = Some(PatternDrag::MovePoints {
            piece: piece.clone(),
            indices,
            start: q,
            orig,
            snapping,
        });
    }

    pub(super) fn edit_press(&mut self, press: &Press) {
        let hit = &press.hit;
        if press.double
            && hit.point.is_none()
            && let Some(seg) = hit.segment.clone()
        {
            self.split_at(&seg.piece, &seg.segment, seg.t);
            return;
        }
        if let Some(pt) = hit.point.clone() {
            let sel = &mut self.session.selection;
            let mut points = if sel.active.as_ref() == Some(&pt.piece) {
                sel.points.clone()
            } else {
                Vec::new()
            };
            if press.extend {
                if let Some(k) = points.iter().position(|&i| i == pt.index) {
                    points.remove(k);
                } else {
                    points.push(pt.index);
                }
            } else if !points.contains(&pt.index) {
                points = vec![pt.index];
            }
            sel.select_only(pt.piece.clone());
            sel.points.clone_from(&points);
            let snapping = (points.len() == 1).then_some(pt.index);
            self.start_point_drag(&pt.piece, points, press.q, snapping);
        } else if let Some(seg) = hit.segment.clone() {
            self.session
                .selection
                .select_segment(seg.piece.clone(), seg.segment.clone());
            let indices = self.segment_point_indices(&seg.piece, seg.index);
            self.start_point_drag(&seg.piece, indices, press.q, None);
        } else if let Some((piece, index)) = hit.internal.clone() {
            self.session.selection.select_only(piece.clone());
            self.show_part(Part::Internal {
                piece: piece.clone(),
                index,
            });
            self.pattern_drag = Some(PatternDrag::MoveInternal {
                piece,
                index,
                last: press.q,
            });
        } else if let Some(piece) = hit.piece.clone() {
            self.session.selection.select_only(piece.clone());
            let ids = super::movable(&self.session.project, [piece]);
            self.pattern_drag = Some(PatternDrag::MovePieces { ids, last: press.q });
        } else {
            self.session.selection.clear();
            self.pattern_drag = Some(PatternDrag::SelectBox {
                from: press.q,
                to: press.q,
                points: true,
            });
        }
    }

    /// Indices of the points of segment `k` of `piece`: its corners and curve points.
    fn segment_point_indices(&self, piece: &PieceId, k: usize) -> Vec<usize> {
        let Some(pc) = self.session.project.pieces.get(piece) else {
            return Vec::new();
        };
        let ol = pc.outline();
        let Some(seg) = ol.segments.get(k) else {
            return Vec::new();
        };
        let n = pc.pts.len();
        let mut out = vec![seg.start_point];
        let mut j = (seg.start_point + 1) % n;
        while j != seg.end_point && out.len() < n {
            out.push(j);
            j = (j + 1) % n;
        }
        out.push(seg.end_point);
        out
    }

    /// Splits segment `segment` of `piece` at fraction `t`, keeping parts attached.
    fn split_at(&mut self, piece: &PieceId, segment: &azam_doc::PointId, t: f64) {
        if self.refuse_mirror(piece) {
            return;
        }
        let mut added = None;
        self.session.edit("Add Point", |p| {
            let Some(old) = p.pieces.get(piece).map(|pc| pc.pts.clone()) else {
                return;
            };
            added = with_ids(p, piece, |pc, ids| azam_doc::split_segment(pc, segment, t, ids)).flatten();
            azam_doc::remap_refs(p, piece, &old);
            azam_doc::sync_mirrors(p);
        });
        if let Some(id) = added
            && let Some(index) = self
                .session
                .project
                .pieces
                .get(piece)
                .and_then(|pc| pc.point_index(&id))
        {
            self.session.selection.select_only(piece.clone());
            self.session.selection.points = vec![index];
            self.status = "Added a point".into();
        } else {
            self.status = "Could not split that segment".into();
        }
    }

    /// Whether `piece` is a linked mirror (edit its source instead); says so in the status bar.
    pub(super) fn refuse_mirror(&mut self, piece: &PieceId) -> bool {
        let mirrored = self
            .session
            .project
            .pieces
            .get(piece)
            .is_some_and(|p| p.mirror.is_some());
        if mirrored {
            self.status = "Edit the source piece: this mirror is linked to it".into();
        }
        mirrored
    }

    pub(super) fn add_point_press(&mut self, press: &Press) {
        match press.hit.segment.clone() {
            Some(seg) => self.split_at(&seg.piece, &seg.segment, seg.t),
            None => self.status = "Click on a segment to add a point".into(),
        }
    }

    pub(super) fn curvature_press(&mut self, press: &Press) {
        let Some(seg) = press.hit.segment.clone() else {
            return;
        };
        if self.refuse_mirror(&seg.piece) {
            return;
        }
        let Some(orig) = self.session.project.pieces.get(&seg.piece).map(|p| p.pts.clone()) else {
            return;
        };
        self.session
            .selection
            .select_segment(seg.piece.clone(), seg.segment.clone());
        self.pattern_drag = Some(PatternDrag::Curvature {
            piece: seg.piece,
            segment: seg.segment,
            orig,
        });
    }

    pub(super) fn curve_point_press(&mut self, press: &Press) {
        if let Some(pt) = press.hit.point.clone().filter(|p| p.curve) {
            if press.alt {
                if self.refuse_mirror(&pt.piece) {
                    return;
                }
                let mut changed = false;
                self.session.edit("Convert to Corner", |p| {
                    changed = azam_doc::set_point_curve(p, &pt.piece, pt.index, false);
                    azam_doc::sync_mirrors(p);
                });
                self.status = if changed {
                    "Made a corner".into()
                } else {
                    "That point cannot change".into()
                };
                return;
            }
            self.session.selection.select_only(pt.piece.clone());
            self.session.selection.points = vec![pt.index];
            self.start_point_drag(&pt.piece, vec![pt.index], press.q, None);
            return;
        }
        let Some(seg) = press.hit.segment.clone() else {
            return;
        };
        if self.refuse_mirror(&seg.piece) {
            return;
        }
        let mut added = None;
        self.session.edit("Add Curve Point", |p| {
            let Some(old) = p.pieces.get(&seg.piece).map(|pc| pc.pts.clone()) else {
                return;
            };
            added = with_ids(p, &seg.piece, |pc, ids| {
                azam_doc::add_curve_point(pc, &seg.segment, seg.t, seg.pos, ids)
            })
            .flatten();
            azam_doc::remap_refs(p, &seg.piece, &old);
            azam_doc::sync_mirrors(p);
        });
        let index = added.and_then(|id| self.session.project.pieces.get(&seg.piece)?.point_index(&id));
        if let Some(index) = index {
            self.session.selection.select_only(seg.piece.clone());
            self.session.selection.points = vec![index];
            self.start_point_drag(&seg.piece, vec![index], press.q, None);
        }
    }

    pub(super) fn edit_drag(&mut self, drag: PatternDrag, q: Vec2, alt: bool) {
        match drag {
            PatternDrag::MovePoints {
                piece,
                indices,
                start,
                orig,
                snapping,
            } => {
                let mut delta = q - start;
                if let (Some(k), Some(first)) = (snapping, orig.first()) {
                    // Snap the dragged point itself (in the shown size) to other corners.
                    let shown = shown_points(&self.session.project, &piece)
                        .get(k)
                        .map_or(*first, |p| p.0);
                    let base_to_shown = shown
                        - self
                            .session
                            .project
                            .pieces
                            .get(&piece)
                            .map_or(*first, |pc| pc.pts[k].pos);
                    let target = self.snap_point(*first + base_to_shown + delta, Some((&piece, k)), alt);
                    delta = target - base_to_shown - *first;
                }
                self.session.edit("Move Points", |p| {
                    if let Some(pc) = p.pieces.get_mut(&piece) {
                        for (&i, &o) in indices.iter().zip(&orig) {
                            if let Some(pt) = pc.pts.get_mut(i) {
                                pt.pos = o + delta;
                            }
                        }
                    }
                    azam_doc::sync_mirrors(p);
                });
                self.pattern_drag = Some(PatternDrag::MovePoints {
                    piece,
                    indices,
                    start,
                    orig,
                    snapping,
                });
            }
            PatternDrag::MoveInternal { piece, index, last } => {
                let delta = q - last;
                if delta.length_squared() > 0.0 {
                    self.session.edit("Move Internal Line", |p| {
                        azam_doc::translate_internal_line(p, &piece, index, delta);
                    });
                }
                self.pattern_drag = Some(PatternDrag::MoveInternal {
                    piece,
                    index,
                    last: q,
                });
            }
            PatternDrag::Curvature { piece, segment, orig } => {
                self.session.edit("Edit Curvature", |p| {
                    with_ids(p, &piece, |pc, ids| {
                        pc.pts.clone_from(&orig);
                        azam_doc::set_curvature(pc, &segment, q, ids)
                    });
                    azam_doc::sync_mirrors(p);
                });
                self.pattern_drag = Some(PatternDrag::Curvature { piece, segment, orig });
            }
            _ => {}
        }
    }
}
