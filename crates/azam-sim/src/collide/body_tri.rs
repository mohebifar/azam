//! Skin vertices against cloth triangles: keeps the cloth surface (not only its particles) clear of
//! the skin, so body detail smaller than the particle distance cannot poke through. Only avatars
//! with a skin mesh ([`Avatar::body_collision_mesh`]) use it.

use azam_avatar::Avatar;

use crate::numeric::{NO_ENTRY, bucket, cell_hash, load3, max3, min3};
use crate::state::{Particles, PieceRecord};

/// Buckets of the cloth-triangle hash (a power of two).
pub const BODY_HASH_BUCKETS: usize = 1 << 16;
/// Cell size of the cloth-triangle hash, in mm.
pub const BODY_HASH_CELL_MM: f64 = 24.0;
/// Hash entries allowed per triangle (the table does not grow; triangles past it are skipped).
const ENTRIES_PER_TRIANGLE: usize = 8;
/// Extra hash entries beyond the per-triangle allowance.
const EXTRA_ENTRIES: usize = 16;
/// Ray-triangle determinants smaller than this are parallel.
const PARALLEL_DET: f64 = 1e-6;
/// Barycentric tolerance of a ray hit.
const HIT_TOLERANCE: f64 = 0.02;
/// Cloth is kept this fraction of its margin from the skin.
const MARGIN_FACTOR: f64 = 0.8;
/// Cloth further inside the skin than this (mm) is left to the distance grid.
const MAX_DEPTH_MM: f64 = 14.0;
/// Weight sums below this give no correction.
const MIN_WEIGHT: f64 = 1e-6;

/// Cloth triangles and their clearances, plus the hash rebuilt on every use.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyTriangles {
    /// Triangle corners (particle indices), three per triangle.
    pub tris: Vec<u32>,
    /// Clearance of each triangle (smallest corner margin), mm.
    pub margin: Vec<f32>,
    head: Vec<u32>,
    next: Vec<u32>,
    item: Vec<u32>,
}

