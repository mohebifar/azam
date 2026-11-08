//! Length links: two outline segments kept the same length, or one a set amount longer (ease),
//! as a front and back side seam or a sleeve cap and its armhole must be. Whichever side an edit
//! changes, the other follows: its curve is scaled about its start corner, so it keeps its shape
//! and only its far corner moves.
//!
//! Lengths are those of the base size; grading may still make linked segments differ in other
//! sizes.

use serde::{Deserialize, Serialize};

use crate::{PieceId, PointId, Project};

/// Linked segments within this many mm of their target length are left alone.
pub const LINK_TOLERANCE_MM: f64 = 0.01;

/// An outline segment: its piece and the id of its start corner.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SegmentRef {
    /// The piece.
    #[serde(rename = "p")]
    pub piece: PieceId,
    /// The segment (its start corner's id).
    #[serde(rename = "s")]
    pub seg: PointId,
}

impl SegmentRef {
    /// Segment `seg` of `piece`.
    #[must_use]
    pub fn new(piece: &PieceId, seg: &PointId) -> Self {
        Self {
            piece: piece.clone(),
            seg: seg.clone(),
        }
    }
}

/// Segment `b` kept `ease` mm longer than segment `a` (shorter when negative).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LengthLink {
    /// The reference segment.
    pub a: SegmentRef,
    /// The segment that follows it when the link is made.
    pub b: SegmentRef,
    /// How much longer `b` is than `a`, mm.
    #[serde(default)]
    pub ease: f64,
}

/// Each link's segment lengths, taken before an edit so [`sync_length_links`] can tell which side
/// the edit changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkLengths(Vec<(SegmentRef, SegmentRef, Option<f64>, Option<f64>)>);

/// Length of segment `r` in the base size, mm.
#[must_use]
pub fn segment_length(project: &Project, r: &SegmentRef) -> Option<f64> {
    let ol = project.pieces.get(&r.piece)?.outline();
    ol.segment_index(&r.seg).map(|k| ol.segments[k].len)
}

/// The linked segments' lengths now.
#[must_use]
pub fn link_lengths(project: &Project) -> LinkLengths {
    LinkLengths(
        project
            .links
            .iter()
            .map(|l| {
                (
                    l.a.clone(),
                    l.b.clone(),
                    segment_length(project, &l.a),
                    segment_length(project, &l.b),
                )
            })
            .collect(),
    )
}

/// Links segment `b` to segment `a` (`b` = `a` + `ease`) and makes `b` that long. Refused (`false`)
/// for a segment linked to itself, a missing segment, or a pair already linked.
pub fn add_length_link(project: &mut Project, a: &SegmentRef, b: &SegmentRef, ease: f64) -> bool {
    if a == b || segment_length(project, a).is_none() || segment_length(project, b).is_none() {
        return false;
    }
    if project
        .links
        .iter()
        .any(|l| (l.a == *a && l.b == *b) || (l.a == *b && l.b == *a))
    {
        return false;
    }
    let before = link_lengths(project);
    project.links.push(LengthLink {
        a: a.clone(),
        b: b.clone(),
        ease: if ease.is_finite() { ease } else { 0.0 },
    });
    sync_length_links(project, &before);
    true
}

/// Removes link `index`; the segments keep their lengths.
pub fn remove_length_link(project: &mut Project, index: usize) -> bool {
    if index < project.links.len() {
        project.links.remove(index);
        true
    } else {
        false
    }
}

/// Brings every link back to its length relation after an edit. `before` holds the lengths from
/// before it: a side the edit changed leads and the other follows; when both or neither changed
/// (a new link, a new ease), `b` follows `a`. Links whose segments are gone are dropped.
pub fn sync_length_links(project: &mut Project, before: &LinkLengths) {
    let live: Vec<bool> = project
        .links
        .iter()
        .map(|l| segment_length(project, &l.a).is_some() && segment_length(project, &l.b).is_some())
        .collect();
    let mut keep = live.into_iter();
    project.links.retain(|_| keep.next().unwrap_or(false));
    for i in 0..project.links.len() {
        let link = project.links[i].clone();
        let (Some(la), Some(lb)) = (segment_length(project, &link.a), segment_length(project, &link.b))
        else {
            continue;
        };
        if (lb - (la + link.ease)).abs() <= LINK_TOLERANCE_MM {
            continue;
        }
        let was = before
            .0
            .iter()
            .find(|(a, b, _, _)| *a == link.a && *b == link.b)
            .map(|(_, _, la0, lb0)| (*la0, *lb0));
        let moved = |now: f64, then: Option<f64>| then.is_none_or(|t| (now - t).abs() > 1e-9);
        let b_led = was.is_some_and(|(la0, lb0)| moved(lb, lb0) && !moved(la, la0));
        if b_led {
            fit_segment(project, &link.a, lb - link.ease);
        } else {
            fit_segment(project, &link.b, la + link.ease);
        }
    }
}

/// Makes segment `r` `target` mm long by scaling its control polygon about its start corner (its
/// shape stays, its far corner moves). `false` for a missing or zero-length segment, or a target
/// that is not positive.
pub fn fit_segment(project: &mut Project, r: &SegmentRef, target: f64) -> bool {
    // a curve's measured length is that of its flattened polyline, whose step count depends on
    // its size, so scaling lands within a hair of the target and a few rounds close the gap
    for _ in 0..FIT_ROUNDS {
        if !scale_segment(project, r, target) {
            return false;
        }
        if segment_length(project, r).is_some_and(|l| (l - target).abs() < FIT_TOLERANCE_MM) {
            break;
        }
    }
    true
}

