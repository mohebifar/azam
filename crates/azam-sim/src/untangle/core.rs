//! Finding cloth that has passed through cloth.
//!
//! Where two pieces cross, the solver's contacts hold each particle on whichever side it is, so
//! the pieces look welded at a point. This finds mesh edges that pierce cloth triangles, splits
//! each piece along those edges and marks the smaller region (the part that poked through) with
//! the side it should go back to.

use std::collections::{HashMap, HashSet};

use azam_doc::PieceState;
use azam_geom::hypot;

use crate::numeric::{cell_hash, max3, min3};
use crate::state::Simulation;

/// Regions larger than this many particles are not treated as the poked-through part.
pub const DEFAULT_REGION_CAP: usize = 250;
/// Smallest hash cell, in mm.
const MIN_CELL_MM: f64 = 8.0;
/// Hash cells span this factor of the mean mesh edge.
const CELL_EDGE_FACTOR: f64 = 1.2;
/// Mean mesh edge assumed for a simulation without triangles, in mm.
const DEFAULT_EDGE_MM: f64 = 20.0;
/// Triangles spanning more hash cells than this are skipped.
const MAX_TRIANGLE_CELLS: f64 = 64.0;
/// Edges spanning more hash cells than this are skipped.
const MAX_EDGE_CELLS: f64 = 27.0;
/// Own-piece triangles within this many particle distances (squared: 2.2²) in the flat pattern
/// are neighbours of an edge and are not tested.
const NEIGHBOUR_SPACINGS_SQ: f64 = 4.84;
/// Crossings this close to a triangle edge (barycentric) are ignored.
const EDGE_TOLERANCE: f64 = 1e-4;
/// The poked-through region must be at most this fraction of the other region's size.
const SMALLER_REGION: f64 = 0.6;
/// Group key multiplier: one group per (edge piece, triangle piece).
const GROUP_STRIDE: u64 = 4096;
/// Tangle code bit: the particle should be on the triangle normal's side.
const WANT_SIDE_BIT: u32 = 1;
/// Tangle code shift of the crossed piece index (stored plus one).
const PIECE_SHIFT: u32 = 8;

/// Static data of one simulation for the untangler.
#[derive(Clone, Debug, PartialEq)]
pub struct UntangleSetup {
    /// Number of particles.
    pub particles: usize,
    /// Triangles of the active pieces: three corners and the piece index.
    pub tris: Vec<u32>,
    /// Unique mesh edges, two particles each.
    pub edges: Vec<u32>,
    /// Start of each particle's neighbours in `adj` (`particles + 1` entries).
    pub adj_start: Vec<u32>,
    /// Neighbouring particles.
    pub adj: Vec<u32>,
    /// Piece index of each particle.
    pub piece: Vec<u32>,
    /// Flat pattern position of each particle, `x, y`.
    pub xy: Vec<f32>,
    /// Particle distance of each particle's piece, mm.
    pub spacing: Vec<f32>,
    /// Whether each particle is sewn.
    pub sewn: Vec<bool>,
    /// Hash cell size, mm.
    pub cell: f64,
    /// Largest region treated as poked through, in particles.
    pub cap: usize,
}

/// What one untangler run found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UntangleResult {
    /// Per particle: 0, or bit 1 = wanted side (1 = along the crossed triangle's normal) and bits
    /// 8 and up = crossed piece index + 1.
    pub tangles: Vec<u32>,
    /// Edge-triangle crossings found.
    pub crossings: usize,
    /// Groups of crossings (one per pair of pieces).
    pub groups: usize,
    /// Tangle spots resolved.
    pub resolved: usize,
    /// Tangle spots whose poked-through side could not be told.
    pub ambiguous: usize,
    /// Particles marked.
    pub marked: usize,
}

