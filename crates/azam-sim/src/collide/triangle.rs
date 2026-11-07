//! Vertex-triangle cloth contacts.
//!
//! Every particle is kept a collision thickness away from every cloth triangle that is not its
//! neighbour (triangles of its own piece included, so a piece cannot pass through itself when it
//! folds). Within one layer the particle keeps the side it came from; between layers the side is
//! fixed by layer order (the higher layer stays outside along the body normal), which also
//! repairs layers that have crossed. Contacts are detected every few substeps into a list and
//! resolved with friction on every substep.

use azam_avatar::SdfGrid;
use azam_doc::PieceState;
use azam_geom::hypot;

use super::sample_into;
use crate::numeric::{NO_ENTRY, bucket, cell_hash, clamp, hypot3, load3, max3, min3, nudge};
use crate::params::SimParams;
use crate::state::{Particles, PieceRecord, SewingConstraints};

/// Buckets of the triangle hash (a power of two).
pub const TRIANGLE_HASH_BUCKETS: usize = 1 << 17;
/// Hash cells span this factor of the mean mesh edge.
const CELL_EDGE_FACTOR: f64 = 1.1;
/// Hash cell size limits, in mm.
const CELL_RANGE_MM: (f64, f64) = (10.0, 60.0);
/// Triangles spanning more cells than this are degenerate or exploded and are skipped.
const MAX_TRIANGLE_CELLS: f64 = 64.0;
/// Contact search distance beyond the cloth thickness, in mm (plus the slack parameter).
const SEARCH_MARGIN_MM: f64 = 2.0;
/// Own-piece triangles within this many particle distances (in the flat pattern) are neighbours.
const NEIGHBOUR_SPACINGS: f64 = 2.2;
/// Particle distance assumed when none is known, in mm.
const DEFAULT_SPACING_MM: f64 = 20.0;
/// How far outside a triangle (barycentric) a particle may be when a contact is detected.
const DETECT_TOLERANCE: f64 = -0.08;
/// How far outside a triangle a recorded contact may slide before it is dropped.
const RESOLVE_TOLERANCE: f64 = -0.12;
/// Mean mesh edge assumed for a simulation without triangles, in mm.
const DEFAULT_EDGE_MM: f64 = 20.0;
/// Largest normal correction per contact and substep, as a fraction of the mean edge.
const MAX_STEP_EDGE_FACTOR: f64 = 0.25;
/// Smallest cap on the normal correction, in mm.
const MIN_MAX_STEP_MM: f64 = 2.0;

/// Cloth triangles of the active pieces, which pieces each particle is sewn to, and the contacts
/// found by the last detection.
#[derive(Clone, Debug, PartialEq)]
pub struct TriangleContacts {
    /// Triangle corners (particle indices), three per triangle.
    pub tris: Vec<u32>,
    /// Piece record of each triangle.
    pub tri_piece: Vec<u32>,
    /// Per particle, one bit per piece (piece index mod 32) it is sewn to, itself included.
    pub sewn_pieces: Vec<u32>,
    /// Mean first-edge length of the triangles in the flat pattern, mm.
    pub edge: f64,
    head: Vec<u32>,
    next: Vec<u32>,
    item: Vec<u32>,
    contact_particle: Vec<u32>,
    contact_tri: Vec<u32>,
    contact_side: Vec<i8>,
    gradient: [f64; 3],
}

/// Which pieces each particle is sewn to: bit `piece mod 32` of every piece across its seams.
#[must_use]
pub fn sewn_piece_mask(particles: &Particles, sewing: &SewingConstraints) -> Vec<u32> {
    let po = &particles.piece;
    let mut pm = vec![0_u32; particles.len()];
    for c in 0..sewing.len() {
        let (qa, q0, q1) = (sewing.a[c] as usize, sewing.b0[c] as usize, sewing.b1[c] as usize);
        let ba = 1_u32 << (po[qa] & 31);
        let bb = 1_u32 << (po[q0] & 31);
        pm[qa] |= bb | ba;
        pm[q0] |= ba | bb;
        pm[q1] |= ba | bb;
    }
    pm
}

