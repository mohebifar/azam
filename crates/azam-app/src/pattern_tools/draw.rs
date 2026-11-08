//! Drawing tools: pieces and internal lines point by point, rectangles and ellipses dragged out as
//! pieces, and internal ellipses.

use azam_doc::{LinePoint, PieceId, Shape};
use azam_geom::Vec2;

use super::snap::{constrain_45, square_box};
use super::{DraftState, Press};
use crate::app::AzamApp;
use crate::operators::Tool;
use crate::pattern_input::PatternDrag;
use crate::selection::Part;

/// A click this close to a draft's first point closes it, in logical pixels.
const CLOSE_PX: f64 = 9.0;

impl AzamApp {
    pub(super) fn draft_press(&mut self, press: &Press) {
        if press.double {
            // The first press of the double click already placed the last point.
            if self.tools.draft.is_some() {
                self.finish_draft(false);
            }
            return;
        }
        let snapped = self.snap_point(press.q, None, press.alt);
        self.tools.snap = None;
        if self.tools.draft.is_none() {
            let piece = if self.tool == Tool::InternalLine {
                let Some(piece) = press
                    .hit
                    .piece
                    .clone()
                    .or_else(|| press.hit.segment.as_ref().map(|s| s.piece.clone()))
                else {
                    self.status = "Click inside a piece to start an internal line".into();
                    return;
                };
                Some(piece)
            } else {
                None
            };
            self.tools.draft = Some(DraftState {
                piece,
                points: Vec::new(),
            });
        }
        let zoom = self.pattern_view.zoom;
        let Some(draft) = &mut self.tools.draft else {
            return;
        };
        if draft.points.len() >= 2
            && let Some((first, _)) = draft.points.first()
            && first.distance(press.q) * zoom < CLOSE_PX
        {
            self.finish_draft(true);
            return;
        }
        let mut q = snapped;
        if press.shift
            && let Some((last, _)) = draft.points.last()
        {
            q = constrain_45(*last, q);
        }
        let curve = press.alt && !draft.points.is_empty();
        draft.points.push((q, curve));
    }

    /// Finishes the polygon or internal line being drawn; `closed` when it was closed by clicking
    /// its first point.
    pub(super) fn finish_draft(&mut self, closed: bool) {
        let Some(draft) = self.tools.draft.take() else {
            return;
        };
        match draft.piece {
            None => {
                if draft.points.len() < azam_doc::MIN_CORNERS {
                    self.status = "A piece needs at least three points".into();
                    return;
                }
                let mut created = None;
                self.session.edit("Create Pattern", |p| {
                    created = azam_doc::create_piece(p, &draft.points);
                });
                if let Some(id) = created {
                    self.session.selection.select_only(id);
                    self.status = "Created a piece".into();
                }
            }
            Some(piece) => {
                if draft.points.len() < 2 {
                    self.status = "An internal line needs at least two points".into();
                    return;
                }
                let points: Vec<LinePoint> = draft
                    .points
                    .iter()
                    .map(|&(pos, curve)| LinePoint { id: None, pos, curve })
                    .collect();
                let mut created = false;
                self.session.edit("Internal Line", |p| {
                    created = azam_doc::add_internal_line(p, &piece, &points, closed).is_some();
                });
                if created
                    && let Some(index) = self
                        .session
                        .project
                        .pieces
                        .get(&piece)
                        .map(|pc| pc.internals.len() - 1)
                {
                    self.session.selection.select_only(piece.clone());
                    self.show_part(Part::Internal { piece, index });
                    self.status = "Added an internal line".into();
                }
            }
        }
    }

    pub(super) fn shape_press(&mut self, press: &Press) {
        let piece = if self.tool == Tool::InternalEllipse {
            let Some(piece) = press.hit.piece.clone() else {
                self.status = "Drag inside a piece".into();
                return;
            };
            Some(piece)
        } else {
            None
        };
        let from = self.snap_point(press.q, None, press.alt);
        self.pattern_drag = Some(PatternDrag::Shape {
            tool: self.tool,
            piece,
            from,
            to: from,
        });
    }

    pub(super) fn shape_drag(&mut self, drag: PatternDrag, q: Vec2, shift: bool, alt: bool) {
        if let PatternDrag::Shape {
            tool, piece, from, ..
        } = drag
        {
            let mut to = self.snap_point(q, None, alt);
            if shift {
                to = square_box(from, to);
            }
            self.pattern_drag = Some(PatternDrag::Shape {
                tool,
                piece,
                from,
                to,
            });
        }
    }

    pub(super) fn shape_end(&mut self, tool: Tool, piece: Option<&PieceId>, from: Vec2, to: Vec2) {
        let shape = if tool == Tool::Rectangle {
            Shape::Rectangle
        } else {
            Shape::Ellipse
        };
        if shape.points(from, to).is_none() {
            return;
        }
        if let Some(piece) = piece {
            let mut created = false;
            self.session.edit("Internal Ellipse", |p| {
                created = azam_doc::add_internal_ellipse(p, piece, from, to).is_some();
            });
            if created
                && let Some(index) = self
                    .session
                    .project
                    .pieces
                    .get(piece)
                    .map(|pc| pc.internals.len() - 1)
            {
                self.show_part(Part::Internal {
                    piece: piece.clone(),
                    index,
                });
            }
        } else {
            let mut created = None;
            self.session.edit("Create Pattern", |p| {
                created = azam_doc::create_shape_piece(p, shape, from, to);
            });
            if let Some(id) = created {
                self.session.selection.select_only(id);
                self.status = format!("Created a {}", shape.name().to_lowercase());
                self.remember_last(crate::redo_panel::LastKind::Shape(crate::redo_panel::LastShape {
                    shape,
                    from,
                    size: to - from,
                }));
            }
        }
    }
}