impl UntangleSetup {
    /// Collects the untangler's static data from a simulation.
    #[must_use]
    pub fn new(sim: &Simulation) -> Self {
        let n = sim.particle_count();
        let mut tris = Vec::new();
        let mut edges = Vec::new();
        let mut seen = HashSet::new();
        let mut nb: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut xy = vec![0.0_f32; n * 2];
        let mut spacing = vec![0.0_f32; n];
        let mut piece = vec![0_u32; n];
        let (mut sum, mut count) = (0.0, 0_usize);
        for (ri, r) in sim.records.iter().enumerate() {
            for i in 0..r.count {
                let q = r.offset + i;
                xy[2 * q] = r.mesh.xy[2 * i] as f32;
                xy[2 * q + 1] = r.mesh.xy[2 * i + 1] as f32;
                spacing[q] = sim.particles.spacing[q];
                piece[q] = ri as u32;
            }
            if r.state == PieceState::Off {
                continue;
            }
            for t in r.mesh.tris.chunks_exact(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|v| v + r.offset as u32);
                tris.extend([a, b, c, ri as u32]);
                for (p0, p1) in [(a, b), (b, c), (c, a)] {
                    let (lo, hi) = (p0.min(p1), p0.max(p1));
                    if seen.insert((lo, hi)) {
                        edges.extend([lo, hi]);
                        nb[lo as usize].push(hi);
                        nb[hi as usize].push(lo);
                        let (l, h) = (lo as usize, hi as usize);
                        sum += hypot(
                            f64::from(xy[2 * l]) - f64::from(xy[2 * h]),
                            f64::from(xy[2 * l + 1]) - f64::from(xy[2 * h + 1]),
                        );
                        count += 1;
                    }
                }
            }
        }
        let mut adj_start = vec![0_u32; n + 1];
        for q in 0..n {
            adj_start[q + 1] = adj_start[q] + nb[q].len() as u32;
        }
        let adj = nb.into_iter().flatten().collect();
        let edge = if count > 0 {
            sum / count as f64
        } else {
            DEFAULT_EDGE_MM
        };
        Self {
            particles: n,
            tris,
            edges,
            adj_start,
            adj,
            piece,
            xy,
            spacing,
            sewn: sim.particles.sewn.clone(),
            cell: MIN_CELL_MM.max(edge * CELL_EDGE_FACTOR),
            cap: DEFAULT_REGION_CAP,
        }
    }

    /// Finds the crossings in positions `x` (`x, y, z` per particle) and marks the poked-through
    /// regions.
    #[must_use]
    pub fn run(&self, x: &[f32]) -> UntangleResult {
        let crossings = self.crossings(x);
        self.mark(&crossings)
    }

    fn corner(&self, t: usize, c: usize) -> usize {
        self.tris[4 * t + c] as usize
    }

    /// Edge-triangle crossings: `(u, v, triangle, u on the normal side)`.
    fn crossings(&self, x: &[f32]) -> Vec<(usize, usize, usize, bool)> {
        let inv = 1.0 / self.cell;
        let at = |q: usize, d: usize| f64::from(x[3 * q + d]);
        let n_tri = self.tris.len() / 4;
        let mut cells: HashMap<i32, Vec<u32>> = HashMap::new();
        for t in 0..n_tri {
            let (a, b, c) = (self.corner(t, 0), self.corner(t, 1), self.corner(t, 2));
            let lo = |d: usize| (min3(at(a, d), at(b, d), at(c, d)) * inv).floor();
            let hi = |d: usize| (max3(at(a, d), at(b, d), at(c, d)) * inv).floor();
            let (x0, x1, y0, y1, z0, z1) = (lo(0), hi(0), lo(1), hi(1), lo(2), hi(2));
            let span = (x1 - x0 + 1.0) * (y1 - y0 + 1.0) * (z1 - z0 + 1.0);
            if span.is_nan() || span > MAX_TRIANGLE_CELLS {
                continue;
            }
            for_cells([x0, y0, z0], [x1, y1, z1], |i, j, k| {
                cells.entry(cell_hash(i, j, k)).or_default().push(t as u32);
            });
        }
        let mut stamp = vec![usize::MAX; n_tri];
        let mut out = Vec::new();
        for e in 0..self.edges.len() / 2 {
            let (u, v) = (self.edges[2 * e] as usize, self.edges[2 * e + 1] as usize);
            let (ux, uy, uz) = (at(u, 0), at(u, 1), at(u, 2));
            let (vx, vy, vz) = (at(v, 0), at(v, 1), at(v, 2));
            let lo = [
                (ux.min(vx) * inv).floor(),
                (uy.min(vy) * inv).floor(),
                (uz.min(vz) * inv).floor(),
            ];
            let hi = [
                (ux.max(vx) * inv).floor(),
                (uy.max(vy) * inv).floor(),
                (uz.max(vz) * inv).floor(),
            ];
            let span = (hi[0] - lo[0] + 1.0) * (hi[1] - lo[1] + 1.0) * (hi[2] - lo[2] + 1.0);
            if span.is_nan() || span > MAX_EDGE_CELLS {
                continue;
            }
            let pu = self.piece[u];
            let su = f64::from(self.spacing[u]);
            let ex2 = NEIGHBOUR_SPACINGS_SQ * su * su;
            for_cells(lo, hi, |i, j, k| {
                let Some(list) = cells.get(&cell_hash(i, j, k)) else {
                    return;
                };
                for &t in list {
                    let t = t as usize;
                    if stamp[t] == e {
                        continue;
                    }
                    stamp[t] = e;
                    if let Some(side) = self.pierces(x, (u, v), t, pu, ex2) {
                        out.push((u, v, t, side));
                    }
                }
            });
        }
        out
    }

    /// Whether edge `(u, v)` pierces triangle `t` away from its edges; `Some(u is on the normal
    /// side)` if so.
    fn pierces(&self, x: &[f32], (u, v): (usize, usize), t: usize, pu: u32, ex2: f64) -> Option<bool> {
        let at = |q: usize, d: usize| f64::from(x[3 * q + d]);
        let (a, b, c) = (self.corner(t, 0), self.corner(t, 1), self.corner(t, 2));
        if a == u || b == u || c == u || a == v || b == v || c == v {
            return None;
        }
        if self.tris[4 * t + 3] == pu {
            let fx = |q: usize, d: usize| f64::from(self.xy[2 * q + d]);
            for o in [a, b, c] {
                let (dx, dy) = (fx(o, 0) - fx(u, 0), fx(o, 1) - fx(u, 1));
                if dx * dx + dy * dy < ex2 {
                    return None;
                }
            }
        }
        if (self.sewn[u] || self.sewn[v]) && (self.sewn[a] || self.sewn[b] || self.sewn[c]) {
            return None;
        }
        let (ax, ay, az) = (at(a, 0), at(a, 1), at(a, 2));
        let (e1x, e1y, e1z) = (at(b, 0) - ax, at(b, 1) - ay, at(b, 2) - az);
        let (e2x, e2y, e2z) = (at(c, 0) - ax, at(c, 1) - ay, at(c, 2) - az);
        let (nx, ny, nz) = (
            e1y * e2z - e1z * e2y,
            e1z * e2x - e1x * e2z,
            e1x * e2y - e1y * e2x,
        );
        let (ux, uy, uz) = (at(u, 0), at(u, 1), at(u, 2));
        let (vx, vy, vz) = (at(v, 0), at(v, 1), at(v, 2));
        let du = (ux - ax) * nx + (uy - ay) * ny + (uz - az) * nz;
        let dv = (vx - ax) * nx + (vy - ay) * ny + (vz - az) * nz;
        if du == 0.0 || dv == 0.0 || (du > 0.0) == (dv > 0.0) {
            return None;
        }
        let s = du / (du - dv);
        let (px, py, pz) = (
            ux + (vx - ux) * s - ax,
            uy + (vy - uy) * s - ay,
            uz + (vz - uz) * s - az,
        );
        let d00 = e1x * e1x + e1y * e1y + e1z * e1z;
        let d01 = e1x * e2x + e1y * e2y + e1z * e2z;
        let d11 = e2x * e2x + e2y * e2y + e2z * e2z;
        let d20 = px * e1x + py * e1y + pz * e1z;
        let d21 = px * e2x + py * e2y + pz * e2z;
        let den = d00 * d11 - d01 * d01;
        if den.is_nan() || den <= 0.0 {
            return None;
        }
        let bv = (d11 * d20 - d01 * d21) / den;
        let bw = (d00 * d21 - d01 * d20) / den;
        if bv < EDGE_TOLERANCE || bw < EDGE_TOLERANCE || bv + bw > 1.0 - EDGE_TOLERANCE {
            return None;
        }
        Some(du > 0.0)
    }

    /// Groups the crossings, splits pieces along them and marks the poked-through regions.
    fn mark(&self, cr: &[(usize, usize, usize, bool)]) -> UntangleResult {
        let n = self.particles;
        let mut groups: Ordered<u64> = Ordered::default();
        for (i, &(u, _, t, _)) in cr.iter().enumerate() {
            let key = u64::from(self.piece[u]) * GROUP_STRIDE + u64::from(self.tris[4 * t + 3]);
            groups.push(key, i);
        }
        let mut res = UntangleResult {
            tangles: vec![0; n],
            crossings: cr.len(),
            groups: groups.keys.len(),
            ..UntangleResult::default()
        };
        let mut best = vec![usize::MAX; n];
        let mut flood = Flood {
            setup: self,
            visit: vec![0; n],
            pass: 0,
        };
        for (gk, gl) in groups.keys.iter().zip(&groups.lists) {
            let crossed = (gk % GROUP_STRIDE) as u32;
            let cut: HashSet<(usize, usize)> = gl
                .iter()
                .map(|&i| (cr[i].0.min(cr[i].1), cr[i].0.max(cr[i].1)))
                .collect();
            for cl in clusters(cr, gl) {
                let (mut seed_p, mut seed_n) = (Vec::new(), Vec::new());
                for &i in &cl {
                    let (u, v, _, u_pos) = cr[i];
                    if u_pos {
                        seed_p.push(u);
                        seed_n.push(v);
                    } else {
                        seed_n.push(u);
                        seed_p.push(v);
                    }
                }
                let rp = flood.fill(&seed_p, &seed_n, &cut);
                let rn = flood.fill(&seed_n, &seed_p, &cut);
                if rp.merged || rn.merged || (rp.capped && rn.capped) {
                    res.ambiguous += 1;
                    continue;
                }
                let (poked, want) = if rp.capped {
                    (rn, 1)
                } else if rn.capped || rp.set.len() as f64 <= SMALLER_REGION * rn.set.len() as f64 {
                    (rp, 0)
                } else if rn.set.len() as f64 <= SMALLER_REGION * rp.set.len() as f64 {
                    (rn, 1)
                } else {
                    res.ambiguous += 1;
                    continue;
                };
                res.resolved += 1;
                let code = ((crossed + 1) << PIECE_SHIFT) | ((want * WANT_SIDE_BIT) << 1);
                let sz = poked.set.len();
                for &q in &poked.set {
                    if sz < best[q] {
                        best[q] = sz;
                        if res.tangles[q] == 0 {
                            res.marked += 1;
                        }
                        res.tangles[q] = code;
                    }
                }
            }
        }
        res
    }
}

