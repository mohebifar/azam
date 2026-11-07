//! Keeping seams, notches and topstitches attached after a piece's outline structure changes.

use azam_geom::{Outline, OutlinePoint, PointId, distance_to_segment};

use crate::ids::PieceId;
use crate::project::{Project, WHOLE_OUTLINE};

/// A location on the old outline that lies within this distance of a surviving segment stays on
/// that segment, in mm.
const STAY_ON_SEGMENT_MM: f64 = 0.5;
/// Fraction along a segment used to re-anchor a topstitch whose segment disappeared.
const STITCH_ANCHOR_T: f64 = 0.5;

/// Where a location `(segment, t)` of `old` lies on `new`: on the same segment when it survives
/// and the point is still on it, otherwise on the nearest segment.
fn remap(old: &Outline, new: &Outline, segment: &PointId, t: f64) -> Option<(PointId, f64)> {
    let k = old.segment_index(segment)?;
    let p = old.segments[k].point_at(t.clamp(0.0, 1.0)).pos;
    if let Some(kn) = new.segment_index(segment) {
        let seg = &new.segments[kn];
        let mut best_d = f64::INFINITY;
        let mut best_t = 0.0;
        for (i, w) in seg.poly.windows(2).enumerate() {
            let hit = distance_to_segment(p, w[0], w[1]);
            if hit.distance < best_d {
                best_d = hit.distance;
                let len = if seg.len == 0.0 { 1.0 } else { seg.len };
                best_t = (seg.cum[i] + (seg.cum[i + 1] - seg.cum[i]) * hit.t) / len;
            }
        }
        if best_d < STAY_ON_SEGMENT_MM {
            return Some((segment.clone(), best_t));
        }
    }
    let hit = new.nearest(p)?;
    Some((new.segments[hit.segment].id.clone(), hit.t))
}

/// Re-anchors the seams, notches and topstitches of piece `piece` after its points changed from
/// `old_pts` to its current points (points added, removed or turned into curve points).
pub fn remap_refs(project: &mut Project, piece: &PieceId, old_pts: &[OutlinePoint]) {
    let Some(pc) = project.pieces.get(piece) else {
        return;
    };
    let old = Outline::new(old_pts);
    let new = pc.outline();
    for sw in project.sewing.values_mut() {
        for sd in sw.a.iter_mut().chain(sw.b.iter_mut()) {
            if &sd.piece != piece {
                continue;
            }
            let a = remap(&old, &new, &sd.side.a.segment, sd.side.a.t);
            let b = remap(&old, &new, &sd.side.b.segment, sd.side.b.t);
            if let Some((s, t)) = a {
                sd.side.a.segment = s;
                sd.side.a.t = t;
            }
            if let Some((s, t)) = b {
                sd.side.b.segment = s;
                sd.side.b.t = t;
            }
        }
    }
    if let Some(pc) = project.pieces.get_mut(piece) {
        for notch in &mut pc.notches {
            if let Some((s, t)) = remap(&old, &new, &notch.s, notch.t.unwrap_or(0.0)) {
                notch.s = s;
                notch.t = Some(t);
            }
        }
    }
    for st in project.stitches.values_mut() {
        let on_piece = st.p.as_ref() == Some(piece);
        if on_piece
            && st.seg.0 != WHOLE_OUTLINE
            && new.segment_index(&st.seg).is_none()
            && let Some((s, _)) = remap(&old, &new, &st.seg, STITCH_ANCHOR_T)
        {
            st.seg = s;
        }
    }
}
