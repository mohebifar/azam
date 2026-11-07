//! Seams on the outline: numbering, which edges are sewn or free, and seam allowances.

use std::collections::BTreeMap;

use azam_geom::{Outline, PointId, SideDirection, SideRange};
use indexmap::IndexMap;

use crate::ids::{PieceId, SeamId};
use crate::mirror::{src_seg_id, sync_mirrors};
use crate::num::sort_js_object_order;
use crate::piece::Piece;
use crate::project::{Project, Seam, SeamSide};

/// Overlaps and gaps shorter than this are ignored, in mm.
const MIN_OVERLAP_MM: f64 = 0.5;
/// Sides that pass the outline start by less than this end on it, in mm.
const WRAP_EPSILON_MM: f64 = 1e-9;
/// Allowance reported for a seam none of whose segments exist, in mm.
const FALLBACK_SEAM_ALLOWANCE_MM: f64 = 10.0;
/// Seam-number sort key weight of the piece order (more than any point index).
const ORDER_WEIGHT: f64 = 1e4;
/// Seam-number sort key used for a side whose start segment is missing.
const MISSING_INDEX: f64 = 9999.0;
/// Weight of the fraction along the start segment in the seam-number sort key (below 1 so it never
/// reaches the next point).
const T_WEIGHT: f64 = 0.99;

/// Perimeter interval `[start, start + length)` covered by a side, with `start` in `[0, total)`.
fn covered_interval(range: &SideRange, total: f64) -> (f64, f64) {
    let from = match range.direction {
        SideDirection::Forward => range.start,
        SideDirection::Backward => range.start - range.length,
    };
    let s0 = ((from % total) + total) % total;
    (s0, s0 + range.length)
}

/// Stable seam numbers 1..N: seams are ordered by their first piece in pattern order, then by
/// where they start along its outline.
#[must_use]
pub fn seam_numbers(project: &Project) -> IndexMap<SeamId, usize> {
    let key = |sw: &Seam| -> f64 {
        sw.sides()
            .filter_map(|(_, sd)| {
                let pc = project.pieces.get(&sd.piece)?;
                let i = pc
                    .point_index(&sd.side.a.segment)
                    .map_or(MISSING_INDEX, |i| i as f64);
                Some(f64::from(pc.order) * ORDER_WEIGHT + i + sd.side.a.t * T_WEIGHT)
            })
            .fold(f64::INFINITY, f64::min)
    };
    let mut list: Vec<(&SeamId, f64)> = project.sewing.iter().map(|(id, sw)| (id, key(sw))).collect();
    list.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .filter(|o| o.is_ne())
            .unwrap_or_else(|| a.0.cmp(b.0))
    });
    list.into_iter()
        .enumerate()
        .map(|(i, (id, _))| (id.clone(), i + 1))
        .collect()
}

/// One sewing side on a piece's outline.
#[derive(Clone, Debug, PartialEq)]
pub struct CoveredSide {
    /// Perimeter position where the covered stretch starts, in `[0, total)`.
    pub start: f64,
    /// Perimeter position where it ends (`start` plus its length; may pass `total`).
    pub end: f64,
    /// The seam.
    pub seam: SeamId,
    /// Seam number, when numbers were supplied.
    pub number: Option<usize>,
    /// Pieces on the other side of the seam.
    pub partners: Vec<PieceId>,
}

/// A free (unsewn) stretch of one segment.
#[derive(Clone, Debug, PartialEq)]
pub struct FreePart {
    /// Perimeter start.
    pub start: f64,
    /// Perimeter end.
    pub end: f64,
    /// Index of the segment.
    pub segment: usize,
    /// Id of the segment.
    pub segment_id: PointId,
}

/// Which parts of a piece's sewing line are sewn and which are free.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeCoverage {
    /// Every sewing side on the piece.
    pub sides: Vec<CoveredSide>,
    /// Merged sewn intervals, sorted, within `[0, total]`.
    pub sewn: Vec<[f64; 2]>,
    /// Free intervals; a free run through the outline start is joined into one that passes
    /// `total`.
    pub free: Vec<[f64; 2]>,
    /// Free intervals split per segment.
    pub free_parts: Vec<FreePart>,
    /// Perimeter of the outline.
    pub total: f64,
}

