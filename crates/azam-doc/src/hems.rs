//! Hems: default widths, suggesting hems for free edges and setting them.

use std::collections::{BTreeMap, HashSet};

use azam_geom::{PointId, bbox};
use indexmap::IndexMap;

use crate::ids::{FabricId, PieceId};
use crate::mirror::{src_seg_id, sync_mirrors};
use crate::piece::Piece;
use crate::project::Project;
use crate::seams::edge_coverage;

/// Hem of knits (jersey, rib, fleece), in mm: they do not fray and a deep hem flutes.
const KNIT_HEM_MM: f64 = 20.0;
/// Hem of light cloth, in mm: a deep hem drags and shows through.
const LIGHT_CLOTH_HEM_MM: f64 = 25.0;
/// Light cloth is below this density, in g/m².
const LIGHT_CLOTH_DENSITY: f64 = 200.0;
/// Hem of sleeves and ordinary pieces, in mm.
const STANDARD_HEM_MM: f64 = 30.0;
/// Hem of long pieces (robes, long skirts), in mm.
const LONG_PIECE_HEM_MM: f64 = 40.0;
/// Pieces at least this tall count as long, in mm.
const LONG_PIECE_HEIGHT_MM: f64 = 1000.0;
/// Weave or preset names of knits.
const KNIT_NAMES: [&str; 3] = ["jersey", "rib", "fleece"];
/// Arrangement slots of sleeves start with this.
const ARM_SLOT_PREFIX: &str = "arm";

/// Pieces shorter than this get no hem suggestions, in mm.
const MIN_HEMMED_PIECE_HEIGHT_MM: f64 = 200.0;
/// Segments shorter than this are never hemmed, in mm.
const MIN_HEM_SEGMENT_MM: f64 = 40.0;
/// A segment at most this fraction sewn counts as free.
const FREE_MAX_SEWN: f64 = 0.2;
/// A bottom edge rises at most this much per unit of width.
const BOTTOM_EDGE_MAX_SLOPE: f64 = 0.6;
/// A bottom edge reaches into the lowest this fraction of the piece.
const BOTTOM_BAND: f64 = 0.12;
/// A neighbour more than this fraction sewn makes a free edge a slit.
const SLIT_NEIGHBOUR_SEWN: f64 = 0.5;
/// An opening edge runs at most this much sideways per unit of height.
const OPENING_MAX_RUN: f64 = 0.35;
/// An opening edge is at least this fraction of the piece height.
const OPENING_MIN_HEIGHT: f64 = 0.4;
/// Narrow hem given to the other free edges of unlined pieces, in mm.
const NARROW_HEM_MM: f64 = 10.0;

/// Default hem width for `piece`, in mm, from its fabric, arrangement and size.
#[must_use]
pub fn default_hem(project: &Project, piece: &Piece) -> f64 {
    let fabric = project.fabrics.get(&piece.fabric);
    let names = fabric.map_or_else(String::new, |f| format!("{}{}", f.weave, f.preset));
    if KNIT_NAMES.iter().any(|k| names.contains(k)) {
        return KNIT_HEM_MM;
    }
    let density = fabric.map_or(0.0, |f| f.phys.density);
    if density > 0.0 && density < LIGHT_CLOTH_DENSITY {
        return LIGHT_CLOTH_HEM_MM;
    }
    let height = bbox(piece.outline().poly).map_or(0.0, |b| b.size().y);
    if piece.arr.slot.starts_with(ARM_SLOT_PREFIX) {
        STANDARD_HEM_MM
    } else if height >= LONG_PIECE_HEIGHT_MM {
        LONG_PIECE_HEM_MM
    } else {
        STANDARD_HEM_MM
    }
}

