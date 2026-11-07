//! Velocity of the body surface while the avatar moves, for moving-collider friction.

use glam::DVec3;

use crate::grid::GridFrame;

/// Passes that spread velocities into empty cells next to filled ones.
const DILATE_PASSES: usize = 3;

/// A coarse grid of body surface velocities (mm/s): what a point touching the body moves with.
/// Zero away from the body and outside the grid.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyVelocity {
    /// Layout of the grid.
    pub frame: GridFrame,
    /// Velocity `x, y, z` per value, in mm/s, x fastest.
    pub data: Vec<f32>,
}

impl BodyVelocity {
    /// Splats point velocities (position, velocity in mm/s) onto `frame` with trilinear weights,
    /// then spreads them a few cells into the empty neighbourhood.
    #[must_use]
    pub fn from_points(frame: GridFrame, points: impl IntoIterator<Item = (DVec3, DVec3)>) -> Self {
        let [nx, ny, nz] = frame.dims;
        let n = frame.len();
        // only the cells near the points are touched; the rest stay zero
        let mut sum: Vec<DVec3> = Vec::new();
        let mut weight: Vec<f64> = Vec::new();
        let mut slot = vec![u32::MAX; n];
        let mut cells: Vec<usize> = Vec::new();
        for (p, v) in points {
            let f = (p - frame.origin) / frame.cell;
            if f.x < 0.0 || f.y < 0.0 || f.z < 0.0 {
                continue;
            }
            let (i, j, k) = (f.x as usize, f.y as usize, f.z as usize);
            if i + 1 >= nx || j + 1 >= ny || k + 1 >= nz {
                continue;
            }
            let (u, w, s) = (f.x - i as f64, f.y - j as f64, f.z - k as f64);
            for c in 0..8 {
                let (di, dj, dk) = (c & 1, (c >> 1) & 1, (c >> 2) & 1);
                let wt = if di == 1 { u } else { 1.0 - u }
                    * if dj == 1 { w } else { 1.0 - w }
                    * if dk == 1 { s } else { 1.0 - s };
                let o = (i + di) + nx * ((j + dj) + ny * (k + dk));
                if slot[o] == u32::MAX {
                    slot[o] = cells.len() as u32;
                    cells.push(o);
                    sum.push(DVec3::ZERO);
                    weight.push(0.0);
                }
                let at = slot[o] as usize;
                sum[at] += v * wt;
                weight[at] += wt;
            }
        }
        let mut data = vec![0.0_f32; n * 3];
        let mut filled = vec![false; n];
        let mut frontier = Vec::with_capacity(cells.len());
        for (at, &o) in cells.iter().enumerate() {
            if weight[at] > 1e-6 {
                let v = sum[at] / weight[at];
                data[o * 3..o * 3 + 3].copy_from_slice(&v.to_array().map(|c| c as f32));
                filled[o] = true;
                frontier.push(o);
            }
        }
        let nxy = nx * ny;
        let neighbours = |o: usize| {
            let (k, r) = (o / nxy, o % nxy);
            let (j, i) = (r / nx, r % nx);
            [
                (i > 0).then(|| o - 1),
                (i + 1 < nx).then_some(o + 1),
                (j > 0).then(|| o - nx),
                (j + 1 < ny).then_some(o + nx),
                (k > 0).then(|| o - nxy),
                (k + 1 < nz).then_some(o + nxy),
            ]
        };
        // each pass fills the empty cells next to filled ones with their filled neighbours' mean
        let mut seen = vec![0_u8; n];
        for pass in 1..=DILATE_PASSES as u8 {
            let mut grown: Vec<(usize, [f32; 3])> = Vec::new();
            for &o in &frontier {
                for q in neighbours(o).into_iter().flatten() {
                    if filled[q] || seen[q] == pass {
                        continue;
                    }
                    seen[q] = pass;
                    let (mut acc, mut cnt) = (DVec3::ZERO, 0.0);
                    for r in neighbours(q).into_iter().flatten().filter(|&r| filled[r]) {
                        acc += DVec3::new(
                            f64::from(data[r * 3]),
                            f64::from(data[r * 3 + 1]),
                            f64::from(data[r * 3 + 2]),
                        );
                        cnt += 1.0;
                    }
                    grown.push((q, (acc / cnt).to_array().map(|c| c as f32)));
                }
            }
            frontier.clear();
            for (q, v) in grown {
                data[q * 3..q * 3 + 3].copy_from_slice(&v);
                filled[q] = true;
                frontier.push(q);
            }
        }
        Self { frame, data }
    }

    /// Trilinearly interpolated velocity at `p` (zero outside the grid).
    #[must_use]
    pub fn sample(&self, p: DVec3) -> DVec3 {
        let f = (p - self.frame.origin) / self.frame.cell;
        let [nx, ny, nz] = self.frame.dims;
        if f.x < 0.0
            || f.y < 0.0
            || f.z < 0.0
            || f.x >= (nx - 1) as f64
            || f.y >= (ny - 1) as f64
            || f.z >= (nz - 1) as f64
        {
            return DVec3::ZERO;
        }
        let (i, j, k) = (f.x as usize, f.y as usize, f.z as usize);
        let (u, w, s) = (f.x - i as f64, f.y - j as f64, f.z - k as f64);
        let mut out = DVec3::ZERO;
        for c in 0..8 {
            let (di, dj, dk) = (c & 1, (c >> 1) & 1, (c >> 2) & 1);
            let wt = if di == 1 { u } else { 1.0 - u }
                * if dj == 1 { w } else { 1.0 - w }
                * if dk == 1 { s } else { 1.0 - s };
            let o = ((i + di) + nx * ((j + dj) + ny * (k + dk))) * 3;
            out += DVec3::new(
                f64::from(self.data[o]),
                f64::from(self.data[o + 1]),
                f64::from(self.data[o + 2]),
            ) * wt;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uniform_motion_is_sampled_back_near_the_points() {
        let frame = GridFrame::covering(DVec3::ZERO, DVec3::splat(200.0), 20.0);
        let v = DVec3::new(10.0, -5.0, 2.0);
        let pts = (0..50).map(|i| (DVec3::new(100.0, 60.0 + f64::from(i), 100.0), v));
        let field = BodyVelocity::from_points(frame, pts);
        assert!((field.sample(DVec3::new(100.0, 80.0, 100.0)) - v).length() < 1e-6);
        // spread a few cells away
        assert!((field.sample(DVec3::new(140.0, 80.0, 100.0)) - v).length() < 1e-6);
        assert_eq!(field.sample(DVec3::new(-1.0, 0.0, 0.0)), DVec3::ZERO);
        assert_eq!(field.sample(DVec3::new(10.0, 190.0, 10.0)), DVec3::ZERO);
    }
}
