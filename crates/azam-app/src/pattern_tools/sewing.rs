//! Sewing tools: segment sewing (Shift adds segments for M:N seams), free sewing between picked
//! points, and editing seams (select, flip side B, swap sides).

use azam_doc::{OutlineSpot, PieceId, SeamSide};
use azam_geom::Vec2;

use super::Press;
use super::hit::shown_outline;
use crate::app::AzamApp;

/// Seams whose sides differ by more than this are reported, in mm.
const LENGTH_WARNING_MM: f64 = 3.0;

/// A seam being picked: the sides of each group so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SewDraft {
    /// Picking side B (side A is done).
    pub on_b: bool,
    /// Sides of group A.
    pub a: Vec<SeamSide>,
    /// Sides of group B.
    pub b: Vec<SeamSide>,
    /// Free sewing: the first end of the side being picked, and where it is.
    pub pending: Option<(PieceId, OutlineSpot, Vec2)>,
}

impl SewDraft {
    /// The hint for the sewing tools.
    #[must_use]
    pub fn hint(draft: Option<&Self>, free: bool) -> String {
        let pending = draft.is_some_and(|d| d.pending.is_some());
        let on_b = draft.is_some_and(|d| d.on_b);
        match (free, pending, on_b) {
            (true, true, _) => "Click where this side ends, on the same piece (Alt: the long way)".into(),
            (true, false, false) => "Click where side A starts (Shift: more sides, Enter ends the side)".into(),
            (true, false, true) => "Click where side B starts (Esc cancels)".into(),
            (false, _, false) => {
                "Click the first segment near the end to start from (Shift: more segments, Enter ends the side)".into()
            }
            (false, _, true) => "Click the segment to sew it to (Esc cancels)".into(),
        }
    }
}

impl AzamApp {
    pub(super) fn sew_press(&mut self, press: &Press, free: bool) {
        let Some(seg) = press.hit.segment.clone() else {
            self.status = "Click on a segment to sew".into();
            return;
        };
        let draft = self.tools.sew.get_or_insert_with(SewDraft::default);
        let side = if free {
            let spot = OutlineSpot {
                segment: seg.segment.clone(),
                t: seg.t,
            };
            let Some((piece, first, at)) = draft.pending.take() else {
                draft.pending = Some((seg.piece.clone(), spot, seg.pos));
                return;
            };
            if piece != seg.piece {
                self.status = "Free sewing: both ends must be on the same piece".into();
                draft.pending = Some((piece, first, at));
                return;
            }
            let Some(side) = shown_outline(&self.session.project, &piece)
                .and_then(|ol| azam_doc::free_side(&ol, &piece, &first, &spot, press.alt))
            else {
                return;
            };
            side
        } else {
            azam_doc::segment_side_from(&seg.piece, &seg.segment, seg.t)
        };
        if draft.on_b {
            draft.b.push(side);
        } else {
            draft.a.push(side);
        }
        if press.shift {
            self.status = "M:N seam: keep Shift-clicking, press Enter to finish this side".into();
            return;
        }
        self.advance_sew();
    }

    /// Ends the side being picked: side A moves on to side B, side B makes the seam.
    pub(super) fn advance_sew(&mut self) {
        let Some(draft) = &mut self.tools.sew else {
            return;
        };
        if !draft.on_b {
            if !draft.a.is_empty() {
                draft.on_b = true;
                self.status = "Now pick side B".into();
            }
            return;
        }
        if draft.b.is_empty() {
            return;
        }
        let Some(draft) = self.tools.sew.take() else {
            return;
        };
        let mut seam = None;
        let zipper = self.tool == crate::operators::Tool::Zipper;
        self.session.edit(if zipper { "Add Zipper" } else { "Sew" }, |p| {
            seam = Some(p.add_seam(draft.a.clone(), draft.b.clone()));
            if zipper {
                for side in draft.a.iter().chain(&draft.b) {
                    azam_doc::add_zipper_half(p, &side.piece, &side.side.a.segment);
                }
            }
        });
        let Some(seam) = seam else {
            return;
        };
        let ratio = format!("{}:{}", draft.a.len(), draft.b.len());
        self.status = match azam_doc::sew_lengths(&self.session.project).get(&seam) {
            Some(l) if (l.a - l.b).abs() > LENGTH_WARNING_MM => {
                format!(
                    "Sewed {ratio} · lengths differ by {:.1} cm",
                    (l.a - l.b).abs() / 10.0
                )
            }
            _ if zipper => "Added a zipper (the edges are sewn; each has a zipper half)".into(),
            _ => format!("Sewed {ratio}"),
        };
        self.show_seam(seam);
    }

    pub(super) fn edit_sew_press(&mut self, press: &Press) {
        match press.hit.seam.clone() {
            Some(seam) => self.show_seam(seam),
            None => self.session.selection.seam = None,
        }
    }

    /// Reverses side B of the selected seam.
    pub(super) fn flip_selected_seam(&mut self) {
        let Some(seam) = self.session.selection.seam.clone() else {
            self.status = "Select a seam first (Edit Sewing tool)".into();
            return;
        };
        self.session.edit("Flip Seam", |p| {
            azam_doc::flip_seam(p, &seam);
        });
        self.status = "Flipped side B".into();
    }

    /// Swaps the sides of the selected seam.
    pub(super) fn swap_selected_seam(&mut self) {
        let Some(seam) = self.session.selection.seam.clone() else {
            self.status = "Select a seam first (Edit Sewing tool)".into();
            return;
        };
        self.session.edit("Swap Seam Sides", |p| {
            azam_doc::swap_seam_groups(p, &seam);
        });
        self.status = "Swapped the seam's sides".into();
    }
}
