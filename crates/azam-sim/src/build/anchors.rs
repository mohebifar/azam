//! Points of the pattern tied to particles: pins and button-to-buttonhole fasteners.

use azam_doc::{ItemId, PieceId, Project};
use azam_geom::Vec2;

use crate::state::{Fasteners, PieceRecord};

/// The particle nearest the pattern point `(x, y)` of record `r`: the corner with the largest
/// barycentric weight of the triangle containing it (or the nearest triangle).
#[must_use]
#[allow(
    clippy::float_cmp,
    reason = "picks the corner whose weight is the maximum itself"
)]
pub fn nearest_particle(r: &PieceRecord, x: f64, y: f64) -> Option<usize> {
    let hit = r.mesh.locate(Vec2::new(x, y))?;
    let [l1, l2, l3] = hit.barycentric;
    let m = l1.max(l2).max(l3);
    let corner = if m == l1 {
        hit.vertices[0]
    } else if m == l2 {
        hit.vertices[1]
    } else {
        hit.vertices[2]
    };
    Some(r.offset + corner as usize)
}

fn particle_at(
    records: &[PieceRecord],
    piece: Option<&PieceId>,
    x: Option<f64>,
    y: Option<f64>,
) -> Option<usize> {
    let r = records.iter().find(|r| Some(&r.id) == piece)?;
    nearest_particle(r, x?, y?)
}

/// Buttons fastened to buttonholes.
pub(crate) fn fasteners(project: &Project, records: &[PieceRecord]) -> Fasteners {
    let mut out = Fasteners::default();
    for button in project.buttons.values() {
        let Some(hole_id) = button
            .extra
            .get("fasten")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(hole) = project.holes.get(&ItemId::new(hole_id)) else {
            continue;
        };
        let a = particle_at(records, button.p.as_ref(), button.x, button.y);
        let b = particle_at(records, hole.p.as_ref(), hole.x, hole.y);
        if let (Some(a), Some(b)) = (a, b) {
            out.a.push(a as u32);
            out.b.push(b as u32);
        }
    }
    out
}

/// Pinned particles of every piece, records in order, with where each pin holds the cloth if it
/// says.
pub(crate) fn pinned(project: &Project, records: &[PieceRecord]) -> Vec<(u32, Option<[f64; 3]>)> {
    let mut out = Vec::new();
    for r in records {
        let Some(piece) = project.pieces.get(&r.id) else {
            continue;
        };
        for pin in &piece.pins {
            if let Some(q) = nearest_particle(r, pin.x, pin.y) {
                out.push((q as u32, pin.at));
            }
        }
    }
    out
}