/// Hem suggestions for free (unsewn) edges, by piece and segment, in mm:
///
/// - bottom edges (mostly horizontal, in the lowest 12 % of the piece) get a hem;
/// - open vertical edges (front openings: long, not a slit between two sewn edges) get a hem of the
///   same width;
/// - a fabric with an open front edge is treated as unlined: the other free edges of its pieces
///   (neck, slits) get a 10 mm narrow hem.
///
/// Linked mirrors are skipped (they follow their source).
#[must_use]
pub fn detect_hems(project: &Project) -> IndexMap<PieceId, BTreeMap<PointId, f64>> {
    struct Found<'a> {
        piece: &'a Piece,
        free: Vec<bool>,
        hem: BTreeMap<PointId, f64>,
        segments: Vec<PointId>,
    }
    let mut ordered: Vec<(String, &Piece)> = project.pieces.iter().map(|(k, p)| (k.0.clone(), p)).collect();
    ordered.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    let mut unlined: HashSet<&FabricId> = HashSet::new();
    let mut found = Vec::new();
    for (_, pc) in ordered {
        if pc.pts.len() < 3 || pc.mirror.is_some() {
            continue;
        }
        let ol = pc.outline();
        let Some(bb) = bbox(ol.poly.iter().copied()) else {
            continue;
        };
        let height = bb.size().y;
        if height < MIN_HEMMED_PIECE_HEIGHT_MM {
            continue;
        }
        let cov = edge_coverage(project, pc, &ol, None);
        let w = default_hem(project, pc);
        let n = ol.segments.len();
        let sewn_frac = |k: usize| {
            let s0 = ol.starts[k];
            let s1 = s0 + ol.segments[k].len;
            let v: f64 = cov
                .sewn
                .iter()
                .map(|m| (s1.min(m[1]) - s0.max(m[0])).max(0.0))
                .sum();
            let len = ol.segments[k].len;
            v / if len == 0.0 { 1.0 } else { len }
        };
        let free: Vec<bool> = (0..n)
            .map(|k| ol.segments[k].len >= MIN_HEM_SEGMENT_MM && sewn_frac(k) <= FREE_MAX_SEWN)
            .collect();
        let mut hem = BTreeMap::new();
        for (k, sg) in ol.segments.iter().enumerate() {
            if !free[k] {
                continue;
            }
            let (Some(&a), Some(&b)) = (sg.poly.first(), sg.poly.last()) else {
                continue;
            };
            let mid = sg.point_at(0.5).pos;
            let (dx, dy) = ((b.x - a.x).abs(), (b.y - a.y).abs());
            let lowest = a.y.max(b.y).max(mid.y);
            if dy <= dx * BOTTOM_EDGE_MAX_SLOPE && lowest >= bb.max.y - height * BOTTOM_BAND {
                hem.insert(sg.id.clone(), w);
                continue;
            }
            let slit = sewn_frac((k + n - 1) % n) > SLIT_NEIGHBOUR_SEWN
                && sewn_frac((k + 1) % n) > SLIT_NEIGHBOUR_SEWN;
            if dx <= dy * OPENING_MAX_RUN && sg.len >= height * OPENING_MIN_HEIGHT && !slit {
                hem.insert(sg.id.clone(), w);
                unlined.insert(&pc.fabric);
            }
        }
        let segments = ol.segments.iter().map(|s| s.id.clone()).collect();
        found.push(Found {
            piece: pc,
            free,
            hem,
            segments,
        });
    }
    let mut out = IndexMap::new();
    for mut f in found {
        if unlined.contains(&f.piece.fabric) {
            for (k, id) in f.segments.iter().enumerate() {
                if f.free[k] && !f.hem.contains_key(id) {
                    f.hem.insert(id.clone(), NARROW_HEM_MM);
                }
            }
        }
        if !f.hem.is_empty() {
            out.insert(f.piece.id.clone(), f.hem);
        }
    }
    out
}

/// Applies [`detect_hems`]: adds the suggested hems to edges that have none. Returns the number of
/// hems added.
pub fn mark_hems(project: &mut Project) -> usize {
    let found = detect_hems(project);
    let mut added = 0;
    for (id, hems) in found {
        let Some(pc) = project.pieces.get_mut(&id) else {
            continue;
        };
        let h = pc.hem.get_or_insert_with(BTreeMap::new);
        for (seg, w) in hems {
            if let std::collections::btree_map::Entry::Vacant(e) = h.entry(seg) {
                e.insert(w);
                added += 1;
            }
        }
    }
    sync_mirrors(project);
    added
}

/// Sets the hem of segment `segment` of `piece` to `mm` (removes it when `mm` is not positive).
/// Hems of linked mirrors are written through their source. Returns `false` if there is no such
/// piece.
pub fn set_hem(project: &mut Project, piece: &PieceId, segment: &PointId, mm: f64) -> bool {
    let Some(pc) = project.pieces.get(piece) else {
        return false;
    };
    let (target, seg) = match pc.mirror.as_ref().and_then(|m| project.pieces.get(&m.src)) {
        Some(src) => match src_seg_id(src, segment) {
            Some(s2) => (src.id.clone(), s2),
            None => (piece.clone(), segment.clone()),
        },
        None => (piece.clone(), segment.clone()),
    };
    if let Some(tp) = project.pieces.get_mut(&target) {
        let mut h = tp.hem.take().unwrap_or_default();
        if mm > 0.0 {
            h.insert(seg, mm);
        } else {
            h.remove(&seg);
        }
        tp.hem = (!h.is_empty()).then_some(h);
    }
    sync_mirrors(project);
    true
}