impl TriangleContacts {
    /// Collects the triangles of the active pieces.
    #[must_use]
    pub fn new(records: &[PieceRecord], particles: &Particles, sewing: &SewingConstraints) -> Self {
        let mut tris = Vec::new();
        let mut tri_piece = Vec::new();
        let (mut sum, mut count) = (0.0, 0_usize);
        for (ri, r) in records.iter().enumerate() {
            if r.state == PieceState::Off {
                continue;
            }
            let xy = &r.mesh.xy;
            for t in r.mesh.tris.chunks_exact(3) {
                tris.extend(t.iter().map(|&v| v + r.offset as u32));
                tri_piece.push(ri as u32);
                let (a, b) = (t[0] as usize, t[1] as usize);
                sum += hypot(xy[2 * a] - xy[2 * b], xy[2 * a + 1] - xy[2 * b + 1]);
                count += 1;
            }
        }
        Self {
            tris,
            tri_piece,
            sewn_pieces: sewn_piece_mask(particles, sewing),
            edge: if count > 0 {
                sum / count as f64
            } else {
                DEFAULT_EDGE_MM
            },
            head: vec![NO_ENTRY; TRIANGLE_HASH_BUCKETS],
            next: Vec::new(),
            item: Vec::new(),
            contact_particle: Vec::new(),
            contact_tri: Vec::new(),
            contact_side: Vec::new(),
            gradient: [0.0; 3],
        }
    }

    /// Number of contacts recorded by the last detection.
    #[must_use]
    pub fn contact_count(&self) -> usize {
        self.contact_particle.len()
    }

    fn corners(&self, t: usize) -> (usize, usize, usize) {
        (
            self.tris[3 * t] as usize,
            self.tris[3 * t + 1] as usize,
            self.tris[3 * t + 2] as usize,
        )
    }

    /// Hashes every triangle into the cells its bounds (grown by `r`) cover.
    fn hash_triangles(&mut self, x: &[f32], r: f64, c: f64) {
        self.head.fill(NO_ENTRY);
        self.item.clear();
        self.next.clear();
        for t in 0..self.tri_piece.len() {
            let (ia, ib, ic) = self.corners(t);
            let (a, b, cc) = (load3(x, ia), load3(x, ib), load3(x, ic));
            let lo = |d: usize| ((min3(a[d], b[d], cc[d]) - r) / c).floor();
            let hi = |d: usize| ((max3(a[d], b[d], cc[d]) + r) / c).floor();
            let (i0, i1, j0, j1, k0, k1) = (lo(0), hi(0), lo(1), hi(1), lo(2), hi(2));
            let cells = (i1 - i0 + 1.0) * (j1 - j0 + 1.0) * (k1 - k0 + 1.0);
            if cells.is_nan() || cells > MAX_TRIANGLE_CELLS {
                continue;
            }
            let mut i = i0;
            while i <= i1 {
                let mut j = j0;
                while j <= j1 {
                    let mut k = k0;
                    while k <= k1 {
                        let h = bucket(cell_hash(i, j, k), TRIANGLE_HASH_BUCKETS);
                        self.item.push(t as u32);
                        self.next.push(self.head[h]);
                        self.head[h] = (self.item.len() - 1) as u32;
                        k += 1.0;
                    }
                    j += 1.0;
                }
                i += 1.0;
            }
        }
    }

