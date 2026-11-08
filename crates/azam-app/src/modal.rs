//! Blender's modal transforms in the pattern editor: with the Select tool and pieces selected, G
//! moves them with the pointer, R turns them about their middle and S scales them from it, without
//! holding a button. X or Y locks the move or scale to that axis (again: free). A click or Enter
//! keeps the change; Esc or a right-click puts everything back. The keys start a transform only
//! with the pointer over the selection; elsewhere they pick their tools (CLO's keys).

use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::operators::Tool;
use crate::pattern_input::PatternDrag;

/// How far outside the selection's bounds the pointer may be for G, R or S to transform it,
/// logical pixels.
const REACH_PX: f64 = 24.0;

/// What a modal transform does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalKind {
    /// Move (G).
    Move,
    /// Rotate (R).
    Rotate,
    /// Scale (S).
    Scale,
}

/// An axis a modal move or scale is locked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Horizontal.
    X,
    /// Vertical.
    Y,
}

/// A modal transform in progress (a session gesture: one undo step, or none if cancelled).
#[derive(Clone, Debug)]
pub struct Modal {
    kind: ModalKind,
    axis: Option<Axis>,
    /// Scale: the pieces, the middle, the pointer's starting distance from it, and the factor
    /// applied so far.
    scale: Option<(Vec<azam_doc::PieceId>, azam_geom::Vec2, f64, azam_geom::Vec2)>,
}

impl AzamApp {
    /// Starts modal transform `kind` from the pointer at `at`, if the Select tool is active, pieces
    /// are selected and the pointer is over them (their bounds, give or take a few pixels).
    /// Returns `true` if it started.
    pub fn begin_modal(&mut self, kind: ModalKind, at: Point<Pixels>) -> bool {
        if self.tool != Tool::Select || self.modal.is_some() || self.session.selection.pieces.is_empty() {
            return false;
        }
        let Some(s) = self.pattern_local(at) else {
            return false;
        };
        let q = self.pattern_view.to_pattern(s);
        let ids: Vec<_> = self.session.selection.pieces.iter().cloned().collect();
        let pts = ids
            .iter()
            .filter_map(|id| self.session.project.pieces.get(id))
            .flat_map(|pc| pc.pts.iter().map(|p| p.pos));
        let Some(bounds) = azam_geom::bbox(pts) else {
            return false;
        };
        let reach = REACH_PX / self.pattern_view.zoom;
        let inside = q.x >= bounds.min.x - reach
            && q.x <= bounds.max.x + reach
            && q.y >= bounds.min.y - reach
            && q.y <= bounds.max.y + reach;
        if !inside {
            return false;
        }
        let center = bounds.center();
        self.session.begin_gesture();
        let mut modal = Modal {
            kind,
            axis: None,
            scale: None,
        };
        self.pattern_drag = match kind {
            ModalKind::Move => Some(PatternDrag::MovePieces { ids, last: q }),
            ModalKind::Rotate => {
                let d = q - center;
                Some(PatternDrag::Rotate {
                    ids,
                    center,
                    start: d.y.atan2(d.x),
                    applied: 0.0,
                })
            }
            ModalKind::Scale => {
                modal.scale = Some((
                    ids,
                    center,
                    q.distance(center).max(1.0),
                    azam_geom::Vec2::splat(1.0),
                ));
                None
            }
        };
        self.modal = Some(modal);
        self.status = match kind {
            ModalKind::Move => "Move: click or Enter to keep, Esc or right-click to cancel".into(),
            ModalKind::Rotate => "Rotate (Shift: 15° steps): click or Enter to keep, Esc to cancel".into(),
            ModalKind::Scale => "Scale (X/Y: one axis): click or Enter to keep, Esc to cancel".into(),
        };
        true
    }

    /// The modal transform in progress, if any.
    #[must_use]
    pub fn modal_kind(&self) -> Option<ModalKind> {
        self.modal.as_ref().map(|m| m.kind)
    }

    /// The axis the modal move or scale is locked to, if any.
    #[must_use]
    pub fn modal_axis(&self) -> Option<Axis> {
        self.modal.as_ref().and_then(|m| m.axis)
    }

    /// Locks the modal move or scale to `axis`, or frees it if it is locked there already.
    pub fn toggle_modal_axis(&mut self, axis: Axis) {
        if let Some(m) = self.modal.as_mut()
            && m.kind != ModalKind::Rotate
        {
            m.axis = if m.axis == Some(axis) { None } else { Some(axis) };
            self.status = match m.axis {
                Some(Axis::X) => "Locked to X".into(),
                Some(Axis::Y) => "Locked to Y".into(),
                None => "Free".into(),
            };
        }
    }

    /// Follows the pointer at `at` with a modal scale; returns `true` if one is running.
    pub(crate) fn modal_scale_move(&mut self, at: Point<Pixels>) -> bool {
        let Some(Modal {
            scale: Some((ids, center, start, applied)),
            axis,
            ..
        }) = self.modal.clone()
        else {
            return false;
        };
        let Some(s) = self.pattern_local(at) else {
            return true;
        };
        let k = (self.pattern_view.to_pattern(s).distance(center) / start).max(0.05);
        let want = azam_geom::Vec2::new(
            if axis == Some(Axis::Y) { 1.0 } else { k },
            if axis == Some(Axis::X) { 1.0 } else { k },
        );
        let step = azam_geom::Vec2::new(want.x / applied.x, want.y / applied.y);
        if (step.x - 1.0).abs() > 1e-9 || (step.y - 1.0).abs() > 1e-9 {
            self.session.edit("Scale Pieces", |p| {
                for id in &ids {
                    azam_doc::transform_piece(p, id, |v| {
                        center + azam_geom::Vec2::new((v.x - center.x) * step.x, (v.y - center.y) * step.y)
                    });
                }
                azam_doc::sync_mirrors(p);
            });
            if let Some(m) = self.modal.as_mut() {
                m.scale = Some((ids, center, start, want));
            }
        }
        true
    }

    /// Keeps the modal transform's change.
    pub fn confirm_modal(&mut self) {
        if self.modal.take().is_some() {
            self.pattern_drag_end();
            self.session.end_gesture(true);
            self.force_scene();
            self.status = "Done".into();
        }
    }

    /// Puts back everything the modal transform changed.
    pub fn cancel_modal(&mut self) {
        if self.modal.take().is_none() {
            return;
        }
        self.pattern_drag = None;
        self.session.end_gesture(false);
        self.force_scene();
        self.status = "Cancelled".into();
    }
}
