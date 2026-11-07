//! Bringing pattern pieces in from other software.

use azam_doc::{LinePoint, PieceId, Project};
use azam_io::ImportedPiece;

/// Adds `pieces` (read from a DXF file) to `project` with their internal lines, and places each
/// around the avatar by its name ("Back", "Sleeve L", "Collar"…). A second unsided sleeve goes on
/// the right arm. Returns the new pieces' ids.
pub fn add_imported_pieces(project: &mut Project, pieces: &[ImportedPiece]) -> Vec<PieceId> {
    let mut created = Vec::new();
    let mut arm_left_taken = false;
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
        if slot == Some("armL") {
            if arm_left_taken && !names_a_side(&piece.name) {
                slot = Some("armR");
            }
            arm_left_taken = true;
        }
        if let Some(pc) = project.pieces.get_mut(&id) {
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
        ];
        let ids = add_imported_pieces(&mut project, &pieces);
        assert_eq!(ids.len(), 4);
        let slot = |i: usize| project.pieces[&ids[i]].arr.slot.clone();
        assert_eq!(slot(0), "back");
        assert_eq!(slot(1), "armL");
        assert_eq!(slot(2), "armR", "the second sleeve goes on the other arm");
        assert_eq!(slot(3), azam_doc::DEFAULT_SLOT);
        assert_eq!(
            project.pieces[&ids[0]].internals.len(),
            1,
            "the internal line comes along"
        );
        assert_eq!(project.pieces[&ids[0]].name, "Back");
    }
}