/// Calls `f` for every integer cell from `lo` to `hi` (inclusive), cells given as floats.
fn for_cells(lo: [f64; 3], hi: [f64; 3], mut f: impl FnMut(f64, f64, f64)) {
    let mut i = lo[0];
    while i <= hi[0] {
        let mut j = lo[1];
        while j <= hi[1] {
            let mut k = lo[2];
            while k <= hi[2] {
                f(i, j, k);
                k += 1.0;
            }
            j += 1.0;
        }
        i += 1.0;
    }
}

/// Lists keyed in first-insertion order.
struct Ordered<K> {
    keys: Vec<K>,
    lists: Vec<Vec<usize>>,
    index: HashMap<K, usize>,
}

impl<K> Default for Ordered<K> {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            lists: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<K: std::hash::Hash + Eq + Copy> Ordered<K> {
    fn push(&mut self, key: K, value: usize) {
        let i = *self.index.entry(key).or_insert_with(|| {
            self.keys.push(key);
            self.lists.push(Vec::new());
            self.keys.len() - 1
        });
        self.lists[i].push(value);
    }
}

/// Splits a group's crossings into tangle spots: crossing edges that share a particle belong to
/// the same intersection curve. Spots come in order of their first crossing's root.
fn clusters(cr: &[(usize, usize, usize, bool)], gl: &[usize]) -> Vec<Vec<usize>> {
    let mut par: Vec<usize> = (0..gl.len()).collect();
    let find = |par: &mut Vec<usize>, mut k: usize| {
        while par[k] != k {
            par[k] = par[par[k]];
            k = par[k];
        }
        k
    };
    let mut by_vertex: HashMap<usize, usize> = HashMap::new();
    for (k, &i) in gl.iter().enumerate() {
        for x in [cr[i].0, cr[i].1] {
            match by_vertex.get(&x) {
                None => {
                    by_vertex.insert(x, k);
                }
                Some(&o) => {
                    let (ra, rb) = (find(&mut par, o), find(&mut par, k));
                    if ra != rb {
                        par[ra] = rb;
                    }
                }
            }
        }
    }
    let mut out: Ordered<usize> = Ordered::default();
    for (k, &i) in gl.iter().enumerate() {
        let r = find(&mut par, k);
        out.push(r, i);
    }
    out.lists
}

/// A flood fill's region.
struct Region {
    set: Vec<usize>,
    capped: bool,
    merged: bool,
}

/// Breadth-first flood fills over the mesh that do not cross cut edges.
struct Flood<'a> {
    setup: &'a UntangleSetup,
    visit: Vec<u32>,
    pass: u32,
}

