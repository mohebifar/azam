//! Bringing pattern pieces in from other software: DXF (AAMA blocks) and SVG drawings.

use std::path::PathBuf;

use azam_doc::{LinePoint, PieceId, Project};
use azam_io::ImportedPiece;

use crate::app::AzamApp;

/// A file format pieces can be imported from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat {
    /// AAMA-style DXF.
    Dxf,
    /// An SVG drawing.
    Svg,
}

impl ImportFormat {
    /// Name, as the file browser and the undo history show it.
    const fn name(self) -> &'static str {
        match self {
            Self::Dxf => "DXF",
            Self::Svg => "SVG",
        }
    }

    /// The file browser's filter.
    const fn filter(self) -> &'static str {
        match self {
            Self::Dxf => "DXF\t*.dxf",
            Self::Svg => "SVG\t*.svg",
        }
    }

    /// The pieces in `text`.
    fn read(self, text: &str) -> Result<Vec<ImportedPiece>, String> {
        match self {
            Self::Dxf => azam_io::import_dxf(text).map_err(|e| e.to_string()),
            Self::Svg => azam_io::import_svg(text).map_err(|e| e.to_string()),
        }
    }
}

impl AzamApp {
    /// Asks for a file in `format` and adds its pieces to the project, as one undoable edit.
    pub(crate) fn import_file(&mut self, format: ImportFormat) {
        let dir = std::env::var_os("HOME").map(PathBuf::from);
        let path = match blenderui::file_dialog(
            &format!("Import {}", format.name()),
            blenderui::FileDialogMode::Open,
            dir.as_deref(),
            None,
            &[format.filter()],
            false,
        ) {
            blenderui::FileDialogResult::Chosen(paths) => match paths.into_iter().next() {
                Some(p) => p,
                None => return,
            },
            blenderui::FileDialogResult::Cancelled => return,
            blenderui::FileDialogResult::Unavailable => {
                self.status = "Importing needs a window for the file browser".into();
                return;
            }
        };
        let imported = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| format.read(&text));
        match imported {
            Ok(pieces) => {
                let count = pieces.len();
                self.session.edit(&format!("Import {}", format.name()), |p| {
                    add_imported_pieces(p, &pieces);
                });
                self.status = format!("Imported {count} piece(s) from {}", path.display());
            }
            Err(e) => self.status = format!("Import failed: {e}"),
        }
    }
}

impl AzamApp {
    /// Asks for an OBJ body mesh and makes it the avatar (kept with the project as a file path).
    pub(crate) fn import_avatar(&mut self) {
        let dir = std::env::var_os("HOME").map(PathBuf::from);
        match blenderui::file_dialog(
            "Import Avatar OBJ",
            blenderui::FileDialogMode::Open,
            dir.as_deref(),
            None,
            &["OBJ\t*.obj"],
            false,
        ) {
            blenderui::FileDialogResult::Chosen(paths) => {
                if let Some(path) = paths.into_iter().next() {
                    self.set_custom_body(Some(&path.to_string_lossy()));
                }
            }
            blenderui::FileDialogResult::Cancelled => {}
            blenderui::FileDialogResult::Unavailable => {
                self.status = "Importing needs a window for the file browser".into();
            }
        }
    }

    /// Which body model the avatar was built from, once built.
    #[must_use]
    pub fn avatar_kind(&self) -> Option<azam_avatar::AvatarKind> {
        self.scene3d.avatar.as_ref().map(|a| a.kind)
    }

    /// Uses the OBJ body at `path` as the avatar, or the built-in body with `None`.
    pub fn set_custom_body(&mut self, path: Option<&str>) {
        let label = if path.is_some() {
            "Custom Body"
        } else {
            "Built-in Body"
        };
        self.session.edit(label, |p| match path {
            Some(path) => {
                p.avatar
                    .extra
                    .insert(crate::garment3d::CUSTOM_BODY_KEY.to_owned(), path.into());
            }
            None => {
                p.avatar.extra.remove(crate::garment3d::CUSTOM_BODY_KEY);
            }
        });
        self.status = match path {
            Some(path) => format!("Avatar body from {path}"),
            None => "Built-in avatar body".into(),
        };
    }
}

