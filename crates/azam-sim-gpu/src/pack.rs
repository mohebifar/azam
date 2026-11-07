//! Packing a [`Simulation`] into the GPU's buffer layouts (pure CPU work, testable without a GPU).
//!
//! Particles become `X` (position, inverse mass), `P` (substep start), `V` (velocity), `PP`
//! (damping, margin, friction, thickness) and `PD` (piece, layer, flags, sewn-piece mask; flat
//! pattern x and y, particle distance, pressure). Constraints become colour-ordered rows with a
//! dispatch descriptor per colour.

use azam_avatar::{BodyCollisionMesh, BodyVelocity, SdfGrid};
use azam_sim::{ColorBatches, Grab, Pins, Simulation, TriangleContacts};

use crate::layout::{
    CONTACTS_PER_PARTICLE, ConRow, DispatchDesc, FASTENER_GAP_MM, FrameParams, HASH_ENTRIES_PER_TRIANGLE,
    KIND_BEND, KIND_FASTENER, KIND_STRETCH, MAX_PIECES, MAX_SPEED_MM_S, MIN_CONTACT_CAP, MIN_HASH_ENTRIES,
    SKIN_CELL_MM, SKIN_HASH_BUCKETS, STATE_LEN, STATE_PIECE_ACCEL, STATE_PIECE_GRAVITY,
    TRIANGLE_HASH_BUCKETS,
};
use crate::options::GpuOptions;

/// Bit 0 of a particle's flags: its piece takes part in the simulation.
pub const ACTIVE_BIT: u32 = 1;
/// Hash cells span this factor of the mean mesh edge.
const CELL_EDGE_FACTOR: f64 = 1.1;
/// Triangle-hash cell limits, mm.
const CELL_RANGE_MM: (f64, f64) = (10.0, 60.0);
/// Largest normal correction per contact and substep, as a fraction of the mean edge.
const MAX_STEP_EDGE_FACTOR: f64 = 0.25;
/// Smallest cap on the normal correction, mm.
const MIN_MAX_STEP_MM: f64 = 2.0;
/// Contact search distance beyond the cloth thickness (before the slack), mm.
const SEARCH_MARGIN_MM: f32 = 2.0;
/// Extra gap between layers as a fraction of the coarser particle distance.
const LAYER_GAP_FACTOR: f32 = 0.3;
/// Particle distance assumed when none is known, mm.
const DEFAULT_SPACING_MM: f32 = 20.0;

/// Colour batches of one constraint buffer: per batch, its row count and its dispatch descriptor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batches(pub Vec<(u32, u32)>);

/// Every constraint buffer's colour batches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    /// Stretch rows of `CON`.
    pub stretch: Batches,
    /// Bending rows of `CON`.
    pub bend: Batches,
    /// Fastener rows of `CON`.
    pub fasteners: Batches,
    /// Rows of the colour-ordered sewing buffer.
    pub sewing: Batches,
}

/// Index of the dispatch descriptor whose flag is 0 (the plain apply pass).
pub const DESC_APPLY: u32 = 0;
/// Index of the dispatch descriptor whose flag is 1 (apply that also moves the substep start).
pub const DESC_APPLY_SHIFT: u32 = 1;

/// The static skin-vertex hash: per vertex position and normal (`x, y, z, 0` each), bucket heads
/// and next links (−1 ends a list).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkinHash {
    /// Positions and normals, eight values per vertex.
    pub rows: Vec<[f32; 8]>,
    /// First vertex of every bucket.
    pub head: Vec<i32>,
    /// Next vertex of every vertex's bucket.
    pub next: Vec<i32>,
}

impl SkinHash {
    /// Hashes the first `count` skin vertices of `positions` and `normals`.
    #[must_use]
    #[allow(clippy::cast_possible_wrap, reason = "vertex indices stay far below 2^31")]
    pub fn new(positions: &[f32], normals: &[f32], count: usize) -> Self {
        let mut rows = Vec::with_capacity(count);
        let mut head = vec![-1_i32; SKIN_HASH_BUCKETS as usize];
        let mut next = vec![-1_i32; count];
        let c = SKIN_CELL_MM;
        for (v, link) in next.iter_mut().enumerate() {
            let p = [positions[3 * v], positions[3 * v + 1], positions[3 * v + 2]];
            let n = [normals[3 * v], normals[3 * v + 1], normals[3 * v + 2]];
            rows.push([p[0], p[1], p[2], 0.0, n[0], n[1], n[2], 0.0]);
            let h = skin_bucket((p[0] / c).floor(), (p[1] / c).floor(), (p[2] / c).floor());
            *link = head[h];
            head[h] = v as i32;
        }
        Self { rows, head, next }
    }

