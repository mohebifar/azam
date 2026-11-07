//! Sewing constraints from the project's seams.
//!
//! Each side of a seam lists the boundary particles it covers with their position along the side;
//! sides in one group are joined end to end and normalised to 0–1. Every particle of one group is
//! then tied to the matching point between two neighbouring particles of the other group, in both
//! directions.

use std::collections::HashMap;

use azam_doc::{PieceId, PieceState, Project, SeamSide};
use azam_geom::SideDirection;

use crate::numeric::clamp;
use crate::state::{PieceRecord, SewingConstraints};

/// Boundary samples this close to the side's start (mm) count as at its start.
const END_SNAP_MM: f64 = 0.5;
/// Particles of the other group closer than this in normalised position share a point.
const MIN_PARAM_GAP: f64 = 1e-9;

/// A boundary particle and its position along a side or group.
#[derive(Clone, Copy, Debug)]
struct SidePoint {
    particle: u32,
    u: f64,
}

/// The particles a side covers, sorted along it, and the side's length.
fn side_points(
    records: &[PieceRecord],
    by_id: &HashMap<&PieceId, usize>,
    side: &SeamSide,
) -> Option<(Vec<SidePoint>, f64)> {
    let r = &records[*by_id.get(&side.piece)?];
    if r.state == PieceState::Off {
        return None;
    }
    let mesh = &r.mesh;
    let ol = &mesh.outline;
    let range = side.side.range(ol)?;
    let total = ol.total;
    let mut out = Vec::new();
    for i in 0..mesh.boundary_count() {
        let k = mesh.boundary_segment[i] as usize;
        let p = ol.starts[k] + ol.segments[k].len * f64::from(mesh.boundary_t[i]);
        let mut o = match range.direction {
            SideDirection::Forward => p - range.start,
            SideDirection::Backward => range.start - p,
        };
        o = ((o % total) + total) % total;
        if o > total - END_SNAP_MM {
            o = 0.0;
        }
        if o <= range.length + END_SNAP_MM {
            out.push(SidePoint {
                particle: (r.offset + mesh.boundary[i] as usize) as u32,
                u: o.min(range.length),
            });
        }
    }
    out.sort_by(|a, b| a.u.partial_cmp(&b.u).unwrap_or(std::cmp::Ordering::Equal));
    Some((out, range.length))
}

/// The particles of a group of sides joined end to end, positions normalised to 0–1.
fn group_points(
    records: &[PieceRecord],
    by_id: &HashMap<&PieceId, usize>,
    sides: &[SeamSide],
) -> Vec<SidePoint> {
    let parts: Vec<(Vec<SidePoint>, f64)> = sides
        .iter()
        .filter_map(|s| side_points(records, by_id, s))
        .filter(|(list, _)| !list.is_empty())
        .collect();
    let total: f64 = parts.iter().map(|(_, l)| l).sum();
    if parts.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let mut all = Vec::new();
    let mut acc = 0.0;
    for (list, len) in &parts {
        all.extend(list.iter().map(|e| SidePoint {
            particle: e.particle,
            u: (acc + e.u) / total,
        }));
        acc += len;
    }
    all
}

/// Ties every particle of `a` to the matching point of `b`.
fn pair_up(out: &mut SewingConstraints, a: &[SidePoint], b: &[SidePoint], seam: u32) {
    if b.len() < 2 {
        return;
    }
    let mut j = 0;
    for p in a {
        while j + 2 < b.len() && b[j + 1].u < p.u {
            j += 1;
        }
        let (b0, b1) = (b[j], b[j + 1]);
        let du = b1.u - b0.u;
        let u = if du > MIN_PARAM_GAP {
            clamp((p.u - b0.u) / du, 0.0, 1.0)
        } else {
            0.0
        };
        if b0.particle == p.particle || b1.particle == p.particle {
            continue;
        }
        out.a.push(p.particle);
        out.b0.push(b0.particle);
        out.b1.push(b1.particle);
        out.u.push(u as f32);
        out.seam.push(seam);
    }
}

/// Sewing constraints of every seam, seams in id order.
pub(crate) fn sewing(project: &Project, records: &[PieceRecord]) -> SewingConstraints {
    let by_id: HashMap<&PieceId, usize> = records.iter().enumerate().map(|(i, r)| (&r.id, i)).collect();
    let mut seams: Vec<_> = project.sewing.iter().collect();
    seams.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = SewingConstraints::default();
    for (index, (_, seam)) in seams.iter().enumerate() {
        let a = group_points(records, &by_id, &seam.a);
        let b = group_points(records, &by_id, &seam.b);
        if a.is_empty() || b.is_empty() {
            continue;
        }
        pair_up(&mut out, &a, &b, index as u32);
        pair_up(&mut out, &b, &a, index as u32);
    }
    out
}
