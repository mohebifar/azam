//! Placing parts on pieces: notches, topstitches, buttons, buttonholes, fastening, graphics and
//! comment pins, and dragging the ones that move.

use azam_doc::{Extras, ItemKind, PieceId};
use azam_geom::Vec2;

use super::Press;
use crate::app::AzamApp;
use crate::pattern_input::PatternDrag;
use crate::selection::Part;

/// Text of a newly pinned comment, to be edited in the properties editor.
const NEW_COMMENT_TEXT: &str = "New comment";
/// A press this close to a graphic's corner scales it, in logical pixels.
const GRAPHIC_CORNER_PX: f64 = 8.0;
/// Smallest graphic width, mm.
const MIN_GRAPHIC_MM: f64 = 5.0;

/// Milliseconds since the Unix epoch, for comment timestamps.
fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

impl AzamApp {
    pub(super) fn notch_press(&mut self, press: &Press) {
        let Some(seg) = press.hit.segment.clone() else {
            self.status = "Click on a segment to add a notch".into();
            return;
        };
        let mut added = false;
        self.session.edit("Add Notch", |p| {
            added = azam_doc::add_notch(p, &seg.piece, &seg.segment, seg.t).is_some();
        });
        if added
            && let Some(index) = self
                .session
                .project
                .pieces
                .get(&seg.piece)
                .map(|pc| pc.notches.len() - 1)
        {
            self.show_part(Part::Notch {
                piece: seg.piece,
                index,
            });
            self.status = "Added a notch".into();
        }
    }

    pub(super) fn dart_press(&mut self, press: &Press) {
        let Some(seg) = press.hit.segment.clone() else {
            self.status = "Click on a segment to add a dart".into();
            return;
        };
        if self.refuse_mirror(&seg.piece) {
            return;
        }
        let dart = self.tools.dart;
        let mut result = Err(azam_doc::DartError::NoSuchSegment);
        self.session.edit("Add Dart", |p| {
            result = azam_doc::add_dart(p, &seg.piece, &seg.segment, seg.t, dart.width, dart.depth);
            if result.is_ok() {
                azam_doc::sync_mirrors(p);
            }
        });
        self.status = match result {
            Ok(_) => "Added a dart (its legs are sewn together)".into(),
            Err(e) => format!("No dart: {e}"),
        };
        if result.is_ok() {
            self.remember_last(crate::redo_panel::LastKind::Dart(crate::redo_panel::LastDart {
                piece: seg.piece.clone(),
                segment: seg.segment.clone(),
                t: seg.t,
                width: dart.width,
                depth: dart.depth,
            }));
        }
    }

    pub(super) fn stitch_press(&mut self, press: &Press) {
        let Some(seg) = press.hit.segment.clone() else {
            self.status = "Click on a segment to topstitch it".into();
            return;
        };
        let whole = press.shift;
        let mut added = None;
        self.session.edit("Add Topstitch", |p| {
            added = azam_doc::add_stitch(p, &seg.piece, (!whole).then_some(&seg.segment));
        });
        if let Some(id) = added {
            self.show_part(Part::Stitch(id));
            self.status = if whole {
                "Topstitched the whole outline".into()
            } else {
                "Topstitched the segment".into()
            };
        }
    }

    pub(super) fn edit_stitch_press(&mut self, press: &Press) {
        match press.hit.stitch.clone() {
            Some(id) => self.show_part(Part::Stitch(id)),
            None => self.session.selection.part = None,
        }
    }

    /// Places an item of `kind` with `add`, or picks up the one under the pointer to move it.
    fn item_press(
        &mut self,
        press: &Press,
        kind: ItemKind,
        under: Option<azam_doc::ItemId>,
        add: fn(&mut azam_doc::Project, &PieceId, Vec2) -> Option<azam_doc::ItemId>,
    ) {
        if let Some(id) = under {
            self.show_part(Part::Item(kind, id.clone()));
            self.pattern_drag = Some(PatternDrag::MoveItem {
                kind,
                id,
                last: press.q,
            });
            return;
        }
        let Some(piece) = press.hit.piece.clone() else {
            self.status = "Click inside a piece".into();
            return;
        };
        let label = match kind {
            ItemKind::Button => "Add Button",
            ItemKind::Hole => "Add Buttonhole",
            ItemKind::Comment => "Add Comment",
        };
        let mut added = None;
        self.session.edit(label, |p| added = add(p, &piece, press.q));
        if let Some(id) = added {
            self.show_part(Part::Item(kind, id));
        }
    }

    pub(super) fn button_press(&mut self, press: &Press) {
        self.item_press(
            press,
            ItemKind::Button,
            press.hit.button.clone(),
            azam_doc::add_button,
        );
    }

    pub(super) fn hole_press(&mut self, press: &Press) {
        self.item_press(press, ItemKind::Hole, press.hit.hole.clone(), azam_doc::add_hole);
    }

