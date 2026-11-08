//! Zippers in the simulation: the cloth under a zipper half's tape stretches and bends much less
//! (a woven tape with a row of teeth), and the zipper's weight hangs on the edge it is sewn to.

use azam_doc::{Piece, Project, ZIPPER_TAPE_MM, is_zipper};
use azam_geom::hypot;

use crate::numeric::clamp;
use crate::state::PieceRecord;

/// Stretch compliance multiplier under a zipper's tape (as for an interfaced piece).
pub(crate) const ZIPPER_STRETCH_FACTOR: f64 = 0.02;
/// Bending compliance multiplier under a zipper's tape.
pub(crate) const ZIPPER_BEND_FACTOR: f64 = 0.02;
/// The band a zipper stiffens reaches at least this many mesh spacings in from the edge, so a
/// coarse mesh still gets a row of triangles under the tape.
const BAND_SPACINGS: f64 = 1.2;
/// Weight of one zipper half (tape and teeth of a #5 coil zipper), g/mm.
pub(crate) const ZIPPER_HALF_G_PER_MM: f64 = 0.011;
/// How much more loosely a tape chord holds when it is shorter than its rest length: chords only
/// keep the tape from stretching, it still bends and curves freely.
pub(crate) const CHORD_COMPRESSION: f64 = 1.0e4;

/// What the zippers sewn to a piece change in its cloth.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ZipperTape {
    /// Per vertex: whether it lies under a zipper's tape.
    pub band: Vec<bool>,
    /// Per vertex: zipper weight it carries, g.
    pub weight: Vec<f64>,
    /// The vertices along each zippered segment, in outline order. The tape is held along them by
    /// long-range chords as well as by its mesh edges: a chain of stiff edges converges slower the
    /// more vertices it has, so without them a finer mesh would let the zipper's weight stretch
    /// its tape.
    pub chains: Vec<Vec<usize>>,
}

impl ZipperTape {
    /// The zippers sewn to `piece` (meshed as `record`).
    pub(crate) fn of(project: &Project, piece: &Piece, record: &PieceRecord) -> Self {
        let mesh = &record.mesh;
        let mut tape = Self {
            band: vec![false; record.count],
            weight: vec![0.0; record.count],
            chains: Vec::new(),
        };
        let nb = mesh.boundary_count();
        if nb < 2 {
            return tape;
        }
        let xy = |v: usize| (mesh.xy[2 * v], mesh.xy[2 * v + 1]);
        let reach = ZIPPER_TAPE_MM.max(mesh.spacing * BAND_SPACINGS);
        for st in project.stitches.values() {
            if st.p.as_ref() != Some(&piece.id) || !is_zipper(st) {
                continue;
            }
            let Some(k) = mesh.outline.segments.iter().position(|sg| sg.id == st.seg) else {
                continue;
            };
            // the boundary edges along the zippered segment
            let edges: Vec<(usize, usize)> = (0..nb)
                .filter(|&i| mesh.boundary_segment[i] as usize == k)
                .map(|i| (mesh.boundary[i] as usize, mesh.boundary[(i + 1) % nb] as usize))
                .collect();
            for &(a, b) in &edges {
                let half = hypot(xy(b).0 - xy(a).0, xy(b).1 - xy(a).1) * ZIPPER_HALF_G_PER_MM / 2.0;
                tape.weight[a] += half;
                tape.weight[b] += half;
            }
            tape.chains
                .extend(chains(&mesh.boundary, &mesh.boundary_segment, k));
            for (v, inside) in tape.band.iter_mut().enumerate() {
                let p = xy(v);
                *inside |= edges
                    .iter()
                    .any(|&(a, b)| distance_to_segment(p, xy(a), xy(b)) <= reach);
            }
        }
        tape
    }
}

/// The runs of boundary vertices along outline segment `k`, each in outline order and including
/// the vertex that ends its last edge (a run may wrap past the first boundary vertex).
fn chains(boundary: &[u32], boundary_segment: &[u32], k: usize) -> Vec<Vec<usize>> {
    let nb = boundary.len();
    let on = |i: usize| boundary_segment[i] as usize == k;
    let mut out = Vec::new();
    for start in (0..nb).filter(|&i| on(i) && !on((i + nb - 1) % nb)) {
        let mut chain = vec![boundary[start] as usize];
        let mut i = start;
        while on(i) && chain.len() <= nb {
            i = (i + 1) % nb;
            chain.push(boundary[i] as usize);
        }
        out.push(chain);
    }
    if out.is_empty() && (0..nb).all(on) {
        // the whole outline is one segment
        out.push((0..=nb).map(|i| boundary[i % nb] as usize).collect());
    }
    out
}

/// Distance from `p` to the segment `a`–`b`, mm.
fn distance_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        clamp(((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2, 0.0, 1.0)
    } else {
        0.0
    };
    hypot(p.0 - a.0 - t * dx, p.1 - a.1 - t * dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_follow_the_segment_and_wrap_past_the_start() {
        // boundary vertices 10..16, segments: 1 1 0 0 0 1
        let boundary = [10, 11, 12, 13, 14, 15];
        let seg = [1, 1, 0, 0, 0, 1];
        assert_eq!(chains(&boundary, &seg, 0), vec![vec![12, 13, 14, 15]]);
        assert_eq!(chains(&boundary, &seg, 1), vec![vec![15, 10, 11, 12]]);
        assert!(chains(&boundary, &seg, 2).is_empty());
    }

    #[test]
    fn distance_to_a_segment_clamps_to_its_ends() {
        assert!((distance_to_segment((5.0, 3.0), (0.0, 0.0), (10.0, 0.0)) - 3.0).abs() < 1e-12);
        assert!((distance_to_segment((13.0, 4.0), (0.0, 0.0), (10.0, 0.0)) - 5.0).abs() < 1e-12);
    }
}