    /// Detects vertex-triangle contacts, then resolves them once.
    pub(crate) fn detect(
        &mut self,
        p: &mut Particles,
        records: &[PieceRecord],
        grid: &SdfGrid,
        params: &SimParams,
    ) {
        let h0 = params.cloth_thickness;
        let r = h0 + SEARCH_MARGIN_MM + params.contact_slack;
        let c = clamp(self.edge * CELL_EDGE_FACTOR, CELL_RANGE_MM.0, CELL_RANGE_MM.1);
        self.hash_triangles(&p.x, r, c);
        self.contact_particle.clear();
        self.contact_tri.clear();
        self.contact_side.clear();
        let mut g = self.gradient;
        for q in 0..p.len() {
            if !p.active[q] {
                continue;
            }
            let [qx, qy, qz] = load3(&p.x, q);
            let h = bucket(
                cell_hash((qx / c).floor(), (qy / c).floor(), (qz / c).floor()),
                TRIANGLE_HASH_BUCKETS,
            );
            let mut e = self.head[h];
            if e == NO_ENTRY {
                continue;
            }
            let pq = p.piece[q];
            let rq = &records[pq as usize];
            let xyq = &rq.mesh.xy;
            let lq = q - rq.offset;
            let spacing = f64::from(p.spacing[q]);
            let ex2 = (NEIGHBOUR_SPACINGS
                * if spacing == 0.0 {
                    DEFAULT_SPACING_MM
                } else {
                    spacing
                })
            .powi(2);
            let (sq, mq, wq) = (p.sewn[q], self.sewn_pieces[q], p.inv_mass[q]);
            while e != NO_ENTRY {
                let t = self.item[e as usize] as usize;
                e = self.next[e as usize];
                let (ia, ib, ic) = self.corners(t);
                if ia == q || ib == q || ic == q {
                    continue;
                }
                if wq == 0.0 && p.inv_mass[ia] == 0.0 && p.inv_mass[ib] == 0.0 && p.inv_mass[ic] == 0.0 {
                    continue;
                }
                let pt = self.tri_piece[t];
                if pt == pq {
                    let near = [ia, ib, ic].iter().any(|&v| {
                        let lv = v - rq.offset;
                        let (dx, dy) = (xyq[2 * lv] - xyq[2 * lq], xyq[2 * lv + 1] - xyq[2 * lq + 1]);
                        dx * dx + dy * dy < ex2
                    });
                    if near {
                        continue;
                    }
                }
                if sq && (mq & (1 << (pt & 31))) != 0 && (p.sewn[ia] || p.sewn[ib] || p.sewn[ic]) {
                    continue;
                }
                let Some(candidate) = Candidate::near(&p.x, q, [ia, ib, ic], r) else {
                    continue;
                };
                let side = candidate.side(p, grid, &mut g);
                self.contact_particle.push(q as u32);
                self.contact_tri.push(t as u32);
                self.contact_side.push(side);
            }
        }
        self.gradient = g;
        self.resolve(p, params);
    }

    /// Resolves the recorded contacts: pushes touching particles out to the cloth thickness and
    /// applies friction.
    pub(crate) fn resolve(&self, p: &mut Particles, params: &SimParams) {
        let h0 = params.cloth_thickness;
        let max_step = (self.edge * MAX_STEP_EDGE_FACTOR).max(MIN_MAX_STEP_MM);
        for ((&q, &t), &side) in self
            .contact_particle
            .iter()
            .zip(&self.contact_tri)
            .zip(&self.contact_side)
        {
            let (q, t, side) = (q as usize, t as usize, f64::from(side));
            let (ia, ib, ic) = self.corners(t);
            resolve_one(p, params, q, [ia, ib, ic], side, h0, max_step);
        }
    }
}

/// A particle found close to a cloth triangle.
struct Candidate {
    /// The particle.
    q: usize,
    /// The triangle's corners and their positions.
    tri: [usize; 3],
    pos: [[f64; 3]; 3],
    /// Barycentric coordinates of the particle's projection.
    bary: [f64; 3],
    /// Unit normal of the triangle.
    normal: [f64; 3],
    /// Signed distance of the particle along the normal.
    distance: f64,
}

