//! A fast signed distance grid of a triangle mesh for re-posing the body every frame.
//!
//! [`crate::mesh_sdf`] propagates distances over the whole grid with a sequential flood fill and
//! chamfer, which takes a few hundred milliseconds for the body. Re-posing needs a grid per frame,
//! so this variant works in three parallel passes:
//!
//! 1. exact distances in a narrow band around the triangles (1.3 cells; sign from the
//!    interpolated vertex normal), in parallel z-slabs — everything the cloth touches;
//! 2. unsigned distances on a four times coarser grid: exact band values where a coarse value
//!    sits on a band cell, distances to the mesh vertices nearby, then a few parallel relaxation
//!    passes out to [`DISTANCE_CLAMP_MM`];
//! 3. every cell outside the band takes the coarse distance (at least the band width) and the sign
//!    of the band cells bounding its run along x (the grid border counts as outside), checked
//!    along y where the two ends disagree. Gaps narrower than twice the band are closed, much like
//!    the flood fill's walls close gaps narrower than a cell.

use glam::{DVec3, Vec3A};
use rayon::prelude::*;

use crate::grid::{DISTANCE_CLAMP_MM, GridFrame, SdfGrid};

/// The coarse grid's cells are this many fine cells.
const COARSE_FACTOR: usize = 4;
/// Exact distances on the fine grid reach this many cells (about 10 mm at the body's 7.5 mm
/// cells: everything the cloth touches).
const FINE_BAND_CELLS: f64 = 1.3;
/// Vertex distances on the coarse grid reach this many coarse cells.
const COARSE_REACH_CELLS: f64 = 1.0;
/// Relaxation passes that carry coarse distances outwards (each moves them at least a cell).
const COARSE_PASSES: usize = 2;
/// Z layers per parallel slab of the narrow band.
const SLAB_LAYERS: usize = 2;
/// Marks a cell the narrow band did not reach (band values are never exactly zero).
const UNSET: f32 = 0.0;

/// Closest point on the triangle `(0, ab, ac)` to `ap`, as barycentric weights of `b` and `c`
/// (single-precision twin of the reference grid's routine).
fn closest_on_triangle_f32(ap: Vec3A, ab: Vec3A, ac: Vec3A) -> (f32, f32) {
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (0.0, 0.0);
    }
    let bp = ap - ab;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (1.0, 0.0);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return (d1 / (d1 - d3), 0.0);
    }
    let cp = ap - ac;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
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

