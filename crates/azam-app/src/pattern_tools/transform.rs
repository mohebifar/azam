//! The select / transform tool: picking pieces, moving them, turning them with the rotation
//! handle, box selection, and the flip and quarter-turn operators.

use azam_doc::{FlipAxis, PieceId, PieceTransform};
use azam_geom::{Vec2, bbox};

use super::hit::{piece_order, shown_outline, shown_points};
use super::{Press, movable};
use crate::app::AzamApp;
use crate::pattern::rotate_handle_px;
use crate::pattern_input::PatternDrag;
use crate::properties::PropTab;

/// Pick radius of the rotation handle, in logical pixels.
const ROTATE_PICK_PX: f64 = 10.0;
/// Shift snaps rotation to multiples of this angle, radians.
const ROTATE_STEP: f64 = std::f64::consts::PI / 12.0;
/// Boxes smaller than this select nothing (a click), in logical pixels.
const MIN_BOX_PX: f64 = 2.0;

impl AzamApp {
    /// Bounding box of the selected pieces' outlines, if any are selected.
    pub(crate) fn selection_box(&self) -> Option<(Vec2, Vec2)> {
        let project = &self.session.project;
        let pts = self
            .session
            .selection
            .pieces
            .iter()
            .filter_map(|id| shown_outline(project, id))
            .flat_map(|ol| ol.poly);
        let b = bbox(pts)?;
        Some((b.min, b.max))
    }

    /// Canvas-local position of the rotation handle, when the transform tool shows one.
    fn rotate_handle(&self) -> Option<(Vec2, (Vec2, Vec2))> {
        let (lo, hi) = self.selection_box()?;
        let view = self.pattern_view;
        let to_px = |q: Vec2| {
            let s = view.to_screen(q);
            (s.x as f32, s.y as f32)
        };
        let (x, y) = rotate_handle_px(lo, hi, &to_px);
        Some((Vec2::new(f64::from(x), f64::from(y)), (lo, hi)))
    }

    pub(super) fn transform_press(&mut self, press: &Press) {
        if let Some((handle, (lo, hi))) = self.rotate_handle()
            && handle.distance(press.s) < ROTATE_PICK_PX
        {
            let center = (lo + hi) / 2.0;
            let d = press.q - center;
            let ids = movable(
                &self.session.project,
                self.session.selection.pieces.iter().cloned(),
            );
            self.pattern_drag = Some(PatternDrag::Rotate {
                ids,
                center,
                start: d.y.atan2(d.x),
                applied: 0.0,
            });
            return;
        }
        let sel = &mut self.session.selection;
        if let Some(id) = press.hit.piece.clone() {
            if press.extend {
                sel.toggle(id);
            } else if sel.pieces.contains(&id) {
                sel.active = Some(id);
                sel.part = None;
                sel.seam = None;
                sel.segment = None;
            } else {
                sel.select_only(id);
            }
            if self.prop_tab == PropTab::Part {
                self.prop_tab = PropTab::Piece;
            }
            let ids = movable(
                &self.session.project,
                self.session.selection.pieces.iter().cloned(),
            );
            self.pattern_drag = Some(PatternDrag::MovePieces { ids, last: press.q });
        } else {
            if !press.extend {
                sel.clear();
            }
            self.pattern_drag = Some(PatternDrag::SelectBox {
                from: press.q,
                to: press.q,
                points: false,
            });
        }
    }