impl BodyTriangles {
    /// The cloth triangles of every piece, or `None` when the avatar has no skin mesh.
    #[must_use]
    pub fn new(avatar: &Avatar, records: &[PieceRecord], margin: &[f32]) -> Option<Self> {
        avatar.body_collision_mesh()?;
        let mut tris = Vec::new();
        let mut tm = Vec::new();
        for r in records {
            for t in r.mesh.tris.chunks_exact(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|v| v + r.offset as u32);
                tris.extend([a, b, c]);
                tm.push(margin[a as usize].min(margin[b as usize]).min(margin[c as usize]));
            }
        }
        Some(Self::from_triangles(tris, tm))
    }

    /// Cloth triangles `tris` (three particles each) with clearances `margin` (one per
    /// triangle, mm).
    #[must_use]
    pub fn from_triangles(tris: Vec<u32>, margin: Vec<f32>) -> Self {
        let cap = tris.len() / 3 * ENTRIES_PER_TRIANGLE + EXTRA_ENTRIES;
        Self {
            tris,
            margin,
            head: vec![NO_ENTRY; BODY_HASH_BUCKETS],
            next: vec![0; cap],
            item: vec![0; cap],
        }
    }

    /// Pushes cloth triangles off the avatar's skin; see [`BodyTriangles::collide_with_skin`].
    pub fn collide(&mut self, p: &mut Particles, avatar: &Avatar) -> usize {
        match avatar.body_collision_mesh() {
            Some(skin) => self.collide_with_skin(p, skin.positions, skin.normals, skin.body_vertex_count),
            None => 0,
        }
    }

    /// Pushes cloth triangles off the first `count` skin vertices (`positions` and unit
    /// `normals`, `x, y, z` each) along the skin normals; returns the number of corrections.
    /// Positions and substep-start positions move together, so no velocity is added.
    pub fn collide_with_skin(
        &mut self,
        p: &mut Particles,
        positions: &[f32],
        normals: &[f32],
        count: usize,
    ) -> usize {
        let c = BODY_HASH_CELL_MM;
        let w = &p.inv_mass;
        self.head.fill(NO_ENTRY);
        let mut cnt = 0;
        let nt = self.tris.len() / 3;
        for t in 0..nt {
            let (ia, ib, ic) = (
                self.tris[3 * t] as usize,
                self.tris[3 * t + 1] as usize,
                self.tris[3 * t + 2] as usize,
            );
            if w[ia] == 0.0 && w[ib] == 0.0 && w[ic] == 0.0 {
                continue;
            }
            let (a, b, cc) = (load3(&p.x, ia), load3(&p.x, ib), load3(&p.x, ic));
            let lo = |d: usize| (min3(a[d], b[d], cc[d]) / c).floor();
            let hi = |d: usize| (max3(a[d], b[d], cc[d]) / c).floor();
            let mut i = lo(0);
            while i <= hi(0) {
                let mut j = lo(1);
                while j <= hi(1) {
                    let mut k = lo(2);
                    while k <= hi(2) {
                        if cnt >= self.item.len() {
                            break;
                        }
                        let h = bucket(cell_hash(i, j, k), BODY_HASH_BUCKETS);
                        self.item[cnt] = t as u32;
                        self.next[cnt] = self.head[h];
                        self.head[h] = cnt as u32;
                        cnt += 1;
                        k += 1.0;
                    }
                    j += 1.0;
                }
                i += 1.0;
            }
        }
        let mut fixes = 0;
        for v in 0..count {
            let s3 = |buf: &[f32]| {
                [
                    f64::from(buf[3 * v]),
                    f64::from(buf[3 * v + 1]),
                    f64::from(buf[3 * v + 2]),
                ]
            };
            let [px, py, pz] = s3(positions);
            let h = bucket(
                cell_hash((px / c).floor(), (py / c).floor(), (pz / c).floor()),
                BODY_HASH_BUCKETS,
            );
            let mut e = self.head[h];
            if e == NO_ENTRY {
                continue;
            }
            let [nx, ny, nz] = s3(normals);
            while e != NO_ENTRY {
                let t = self.item[e as usize] as usize;
                e = self.next[e as usize];
                if self.push_off(p, t, [px, py, pz], [nx, ny, nz]) {
                    fixes += 1;
                }
            }
        }
        fixes
    }

    /// Casts the skin point `s` along its normal `n` against triangle `t`; pushes the triangle out
    /// if it is closer than its clearance. Returns whether it was pushed.
    fn push_off(&self, p: &mut Particles, t: usize, s: [f64; 3], n: [f64; 3]) -> bool {
        let [ia, ib, ic] = [self.tris[3 * t], self.tris[3 * t + 1], self.tris[3 * t + 2]].map(|v| v as usize);
        let (a, b, c) = (ia * 3, ib * 3, ic * 3);
        let (xa, xb, xc) = (load3(&p.x, ia), load3(&p.x, ib), load3(&p.x, ic));
        let (e1x, e1y, e1z) = (xb[0] - xa[0], xb[1] - xa[1], xb[2] - xa[2]);
        let (e2x, e2y, e2z) = (xc[0] - xa[0], xc[1] - xa[1], xc[2] - xa[2]);
        let [nx, ny, nz] = n;
        let (hx, hy, hz) = (ny * e2z - nz * e2y, nz * e2x - nx * e2z, nx * e2y - ny * e2x);
        let det = e1x * hx + e1y * hy + e1z * hz;
        if det > -PARALLEL_DET && det < PARALLEL_DET {
            return false;
        }
        let inv = 1.0 / det;
        let (sx, sy, sz) = (s[0] - xa[0], s[1] - xa[1], s[2] - xa[2]);
        let u = (sx * hx + sy * hy + sz * hz) * inv;
        if !(-HIT_TOLERANCE..=1.0 + HIT_TOLERANCE).contains(&u) {
            return false;
        }
        let (qx, qy, qz) = (sy * e1z - sz * e1y, sz * e1x - sx * e1z, sx * e1y - sy * e1x);
        let vv = (nx * qx + ny * qy + nz * qz) * inv;
        if vv < -HIT_TOLERANCE || u + vv > 1.0 + HIT_TOLERANCE {
            return false;
        }
        let dist = (e2x * qx + e2y * qy + e2z * qz) * inv;
        let m = f64::from(self.margin[t]) * MARGIN_FACTOR;
        if dist >= m || dist < -MAX_DEPTH_MM {
            return false;
        }
        let w = &p.inv_mass;
        let b0 = (1.0 - u - vv).max(0.0);
        let b1 = u.max(0.0);
        let b2 = vv.max(0.0);
        let wa = if w[ia] == 0.0 { 0.0 } else { b0 };
        let wb = if w[ib] == 0.0 { 0.0 } else { b1 };
        let wc = if w[ic] == 0.0 { 0.0 } else { b2 };
        let den = wa * wa + wb * wb + wc * wc;
        if den < MIN_WEIGHT {
            return false;
        }
        let f = (m - dist) / den;
        for (k, wk) in [(a, wa), (b, wb), (c, wc)] {
            if wk == 0.0 {
                continue;
            }
            let (dx, dy, dz) = (nx * f * wk, ny * f * wk, nz * f * wk);
            for (d, delta) in [dx, dy, dz].into_iter().enumerate() {
                crate::numeric::nudge(&mut p.x[k + d], delta);
                crate::numeric::nudge(&mut p.prev[k + d], delta);
            }
        }
        true
    }
}
