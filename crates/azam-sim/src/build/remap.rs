//! Initial positions: the previous drape for unchanged pieces, the previous drape mapped through
//! the pattern for edited pieces, and the arrangement around the avatar for new ones.

use azam_avatar::{Avatar, arrange_positions};
use azam_doc::{PieceId, rigid_map};
use azam_geom::{Vec2, bbox};

use crate::state::{PieceRecord, Simulation};

/// Places record `r`'s particles at its arrangement around the avatar.
pub(crate) fn arrange(avatar: &Avatar, r: &PieceRecord, x: &mut [f32]) {
    let Some(bounds) = bbox(r.mesh.outline.poly.iter().copied()) else {
        return;
    };
    let out = &mut x[r.offset * 3..(r.offset + r.count) * 3];
    arrange_positions(avatar, &r.arrangement, &bounds, &r.mesh.xy, out);
}

/// Maps the previous drape of an edited piece onto its new mesh: every new vertex is found in the
/// previous mesh (after undoing any rigid move of the pattern) and takes the barycentric blend of
/// the previous positions. Vertices outside every previous triangle keep their current position.
fn remap(r: &PieceRecord, prev: &Simulation, pr: &PieceRecord, x: &mut [f32]) {
    let rigid = rigid_map(&pr.graded, &r.graded);
    let px = &prev.particles.x;
    for i in 0..r.count {
        let q = Vec2::new(r.mesh.xy[2 * i], r.mesh.xy[2 * i + 1]);
        let q = rigid.as_ref().map_or(q, |m| m.apply(q));
        let Some(hit) = pr.mesh.locate(q) else { continue };
        let [l1, l2, l3] = hit.barycentric;
        let [a, b, c] = hit.vertices.map(|v| (pr.offset + v as usize) * 3);
        let k = (r.offset + i) * 3;
        for d in 0..3 {
            x[k + d] =
                (l1 * f64::from(px[a + d]) + l2 * f64::from(px[b + d]) + l3 * f64::from(px[c + d])) as f32;
        }
    }
}

/// Fills `x` with the initial positions of every record.
pub(crate) fn initial_positions(
    avatar: &Avatar,
    records: &[PieceRecord],
    previous: Option<&Simulation>,
    rearrange: bool,
    rearrange_ids: &[PieceId],
    x: &mut [f32],
) {
    for r in records {
        let pr = previous.and_then(|p| p.records.iter().find(|q| q.id == r.id).map(|q| (p, q)));
        match pr {
            Some((prev, pr)) if pr.key == r.key => {
                let src = &prev.particles.x[pr.offset * 3..(pr.offset + pr.count) * 3];
                x[r.offset * 3..(r.offset + r.count) * 3].copy_from_slice(src);
            }
            Some((prev, pr)) if !rearrange => remap(r, prev, pr, x),
            _ => arrange(avatar, r, x),
        }
    }
    for r in records.iter().filter(|r| rearrange_ids.contains(&r.id)) {
        arrange(avatar, r, x);
    }
}