/// Exact signed distances (sign from the interpolated vertex normal) of the cells closer than
/// `band_cells` cells to the mesh; other cells hold [`UNSET`].
#[allow(clippy::too_many_lines, reason = "one rasterisation loop, read top to bottom")]
fn narrow_band(
    positions: &[f32],
    triangles: &[u32],
    normals: &[f32],
    f: &GridFrame,
    band_cells: f64,
) -> Vec<f32> {
    let [nx, ny, nz] = f.dims;
    let (g, o) = (f.cell, f.origin);
    let nxy = nx * ny;
    let band = g * band_cells;
    let band2 = (band * band) as f32;
    let first = |v: f64, o: f64| ((v - band - o) / g).floor().max(0.0) as usize;
    let last = |v: f64, o: f64, n: usize| (((v + band - o) / g).ceil().max(0.0) as usize).min(n - 1);
    let tri_count = triangles.len() / 3;
    // the triangles of each slab
    let slabs = nz.div_ceil(SLAB_LAYERS);
    let mut lists: Vec<Vec<u32>> = vec![Vec::new(); slabs];
    for t in 0..tri_count {
        let z = [0, 1, 2].map(|c| f64::from(positions[triangles[t * 3 + c] as usize * 3 + 2]));
        let lo = z[0].min(z[1]).min(z[2]);
        let hi = z[0].max(z[1]).max(z[2]);
        if hi + band < o.z || lo - band > o.z + (nz - 1) as f64 * g {
            continue;
        }
        let (k0, k1) = (first(lo, o.z), last(hi, o.z, nz));
        for list in &mut lists[k0 / SLAB_LAYERS..=k1 / SLAB_LAYERS] {
            list.push(t as u32);
        }
    }
    // zeroed memory comes straight from the allocator; slabs fault their pages in in parallel
    let mut dist = vec![UNSET; f.len()];
    dist.par_chunks_mut(nxy * SLAB_LAYERS)
        .zip(lists.par_iter())
        .enumerate()
        .for_each(|(s, (dist, list))| {
            let ks = s * SLAB_LAYERS;
            let ke = (ks + SLAB_LAYERS).min(nz) - 1;
            for &t in list {
                let t = t as usize;
                let (ia, ib, ic) = (
                    triangles[t * 3] as usize,
                    triangles[t * 3 + 1] as usize,
                    triangles[t * 3 + 2] as usize,
                );
                let (a, b, c) = (vec3(positions, ia), vec3(positions, ib), vec3(positions, ic));
                let (i0, i1) = (
                    first(a.x.min(b.x).min(c.x), o.x),
                    last(a.x.max(b.x).max(c.x), o.x, nx),
                );
                let (j0, j1) = (
                    first(a.y.min(b.y).min(c.y), o.y),
                    last(a.y.max(b.y).max(c.y), o.y, ny),
                );
                let k0 = first(a.z.min(b.z).min(c.z), o.z).max(ks);
                let k1 = last(a.z.max(b.z).max(c.z), o.z, nz).min(ke);
                if i0 > i1 || j0 > j1 || k0 > k1 {
                    continue;
                }
                let (ab, ac) = (b - a, c - a);
                let face = ab.cross(ac).normalize_or_zero();
                // a sphere around the triangle: cells beyond it plus the band are out of reach
                let centre = (a + b + c) / 3.0;
                let reach = (a - centre)
                    .length()
                    .max((b - centre).length())
                    .max((c - centre).length())
                    + band;
                let reach2 = reach * reach;
                let (na, nb, nc) = (
                    vec3(normals, ia).as_vec3a(),
                    vec3(normals, ib).as_vec3a(),
                    vec3(normals, ic).as_vec3a(),
                );
                for k in k0..=k1 {
                    let pz = o.z + k as f64 * g;
                    for j in j0..=j1 {
                        let py = o.y + j as f64 * g;
                        // the cells of this row within the band of the triangle's plane and
                        // within reach of its centre
                        let row = DVec3::new(o.x, py, pz);
                        let base = (row - a).dot(face);
                        let (mut x0, mut x1) = (i0 as f64, i1 as f64);
                        let fx = face.x * g;
                        if fx.abs() > 1e-12 {
                            let (u, w) = ((-band - base) / fx, (band - base) / fx);
                            x0 = x0.max(u.min(w).ceil());
                            x1 = x1.min(u.max(w).floor());
                        } else if base.abs() >= band {
                            continue;
                        }
                        let r2 = reach2 - (py - centre.y).powi(2) - (pz - centre.z).powi(2);
                        if r2 <= 0.0 {
                            continue;
                        }
                        let r = r2.sqrt();
                        x0 = x0.max(((centre.x - r - o.x) / g).ceil());
                        x1 = x1.min(((centre.x + r - o.x) / g).floor());
                        if x0 > x1 {
                            continue;
                        }
                        // cell positions relative to corner a, in single precision
                        let rel = (row - a).as_vec3a();
                        let (ab, ac) = (ab.as_vec3a(), ac.as_vec3a());
                        let step = Vec3A::new(g as f32, 0.0, 0.0);
                        for i in x0 as usize..=x1 as usize {
                            let at = i + nx * j + nxy * (k - ks);
                            let cur = dist[at].abs();
                            // the plane distance bounds the distance from below: skip cells
                            // another triangle already brought closer
                            if cur != 0.0 && (base + i as f64 * fx).abs() >= f64::from(cur) {
                                continue;
                            }
                            let ap = rel + step * i as f32;
                            let (u, v) = closest_on_triangle_f32(ap, ab, ac);
                            let dd = ap - (ab * u + ac * v);
                            let d2 = dd.length_squared();
                            if d2 < band2 && (cur == 0.0 || d2 < cur * cur) {
                                let n = na * (1.0 - u - v) + nb * u + nc * v;
                                let d = d2.sqrt().max(f32::MIN_POSITIVE);
                                dist[at] = if dd.dot(n) >= 0.0 { d } else { -d };
                            }
                        }
                    }
                }
            }
        });
    dist
}

