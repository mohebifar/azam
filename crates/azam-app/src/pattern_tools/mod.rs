//! The pattern editor's tools: what a press does with each tool, how drags continue and end, the
//! state of shapes and seams being drawn, and the hints shown for each tool.
//!
//! Every change to the project goes through [`crate::session::Session::edit`], so it can be undone.

pub mod hit;

mod draw;
mod edit;
mod overlay;
mod sewing;
mod snap;
mod transform;
mod trims;

use azam_doc::{ItemId, PieceId, SeamSide};
use azam_geom::Vec2;

use crate::app::AzamApp;
use crate::operators::{Operator, Tool};
use crate::pattern_input::PatternDrag;
use crate::properties::PropTab;
use crate::selection::Part;
use hit::Hit;

pub use sewing::SewDraft;

/// The second press of a double click lands within this distance of the first, in logical
/// pixels.
pub const DOUBLE_CLICK_PX: f64 = 4.0;

/// A left press on the pattern canvas.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools, reason = "the modifier keys of one press")]
pub struct Press {
    /// Canvas-local position, logical pixels.
    pub s: Vec2,
    /// Pattern-space position.
    pub q: Vec2,
    /// What lies under it.
    pub hit: Hit,
    /// Shift held: constrain, or add to a seam side.
    pub shift: bool,
    /// Alt held: no snapping, curve points, the other direction.
    pub alt: bool,
    /// Shift or Ctrl held: extend the selection.
    pub extend: bool,
    /// The second press of a double click.
    pub double: bool,
}

/// A polygon or internal line being drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftState {
    /// The piece an internal line goes on; `None` while drawing a new piece.
    pub piece: Option<PieceId>,
    /// Points placed so far: position and whether it is a curve point.
    pub points: Vec<(Vec2, bool)>,
}

/// What the pattern tools are in the middle of, apart from a drag.
#[derive(Clone, Debug, Default)]
pub struct PatternTools {
    /// What lies under the pointer.
    pub hover: Option<Hit>,
    /// The pointer in pattern space, while it is over the canvas.
    pub cursor: Option<Vec2>,
    /// A polygon or internal line being drawn.
    pub draft: Option<DraftState>,
    /// A seam being picked.
    pub sew: Option<SewDraft>,
    /// The button picked to fasten.
    pub fasten_from: Option<ItemId>,
    /// Where the pointer snapped to during a drag.
    pub snap: Option<Vec2>,
    /// The last measurement: from and to.
    pub measure: Option<(Vec2, Vec2)>,
    /// Where the last press was, canvas-local: a double click must land there again.
    pub last_press: Option<Vec2>,
}

impl AzamApp {
    /// The current tool.
    #[must_use]
    pub fn current_tool(&self) -> Tool {
        self.tool
    }

    /// Runs the current tool for a press.
    pub(crate) fn tool_press(&mut self, press: &Press) {
        match self.tool {
            Tool::Select | Tool::Grab | Tool::Pin => self.transform_press(press),
            Tool::EditPoints => self.edit_press(press),
            Tool::Curvature => self.curvature_press(press),
            Tool::CurvePoint => self.curve_point_press(press),
            Tool::AddPoint => self.add_point_press(press),
            Tool::Polygon | Tool::InternalLine => self.draft_press(press),
            Tool::Rectangle | Tool::Ellipse | Tool::InternalEllipse => self.shape_press(press),
            Tool::Notch => self.notch_press(press),
            Tool::Dart => self.dart_press(press),
            Tool::Sew => self.sew_press(press, false),
            Tool::FreeSew => self.sew_press(press, true),
            Tool::EditSew => self.edit_sew_press(press),
            Tool::Topstitch => self.stitch_press(press),
            Tool::EditTopstitch => self.edit_stitch_press(press),
            Tool::Button => self.button_press(press),
            Tool::Buttonhole => self.hole_press(press),
            Tool::FastenButton => self.fasten_press(press),
            Tool::Graphic => self.graphic_press(press),
            Tool::Comment => self.comment_press(press),
            Tool::Measure => {
                let from = self.snap_point(press.q, None, press.alt);
                self.tools.measure = Some((from, from));
                self.pattern_drag = Some(PatternDrag::Measure { from });
            }
        }
    }