impl Candidate {
    /// Particle `q` projected onto triangle `tri`, if it lies within `r` of its plane and (nearly)
    /// over the triangle.
    fn near(x: &[f32], q: usize, tri: [usize; 3], r: f64) -> Option<Self> {
        let [qx, qy, qz] = load3(x, q);
        let [a, b, cc] = tri.map(|v| load3(x, v));
        let (e1x, e1y, e1z) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
        let (e2x, e2y, e2z) = (cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]);
        let (mut nx, mut ny, mut nz) = (
            e1y * e2z - e1z * e2y,
            e1z * e2x - e1x * e2z,
            e1x * e2y - e1y * e2x,
        );
        let nl = (nx * nx + ny * ny + nz * nz).sqrt();
        if nl < 1e-9 {
            return None;
        }
        nx /= nl;
        ny /= nl;
        nz /= nl;
        let (rx, ry, rz) = (qx - a[0], qy - a[1], qz - a[2]);
        let sd = rx * nx + ry * ny + rz * nz;
        if sd > r || sd < -r {
            return None;
        }
        let d00 = e1x * e1x + e1y * e1y + e1z * e1z;
        let d01 = e1x * e2x + e1y * e2y + e1z * e2z;
        let d11 = e2x * e2x + e2y * e2y + e2z * e2z;
        let d20 = rx * e1x + ry * e1y + rz * e1z;
        let d21 = rx * e2x + ry * e2y + rz * e2z;
        let den = d00 * d11 - d01 * d01;
        if den < 1e-9 {
            return None;
        }
        let bv = (d11 * d20 - d01 * d21) / den;
        let bw = (d00 * d21 - d01 * d20) / den;
        let bu = 1.0 - bv - bw;
        if bu < DETECT_TOLERANCE || bv < DETECT_TOLERANCE || bw < DETECT_TOLERANCE {
            return None;
        }
        Some(Self {
            q,
            tri,
            pos: [a, b, cc],
            bary: [bu, bv, bw],
            normal: [nx, ny, nz],
            distance: sd,
        })
    }

    /// Which side of the triangle the particle must stay on (+1 along the normal, -1 against
    /// it). Within one layer it is the side the particle was on at the substep start (or now,
    /// when it was on the plane); between layers the higher layer stays outside along the body
    /// normal (sampled into the scratch gradient `g`).
    fn side(&self, p: &Particles, grid: &SdfGrid, g: &mut [f64; 3]) -> i8 {
        let q = self.q;
        let [ia, ib, ic] = self.tri;
        let [a, b, c] = self.pos;
        let [bu, bv, bw] = self.bary;
        let [nx, ny, nz] = self.normal;
        let sd = self.distance;
        if p.layer[q] == p.layer[ia] {
            let (pa, pb, pc) = (load3(&p.prev, ia), load3(&p.prev, ib), load3(&p.prev, ic));
            let pq = load3(&p.prev, q);
            let sp = (pq[0] - (bu * pa[0] + bv * pb[0] + bw * pc[0])) * nx
                + (pq[1] - (bu * pa[1] + bv * pb[1] + bw * pc[1])) * ny
                + (pq[2] - (bu * pa[2] + bv * pb[2] + bw * pc[2])) * nz;
            return if (if sp.abs() > 1e-6 { sp } else { sd }) >= 0.0 {
                1
            } else {
                -1
            };
        }
        let centroid = [
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ];
        sample_into(grid, centroid, g);
        let so = if g[0] * nx + g[1] * ny + g[2] * nz >= 0.0 {
            1
        } else {
            -1
        };
        if p.layer[q] > p.layer[ia] { so } else { -so }
    }
}