/// Unsigned distances on the coarse grid `c` (clamped to [`DISTANCE_CLAMP_MM`]): the fine band's
/// values where a coarse value sits on a band cell, distances to the triangles' vertices within
/// reach, then relaxation passes outwards.
fn coarse_distance(
    positions: &[f32],
    triangles: &[u32],
    fine: Option<&[f32]>,
    f: &GridFrame,
    c: &GridFrame,
) -> Vec<f32> {
    let [nx, ny, nz] = c.dims;
    let nxy = nx * ny;
    let (g, o) = (c.cell, c.origin);
    let reach = g * COARSE_REACH_CELLS;
    let cf = COARSE_FACTOR;
    let far = DISTANCE_CLAMP_MM as f32;
    let mut used = vec![false; positions.len() / 3];
    for &v in triangles {
        used[v as usize] = true;
    }
    let mut lists: Vec<Vec<u32>> = vec![Vec::new(); nz];
    for (v, _) in used.iter().enumerate().filter(|(_, u)| **u) {
        let z = f64::from(positions[v * 3 + 2]);
        let k0 = ((z - reach - o.z) / g).ceil().max(0.0) as usize;
        let k1 = (((z + reach - o.z) / g).floor().max(-1.0) + 1.0) as usize;
        for list in lists.iter_mut().take(k1.min(nz)).skip(k0) {
            list.push(v as u32);
        }
    }
    let [fnx, fny, fnz] = f.dims;
    let mut dist = vec![far; c.len()];
    dist.par_chunks_mut(nxy)
        .zip(lists.par_iter())
        .enumerate()
        .for_each(|(k, (slab, list))| {
            let pz = o.z + k as f64 * g;
            for &v in list {
                let p = vec3(positions, v as usize);
                let lo = |x: f64, o: f64| ((x - reach - o) / g).ceil().max(0.0) as usize;
                let hi = |x: f64, o: f64, n: usize| {
                    ((((x + reach - o) / g).floor()).max(-1.0) + 1.0).min(n as f64) as usize
                };
                for j in lo(p.y, o.y)..hi(p.y, o.y, ny) {
                    let py = o.y + j as f64 * g;
                    for i in lo(p.x, o.x)..hi(p.x, o.x, nx) {
                        let d = (DVec3::new(o.x + i as f64 * g, py, pz) - p).length() as f32;
                        let at = i + nx * j;
                        if d < slab[at] {
                            slab[at] = d;
                        }
                    }
                }
            }
            if let Some(fine) = fine
                && k * cf < fnz
            {
                for j in (0..ny).filter(|j| j * cf < fny) {
                    for i in (0..nx).filter(|i| i * cf < fnx) {
                        let d = fine[i * cf + fnx * (j * cf + fny * k * cf)];
                        if d != UNSET && d.abs() < slab[i + nx * j] {
                            slab[i + nx * j] = d.abs();
                        }
                    }
                }
            }
        });
    // relax outwards: each pass takes the best neighbour plus the step to it
    let steps = [g, g * std::f64::consts::SQRT_2, g * 3.0_f64.sqrt()].map(|s| s as f32);
    for _ in 0..COARSE_PASSES {
        let prev = dist.clone();
        dist.par_chunks_mut(nxy).enumerate().for_each(|(k, slab)| {
            for j in 0..ny {
                for i in 0..nx {
                    let mut best = prev[i + nx * (j + ny * k)];
                    let around = |c: usize, n: usize| c.saturating_sub(1)..=(c + 1).min(n - 1);
                    for kk in around(k, nz) {
                        for jj in around(j, ny) {
                            for ii in around(i, nx) {
                                let order =
                                    usize::from(ii != i) + usize::from(jj != j) + usize::from(kk != k);
                                if order > 0 {
                                    let q = ii + nx * (jj + ny * kk);
                                    best = best.min(prev[q] + steps[order - 1]);
                                }
                            }
                        }
                    }
                    slab[i + nx * j] = best.min(far);
                }
            }
        });
    }
    dist
}

/// The runs of cells outside the band in one z layer, as (first index, end index, sign): the sign
/// of the band cells at both ends of the run along x (outside past the border), or along y
/// where those disagree.
fn layer_runs(slab: &[f32], nx: usize, ny: usize) -> Vec<(usize, usize, f32)> {
    let side = |v: Option<&f32>| match v {
        Some(&d) if d < 0.0 => -1_i8,
        _ => 1,
    };
    let mut runs = Vec::new();
    for j in 0..ny {
        let row = &slab[nx * j..nx * (j + 1)];
        let mut i = 0;
        while i < nx {
            if row[i] != UNSET {
                i += 1;
                continue;
            }
            let start = i;
            while i < nx && row[i] == UNSET {
                i += 1;
            }
            let left = side(start.checked_sub(1).map(|s| &row[s]));
            let right = side(row.get(i));
            if left == right {
                runs.push((nx * j + start, nx * j + i, f32::from(left)));
                continue;
            }
            // the ends disagree: decide cell by cell along y
            for x in start..i {
                let up = (0..j).rev().map(|y| slab[x + nx * y]).find(|&d| d != UNSET);
                let down = (j + 1..ny).map(|y| slab[x + nx * y]).find(|&d| d != UNSET);
                let (u, d) = (side(up.as_ref()), side(down.as_ref()));
                let sign = if u == d {
                    u
                } else if left + right + u + d < 0 {
                    -1
                } else {
                    1
                };
                runs.push((nx * j + x, nx * j + x + 1, f32::from(sign)));
            }
        }
    }
    runs
}

