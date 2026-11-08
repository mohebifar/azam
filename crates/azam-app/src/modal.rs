//! Blender's modal transforms in the pattern editor: with the Select tool and pieces selected, G
//! moves them with the pointer and R turns them about their middle, without holding a button.
//! A click or Enter keeps the change; Esc or a right-click puts everything back.

use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::operators::Tool;
use crate::pattern_input::PatternDrag;

/// What a modal transform does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalKind {
    /// Move (G).
    Move,
    /// Rotate (R).
    Rotate,
}

/// A modal transform in progress.
#[derive(Clone, Copy, Debug)]
pub struct Modal {
    kind: ModalKind,
    /// Undo steps before it began: cancelling undoes back to here.
    undo_depth: usize,
}

impl AzamApp {
    /// Starts modal transform `kind` from the pointer at `at`, if the Select tool is active, the
    /// pointer is over the pattern editor and pieces are selected. Returns `true` if it started.
    pub fn begin_modal(&mut self, kind: ModalKind, at: Point<Pixels>) -> bool {
        if self.tool != Tool::Select || self.modal.is_some() || self.session.selection.pieces.is_empty() {
            return false;
        }
        let Some(s) = self.pattern_local(at) else {
            return false;
        };
        let q = self.pattern_view.to_pattern(s);
        let ids: Vec<_> = self.session.selection.pieces.iter().cloned().collect();
        self.session.seal();
        self.modal = Some(Modal {
            kind,
            undo_depth: self.session.undo_depth(),
        });
        self.pattern_drag = Some(match kind {
            ModalKind::Move => PatternDrag::MovePieces { ids, last: q },
            ModalKind::Rotate => {
                let pts = ids
                    .iter()
                    .filter_map(|id| self.session.project.pieces.get(id))
                    .flat_map(|pc| pc.pts.iter().map(|p| p.pos));
                let center = azam_geom::bbox(pts).map_or(q, |b| b.center());
                let d = q - center;
                PatternDrag::Rotate {
                    ids,
                    center,
                    start: d.y.atan2(d.x),
                    applied: 0.0,
                }
            }
        });
        self.status = match kind {
            ModalKind::Move => "Move: click or Enter to keep, Esc or right-click to cancel".into(),
            ModalKind::Rotate => "Rotate (Shift: 15° steps): click or Enter to keep, Esc to cancel".into(),
        };
        true
    }

    /// The modal transform in progress, if any.
    #[must_use]
    pub fn modal_kind(&self) -> Option<ModalKind> {
        self.modal.map(|m| m.kind)
    }

    /// Keeps the modal transform's change.
    pub fn confirm_modal(&mut self) {
        if self.modal.take().is_some() {
            self.pattern_drag_end();
            self.session.seal();
            self.status = "Done".into();
        }
    }

    /// Puts back everything the modal transform changed.
    pub fn cancel_modal(&mut self) {
        let Some(modal) = self.modal.take() else { return };
        self.pattern_drag = None;
        while self.session.undo_depth() > modal.undo_depth {
            if self.session.undo().is_none() {
                break;
            }
        }
        self.session.seal();
        self.status = "Cancelled".into();
    }
}