    pub(super) fn comment_press(&mut self, press: &Press) {
        if let Some(id) = press.hit.comment.clone() {
            self.show_part(Part::Item(ItemKind::Comment, id.clone()));
            self.pattern_drag = Some(PatternDrag::MoveItem {
                kind: ItemKind::Comment,
                id,
                last: press.q,
            });
            return;
        }
        let Some(piece) = press.hit.piece.clone() else {
            self.status = "Click on a piece to pin a comment".into();
            return;
        };
        let mut added = None;
        self.session.edit("Add Comment", |p| {
            added = azam_doc::add_comment(p, &piece, press.q, NEW_COMMENT_TEXT, now_ms());
        });
        if let Some(id) = added {
            self.show_part(Part::Item(ItemKind::Comment, id));
            self.status = "Pinned a comment: write it in the properties editor".into();
        }
    }

    pub(super) fn fasten_press(&mut self, press: &Press) {
        if let Some(button) = press.hit.button.clone() {
            self.tools.fasten_from = Some(button);
            self.status = "Now click the buttonhole to fasten to".into();
            return;
        }
        if let (Some(hole), Some(button)) = (press.hit.hole.clone(), self.tools.fasten_from.clone()) {
            let mut done = false;
            self.session.edit("Fasten Button", |p| {
                done = azam_doc::fasten_button(p, &button, Some(&hole));
            });
            self.tools.fasten_from = None;
            if done {
                self.show_part(Part::Item(ItemKind::Button, button));
                self.status = "Fastened: simulate to close it".into();
            }
        }
    }

    pub(super) fn graphic_press(&mut self, press: &Press) {
        if let Some((piece, index)) = press.hit.graphic.clone() {
            self.show_part(Part::Graphic {
                piece: piece.clone(),
                index,
            });
            let Some(g) = self
                .session
                .project
                .pieces
                .get(&piece)
                .and_then(|pc| pc.graphics.get(index))
            else {
                return;
            };
            let size = azam_doc::graphic_size(g);
            let margin = GRAPHIC_CORNER_PX / self.pattern_view.zoom;
            let d = press.q - Vec2::new(g.x, g.y);
            let corner = d.x.abs() > size.x / 2.0 - margin && d.y.abs() > size.y / 2.0 - margin;
            self.pattern_drag = Some(if corner {
                PatternDrag::ScaleGraphic {
                    piece,
                    index,
                    ratio: size.x / size.y.max(f64::EPSILON),
                }
            } else {
                PatternDrag::MoveGraphic {
                    piece,
                    index,
                    last: press.q,
                }
            });
            return;
        }
        let Some(piece) = press.hit.piece.clone() else {
            self.status = "Click inside a piece to place a graphic".into();
            return;
        };
        let size = Vec2::splat(azam_doc::DEFAULT_GRAPHIC_WIDTH_MM);
        let mut added = false;
        self.session.edit("Place Graphic", |p| {
            added = azam_doc::add_graphic(p, &piece, press.q, size, None).is_some();
        });
        if added
            && let Some(index) = self
                .session
                .project
                .pieces
                .get(&piece)
                .map(|pc| pc.graphics.len() - 1)
        {
            self.show_part(Part::Graphic { piece, index });
            self.status = "Placed a graphic: choose its image file in the properties editor".into();
        }
    }

    pub(super) fn trim_drag(&mut self, drag: PatternDrag, q: Vec2) {
        match drag {
            PatternDrag::MoveItem { kind, id, last } => {
                let delta = q - last;
                if delta.length_squared() > 0.0 {
                    self.session.edit("Move", |p| {
                        azam_doc::move_item(p, kind, &id, delta);
                    });
                }
                self.pattern_drag = Some(PatternDrag::MoveItem { kind, id, last: q });
            }
            PatternDrag::MoveGraphic { piece, index, last } => {
                let delta = q - last;
                if delta.length_squared() > 0.0 {
                    self.session.edit("Move Graphic", |p| {
                        if let Some(g) = p.pieces.get_mut(&piece).and_then(|pc| pc.graphics.get_mut(index)) {
                            g.x += delta.x;
                            g.y += delta.y;
                        }
                    });
                }
                self.pattern_drag = Some(PatternDrag::MoveGraphic {
                    piece,
                    index,
                    last: q,
                });
            }
            PatternDrag::ScaleGraphic { piece, index, ratio } => {
                self.session.edit("Scale Graphic", |p| {
                    if let Some(g) = p.pieces.get_mut(&piece).and_then(|pc| pc.graphics.get_mut(index)) {
                        let w = ((q.x - g.x).abs() * 2.0).max(MIN_GRAPHIC_MM);
                        g.set(azam_doc::GRAPHIC_WIDTH_KEY, w);
                        g.set(azam_doc::GRAPHIC_HEIGHT_KEY, w / ratio);
                    }
                });
                self.pattern_drag = Some(PatternDrag::ScaleGraphic { piece, index, ratio });
            }
            _ => {}
        }
    }
}