/// Rounds of scaling [`fit_segment`] makes at most.
const FIT_ROUNDS: usize = 4;
/// How close [`fit_segment`] brings a segment to its target, mm.
const FIT_TOLERANCE_MM: f64 = 1e-7;

/// One round of [`fit_segment`].
fn scale_segment(project: &mut Project, r: &SegmentRef, target: f64) -> bool {
    let Some(pc) = project.pieces.get_mut(&r.piece) else {
        return false;
    };
    let ol = pc.outline();
    let Some(seg) = ol.segment_index(&r.seg).map(|k| &ol.segments[k]) else {
        return false;
    };
    if seg.len <= 0.0 || target.is_nan() || target <= 0.0 {
        return false;
    }
    let scale = target / seg.len;
    let n = pc.pts.len();
    let start = pc.pts[seg.start_point].pos;
    // the points after the start corner, through the end corner (wrapping past the last point)
    let mut i = (seg.start_point + 1) % n;
    loop {
        let p = &mut pc.pts[i];
        p.pos = start + (p.pos - start) * scale;
        if i == seg.end_point {
            break;
        }
        i = (i + 1) % n;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use azam_geom::Vec2;

    /// A 100 × 200 rectangle and a 120 × 180 one, side by side.
    fn two_boxes() -> (Project, PieceId, PieceId) {
        let mut p = Project::new(None, crate::Sex::Female, 0.0);
        let rect = |x: f64, w: f64, h: f64| {
            [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(px, py)| (Vec2::new(px + x, py), false))
        };
        let a = crate::create_piece(&mut p, &rect(0.0, 100.0, 200.0)).expect("a");
        let b = crate::create_piece(&mut p, &rect(300.0, 120.0, 180.0)).expect("b");
        (p, a, b)
    }

    /// The right side (second segment) of piece `id`.
    fn side(p: &Project, id: &PieceId) -> SegmentRef {
        SegmentRef::new(id, &p.pieces[id].pts[1].id)
    }

    #[test]
    fn linking_matches_the_follower_and_editing_either_side_carries_the_other() {
        let (mut p, a, b) = two_boxes();
        let (ra, rb) = (side(&p, &a), side(&p, &b));
        assert!(add_length_link(&mut p, &ra, &rb, 5.0));
        let len = |p: &Project, r: &SegmentRef| segment_length(p, r).expect("segment");
        assert!((len(&p, &rb) - 205.0).abs() < 1e-9, "b follows a + ease");
        assert!((len(&p, &ra) - 200.0).abs() < 1e-9, "a stays");
        assert!(!add_length_link(&mut p, &rb, &ra, 0.0), "already linked");
        assert!(!add_length_link(&mut p, &ra, &ra, 0.0), "not to itself");

        // lengthen a: b follows
        let before = link_lengths(&p);
        p.pieces.get_mut(&a).expect("a").pts[2].pos.y = 260.0;
        sync_length_links(&mut p, &before);
        assert!((len(&p, &rb) - 265.0).abs() < 1e-9);

        // shorten b: a follows
        let before = link_lengths(&p);
        let rbi = p.pieces[&b].pts[2].pos;
        p.pieces.get_mut(&b).expect("b").pts[2].pos = Vec2::new(rbi.x, 165.0);
        sync_length_links(&mut p, &before);
        assert!((len(&p, &ra) - 160.0).abs() < 1e-9, "a = b - ease");

        // a deleted piece drops the link
        p.pieces.shift_remove(&a);
        let lengths = link_lengths(&p);
        sync_length_links(&mut p, &lengths);
        assert!(p.links.is_empty());
    }

    #[test]
    fn fitting_a_curve_keeps_its_shape() {
        let (mut p, a, _) = two_boxes();
        // bow the right side out with a curve point
        let pc = p.pieces.get_mut(&a).expect("a");
        pc.pts
            .insert(2, azam_geom::OutlinePoint::curve("bow", 140.0, 100.0));
        let r = side(&p, &a);
        let len0 = segment_length(&p, &r).expect("curved side");
        assert!(fit_segment(&mut p, &r, len0 * 1.25));
        assert!((segment_length(&p, &r).expect("side") - len0 * 1.25).abs() < 1e-6);
        let pts = &p.pieces[&a].pts;
        assert_eq!(pts[1].pos, Vec2::new(100.0, 0.0), "the start corner stays");
        assert!(
            (pts[2].pos - Vec2::new(150.0, 125.0)).length() < 0.5,
            "the curve point scales with it"
        );
        assert!(!fit_segment(&mut p, &r, 0.0));
    }

    #[test]
    fn links_survive_a_save_and_load() {
        let (mut p, a, b) = two_boxes();
        let (ra, rb) = (side(&p, &a), side(&p, &b));
        assert!(add_length_link(&mut p, &ra, &rb, -2.5));
        let json = serde_json::to_value(&p).expect("serialised");
        assert_eq!(json["lengthLinks"][0]["ease"], -2.5);
        assert_eq!(json["lengthLinks"][0]["a"]["p"], a.0.as_str());
        let back: Project = serde_json::from_value(json).expect("loaded");
        assert_eq!(back.links, p.links);
        let plain = serde_json::to_value(Project::new(None, crate::Sex::Female, 0.0)).expect("serialised");
        assert!(plain.get("lengthLinks").is_none(), "no links, no field");
    }
}