    /// Continues `drag` to pattern point `q`.
    pub(crate) fn drag_to(&mut self, drag: PatternDrag, q: Vec2, shift: bool, alt: bool) {
        match drag {
            PatternDrag::Pan { .. } => {}
            PatternDrag::MovePieces { .. } | PatternDrag::Rotate { .. } | PatternDrag::SelectBox { .. } => {
                self.transform_drag(drag, q, shift);
            }
            PatternDrag::MovePoints { .. }
            | PatternDrag::MoveInternal { .. }
            | PatternDrag::Curvature { .. } => {
                self.edit_drag(drag, q, alt);
            }
            PatternDrag::Shape { .. } => self.shape_drag(drag, q, shift, alt),
            PatternDrag::MoveItem { .. }
            | PatternDrag::MoveGraphic { .. }
            | PatternDrag::ScaleGraphic { .. } => {
                self.trim_drag(drag, q);
            }
            PatternDrag::Measure { from } => {
                let to = self.snap_point(q, None, alt);
                self.tools.measure = Some((from, to));
                self.pattern_drag = Some(PatternDrag::Measure { from });
            }
        }
    }

    /// Ends `drag`.
    pub(crate) fn drag_end(&mut self, drag: PatternDrag) {
        match drag {
            PatternDrag::SelectBox { from, to, points } => self.box_select(from, to, points),
            PatternDrag::Shape {
                tool,
                piece,
                from,
                to,
            } => self.shape_end(tool, piece.as_ref(), from, to),
            PatternDrag::Measure { from } => {
                if let Some((_, to)) = self.tools.measure {
                    self.status = format!("Distance {:.1} cm", from.distance(to) / 10.0);
                }
            }
            _ => {}
        }
    }

    /// Cancels whatever the tool is drawing or picking; returns `true` if there was something.
    pub(crate) fn cancel_pattern_tool(&mut self) -> bool {
        let t = &mut self.tools;
        let busy = t.draft.take().is_some()
            | t.sew.take().is_some()
            | t.fasten_from.take().is_some()
            | t.measure.take().is_some();
        self.pattern_drag = None;
        busy
    }

    /// Runs a pattern-editing operator from a menu or key.
    pub(crate) fn run_pattern_op(&mut self, op: Operator) {
        match op {
            Operator::FlipHorizontal => self.flip_selected(azam_doc::FlipAxis::Horizontal),
            Operator::FlipVertical => self.flip_selected(azam_doc::FlipAxis::Vertical),
            Operator::Rotate90 => self.rotate_selected_90(),
            Operator::FlipSeam => self.flip_selected_seam(),
            Operator::SwapSeamSides => self.swap_selected_seam(),
            Operator::FinishDraft => {
                if self.tools.draft.is_some() {
                    self.finish_draft(false);
                } else if self.tools.sew.is_some() {
                    self.advance_sew();
                }
            }
            _ => {}
        }
        self.force_scene();
    }

    /// Deletes the selected seam, part, or points (in the edit tools), or removes the last point
    /// of a draft. Returns `false` when nothing but pieces is selected, so they are deleted
    /// instead.
    pub(crate) fn delete_selection(&mut self) -> bool {
        if let Some(draft) = &mut self.tools.draft
            && draft.points.pop().is_some()
        {
            return true;
        }
        let sel = self.session.selection.clone();
        if let Some(part) = sel.part {
            self.delete_part(&part);
            return true;
        }
        if let Some(seam) = sel.seam {
            self.session.edit("Delete Seam", |p| {
                p.sewing.shift_remove(&seam);
            });
            self.status = "Deleted the seam".into();
            return true;
        }
        if matches!(self.tool, Tool::EditPoints | Tool::CurvePoint)
            && !sel.points.is_empty()
            && let Some(piece) = sel.active
        {
            self.delete_points(&piece, &sel.points);
            return true;
        }
        false
    }

    fn delete_points(&mut self, piece: &PieceId, indices: &[usize]) {
        let Some(pc) = self.session.project.pieces.get(piece) else {
            return;
        };
        let corners_left = pc
            .pts
            .iter()
            .enumerate()
            .filter(|(i, p)| !p.curve && !indices.contains(i))
            .count();
        if corners_left < azam_doc::MIN_CORNERS {
            self.status = "A piece needs at least three corners".into();
            return;
        }
        let ids: Vec<_> = indices
            .iter()
            .filter_map(|&i| pc.pts.get(i).map(|p| p.id.clone()))
            .collect();
        self.session.edit("Delete Points", |p| {
            // Enough corners remain (checked above), so this cannot be refused.
            let _ = azam_doc::delete_points(p, piece, &ids);
        });
        self.session.selection.points.clear();
        self.status = format!("Deleted {} point(s)", ids.len());
    }