    /// Number of skin vertices.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }
}

/// Bucket of integer cell `(i, j, k)` (given as floats) in a hash of `buckets` entries, as the
/// shaders compute it (32-bit wrapping products).
#[must_use]
pub fn bucket(i: f32, j: f32, k: f32, buckets: u32) -> usize {
    let h = (i as i32).wrapping_mul(73_856_093)
        ^ (j as i32).wrapping_mul(19_349_663)
        ^ (k as i32).wrapping_mul(83_492_791);
    (h as u32 & (buckets - 1)) as usize
}

/// Rows of the body velocity buffer: origin and cell, dimensions (as floats), then one velocity
/// (`x, y, z, 0`, mm/s) per value. Without a velocity field a single zero value.
#[must_use]
pub fn velocity_rows(velocity: Option<&BodyVelocity>) -> Vec<[f32; 4]> {
    let Some(v) = velocity else {
        return vec![[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 0.0], [0.0; 4]];
    };
    let f = &v.frame;
    let mut rows = Vec::with_capacity(2 + f.len());
    rows.push([
        f.origin.x as f32,
        f.origin.y as f32,
        f.origin.z as f32,
        f.cell as f32,
    ]);
    rows.push([f.dims[0] as f32, f.dims[1] as f32, f.dims[2] as f32, 0.0]);
    rows.extend(v.data.chunks_exact(3).map(|c| [c[0], c[1], c[2], 0.0]));
    rows
}

fn skin_bucket(i: f32, j: f32, k: f32) -> usize {
    bucket(i, j, k, SKIN_HASH_BUCKETS)
}

/// Everything uploaded when a simulation is attached.
#[derive(Clone, Debug)]
pub struct Packed {
    /// Positions and inverse masses.
    pub x: Vec<[f32; 4]>,
    /// Substep starts.
    pub prev: Vec<[f32; 4]>,
    /// Velocities.
    pub vel: Vec<[f32; 4]>,
    /// Damping, margin, friction, thickness.
    pub pp: Vec<[f32; 4]>,
    /// Piece, layer, flags, sewn-piece mask, then flat x and y, particle distance and pressure
    /// (as bits).
    pub pd: Vec<[u32; 8]>,
    /// Triangles of the active pieces: corners and piece.
    pub tri: Vec<[u32; 4]>,
    /// Distance constraints in colour order: stretch, bending, fasteners.
    pub con: Vec<ConRow>,
    /// Sewing constraints in natural order (for the seam gap): `a, b0, b1, u` (bits).
    pub sew_natural: Vec<[u32; 4]>,
    /// Sewing constraints in colour order.
    pub sew_colored: Vec<[u32; 4]>,
    /// Dispatch descriptors: the two apply flags, then one per colour batch.
    pub dispatch: Vec<DispatchDesc>,
    /// Body surface velocity rows (see [`velocity_rows`]).
    pub body_velocity: Vec<[f32; 4]>,
    /// Colour batches.
    pub groups: Groups,
    /// Skin-vertex hash (empty without a skin mesh).
    pub skin: SkinHash,
    /// Pins: position and particle (bits).
    pub pins: Vec<[f32; 4]>,
    /// Initial solver state (hold-back scales and accelerations).
    pub state: Vec<f32>,
    /// Frame parameters with every static field set.
    pub params: FrameParams,
}

/// Packs `sim` with the body grid `grid` and skin `skin`.
#[must_use]
pub fn pack(
    sim: &Simulation,
    grid: &SdfGrid,
    skin: Option<&BodyCollisionMesh<'_>>,
    options: &GpuOptions,
) -> Packed {
    let contacts = TriangleContacts::new(&sim.records, &sim.particles, &sim.sewing);
    let (x, prev, vel, pp) = particles(sim);
    let pd = particle_data(sim, &contacts.sewn_pieces, &vec![0; sim.particle_count()]);
    let tri: Vec<[u32; 4]> = contacts
        .tri_piece
        .iter()
        .enumerate()
        .map(|(t, &piece)| {
            [
                contacts.tris[3 * t],
                contacts.tris[3 * t + 1],
                contacts.tris[3 * t + 2],
                piece,
            ]
        })
        .collect();
    let mut dispatch = vec![
        DispatchDesc::default(),
        DispatchDesc {
            flag: 1,
            ..DispatchDesc::default()
        },
    ];
    let (con, stretch, bend, fasteners) = constraint_rows(sim, &mut dispatch);
    let sew_natural: Vec<[u32; 4]> = (0..sim.sewing.len()).map(|c| sew_row(sim, c)).collect();
    let sew_colored: Vec<[u32; 4]> = sim
        .coloring
        .sewing
        .order
        .iter()
        .map(|&c| sew_row(sim, c as usize))
        .collect();
    let sewing = batches(&sim.coloring.sewing, 0, &mut dispatch);
    let skin = skin.map_or_else(SkinHash::default, |s| {
        SkinHash::new(s.positions, s.normals, s.body_vertex_count)
    });
    let params = static_params(sim, grid, contacts.edge, tri.len(), skin.len(), options);
    Packed {
        x,
        prev,
        vel,
        pp,
        pd,
        tri,
        con,
        sew_natural,
        sew_colored,
        dispatch,
        groups: Groups {
            stretch,
            bend,
            fasteners,
            sewing,
        },
        skin,
        body_velocity: velocity_rows(None),
        pins: pins(&sim.pins),
        state: initial_state(sim),
        params,
    }
}