/// Resolves one vertex-triangle contact.
fn resolve_one(
    p: &mut Particles,
    params: &SimParams,
    q: usize,
    tri: [usize; 3],
    side: f64,
    h0: f64,
    max_step: f64,
) {
    let [ia, ib, ic] = tri;
    let (kq, a, b, c) = (q * 3, ia * 3, ib * 3, ic * 3);
    let (xa, xb, xc, xq) = (load3(&p.x, ia), load3(&p.x, ib), load3(&p.x, ic), load3(&p.x, q));
    let (e1x, e1y, e1z) = (xb[0] - xa[0], xb[1] - xa[1], xb[2] - xa[2]);
    let (e2x, e2y, e2z) = (xc[0] - xa[0], xc[1] - xa[1], xc[2] - xa[2]);
    let (mut nx, mut ny, mut nz) = (
        e1y * e2z - e1z * e2y,
        e1z * e2x - e1x * e2z,
        e1x * e2y - e1y * e2x,
    );
    let nl = (nx * nx + ny * ny + nz * nz).sqrt();
    if nl < 1e-9 {
        return;
    }
    nx /= nl;
    ny /= nl;
    nz /= nl;
    let (rx, ry, rz) = (xq[0] - xa[0], xq[1] - xa[1], xq[2] - xa[2]);
    let d00 = e1x * e1x + e1y * e1y + e1z * e1z;
    let d01 = e1x * e2x + e1y * e2y + e1z * e2z;
    let d11 = e2x * e2x + e2y * e2y + e2z * e2z;
    let d20 = rx * e1x + ry * e1y + rz * e1z;
    let d21 = rx * e2x + ry * e2y + rz * e2z;
    let den0 = d00 * d11 - d01 * d01;
    if den0 < 1e-9 {
        return;
    }
    let mut bv = (d11 * d20 - d01 * d21) / den0;
    let mut bw = (d00 * d21 - d01 * d20) / den0;
    let mut bu = 1.0 - bv - bw;
    if bu < RESOLVE_TOLERANCE || bv < RESOLVE_TOLERANCE || bw < RESOLVE_TOLERANCE {
        return;
    }
    bu = if bu < 0.0 { 0.0 } else { bu };
    bv = if bv < 0.0 { 0.0 } else { bv };
    bw = if bw < 0.0 { 0.0 } else { bw };
    let bs = bu + bv + bw;
    bu /= bs;
    bv /= bs;
    bw /= bs;
    let th = &p.thickness;
    let h = h0 + 0.5 * (f64::from(th[q]) + (f64::from(th[ia]) + f64::from(th[ib]) + f64::from(th[ic])) / 3.0);
    let s_cur = side
        * ((xq[0] - (bu * xa[0] + bv * xb[0] + bw * xc[0])) * nx
            + (xq[1] - (bu * xa[1] + bv * xb[1] + bw * xc[1])) * ny
            + (xq[2] - (bu * xa[2] + bv * xb[2] + bw * xc[2])) * nz);
    if s_cur >= h {
        return;
    }
    let wq = f64::from(p.inv_mass[q]);
    let wa = f64::from(p.inv_mass[ia]) * bu;
    let wb = f64::from(p.inv_mass[ib]) * bv;
    let wc = f64::from(p.inv_mass[ic]) * bw;
    let den = wq + wa * bu + wb * bv + wc * bw;
    if den <= 0.0 {
        return;
    }
    let cn = (h - s_cur).min(max_step);
    let lam = cn / den;
    let (fx, fy, fz) = (side * nx * lam, side * ny * lam, side * nz * lam);
    let x = &mut p.x;
    nudge(&mut x[kq], fx * wq);
    nudge(&mut x[kq + 1], fy * wq);
    nudge(&mut x[kq + 2], fz * wq);
    for (k, w) in [(a, wa), (b, wb), (c, wc)] {
        nudge(&mut x[k], -(fx * w));
        nudge(&mut x[k + 1], -(fy * w));
        nudge(&mut x[k + 2], -(fz * w));
    }
    let mu = (f64::from(p.friction[q]) + f64::from(p.friction[ia])) * 0.5;
    if mu.is_nan() || mu <= 0.0 {
        return;
    }
    let pp = &p.prev;
    let x = &p.x;
    let slip = |d: usize| {
        (f64::from(x[kq + d]) - f64::from(pp[kq + d]))
            - (bu * (f64::from(x[a + d]) - f64::from(pp[a + d]))
                + bv * (f64::from(x[b + d]) - f64::from(pp[b + d]))
                + bw * (f64::from(x[c + d]) - f64::from(pp[c + d])))
    };
    let (mut tx, mut ty, mut tz) = (slip(0), slip(1), slip(2));
    let tn = tx * nx + ty * ny + tz * nz;
    tx -= tn * nx;
    ty -= tn * ny;
    tz -= tn * nz;
    let tl = hypot3(tx, ty, tz);
    if tl < 1e-9 {
        return;
    }
    let f = params.friction_fraction(mu, cn, tl) / den;
    let x = &mut p.x;
    nudge(&mut x[kq], -(tx * f * wq));
    nudge(&mut x[kq + 1], -(ty * f * wq));
    nudge(&mut x[kq + 2], -(tz * f * wq));
    for (k, w) in [(a, wa), (b, wb), (c, wc)] {
        nudge(&mut x[k], tx * f * w);
        nudge(&mut x[k + 1], ty * f * w);
        nudge(&mut x[k + 2], tz * f * w);
    }
}