    pub(super) fn transform_drag(&mut self, drag: PatternDrag, q: Vec2, shift: bool) {
        match drag {
            PatternDrag::MovePieces { ids, last } => {
                let delta = q - last;
                if delta.length_squared() > 0.0 && !ids.is_empty() {
                    self.session.edit("Move Pieces", |p| {
                        for id in &ids {
                            azam_doc::transform_piece(p, id, |v| v + delta);
                        }
                        azam_doc::sync_mirrors(p);
                    });
                }
                self.pattern_drag = Some(PatternDrag::MovePieces { ids, last: q });
            }
            PatternDrag::Rotate {
                ids,
                center,
                start,
                applied,
            } => {
                let d = q - center;
                let mut angle = d.y.atan2(d.x) - start;
                if shift {
                    angle = (angle / ROTATE_STEP).round() * ROTATE_STEP;
                }
                let step = angle - applied;
                if step.abs() > 0.0 && !ids.is_empty() {
                    let t = PieceTransform::Rotate { center, angle: step };
                    self.session.edit("Rotate Pieces", |p| {
                        for id in &ids {
                            azam_doc::transform_piece(p, id, |v| t.apply(v));
                        }
                        azam_doc::sync_mirrors(p);
                    });
                    self.status = format!("Rotate {:.1}°", angle.to_degrees());
                }
                self.pattern_drag = Some(PatternDrag::Rotate {
                    ids,
                    center,
                    start,
                    applied: angle,
                });
            }
            PatternDrag::SelectBox { from, points, .. } => {
                self.pattern_drag = Some(PatternDrag::SelectBox { from, to: q, points });
            }
            _ => {}
        }
    }

    /// Selects the pieces touching the box `a`..`b`, or with `points` the points inside it (on the
    /// piece where most fall).
    pub(super) fn box_select(&mut self, a: Vec2, b: Vec2, points: bool) {
        let (lo, hi) = (a.min(b), a.max(b));
        let min = MIN_BOX_PX / self.pattern_view.zoom;
        if hi.x - lo.x < min && hi.y - lo.y < min {
            return;
        }
        let project = &self.session.project;
        let inside = |p: Vec2| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
        if points {
            let best = piece_order(project)
                .into_iter()
                .map(|id| {
                    let idx: Vec<usize> = shown_points(project, &id)
                        .iter()
                        .enumerate()
                        .filter(|(_, (p, _))| inside(*p))
                        .map(|(i, _)| i)
                        .collect();
                    (id, idx)
                })
                .filter(|(_, idx)| !idx.is_empty())
                .max_by_key(|(_, idx)| idx.len());
            if let Some((id, idx)) = best {
                self.session.selection.select_only(id);
                self.status = format!("Selected {} point(s)", idx.len());
                self.session.selection.points = idx;
            }
        } else {
            let ids: Vec<PieceId> = piece_order(project)
                .into_iter()
                .filter(|id| {
                    shown_outline(project, id)
                        .and_then(|ol| bbox(ol.poly))
                        .is_some_and(|bb| {
                            bb.max.x >= lo.x && bb.min.x <= hi.x && bb.max.y >= lo.y && bb.min.y <= hi.y
                        })
                })
                .collect();
            let sel = &mut self.session.selection;
            sel.active = ids.first().cloned().or(sel.active.take());
            sel.pieces.extend(ids);
        }
    }

    /// Flips the selected pieces in place.
    pub(super) fn flip_selected(&mut self, axis: FlipAxis) {
        let ids = movable(
            &self.session.project,
            self.session.selection.pieces.iter().cloned(),
        );
        if ids.is_empty() {
            self.status = "Select pieces to flip (linked mirrors follow their source)".into();
            return;
        }
        let label = match axis {
            FlipAxis::Horizontal => "Flip Horizontally",
            FlipAxis::Vertical => "Flip Vertically",
        };
        self.session.edit(label, |p| {
            for id in &ids {
                azam_doc::flip_piece(p, id, axis);
            }
            azam_doc::sync_mirrors(p);
        });
        self.status = format!("{label}: {} piece(s)", ids.len());
    }

    /// Turns the selected pieces a quarter turn in place.
    pub(super) fn rotate_selected_90(&mut self) {
        let ids = movable(
            &self.session.project,
            self.session.selection.pieces.iter().cloned(),
        );
        if ids.is_empty() {
            self.status = "Select pieces to rotate (linked mirrors follow their source)".into();
            return;
        }
        self.session.edit("Rotate 90°", |p| {
            for id in &ids {
                azam_doc::rotate_piece_90(p, id);
            }
            azam_doc::sync_mirrors(p);
        });
        self.status = format!("Rotated {} piece(s)", ids.len());
    }
}