type Rows4 = Vec<[f32; 4]>;

/// `X`, `P`, `V` and `PP` of every particle.
#[must_use]
pub fn particles(sim: &Simulation) -> (Rows4, Rows4, Rows4, Rows4) {
    let p = &sim.particles;
    let n = p.len();
    let row = |a: &[f32], q: usize, w: f32| [a[3 * q], a[3 * q + 1], a[3 * q + 2], w];
    let x = (0..n).map(|q| row(&p.x, q, p.inv_mass[q])).collect();
    let prev = (0..n).map(|q| row(&p.prev, q, 0.0)).collect();
    let vel = (0..n).map(|q| row(&p.v, q, 0.0)).collect();
    let pp = (0..n)
        .map(|q| [p.damping[q], p.margin[q], p.friction[q], p.thickness[q]])
        .collect();
    (x, prev, vel, pp)
}

/// `X` and `P` rows of the CPU-side positions (for re-uploads after CPU edits).
#[must_use]
pub fn positions(sim: &Simulation) -> (Rows4, Rows4) {
    let (x, prev, _, _) = particles(sim);
    (x, prev)
}

/// `PD` of every particle, with untangle codes `tangles` (bit 0 is ignored).
#[must_use]
pub fn particle_data(sim: &Simulation, sewn_mask: &[u32], tangles: &[u32]) -> Vec<[u32; 8]> {
    let p = &sim.particles;
    let mut pd = vec![[0_u32; 8]; p.len()];
    for r in &sim.records {
        for i in 0..r.count {
            let q = r.offset + i;
            let flags = (u32::from(p.active[q]) * ACTIVE_BIT) | (tangles[q] & !ACTIVE_BIT);
            pd[q] = [
                p.piece[q],
                i32::from(p.layer[q]) as u32,
                flags,
                sewn_mask[q],
                (r.mesh.xy[2 * i] as f32).to_bits(),
                (r.mesh.xy[2 * i + 1] as f32).to_bits(),
                p.spacing[q].to_bits(),
                p.pressure[q].to_bits(),
            ];
        }
    }
    pd
}

/// Applies untangle codes to packed particle data in place (keeping each active flag); returns
/// the number of marked particles.
pub fn apply_tangles(pd: &mut [[u32; 8]], tangles: &[u32]) -> usize {
    let mut marked = 0;
    for (row, &t) in pd.iter_mut().zip(tangles) {
        row[2] = (row[2] & ACTIVE_BIT) | (t & !ACTIVE_BIT);
        if t != 0 {
            marked += 1;
        }
    }
    marked
}

fn sew_row(sim: &Simulation, c: usize) -> [u32; 4] {
    let s = &sim.sewing;
    [s.a[c], s.b0[c], s.b1[c], s.u[c].to_bits()]
}

/// Batches of `cb` over rows starting at `base`, each with a new dispatch descriptor.
fn batches(cb: &ColorBatches, base: u32, dispatch: &mut Vec<DispatchDesc>) -> Batches {
    Batches(
        cb.ranges
            .iter()
            .map(|&(start, count)| {
                dispatch.push(DispatchDesc {
                    offset: base + start,
                    count,
                    ..DispatchDesc::default()
                });
                (count, (dispatch.len() - 1) as u32)
            })
            .collect(),
    )
}

