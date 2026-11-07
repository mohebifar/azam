//! Signed distance grid of the body: what the cloth solver queries for collisions.

use glam::DVec3;

/// Distance reported for points outside the grid, in millimetres.
pub const FAR_DISTANCE_MM: f64 = 1e3;
/// Grid values are clamped to `±` this distance, in millimetres.
pub const DISTANCE_CLAMP_MM: f64 = 80.0;

/// A regular grid of signed distances to the body surface (negative inside), in millimetres.
///
/// Value `(i, j, k)` sits at `origin + cell * (i, j, k)` and is stored at
/// `i + dims[0] * (j + dims[1] * k)`.
#[derive(Clone, Debug, PartialEq)]
pub struct SdfGrid {
    /// Position of value `(0, 0, 0)`.
    pub origin: DVec3,
    /// Spacing between values along every axis.
    pub cell: f64,
    /// Number of values along x, y and z.
    pub dims: [usize; 3],
    /// The distances, x fastest.
    pub data: Vec<f32>,
}

/// The layout of a grid: where it starts, its cell size and how many values it has per axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridFrame {
    /// Position of value `(0, 0, 0)`.
    pub origin: DVec3,
    /// Spacing between values along every axis.
    pub cell: f64,
    /// Number of values along x, y and z.
    pub dims: [usize; 3],
}

impl GridFrame {
    /// The frame covering the box `lo..hi` with cells of size `cell`.
    #[must_use]
    pub fn covering(lo: DVec3, hi: DVec3, cell: f64) -> Self {
        let count = |a: f64, b: f64| ((b - a) / cell).ceil().max(1.0) as usize + 1;
        Self {
            origin: lo,
            cell,
            dims: [count(lo.x, hi.x), count(lo.y, hi.y), count(lo.z, hi.z)],
        }
    }

    /// Number of values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.dims[0] * self.dims[1] * self.dims[2]
    }

    /// Whether the frame has no values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The far corner (position of the last value).
    #[must_use]
    pub fn max(&self) -> DVec3 {
        self.origin
            + DVec3::new(
                (self.dims[0] - 1) as f64,
                (self.dims[1] - 1) as f64,
                (self.dims[2] - 1) as f64,
            ) * self.cell
    }
}

/// Distance and gradient at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdfSample {
    /// Signed distance (negative inside the body).
    pub distance: f64,
    /// Gradient of the trilinear interpolant; points away from the body, roughly unit length.
    pub gradient: DVec3,
}

impl SdfGrid {
    /// The grid's layout.
    #[must_use]
    pub fn frame(&self) -> GridFrame {
        GridFrame {
            origin: self.origin,
            cell: self.cell,
            dims: self.dims,
        }
    }

    /// Index of value `(i, j, k)` in [`SdfGrid::data`].
    #[must_use]
    pub fn index(&self, i: usize, j: usize, k: usize) -> usize {
        i + self.dims[0] * (j + self.dims[1] * k)
    }

    /// Position of value `(i, j, k)`.
    #[must_use]
    pub fn point(&self, i: usize, j: usize, k: usize) -> DVec3 {
        DVec3::new(
            self.origin.x + i as f64 * self.cell,
            self.origin.y + j as f64 * self.cell,
            self.origin.z + k as f64 * self.cell,
        )
    }

    /// Cell coordinates of `p` and the lower corner index, or `None` outside the grid.
    fn locate(&self, p: DVec3) -> Option<(usize, DVec3)> {
        let fx = (p.x - self.origin.x) / self.cell;
        let fy = (p.y - self.origin.y) / self.cell;
        let fz = (p.z - self.origin.z) / self.cell;
        let [nx, ny, nz] = self.dims;
        let inside = fx >= 0.0
            && fy >= 0.0
            && fz >= 0.0
            && fx < nx as f64 - 1.0
            && fy < ny as f64 - 1.0
            && fz < nz as f64 - 1.0;
        if !inside {
            return None;
        }
        let (i, j, k) = (fx as usize, fy as usize, fz as usize);
        let frac = DVec3::new(fx - i as f64, fy - j as f64, fz - k as f64);
        Some((self.index(i, j, k), frac))
    }

    /// The eight corner values of the cell whose lower corner is at `o`.
    fn corners(&self, o: usize) -> [f64; 8] {
        let nx = self.dims[0];
        let nxy = nx * self.dims[1];
        let d = |q: usize| f64::from(self.data[q]);
        [
            d(o),
            d(o + 1),
            d(o + nx),
            d(o + nx + 1),
            d(o + nxy),
            d(o + nxy + 1),
            d(o + nxy + nx),
            d(o + nxy + nx + 1),
        ]
    }

    /// Trilinearly interpolated distance at `p`, or [`FAR_DISTANCE_MM`] outside the grid.
    #[must_use]
    pub fn distance(&self, p: DVec3) -> f64 {
        self.sample(p).map_or(FAR_DISTANCE_MM, |s| s.distance)
    }

    /// Trilinearly interpolated distance and its gradient at `p`, or `None` outside the grid.
    #[must_use]
    pub fn sample(&self, p: DVec3) -> Option<SdfSample> {
        let (o, f) = self.locate(p)?;
        let (u, v, w) = (f.x, f.y, f.z);
        let [c000, c100, c010, c110, c001, c101, c011, c111] = self.corners(o);
        let x00 = c000 + (c100 - c000) * u;
        let x10 = c010 + (c110 - c010) * u;
        let x01 = c001 + (c101 - c001) * u;
        let x11 = c011 + (c111 - c011) * u;
        let y0 = x00 + (x10 - x00) * v;
        let y1 = x01 + (x11 - x01) * v;
        let distance = y0 + (y1 - y0) * w;
        let dx0 = (c100 - c000) + ((c110 - c010) - (c100 - c000)) * v;
        let dx1 = (c101 - c001) + ((c111 - c011) - (c101 - c001)) * v;
        let gradient = DVec3::new(
            (dx0 + (dx1 - dx0) * w) / self.cell,
            ((x10 - x00) + ((x11 - x01) - (x10 - x00)) * w) / self.cell,
            (y1 - y0) / self.cell,
        );
        Some(SdfSample { distance, gradient })
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn plane_grid() -> SdfGrid {
        // distance = y - 5 on a 4 x 11 x 3 grid with unit cells
        let dims = [4, 11, 3];
        let mut data = Vec::new();
        for _k in 0..dims[2] {
            for j in 0..dims[1] {
                for _i in 0..dims[0] {
                    data.push(j as f32 - 5.0);
                }
            }
        }
        SdfGrid {
            origin: DVec3::ZERO,
            cell: 1.0,
            dims,
            data,
        }
    }

    #[test]
    fn samples_a_plane_exactly() {
        let g = plane_grid();
        let s = g.sample(DVec3::new(1.5, 7.25, 0.5)).unwrap();
        assert!((s.distance - 2.25).abs() < 1e-6);
        assert!((s.gradient - DVec3::Y).length() < 1e-6);
        assert_eq!(g.distance(DVec3::new(3.0, 1.0, 1.0)), FAR_DISTANCE_MM);
        assert!(g.sample(DVec3::new(-0.1, 1.0, 1.0)).is_none());
        assert_eq!(g.point(1, 2, 2), DVec3::new(1.0, 2.0, 2.0));
    }
}
