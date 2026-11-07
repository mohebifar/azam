//! What is selected: pieces, the active piece and its points, a segment, a seam or a part placed
//! on a piece.

use std::collections::BTreeSet;

use azam_doc::{ItemId, ItemKind, PieceId, PointId, Project, SeamId, StitchId};

/// A part placed on a piece that can be selected on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// Notch `index` of `piece`.
    Notch {
        /// The piece.
        piece: PieceId,
        /// Index in its notch list.
        index: usize,
    },
    /// Internal line `index` of `piece`.
    Internal {
        /// The piece.
        piece: PieceId,
        /// Index in its internal line list.
        index: usize,
    },
    /// Graphic `index` of `piece`.
    Graphic {
        /// The piece.
        piece: PieceId,
        /// Index in its graphic list.
        index: usize,
    },
    /// A topstitch.
    Stitch(StitchId),
    /// A button, buttonhole or comment.
    Item(ItemKind, ItemId),
}

impl Part {
    /// Whether the part still exists in `project`.
    #[must_use]
    pub fn exists(&self, project: &Project) -> bool {
        let piece = |id: &PieceId| project.pieces.get(id);
        match self {
            Self::Notch { piece: p, index } => piece(p).is_some_and(|pc| *index < pc.notches.len()),
            Self::Internal { piece: p, index } => piece(p).is_some_and(|pc| *index < pc.internals.len()),
            Self::Graphic { piece: p, index } => piece(p).is_some_and(|pc| *index < pc.graphics.len()),
            Self::Stitch(id) => project.stitches.contains_key(id),
            Self::Item(kind, id) => project.items(*kind).contains_key(id),
        }
    }
}

/// The selection, kept apart from the project so selecting is not an undoable edit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    /// Selected pieces.
    pub pieces: BTreeSet<PieceId>,
    /// The active piece (last clicked): the properties editor shows it.
    pub active: Option<PieceId>,
    /// Selected outline points of the active piece, by index.
    pub points: Vec<usize>,
    /// The selected seam.
    pub seam: Option<SeamId>,
    /// The selected segment: piece and segment id.
    pub segment: Option<(PieceId, PointId)>,
    /// The selected part.
    pub part: Option<Part>,
}

impl Selection {
    /// Selects only `piece` and makes it active.
    pub fn select_only(&mut self, piece: PieceId) {
        self.pieces.clear();
        self.pieces.insert(piece.clone());
        self.active = Some(piece);
        self.points.clear();
        self.seam = None;
        self.segment = None;
        self.part = None;
    }

    /// Adds `piece` to the selection, or removes it if it was selected (Shift+click).
    pub fn toggle(&mut self, piece: PieceId) {
        if self.pieces.remove(&piece) {
            if self.active.as_ref() == Some(&piece) {
                self.active = self.pieces.iter().next().cloned();
            }
        } else {
            self.pieces.insert(piece.clone());
            self.active = Some(piece);
        }
        self.points.clear();
        self.segment = None;
        self.part = None;
    }

    /// Selects only `part` (pieces stay selected).
    pub fn select_part(&mut self, part: Part) {
        self.part = Some(part);
        self.seam = None;
        self.segment = None;
        self.points.clear();
    }

    /// Selects only `seam` (pieces stay selected).
    pub fn select_seam(&mut self, seam: SeamId) {
        self.seam = Some(seam);
        self.part = None;
        self.segment = None;
        self.points.clear();
    }

    /// Selects segment `segment` of `piece`, making the piece active.
    pub fn select_segment(&mut self, piece: PieceId, segment: PointId) {
        self.select_only(piece.clone());
        self.segment = Some((piece, segment));
    }

    /// Clears everything.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Selects every piece, or none if all are already selected (Blender's A).
    pub fn toggle_all(&mut self, project: &Project) {
        if self.pieces.len() == project.pieces.len() {
            self.clear();
        } else {
            self.pieces = project.pieces.keys().cloned().collect();
            if self.active.is_none() {
                self.active = project.pieces.keys().next().cloned();
            }
        }
    }

    /// Drops ids that no longer exist in `project` (after undo or delete).
    pub fn retain_existing(&mut self, project: &Project) {
        self.pieces.retain(|p| project.pieces.contains_key(p));
        if self
            .active
            .as_ref()
            .is_some_and(|a| !project.pieces.contains_key(a))
        {
            self.active = self.pieces.iter().next().cloned();
            self.points.clear();
        }
        if let Some(active) = &self.active {
            let n = project.pieces.get(active).map_or(0, |p| p.pts.len());
            self.points.retain(|&i| i < n);
        }
        if self
            .seam
            .as_ref()
            .is_some_and(|s| !project.sewing.contains_key(s))
        {
            self.seam = None;
        }
        if self.segment.as_ref().is_some_and(|(p, s)| {
            project
                .pieces
                .get(p)
                .is_none_or(|pc| pc.point_index(s).is_none_or(|i| pc.pts[i].curve))
        }) {
            self.segment = None;
        }
        if self.part.as_ref().is_some_and(|part| !part.exists(project)) {
            self.part = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::new_project;
    use crate::operators::Template;

    #[test]
    fn deleted_parts_drop_out_of_the_selection() {
        let mut project = new_project(Template::Tee);
        let piece = project.pieces.keys().next().cloned().expect("a piece");
        let seg = project.pieces[&piece].pts[0].id.clone();
        let stitch = azam_doc::add_stitch(&mut project, &piece, Some(&seg)).expect("stitch");
        let mut sel = Selection::default();
        sel.select_part(Part::Stitch(stitch.clone()));
        sel.retain_existing(&project);
        assert_eq!(sel.part, Some(Part::Stitch(stitch.clone())));
        project.stitches.shift_remove(&stitch);
        sel.retain_existing(&project);
        assert_eq!(sel.part, None);
    }
}
