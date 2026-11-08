//! Mouse input of the pattern editor: presses go to the current tool, drags continue what the
//! press started, and moving the pointer without a button updates the hover feedback.

use azam_doc::{ItemKind, PieceId, PointId};
use azam_geom::{OutlinePoint, Vec2};
use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::operators::Tool;
use crate::pattern_tools::hit::hit_test;

/// A drag in the pattern editor.
#[derive(Clone, Debug, PartialEq)]
pub enum PatternDrag {
    /// Middle drag: pan the view.
    Pan {
        /// Last pointer position, canvas-local.
        last: Vec2,
    },
    /// Moving pieces.
    MovePieces {
        /// The pieces (linked mirrors follow their source instead).
        ids: Vec<PieceId>,
        /// Last pointer position in pattern space.
        last: Vec2,
    },
    /// Turning the selected pieces with the rotation handle.
    Rotate {
        /// The pieces.
        ids: Vec<PieceId>,
        /// Centre of rotation.
        center: Vec2,
        /// Pointer angle around the centre when the drag started, radians.
        start: f64,
        /// Angle applied so far, radians.
        applied: f64,
    },
    /// Dragging a selection box.
    SelectBox {
        /// Where the box started.
        from: Vec2,
        /// Where it ends now.
        to: Vec2,
        /// Select points (the edit tool) rather than pieces.
        points: bool,
    },
    /// Moving outline points.
    MovePoints {
        /// The piece.
        piece: PieceId,
        /// Indices of the points.
        indices: Vec<usize>,
        /// Pointer position when the drag started.
        start: Vec2,
        /// Positions of the points when the drag started (base size).
        orig: Vec<Vec2>,
        /// The point that snaps to other corners, when one point is dragged.
        snapping: Option<usize>,
    },
    /// Moving an internal line.
    MoveInternal {
        /// The piece.
        piece: PieceId,
        /// Index of the line.
        index: usize,
        /// Last pointer position.
        last: Vec2,
    },
    /// Bending a segment.
    Curvature {
        /// The piece.
        piece: PieceId,
        /// The segment.
        segment: PointId,
        /// The piece's points when the drag started.
        orig: Vec<OutlinePoint>,
    },
    /// Dragging out a rectangle or ellipse.
    Shape {
        /// The drawing tool.
        tool: Tool,
        /// The piece an internal ellipse goes on.
        piece: Option<PieceId>,
        /// Where the drag started (snapped).
        from: Vec2,
        /// Where it ends now (snapped).
        to: Vec2,
    },
    /// Moving a button, buttonhole or comment.
    MoveItem {
        /// Kind of item.
        kind: ItemKind,
        /// The item.
        id: azam_doc::ItemId,
        /// Last pointer position.
        last: Vec2,
    },
    /// Moving a graphic.
    MoveGraphic {
        /// The piece.
        piece: PieceId,
        /// Index of the graphic.
        index: usize,
        /// Last pointer position.
        last: Vec2,
    },
    /// Scaling a graphic by a corner, keeping its proportions.
    ScaleGraphic {
        /// The piece.
        piece: PieceId,
        /// Index of the graphic.
        index: usize,
        /// Width over height.
        ratio: f64,
    },
    /// Measuring from a point.
    Measure {
        /// Where the measurement starts, in pattern space.
        from: Vec2,
    },
}

impl AzamApp {
    /// Canvas-local position of a window position.
    pub(crate) fn pattern_local(&self, position: Point<Pixels>) -> Option<Vec2> {
        let b = self.pattern_bounds?;
        Some(Vec2::new(
            f64::from(position.x.0 - b.origin.x),
            f64::from(position.y.0 - b.origin.y),
        ))
    }

    /// Whether canvas-local `s` lies on the pattern canvas.
    fn on_pattern_canvas(&self, s: Vec2) -> bool {
        self.pattern_bounds.is_some_and(|b| {
            s.x >= 0.0 && s.y >= 0.0 && s.x <= f64::from(b.size.width) && s.y <= f64::from(b.size.height)
        })
    }