/// Adds `pieces` (read from a DXF or SVG file) to `project` with their internal lines, and places each
/// around the avatar by its name ("Back", "Sleeve L", "Collar"…). A second piece that names no
/// side and would land where an earlier one did goes on the other side ("Sleeve", "Sleeve" →
/// left and right arm; "Front Leg", "Front Leg" → left and right leg). Returns the new pieces' ids.
pub fn add_imported_pieces(project: &mut Project, pieces: &[ImportedPiece]) -> Vec<PieceId> {
    let mut created = Vec::new();
    let mut taken: Vec<&'static str> = Vec::new();
    for piece in pieces {
        let points: Vec<(azam_geom::Vec2, bool)> = piece.points.iter().map(|q| (q.pos, q.curve)).collect();
        let Some(id) = azam_doc::create_piece(project, &points) else {
            continue;
        };
        for line in &piece.internals {
            let pts: Vec<LinePoint> = line
                .points
                .iter()
                .map(|&pos| LinePoint {
                    id: None,
                    pos,
                    curve: false,
                })
                .collect();
            azam_doc::add_internal_line(project, &id, &pts, line.closed);
        }
        let mut slot = azam_doc::slot_for_name(&piece.name);
        if let Some(s) = slot
            && taken.contains(&s)
            && !names_a_side(&piece.name)
            && let Some(other) = azam_doc::mirror_slot(s)
        {
            slot = Some(other);
        }
        if let Some(s) = slot {
            taken.push(s);
        }
        let outline = project.pieces.get(&id).map(azam_doc::Piece::outline);
        for &q in &piece.notches {
            if let Some((segment, t)) = outline
                .as_ref()
                .and_then(|ol| ol.nearest(q).map(|n| (ol.segments[n.segment].id.clone(), n.t)))
            {
                azam_doc::add_notch(project, &id, &segment, t);
            }
        }
        if let Some(pc) = project.pieces.get_mut(&id) {
            if let Some(g) = piece.grain {
                pc.grain = g;
            }
            pc.name.clone_from(&piece.name);
            if let Some(slot) = slot {
                slot.clone_into(&mut pc.arr.slot);
            }
        }
        created.push(id);
    }
    created
}

/// Whether `name` says which side of the body the piece is for.
fn names_a_side(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| matches!(w, "l" | "r" | "lt" | "rt") || w.starts_with("left") || w.starts_with("right"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use azam_geom::{OutlinePoint, Vec2};
    use azam_io::ImportedLine;

    fn square(name: &str, x: f64) -> ImportedPiece {
        let pts = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        ImportedPiece {
            name: name.to_owned(),
            points: pts
                .iter()
                .enumerate()
                .map(|(i, &(px, py))| OutlinePoint {
                    id: azam_doc::PointId::new(format!("p{i}")),
                    pos: Vec2::new(px + x, py),
                    curve: false,
                })
                .collect(),
            internals: vec![ImportedLine {
                points: vec![Vec2::new(x + 50.0, 10.0), Vec2::new(x + 50.0, 90.0)],
                closed: false,
            }],
            notches: vec![Vec2::new(x + 30.0, 0.0)],
            grain: Some(15.0),
        }
    }

    #[test]
    fn imported_pieces_keep_internal_lines_and_get_slots() {
        let mut project = Project::new(None, azam_doc::Sex::Female, 0.0);
        let pieces = [
            square("Back", 0.0),
            square("Sleeve", 200.0),
            square("Sleeve", 400.0),
            square("Pocket", 600.0),
            square("Front Leg", 800.0),
            square("Front Leg", 1000.0),
        ];
        let ids = add_imported_pieces(&mut project, &pieces);
        assert_eq!(ids.len(), 6);
        let slot = |i: usize| project.pieces[&ids[i]].arr.slot.clone();
        assert_eq!(slot(0), "back");
        assert_eq!(slot(1), "armL");
        assert_eq!(slot(2), "armR", "the second sleeve goes on the other arm");
        assert_eq!(slot(3), azam_doc::DEFAULT_SLOT);
        assert_eq!(
            (slot(4).as_str(), slot(5).as_str()),
            ("legLF", "legRF"),
            "the legs pair up too"
        );
        assert_eq!(
            project.pieces[&ids[0]].internals.len(),
            1,
            "the internal line comes along"
        );
        assert_eq!(project.pieces[&ids[0]].name, "Back");
        assert_eq!(project.pieces[&ids[0]].notches.len(), 1, "the notch comes along");
        assert!(
            (project.pieces[&ids[0]].grain - 15.0).abs() < 1e-9,
            "and the grain line"
        );
    }
}
