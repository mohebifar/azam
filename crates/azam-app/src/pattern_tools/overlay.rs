//! What the tools draw over the pattern, built from their state each time the canvas paints.

use azam_geom::Vec2;
use blenderui::Painter;
use blenderui::prelude::*;

use super::hit::{Hit, shown_outline};
use super::side_polyline;
use crate::app::AzamApp;
use crate::operators::Tool;
use crate::pattern::{Draft, Overlay, PaintOptions, ShapePreview};
use crate::pattern_input::PatternDrag;

/// Length chips sit this far off the segment, into the piece's outside, in logical pixels.
const LABEL_OFFSET_PX: f64 = 12.0;

/// Tools that act on the segment under the pointer.
const fn segment_tool(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Sew
            | Tool::Zipper
            | Tool::FreeSew
            | Tool::AddPoint
            | Tool::CurvePoint
            | Tool::Curvature
            | Tool::Notch
            | Tool::Dart
            | Tool::Topstitch
            | Tool::EditPoints
    )
}

/// Tools that show the hovered segment's length.
const fn length_tool(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Sew | Tool::FreeSew | Tool::AddPoint | Tool::Curvature | Tool::Topstitch | Tool::EditPoints
    )
}

impl AzamApp {
    /// Paints the pattern editor: the scene, then the tools' feedback.
    pub(crate) fn paint_pattern(&self, b: Bounds<f32>, p: &mut Painter) {
        let options = PaintOptions {
            all_points: matches!(
                self.tool,
                Tool::EditPoints | Tool::CurvePoint | Tool::Curvature | Tool::AddPoint
            ),
        };
        self.pattern_scene
            .paint(b, p, &self.pattern_view, options, &self.pattern_overlay());
    }

    /// A length chip for segment `index` of `piece`'s shown outline.
    fn length_label(&self, piece: &azam_doc::PieceId, index: usize) -> Option<(Vec2, String)> {
        let ol = shown_outline(&self.session.project, piece)?;
        let seg = ol.segments.get(index)?;
        let mid = seg.point_at(0.5);
        let outward = -crate::scene::left_normal(mid.tangent) * crate::scene::inward_sign(&ol.poly);
        let at = mid.pos + outward * (LABEL_OFFSET_PX / self.pattern_view.zoom);
        Some((at, format!("{:.1}", seg.len / 10.0)))
    }

    /// The tools' feedback for the current state.
    pub(crate) fn pattern_overlay(&self) -> Overlay {
        let project = &self.session.project;
        let mut o = Overlay::default();
        let hover = self.tools.hover.as_ref().filter(|_| self.pattern_drag.is_none());
        if let Some(hit) = hover {
            self.hover_feedback(hit, &mut o);
        }
        if let Some((piece, segment)) = &self.session.selection.segment
            && let Some(k) = shown_outline(project, piece).and_then(|ol| ol.segment_index(segment))
            && let Some(label) = self.length_label(piece, k)
        {
            o.labels.push(label);
        }
        if let Some(sew) = &self.tools.sew {
            o.sew_sides
                .extend(sew.a.iter().map(|s| (side_polyline(project, s), false)));
            o.sew_sides
                .extend(sew.b.iter().map(|s| (side_polyline(project, s), true)));
            if let Some((piece, first, at)) = &sew.pending {
                o.pending_point = Some(*at);
                if let Some(seg) = hover
                    .and_then(|h| h.segment.as_ref())
                    .filter(|s| s.piece == *piece)
                {
                    let spot = azam_doc::OutlineSpot {
                        segment: seg.segment.clone(),
                        t: seg.t,
                    };
                    o.sew_preview = shown_outline(project, piece)
                        .and_then(|ol| azam_doc::free_side(&ol, piece, first, &spot, false))
                        .map(|side| side_polyline(project, &side));
                }
            }
        }
        if let (Some(draft), Some(cursor)) = (&self.tools.draft, self.tools.cursor) {
            o.draft = Some(Draft {
                points: draft.points.clone(),
                cursor,
                internal: draft.piece.is_some(),
            });
        }
        match &self.pattern_drag {
            Some(PatternDrag::Shape { tool, from, to, .. }) => {
                o.shape = Some(ShapePreview {
                    a: *from,
                    b: *to,
                    ellipse: *tool != Tool::Rectangle,
                });
            }
            Some(PatternDrag::SelectBox { from, to, .. }) => o.selection_box = Some((*from, *to)),
            Some(PatternDrag::Cut { from, to }) => o.cut = Some((*from, *to)),
            _ => {}
        }
        if self.pattern_drag.is_some() {
            o.snap = self.tools.snap;
        }
        o.measure = self.tools.measure;
        if self.tool == Tool::Select {
            o.rotate_box = self.selection_box();
        }
        o.fasten_from = self
            .tools
            .fasten_from
            .as_ref()
            .and_then(|b| project.buttons.get(b))
            .and_then(|b| Some(Vec2::new(b.x?, b.y?)));
        o
    }

    fn hover_feedback(&self, hit: &Hit, o: &mut Overlay) {
        let project = &self.session.project;
        if let Some(seg) = hit.segment.as_ref().filter(|_| segment_tool(self.tool))
            && let Some(ol) = shown_outline(project, &seg.piece)
            && let Some(s) = ol.segments.get(seg.index)
        {
            if self.tool != Tool::FreeSew {
                o.hover_edge = Some(s.poly.clone());
            }
            o.hover_dot = Some(seg.pos);
            if matches!(self.tool, Tool::Sew | Tool::Zipper) {
                o.hover_start = Some(s.point_at(if seg.t > 0.5 { 1.0 } else { 0.0 }).pos);
            }
            if length_tool(self.tool)
                && let Some(label) = self.length_label(&seg.piece, seg.index)
            {
                o.labels.push(label);
            }
        }
        if self.tool == Tool::Select
            && let Some(piece) = &hit.piece
            && !self.session.selection.pieces.contains(piece)
        {
            o.hover_outline = shown_outline(project, piece).map(|ol| ol.poly);
        }
    }
}