/// Sewn and free parts of `piece`'s outline `ol`, with seam numbers from `numbers` if given.
#[must_use]
pub fn edge_coverage(
    project: &Project,
    piece: &Piece,
    ol: &Outline,
    numbers: Option<&IndexMap<SeamId, usize>>,
) -> EdgeCoverage {
    let total = ol.total;
    let mut sides = Vec::new();
    for sw in project.sewing.values() {
        for (group, sd) in sw.sides() {
            if sd.piece != piece.id {
                continue;
            }
            let Some(r) = sd.side.range(ol) else {
                continue;
            };
            let (start, end) = covered_interval(&r, total);
            sides.push(CoveredSide {
                start,
                end,
                seam: sw.id.clone(),
                number: numbers.and_then(|n| n.get(&sw.id).copied()),
                partners: sw.group(group.other()).iter().map(|o| o.piece.clone()).collect(),
            });
        }
    }
    let mut intervals = Vec::new();
    for s in &sides {
        if s.end <= total {
            intervals.push([s.start, s.end]);
        } else {
            intervals.push([s.start, total]);
            // A side ending on the outline start (up to rounding) does not wrap.
            if s.end - total > WRAP_EPSILON_MM {
                intervals.push([0.0, s.end - total]);
            }
        }
    }
    intervals.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut sewn: Vec<[f64; 2]> = Vec::new();
    for x in intervals {
        match sewn.last_mut() {
            Some(m) if x[0] <= m[1] + MIN_OVERLAP_MM => m[1] = m[1].max(x[1]),
            _ => sewn.push(x),
        }
    }
    let mut free = Vec::new();
    let mut cur = 0.0_f64;
    for m in &sewn {
        if m[0] - cur > MIN_OVERLAP_MM {
            free.push([cur, m[0]]);
        }
        cur = cur.max(m[1]);
    }
    if total - cur > MIN_OVERLAP_MM {
        free.push([cur, total]);
    }
    let wraps = free.len() > 1
        && free[0][0] < MIN_OVERLAP_MM
        && free.last().is_some_and(|l| l[1] > total - MIN_OVERLAP_MM);
    if wraps && let Some(last) = free.pop() {
        free[0] = [last[0], total + free[0][1]];
    }
    let mut free_parts = Vec::new();
    for f in &free {
        let mut base = (f[0] / total).floor() * total;
        while base < f[1] {
            for (k, sg) in ol.segments.iter().enumerate() {
                let a = f[0].max(base + ol.starts[k]);
                let b = f[1].min(base + ol.starts[k] + sg.len);
                if b - a > MIN_OVERLAP_MM {
                    free_parts.push(FreePart {
                        start: a,
                        end: b,
                        segment: k,
                        segment_id: sg.id.clone(),
                    });
                }
            }
            base += total;
        }
    }
    EdgeCoverage {
        sides,
        sewn,
        free,
        free_parts,
        total,
    }
}

/// Ids of the segments a sewing side overlaps by more than 0.5 mm.
#[must_use]
pub fn side_seg_ids(ol: &Outline, side: &SeamSide) -> Vec<PointId> {
    let Some(r) = side.side.range(ol) else {
        return Vec::new();
    };
    let total = ol.total;
    let (s0, s1) = covered_interval(&r, total);
    ol.segments
        .iter()
        .zip(&ol.starts)
        .filter(|&(sg, &a)| {
            let b = a + sg.len;
            let ov = (b.min(s1) - a.max(s0)).max(0.0) + ((b + total).min(s1) - (a + total).max(s0)).max(0.0);
            ov > MIN_OVERLAP_MM
        })
        .map(|(sg, _)| sg.id.clone())
        .collect()
}

/// Where an allowance for segment `segment` of `piece` is stored: on the source of a linked
/// mirror, else on the piece itself.
fn allowance_target(project: &Project, piece: &Piece, segment: &PointId) -> (PieceId, PointId) {
    if let Some(src) = piece.mirror.as_ref().and_then(|m| project.pieces.get(&m.src))
        && let Some(s2) = src_seg_id(src, segment)
    {
        return (src.id.clone(), s2);
    }
    (piece.id.clone(), segment.clone())
}

fn set_segment_override(project: &mut Project, piece: &PieceId, segment: PointId, mm: f64) {
    if let Some(pc) = project.pieces.get_mut(piece) {
        pc.sa_seg.get_or_insert_with(BTreeMap::new).insert(segment, mm);
    }
}

