//! What the renderer and pickers read: vertex normals and per-piece views of the particle state.

use crate::numeric::hypot3;
use crate::state::{PieceRecord, Simulation};

/// One piece's current geometry, ready to draw.
#[derive(Clone, Copy, Debug)]
pub struct PieceView<'a> {
    /// The piece's record.
    pub record: &'a PieceRecord,
    /// Positions, `x, y, z` per vertex, mm.
    pub positions: &'a [f32],
    /// Unit normals, `x, y, z` per vertex (as of the last [`Simulation::compute_normals`]).
    pub normals: &'a [f32],
    /// Triangles as vertex index triples, local to the piece.
    pub triangles: &'a [u32],
}

impl Simulation {
    /// Recomputes the area-weighted vertex normals into [`Simulation::normals`] and returns them.
    pub fn compute_normals(&mut self) -> &[f32] {
        let nrm = &mut self.normals;
        let x = &self.particles.x;
        nrm.fill(0.0);
        for r in &self.records {
            for t in r.mesh.tris.chunks_exact(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|v| (r.offset + v as usize) * 3);
                let e = |i: usize, j: usize| f64::from(x[i]) - f64::from(x[j]);
                let (e1x, e1y, e1z) = (e(b, a), e(b + 1, a + 1), e(b + 2, a + 2));
                let (e2x, e2y, e2z) = (e(c, a), e(c + 1, a + 1), e(c + 2, a + 2));
                let n = [
                    e1y * e2z - e1z * e2y,
                    e1z * e2x - e1x * e2z,
                    e1x * e2y - e1y * e2x,
                ];
                for k in [a, b, c] {
                    for d in 0..3 {
                        nrm[k + d] = (f64::from(nrm[k + d]) + n[d]) as f32;
                    }
                }
            }
        }
        for k in (0..nrm.len()).step_by(3) {
            let l = hypot3(f64::from(nrm[k]), f64::from(nrm[k + 1]), f64::from(nrm[k + 2]));
            let l = if l == 0.0 || l.is_nan() { 1.0 } else { l };
            for d in 0..3 {
                nrm[k + d] = (f64::from(nrm[k + d]) / l) as f32;
            }
        }
        &self.normals
    }

    /// The current geometry of piece record `ri`.
    #[must_use]
    pub fn piece_view(&self, ri: usize) -> Option<PieceView<'_>> {
        let record = self.records.get(ri)?;
        let range = record.offset * 3..(record.offset + record.count) * 3;
        Some(PieceView {
            record,
            positions: &self.particles.x[range.clone()],
            normals: &self.normals[range],
            triangles: &record.mesh.tris,
        })
    }

    /// The current geometry of every piece, in simulation order.
    pub fn piece_views(&self) -> impl Iterator<Item = PieceView<'_>> {
        (0..self.records.len()).filter_map(|ri| self.piece_view(ri))
    }
}
