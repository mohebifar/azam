//! Signed distance grids and vertex normals of triangle meshes.

use glam::DVec3;

use crate::grid::{DISTANCE_CLAMP_MM, SdfGrid};
use crate::math::hypot3;

/// Exact distances are computed within this many cells of each triangle.
pub(crate) const BAND_CELLS: f64 = 1.9;
/// Cells closer than this many cell sizes to the surface are walls for the outside flood fill.
pub(crate) const WALL_CELLS: f64 = 0.87;
/// Distance of cells not reached yet.
pub(crate) const UNSET_DISTANCE: f32 = 1e9;
/// The grid always reaches at least this far below the floor, in millimetres.
pub(crate) const FLOOR_MARGIN_MM: f64 = 10.0;

/// Closest point on triangle `(a, b, c)` to `p`, as barycentric weights `(u, v)` of `b` and `c`.
pub(crate) fn closest_on_triangle(
    p: DVec3,
    a: DVec3,
    ab: DVec3,
    ac: DVec3,
    b: DVec3,
    c: DVec3,
) -> (f64, f64) {
    let dot = |x: DVec3, y: DVec3| x.x * y.x + x.y * y.y + x.z * y.z;
    let ap = p - a;
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (0.0, 0.0);
    }
    let bp = p - b;
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (1.0, 0.0);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return (d1 / (d1 - d3), 0.0);
    }
    let cp = p - c;
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (0.0, 1.0);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return (0.0, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (1.0 - w, w);
    }
    let den = 1.0 / (va + vb + vc);
    (vb * den, vc * den)
}

fn vec3(buf: &[f32], i: usize) -> DVec3 {
    DVec3::new(
        f64::from(buf[i * 3]),
        f64::from(buf[i * 3 + 1]),
        f64::from(buf[i * 3 + 2]),
    )
}