/// The `CON` rows (stretch, bending, fasteners, each in colour order) and their batches.
fn constraint_rows(
    sim: &Simulation,
    dispatch: &mut Vec<DispatchDesc>,
) -> (Vec<ConRow>, Batches, Batches, Batches) {
    let (s, b, f) = (&sim.stretch, &sim.bend, &sim.fasteners);
    let col = &sim.coloring;
    let mut con = Vec::with_capacity(s.len() + b.len() + f.a.len());
    con.extend(col.stretch.order.iter().map(|&c| {
        let c = c as usize;
        ConRow {
            i: s.i[c],
            j: s.j[c],
            rest: s.rest[c],
            compliance: s.compliance[c],
            compression: s.compression[c],
            kind: KIND_STRETCH,
            pad: [0; 2],
        }
    }));
    let stretch = batches(&col.stretch, 0, dispatch);
    let base = con.len() as u32;
    con.extend(col.bend.order.iter().map(|&c| {
        let c = c as usize;
        ConRow {
            i: b.i[c],
            j: b.j[c],
            rest: b.rest[c],
            compliance: b.compliance[c],
            compression: 1.0,
            kind: KIND_BEND,
            pad: [0; 2],
        }
    }));
    let bend = batches(&col.bend, base, dispatch);
    let base = con.len() as u32;
    con.extend(col.fasteners.order.iter().map(|&c| {
        let c = c as usize;
        ConRow {
            i: f.a[c],
            j: f.b[c],
            rest: FASTENER_GAP_MM,
            compliance: 0.0,
            compression: 0.0,
            kind: KIND_FASTENER,
            pad: [0; 2],
        }
    }));
    let fasteners = batches(&col.fasteners, base, dispatch);
    (con, stretch, bend, fasteners)
}

/// `PIN` rows: held position and particle index (as bits).
#[must_use]
pub fn pins(pins: &Pins) -> Vec<[f32; 4]> {
    pins.vertex
        .iter()
        .enumerate()
        .map(|(e, &q)| {
            [
                pins.pos[3 * e],
                pins.pos[3 * e + 1],
                pins.pos[3 * e + 2],
                f32::from_bits(q),
            ]
        })
        .collect()
}

/// `GR` rows of the first `cap` grabbed particles: offset and weight, then the particle index
/// (as bits).
#[must_use]
pub fn grab_rows(grab: &Grab, cap: usize) -> Vec<[f32; 4]> {
    let m = grab.vertices.len().min(cap);
    let mut rows = Vec::with_capacity(2 * m);
    for e in 0..m {
        let o = &grab.offsets[3 * e..3 * e + 3];
        rows.push([o[0] as f32, o[1] as f32, o[2] as f32, grab.weights[e] as f32]);
        rows.push([f32::from_bits(grab.vertices[e]), 0.0, 0.0, 0.0]);
    }
    rows
}

/// The solver state at upload: hold-back scale (−1 before the first frame), per-piece hold-back
/// and per-piece vertical acceleration.
#[must_use]
pub fn initial_state(sim: &Simulation) -> Vec<f32> {
    let mut st = vec![0.0_f32; STATE_LEN];
    st[0] = sim.gravity_scale.map_or(-1.0, |g| g as f32);
    for r in 0..MAX_PIECES {
        st[STATE_PIECE_GRAVITY + r] = sim.piece_gravity.get(r).copied().unwrap_or(1.0);
        st[STATE_PIECE_ACCEL + r] = -sim.params.gravity as f32;
    }
    st
}

/// Largest extra gap between layers: 0.3 of the largest particle distance when there are several
/// layers.
#[must_use]
pub fn layer_gap_max(sim: &Simulation) -> f32 {
    let p = &sim.particles;
    let lmin = p.layer.iter().copied().min().unwrap_or(0);
    let lmax = p.layer.iter().copied().max().unwrap_or(0);
    let pd_max = p.spacing.iter().copied().fold(0.0_f32, f32::max);
    if lmax > lmin {
        LAYER_GAP_FACTOR * if pd_max > 0.0 { pd_max } else { DEFAULT_SPACING_MM }
    } else {
        0.0
    }
}

/// Contact slots for `n` particles.
#[must_use]
pub fn contact_cap(n: usize) -> u32 {
    MIN_CONTACT_CAP.max(n as u32 * CONTACTS_PER_PARTICLE)
}

/// Triangle-hash entry slots for `n_tri` triangles.
#[must_use]
pub fn entry_cap(n_tri: usize) -> u32 {
    MIN_HASH_ENTRIES.max(n_tri as u32 * HASH_ENTRIES_PER_TRIANGLE)
}

