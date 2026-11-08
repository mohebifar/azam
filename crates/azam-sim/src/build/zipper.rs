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

/// What the zippers sewn to a piece change in its cloth.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ZipperTape {
    /// Per vertex: whether it lies under a zipper's tape.
    pub band: Vec<bool>,
    /// Per vertex: zipper weight it carries, g.
    pub weight: Vec<f64>,
}

impl ZipperTape {
    /// The zippers sewn to `piece` (meshed as `record`).
    pub(crate) fn of(project: &Project, piece: &Piece, record: &PieceRecord) -> Self {
        let mesh = &record.mesh;
        let mut tape = Self {
            band: vec![false; record.count],
            weight: vec![0.0; record.count],
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
    fn distance_to_a_segment_clamps_to_its_ends() {
        assert!((distance_to_segment((5.0, 3.0), (0.0, 0.0), (10.0, 0.0)) - 3.0).abs() < 1e-12);
        assert!((distance_to_segment((13.0, 4.0), (0.0, 0.0), (10.0, 0.0)) - 5.0).abs() < 1e-12);
    }
}