/// Signed distance grid of a closed triangle mesh.
///
/// `positions` and `normals` hold `x, y, z` per vertex; `triangles` three indices each. The grid
/// covers the mesh plus `pad` on every side (and at least to 10 mm below the floor) with cells of
/// size `cell`. Distances are exact within a narrow band around the triangles, where the sign comes
/// from the interpolated vertex normal; elsewhere the sign comes from a flood fill from the grid
/// border and the magnitude from chamfer propagation. Values are clamped to
/// [`DISTANCE_CLAMP_MM`].
///
/// # Panics
///
/// Panics if a triangle index is out of range of `positions` or `normals`.
#[must_use]
pub fn mesh_sdf(positions: &[f32], triangles: &[u32], cell: f64, pad: f64, normals: &[f32]) -> SdfGrid {
    let g = cell;
    let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    for &t in triangles {
        let p = vec3(positions, t as usize);
        lo = DVec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = DVec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    let x0 = lo.x - pad;
    let y0 = (lo.y - pad).min(-FLOOR_MARGIN_MM);
    let z0 = lo.z - pad;
    let (x1, y1, z1) = (hi.x + pad, hi.y + pad, hi.z + pad);
    let count = |a: f64, b: f64| ((b - a) / g).ceil() as usize + 1;
    let (nx, ny, nz) = (count(x0, x1), count(y0, y1), count(z0, z1));
    let nxy = nx * ny;
    let total = nxy * nz;
    let mut dist = vec![UNSET_DISTANCE; total];
    let mut sign = vec![0_i8; total];
    let band = g * BAND_CELLS;
    let first = |v: f64, o: f64| ((v - band - o) / g).floor().max(0.0) as usize;
    let last = |v: f64, o: f64, n: usize| (((v + band - o) / g).ceil().max(0.0) as usize).min(n - 1);
    for tri in triangles.chunks_exact(3) {
        let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let (a, b, c) = (vec3(positions, ia), vec3(positions, ib), vec3(positions, ic));
        let (i0, i1) = (
            first(a.x.min(b.x).min(c.x), x0),
            last(a.x.max(b.x).max(c.x), x0, nx),
        );
        let (j0, j1) = (
            first(a.y.min(b.y).min(c.y), y0),
            last(a.y.max(b.y).max(c.y), y0, ny),
        );
        let (k0, k1) = (
            first(a.z.min(b.z).min(c.z), z0),
            last(a.z.max(b.z).max(c.z), z0, nz),
        );
        let (ab, ac) = (b - a, c - a);
        let (na, nb, nc) = (vec3(normals, ia), vec3(normals, ib), vec3(normals, ic));
        for k in k0..=k1 {
            let pz = z0 + k as f64 * g;
            for j in j0..=j1 {
                let py = y0 + j as f64 * g;
                for i in i0..=i1 {
                    let p = DVec3::new(x0 + i as f64 * g, py, pz);
                    let (u, v) = closest_on_triangle(p, a, ab, ac, b, c);
                    let q = DVec3::new(
                        a.x + ab.x * u + ac.x * v,
                        a.y + ab.y * u + ac.y * v,
                        a.z + ab.z * u + ac.z * v,
                    );
                    let dd = p - q;
                    let d = (dd.x * dd.x + dd.y * dd.y + dd.z * dd.z).sqrt();
                    let o = i + nx * j + nxy * k;
                    if d < f64::from(dist[o]) {
                        dist[o] = d as f32;
                        let w0 = 1.0 - u - v;
                        let n = DVec3::new(
                            w0 * na.x + u * nb.x + v * nc.x,
                            w0 * na.y + u * nb.y + v * nc.y,
                            w0 * na.z + u * nb.z + v * nc.z,
                        );
                        sign[o] = if dd.x * n.x + dd.y * n.y + dd.z * n.z >= 0.0 {
                            1
                        } else {
                            -1
                        };
                    }
                }
            }
        }
    }
    let outside = flood_outside(&dist, [nx, ny, nz], g * WALL_CELLS);
    chamfer(&mut dist, [nx, ny, nz], g);
    let wall = g * WALL_CELLS;
    let data = (0..total)
        .map(|o| {
            let d = f64::from(dist[o]);
            let out_sign = if outside[o] { 1.0 } else { -1.0 };
            let s = if d < wall && sign[o] != 0 {
                f64::from(sign[o])
            } else {
                out_sign
            };
            (s * d.min(DISTANCE_CLAMP_MM)) as f32
        })
        .collect();
    SdfGrid {
        origin: DVec3::new(x0, y0, z0),
        cell: g,
        dims: [nx, ny, nz],
        data,
    }
}

/// Cells reachable from the grid border through cells at least `wall` from the surface
/// (6-connected).
pub(crate) fn flood_outside(dist: &[f32], dims: [usize; 3], wall: f64) -> Vec<bool> {
    let [nx, ny, nz] = dims;
    let nxy = nx * ny;
    let mut out = vec![false; dist.len()];
    let mut stack = Vec::new();
    let push = |o: usize, out: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if !out[o] && f64::from(dist[o]) >= wall {
            out[o] = true;
            stack.push(o);
        }
    };
    for k in 0..nz {
        for j in 0..ny {
            push(nxy * k + nx * j, &mut out, &mut stack);
            push(nxy * k + nx * j + nx - 1, &mut out, &mut stack);
        }
    }
    for k in 0..nz {
        for i in 0..nx {
            push(nxy * k + i, &mut out, &mut stack);
            push(nxy * k + nx * (ny - 1) + i, &mut out, &mut stack);
        }
    }
    for j in 0..ny {
        for i in 0..nx {
            push(nx * j + i, &mut out, &mut stack);
            push(nxy * (nz - 1) + nx * j + i, &mut out, &mut stack);
        }
    }
    while let Some(o) = stack.pop() {
        let k = o / nxy;
        let r = o - k * nxy;
        let j = r / nx;
        let i = r - j * nx;
        if i > 0 {
            push(o - 1, &mut out, &mut stack);
        }
        if i < nx - 1 {
            push(o + 1, &mut out, &mut stack);
        }
        if j > 0 {
            push(o - nx, &mut out, &mut stack);
        }
        if j < ny - 1 {
            push(o + nx, &mut out, &mut stack);
        }
        if k > 0 {
            push(o - nxy, &mut out, &mut stack);
        }
        if k < nz - 1 {
            push(o + nxy, &mut out, &mut stack);
        }
    }
    out
}

/// A neighbour offset and its step length.
type Neighbour = ([isize; 3], f64);

#[allow(clippy::cast_possible_wrap)] // grid sizes fit in isize
/// Propagates distances with a forward and a backward 26-neighbour chamfer pass.
pub(crate) fn chamfer(dist: &mut [f32], dims: [usize; 3], g: f64) {
    let [nx, ny, nz] = dims;
    let steps = [g, g * std::f64::consts::SQRT_2, g * 3.0_f64.sqrt()];
    let mut all: Vec<Neighbour> = Vec::with_capacity(26);
    for dk in -1..=1_isize {
        for dj in -1..=1_isize {
            for di in -1..=1_isize {
                let c = di.unsigned_abs() + dj.unsigned_abs() + dk.unsigned_abs();
                if c > 0 {
                    all.push(([di, dj, dk], steps[c - 1]));
                }
            }
        }
    }
    let is_forward = |n: &[isize; 3]| n[2] < 0 || (n[2] == 0 && (n[1] < 0 || (n[1] == 0 && n[0] < 0)));
    let forward: Vec<Neighbour> = all.iter().copied().filter(|(n, _)| is_forward(n)).collect();
    let backward: Vec<Neighbour> = all.iter().copied().filter(|(n, _)| !is_forward(n)).collect();
    let dims_i = [nx as isize, ny as isize, nz as isize];
    let mut relax = |i: usize, j: usize, k: usize, list: &[Neighbour]| {
        let o = i + nx * j + nx * ny * k;
        let mut best = f64::from(dist[o]);
        for (n, step) in list {
            let (ii, jj, kk) = (i as isize + n[0], j as isize + n[1], k as isize + n[2]);
            if ii < 0 || jj < 0 || kk < 0 || ii >= dims_i[0] || jj >= dims_i[1] || kk >= dims_i[2] {
                continue;
            }
            let q = ii as usize + nx * jj as usize + nx * ny * kk as usize;
            let c = f64::from(dist[q]) + step;
            if c < best {
                best = c;
            }
        }
        dist[o] = best as f32;
    };
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                relax(i, j, k, &forward);
            }
        }
    }
    for k in (0..nz).rev() {
        for j in (0..ny).rev() {
            for i in (0..nx).rev() {
                relax(i, j, k, &backward);
            }
        }
    }
}