/// Frame parameters with every field that does not change per frame.
#[must_use]
pub fn static_params(
    sim: &Simulation,
    grid: &SdfGrid,
    edge: f64,
    n_tri: usize,
    n_skin: usize,
    options: &GpuOptions,
) -> FrameParams {
    let sp = &sim.params;
    let n = sim.particle_count();
    let h0 = sp.cloth_thickness as f32;
    let hx_max = layer_gap_max(sim);
    let cell = edge * CELL_EDGE_FACTOR;
    let cell = if cell < CELL_RANGE_MM.0 {
        CELL_RANGE_MM.0
    } else if cell > CELL_RANGE_MM.1 {
        CELL_RANGE_MM.1
    } else {
        cell
    };
    let slide = sp.friction_model == azam_sim::FrictionModel::Slide;
    FrameParams {
        vmax: MAX_SPEED_MM_S,
        h0,
        max_step: (edge * MAX_STEP_EDGE_FACTOR).max(MIN_MAX_STEP_MM) as f32,
        grid_origin: [grid.origin.x as f32, grid.origin.y as f32, grid.origin.z as f32],
        grid_cell: grid.cell as f32,
        grid_dims: grid.dims.map(|d| d as u32),
        n: n as u32,
        n_tri: n_tri as u32,
        n_skin: n_skin as u32,
        hash_buckets: TRIANGLE_HASH_BUCKETS,
        contact_cap: contact_cap(n),
        cell: cell as f32,
        radius: h0 + SEARCH_MARGIN_MM + hx_max + options.contact_slack as f32,
        entry_cap: entry_cap(n_tri),
        n_sew: sim.sewing.len() as u32,
        skin_cell: SKIN_CELL_MM,
        skin_buckets: SKIN_HASH_BUCKETS,
        n_pin: sim.pins.vertex.len() as u32,
        has_pressure: u32::from(sim.particles.pressure.iter().any(|&a| a != 0.0)),
        n_pieces: sim.records.len() as u32,
        n_tri_hash: n_tri as u32,
        layer_order: u32::from(options.layer_order),
        hx_max,
        edge_contacts: u32::from(options.edge_contacts),
        friction_scale: sp.cc_fric as f32,
        friction_band: options.friction_band as f32,
        layer_repair: u32::from(sp.cc_repair),
        repair_margin: options.repair_margin as f32,
        friction_mode: u32::from(!slide),
        mu_cap: sp.cc_mu_cap as f32,
        ..FrameParams::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_hash_lists_every_vertex_in_its_bucket() {
        let positions = [0.0, 0.0, 0.0, 10.0, 5.0, 1.0, 30.0, 0.0, 0.0, -1.0, -1.0, -1.0];
        let normals = [0.0, 1.0, 0.0].repeat(4);
        let h = SkinHash::new(&positions, &normals, 4);
        assert_eq!(h.len(), 4);
        for v in 0..4 {
            let p = &positions[3 * v..3 * v + 3];
            let b = bucket(
                (p[0] / SKIN_CELL_MM).floor(),
                (p[1] / SKIN_CELL_MM).floor(),
                (p[2] / SKIN_CELL_MM).floor(),
                SKIN_HASH_BUCKETS,
            );
            let mut e = h.head[b];
            let mut found = false;
            while e >= 0 {
                found |= e as usize == v;
                e = h.next[e as usize];
            }
            assert!(found, "vertex {v} missing from bucket {b}");
        }
        // vertices 0 and 1 share cell (0, 0, 0): the later one heads the list
        let b0 = bucket(0.0, 0.0, 0.0, SKIN_HASH_BUCKETS);
        assert_eq!(h.head[b0], 1);
        assert_eq!(h.next[1], 0);
    }

    #[test]
    fn bucket_matches_wrapping_products() {
        assert_eq!(bucket(1.0, 0.0, 0.0, 1 << 16), 73_856_093 & 0xffff);
        assert_eq!(
            bucket(-1.0, 0.0, 0.0, 1 << 16),
            ((-73_856_093_i32) as u32 & 0xffff) as usize
        );
    }

    #[test]
    fn tangles_keep_the_active_bit() {
        let mut pd = vec![[0, 0, 1, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0, 0, 0]];
        let marked = apply_tangles(&mut pd, &[(3 << 8) | 2 | 1, 4 << 8]);
        assert_eq!(marked, 2);
        assert_eq!(pd[0][2], (3 << 8) | 2 | 1);
        assert_eq!(pd[1][2], 4 << 8);
        apply_tangles(&mut pd, &[0, 0]);
        assert_eq!(pd[0][2], 1);
        assert_eq!(pd[1][2], 0);
    }
}