/// Signed distance grid of a closed triangle mesh in the given layout, fast enough to rebuild
/// every frame while the body moves (see the module notes for how it differs from
/// [`crate::mesh_sdf`]). Values are clamped to [`DISTANCE_CLAMP_MM`].
///
/// # Panics
///
/// Panics if a triangle index is out of range of `positions` or `normals`, or the frame is
/// empty.
#[must_use]
pub fn mesh_sdf_in(positions: &[f32], triangles: &[u32], normals: &[f32], frame: &GridFrame) -> SdfGrid {
    assert!(!frame.is_empty(), "empty grid frame");
    let g = frame.cell;
    let [nx, ny, _] = frame.dims;
    let cf = COARSE_FACTOR;
    let coarse = GridFrame {
        origin: frame.origin,
        cell: g * cf as f64,
        dims: frame.dims.map(|n| (n - 1).div_ceil(cf) + 2),
    };
    let (mut data, cdist) = rayon::join(
        || narrow_band(positions, triangles, normals, frame, FINE_BAND_CELLS),
        || coarse_distance(positions, triangles, None, frame, &coarse),
    );
    let band = (g * FINE_BAND_CELLS) as f32;
    let clamp = DISTANCE_CLAMP_MM as f32;
    let [cnx, cny, _] = coarse.dims;
    let cnxy = cnx * cny;
    data.par_chunks_mut(nx * ny).enumerate().for_each(|(k, slab)| {
        let runs = layer_runs(slab, nx, ny);
        let (ck, wz) = (k / cf, (k % cf) as f32 / cf as f32);
        // the coarse distances of the current row, interpolated in y and z
        let mut line = vec![0.0_f32; cnx];
        let mut line_row = usize::MAX;
        for (start, end, sign) in runs {
            let j = start / nx;
            if j != line_row {
                line_row = j;
                let (cj, wy) = (j / cf, (j % cf) as f32 / cf as f32);
                for (ci, v) in line.iter_mut().enumerate() {
                    let q = ci + cnx * cj + cnxy * ck;
                    let y0 = cdist[q] + (cdist[q + cnx] - cdist[q]) * wy;
                    let y1 = cdist[q + cnxy] + (cdist[q + cnxy + cnx] - cdist[q + cnxy]) * wy;
                    *v = y0 + (y1 - y0) * wz;
                }
            }
            for (at, out) in slab[start..end].iter_mut().enumerate() {
                let i = start + at - nx * j;
                let (ci, wx) = (i / cf, (i % cf) as f32 / cf as f32);
                let m = line[ci] + (line[ci + 1] - line[ci]) * wx;
                *out = sign * m.clamp(band, clamp);
            }
        }
    });
    SdfGrid {
        origin: frame.origin,
        cell: g,
        dims: frame.dims,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_sdf::{mesh_sdf, vertex_normals};

    /// A closed UV sphere.
    fn sphere(r: f64, c: DVec3) -> (Vec<f32>, Vec<u32>) {
        let (rings, segs) = (24_u32, 48_u32);
        let mut p = Vec::new();
        p.extend([c.x, c.y + r, c.z].map(|v| v as f32));
        for i in 1..rings {
            let th = std::f64::consts::PI * f64::from(i) / f64::from(rings);
            for j in 0..segs {
                let ph = std::f64::consts::TAU * f64::from(j) / f64::from(segs);
                let q = c + DVec3::new(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()) * r;
                p.extend([q.x, q.y, q.z].map(|v| v as f32));
            }
        }
        p.extend([c.x, c.y - r, c.z].map(|v| v as f32));
        let bottom = 1 + (rings - 1) * segs;
        let at = |i: u32, j: u32| 1 + (i - 1) * segs + j % segs;
        let mut t = Vec::new();
        for j in 0..segs {
            t.extend([0, at(1, j + 1), at(1, j)]);
            t.extend([bottom, at(rings - 1, j), at(rings - 1, j + 1)]);
        }
        for i in 1..rings - 1 {
            for j in 0..segs {
                t.extend([at(i, j), at(i, j + 1), at(i + 1, j + 1)]);
                t.extend([at(i, j), at(i + 1, j + 1), at(i + 1, j)]);
            }
        }
        (p, t)
    }

    #[test]
    fn agrees_with_the_reference_grid_near_the_surface_and_in_sign() {
        let c = DVec3::new(0.0, 200.0, 0.0);
        let (p, t) = sphere(120.0, c);
        let n = vertex_normals(&p, &t, p.len() / 3);
        let reference = mesh_sdf(&p, &t, 7.5, 60.0, &n);
        let fast = mesh_sdf_in(&p, &t, &n, &reference.frame());
        assert_eq!(fast.dims, reference.dims);
        for (a, b) in fast.data.iter().zip(&reference.data) {
            let (a, b) = (f64::from(*a), f64::from(*b));
            if b.abs() < 9.0 {
                assert!((a - b).abs() < 0.01, "{a} vs {b}");
            } else {
                assert_eq!(a > 0.0, b > 0.0, "{a} vs {b}");
                assert!((a - b).abs() < 12.0, "{a} vs {b}");
            }
        }
        assert!(fast.distance(c) < -79.0);
    }
}