    pub(crate) fn on_pattern_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.pattern_local(e.position) else {
            return;
        };
        let q = self.pattern_view.to_pattern(s);
        if self.modal.is_some() {
            match e.button {
                MouseButton::Left => self.confirm_modal(),
                MouseButton::Right => self.cancel_modal(),
                _ => {}
            }
            cx.notify();
            return;
        }
        if e.button == MouseButton::Middle {
            self.pattern_drag = Some(PatternDrag::Pan { last: s });
            return;
        }
        if e.button == MouseButton::Right {
            // right-click: the piece under the pointer is selected and its commands offered
            let hit = hit_test(&self.session.project, q, self.pattern_view.zoom, false);
            let menu = match hit.piece {
                Some(piece) => {
                    if !self.session.selection.pieces.contains(&piece) {
                        self.session.selection.select_only(piece);
                    }
                    crate::operators::ContextMenu::Piece
                }
                None => crate::operators::ContextMenu::Pattern,
            };
            self.open_context_menu(menu, e.position);
            cx.notify();
            return;
        }
        if e.button != MouseButton::Left {
            return;
        }
        let hit = hit_test(&self.session.project, q, self.pattern_view.zoom, false);
        self.tools.cursor = Some(q);
        self.tools.hover = Some(hit.clone());
        let m = e.modifiers;
        let near_last = self
            .tools
            .last_press
            .replace(s)
            .is_some_and(|last| last.distance(s) <= crate::pattern_tools::DOUBLE_CLICK_PX);
        let press = crate::pattern_tools::Press {
            s,
            q,
            hit,
            shift: m.shift,
            alt: m.alt,
            extend: m.shift || m.control,
            double: e.click_count >= 2 && near_last,
        };
        self.tool_press(&press);
        self.force_scene();
        cx.notify();
    }

    /// Continues a pattern drag, or updates the hover feedback without one; returns `true` if
    /// anything changed.
    pub(crate) fn pattern_drag_move(&mut self, position: Point<Pixels>, modifiers: Modifiers) -> bool {
        let Some(s) = self.pattern_local(position) else {
            return false;
        };
        let q = self.pattern_view.to_pattern(s);
        let Some(drag) = self.pattern_drag.clone() else {
            return self.pattern_hover(s, q);
        };
        self.tools.cursor = Some(q);
        if let PatternDrag::Pan { last } = drag {
            self.pattern_view.offset += s - last;
            self.pattern_drag = Some(PatternDrag::Pan { last: s });
            return true;
        }
        self.drag_to(drag, q, modifiers.shift, modifiers.alt);
        true
    }

    /// Updates what lies under the pointer; returns `true` if the feedback changed.
    fn pattern_hover(&mut self, s: Vec2, q: Vec2) -> bool {
        if !self.on_pattern_canvas(s) {
            let had = self.tools.hover.take().is_some() | self.tools.cursor.take().is_some();
            return had;
        }
        let hit = hit_test(&self.session.project, q, self.pattern_view.zoom, false);
        let changed = self.tools.hover.as_ref() != Some(&hit) || self.tools.draft.is_some();
        self.tools.cursor = Some(q);
        self.tools.hover = Some(hit);
        changed
    }

    /// Ends a pattern drag; returns `true` if one was in progress.
    pub(crate) fn pattern_drag_end(&mut self) -> bool {
        let Some(drag) = self.pattern_drag.take() else {
            return false;
        };
        self.tools.snap = None;
        self.drag_end(drag);
        self.force_scene();
        true
    }

    pub(crate) fn on_pattern_wheel(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pattern_local(e.position) {
            self.pattern_view.zoom_at(s, f64::from(e.delta));
            cx.notify();
        }
    }

    /// Rebuilds the pattern scene on the next render even if the project did not change.
    pub(crate) fn force_scene(&mut self) {
        self.scene_revision = 0;
    }
}