    fn delete_part(&mut self, part: &Part) {
        let part = part.clone();
        self.session.edit("Delete", |p| match &part {
            Part::Notch { piece, index } => {
                if let Some(pc) = p.pieces.get_mut(piece)
                    && *index < pc.notches.len()
                {
                    pc.notches.remove(*index);
                }
            }
            Part::Internal { piece, index } => {
                azam_doc::delete_internal_line(p, piece, *index);
            }
            Part::Graphic { piece, index } => {
                if let Some(pc) = p.pieces.get_mut(piece)
                    && *index < pc.graphics.len()
                {
                    pc.graphics.remove(*index);
                }
            }
            Part::Stitch(id) => {
                p.stitches.shift_remove(id);
            }
            Part::Item(kind, id) => {
                azam_doc::delete_item(p, *kind, id);
            }
        });
        self.session.selection.part = None;
        self.status = "Deleted".into();
    }

    /// Selects `part` and shows it in the properties editor.
    pub(crate) fn show_part(&mut self, part: Part) {
        self.session.selection.select_part(part);
        self.prop_tab = PropTab::Part;
    }

    /// Selects `seam` and shows it in the properties editor.
    pub(crate) fn show_seam(&mut self, seam: azam_doc::SeamId) {
        self.session.selection.select_seam(seam);
        self.prop_tab = PropTab::Part;
    }

    /// The hint shown in the pattern editor for the current tool.
    pub(crate) fn tool_hint(&self) -> String {
        let sewing = self.tools.sew.as_ref();
        match self.tool {
            Tool::Select | Tool::Grab | Tool::Pin => {
                "Click to select · drag to move · drag the handle to rotate (Shift snaps)".into()
            }
            Tool::EditPoints => {
                "Drag points or segments · Shift click adds points · double click splits a segment".into()
            }
            Tool::Curvature => "Drag a segment to bend it".into(),
            Tool::CurvePoint => {
                "Click a segment to add a curve point · drag to move it · Alt click makes a corner".into()
            }
            Tool::AddPoint => "Click a segment to split it".into(),
            Tool::Polygon if self.tools.draft.is_some() => {
                "Click to add points (Shift 45°, Alt curve point) · click the first point or Enter to finish"
                    .into()
            }
            Tool::Polygon => "Click to start a piece".into(),
            Tool::InternalLine if self.tools.draft.is_some() => {
                "Click to add points · click the first point to close · Enter or double click to finish"
                    .into()
            }
            Tool::InternalLine => "Click inside a piece to start an internal line".into(),
            Tool::Rectangle | Tool::Ellipse => "Drag to draw a piece (Shift: square)".into(),
            Tool::InternalEllipse => "Drag inside a piece".into(),
            Tool::Notch => "Click a segment to add a notch".into(),
            Tool::Dart => format!(
                "Click a segment to cut a {:.0} × {:.0} mm dart there; move its points with Edit Points",
                azam_doc::DEFAULT_DART_WIDTH_MM,
                azam_doc::DEFAULT_DART_DEPTH_MM
            ),
            Tool::Sew | Tool::FreeSew => SewDraft::hint(sewing, self.tool == Tool::FreeSew),
            Tool::EditSew => {
                "Click a seam to select it · flip, swap or delete it from the Sewing menu".into()
            }
            Tool::Topstitch => "Click a segment to topstitch it (Shift: the whole outline)".into(),
            Tool::EditTopstitch => "Click a topstitch to select it".into(),
            Tool::Button => "Click inside a piece to place a button · drag one to move it".into(),
            Tool::Buttonhole => "Click inside a piece to place a buttonhole · drag one to move it".into(),
            Tool::FastenButton if self.tools.fasten_from.is_some() => "Now click the buttonhole".into(),
            Tool::FastenButton => "Click a button, then the buttonhole to fasten it to".into(),
            Tool::Graphic => {
                "Click inside a piece to place a graphic · drag to move · drag a corner to scale".into()
            }
            Tool::Comment => "Click on a piece to pin a comment".into(),
            Tool::Measure => "Drag to measure (snaps to corners, Alt: free)".into(),
        }
    }
}

/// The pieces in `ids` that are not linked mirrors (those follow their source).
pub(crate) fn movable(project: &azam_doc::Project, ids: impl IntoIterator<Item = PieceId>) -> Vec<PieceId> {
    ids.into_iter()
        .filter(|id| project.pieces.get(id).is_some_and(|p| p.mirror.is_none()))
        .collect()
}

/// The polyline of seam side `side` in the size shown.
pub(crate) fn side_polyline(project: &azam_doc::Project, side: &SeamSide) -> Vec<Vec2> {
    hit::shown_outline(project, &side.piece).map_or_else(Vec::new, |ol| {
        side.side
            .polyline(&ol, azam_geom::DEFAULT_SIDE_STEP_MM)
            .iter()
            .map(|p| p.pos)
            .collect()
    })
}