impl Flood<'_> {
    /// Fills from `seeds` without entering `other` (reaching it marks the region merged) or
    /// crossing `cut`; stops once the region exceeds the cap.
    fn fill(&mut self, seeds: &[usize], other: &[usize], cut: &HashSet<(usize, usize)>) -> Region {
        self.pass += 1;
        let id = self.pass;
        let s = self.setup;
        let other: HashSet<usize> = other.iter().copied().collect();
        let mut queue = Vec::new();
        let mut set = Vec::new();
        let mut merged = false;
        for &x in seeds {
            if self.visit[x] != id && !other.contains(&x) {
                self.visit[x] = id;
                queue.push(x);
            }
        }
        let mut h = 0;
        while h < queue.len() {
            let x = queue[h];
            h += 1;
            set.push(x);
            if set.len() > s.cap {
                return Region {
                    set,
                    capped: true,
                    merged,
                };
            }
            for k in s.adj_start[x] as usize..s.adj_start[x + 1] as usize {
                let w = s.adj[k] as usize;
                if self.visit[w] == id || cut.contains(&(x.min(w), x.max(w))) {
                    continue;
                }
                if other.contains(&w) {
                    merged = true;
                    continue;
                }
                self.visit[w] = id;
                queue.push(w);
            }
        }
        Region {
            set,
            capped: false,
            merged,
        }
    }
}