/// Sets one allowance for a whole seam: every segment on both sides. Allowances of linked mirrors
/// are written through their source. Returns the number of segments set.
pub fn set_seam_sa(project: &mut Project, seam: &SeamId, mm: f64) -> usize {
    let Some(sw) = project.sewing.get(seam) else {
        return 0;
    };
    let mut writes = Vec::new();
    let mut count = 0;
    for (_, sd) in sw.sides() {
        let Some(pc) = project.pieces.get(&sd.piece) else {
            continue;
        };
        for sid in side_seg_ids(&pc.outline(), sd) {
            let (tp, ts) = allowance_target(project, pc, &sid);
            if tp != pc.id {
                // The mirror keeps its own copy too, until the next sync rewrites it.
                writes.push((pc.id.clone(), sid));
            }
            writes.push((tp, ts));
            count += 1;
        }
    }
    for (p, s) in writes {
        set_segment_override(project, &p, s, mm);
    }
    sync_mirrors(project);
    count
}

/// Allowance of a seam: the most common allowance over its segments, in mm.
#[must_use]
pub fn seam_sa(project: &Project, seam: &Seam) -> f64 {
    let mut counts: Vec<(f64, usize)> = Vec::new();
    for (_, sd) in seam.sides() {
        let Some(pc) = project.pieces.get(&sd.piece) else {
            continue;
        };
        for sid in side_seg_ids(&pc.outline(), sd) {
            let v = pc
                .sa_seg
                .as_ref()
                .and_then(|m| m.get(&sid))
                .copied()
                .unwrap_or(pc.sa);
            #[allow(clippy::float_cmp, reason = "groups identical stored values")]
            let entry = counts
                .iter_mut()
                .find(|(x, _)| *x == v || (x.is_nan() && v.is_nan()));
            match entry {
                Some(entry) => entry.1 += 1,
                None => counts.push((v, 1)),
            }
        }
    }
    // Ties go to the value that comes first in stored object order (whole numbers first).
    let mut keyed: Vec<(String, (f64, usize))> = counts.into_iter().map(|c| (number_key(c.0), c)).collect();
    sort_js_object_order(&mut keyed);
    let mut best: Option<(f64, usize)> = None;
    for (_, (v, n)) in keyed {
        if best.is_none_or(|(_, bn)| n > bn) {
            best = Some((v, n));
        }
    }
    best.map_or(FALLBACK_SEAM_ALLOWANCE_MM, |(v, _)| v)
}

/// Object key a number is stored under: whole non-negative numbers print without a fraction.
fn number_key(v: f64) -> String {
    if v.fract() == 0.0 && (0.0..1e21).contains(&v) {
        format!("{}", v as u64)
    } else {
        format!("{v}")
    }
}

/// Sets the allowance of segment `segment` of `piece`. When the segment is sewn, the allowance of
/// every seam on it is set instead (both sides at once). Returns `false` if there is no such piece
/// or segment.
pub fn set_segment_sa(project: &mut Project, piece: &PieceId, segment: &PointId, mm: f64) -> bool {
    let Some(pc) = project.pieces.get(piece) else {
        return false;
    };
    let ol = pc.outline();
    let Some(k) = ol.segment_index(segment) else {
        return false;
    };
    let cov = edge_coverage(project, pc, &ol, None);
    let (s0, total) = (ol.starts[k], ol.total);
    let s1 = s0 + ol.segments[k].len;
    let mut seams: Vec<SeamId> = Vec::new();
    for x in &cov.sides {
        let overlaps = s1.min(x.end) - s0.max(x.start) > MIN_OVERLAP_MM
            || (s1 + total).min(x.end) - (s0 + total).max(x.start) > MIN_OVERLAP_MM;
        if overlaps && !seams.contains(&x.seam) {
            seams.push(x.seam.clone());
        }
    }
    if seams.is_empty() {
        let (tp, ts) = allowance_target(project, pc, segment);
        set_segment_override(project, &tp, ts, mm);
        sync_mirrors(project);
    } else {
        for id in &seams {
            set_seam_sa(project, id, mm);
        }
    }
    true
}

/// Sets the allowance of every piece to `mm` and clears per-segment overrides.
pub fn set_all_sa(project: &mut Project, mm: f64) {
    for pc in project.pieces.values_mut() {
        pc.sa = mm;
        pc.sa_seg = None;
    }
}