/// Area-weighted unit vertex normals (`x, y, z` per vertex) of a triangle mesh.
///
/// # Panics
///
/// Panics if a triangle index is not below `vertex_count` or out of range of `positions`.
#[must_use]
pub fn vertex_normals(positions: &[f32], triangles: &[u32], vertex_count: usize) -> Vec<f32> {
    let mut n = vec![0.0_f32; vertex_count * 3];
    for tri in triangles.chunks_exact(3) {
        let (a, b, c) = (tri[0] as usize * 3, tri[1] as usize * 3, tri[2] as usize * 3);
        let p = |k: usize| f64::from(positions[k]);
        let (e1x, e1y, e1z) = (p(b) - p(a), p(b + 1) - p(a + 1), p(b + 2) - p(a + 2));
        let (e2x, e2y, e2z) = (p(c) - p(a), p(c + 1) - p(a + 1), p(c + 2) - p(a + 2));
        let f = [
            e1y * e2z - e1z * e2y,
            e1z * e2x - e1x * e2z,
            e1x * e2y - e1y * e2x,
        ];
        for k in [a, b, c] {
            for (axis, fa) in f.iter().enumerate() {
                n[k + axis] = (f64::from(n[k + axis]) + fa) as f32;
            }
        }
    }
    for v in n.chunks_exact_mut(3) {
        let (x, y, z) = (f64::from(v[0]), f64::from(v[1]), f64::from(v[2]));
        let l = hypot3(x, y, z);
        let l = if l > 0.0 { l } else { 1.0 };
        v[0] = (x / l) as f32;
        v[1] = (y / l) as f32;
        v[2] = (z / l) as f32;
    }
    n
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    /// A closed octahedron of radius `r` centred at `c`.
    fn octahedron(r: f32, c: [f32; 3]) -> (Vec<f32>, Vec<u32>) {
        let v = [
            [r, 0.0, 0.0],
            [-r, 0.0, 0.0],
            [0.0, r, 0.0],
            [0.0, -r, 0.0],
            [0.0, 0.0, r],
            [0.0, 0.0, -r],
        ];
        let pos = v
            .iter()
            .flat_map(|p| [p[0] + c[0], p[1] + c[1], p[2] + c[2]])
            .collect();
        let tris = vec![
            0, 2, 4, 2, 1, 4, 1, 3, 4, 3, 0, 4, 2, 0, 5, 1, 2, 5, 3, 1, 5, 0, 3, 5,
        ];
        (pos, tris)
    }

    #[test]
    fn normals_point_outwards() {
        let (pos, tris) = octahedron(100.0, [0.0, 0.0, 0.0]);
        let n = vertex_normals(&pos, &tris, 6);
        assert!((n[0] - 1.0).abs() < 1e-6 && (n[7] - 1.0).abs() < 1e-6 && (n[14] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn octahedron_grid_is_negative_inside_and_positive_outside() {
        let (pos, tris) = octahedron(200.0, [0.0, 300.0, 0.0]);
        let nrm = vertex_normals(&pos, &tris, 6);
        let g = mesh_sdf(&pos, &tris, 10.0, 50.0, &nrm);
        let inside = g.distance(DVec3::new(0.0, 300.0, 0.0));
        assert_eq!(inside, -DISTANCE_CLAMP_MM);
        let near = g.distance(DVec3::new(0.0, 300.0, 180.0));
        assert!(near < 0.0 && near > -20.0, "{near}");
        // (100, 100, 100) from the centre is 100 / sqrt(3) outside the face x + y + z = 200
        let out = g.distance(DVec3::new(100.0, 400.0, 100.0));
        assert!((out - 100.0 / 3.0_f64.sqrt()).abs() < 2.0, "{out}");
        assert!(g.origin.y <= -10.0);
    }
}
