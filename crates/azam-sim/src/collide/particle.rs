//! Particle-sphere cloth contacts: every particle keeps a radius from particles of other pieces and
//! from particles of its own piece that are far away in the pattern (so folds cannot pass through
//! themselves). Detection runs through a spatial hash every few substeps and records the contact
//! pairs; the pairs are re-resolved with friction on the other substeps.

use azam_avatar::SdfGrid;

use super::layers::{LAYER_GAP, layer_contact};
use crate::numeric::{hypot3, nudge, to_int32};
use crate::params::SimParams;
use crate::state::{Particles, PieceRecord};

/// Pairs closer than this factor of their contact distance are recorded as contacts.
const CONTACT_BAND_SQ: f64 = 1.69;
/// Layer cells are this factor larger than the layer gap.
const LAYER_CELL_FACTOR: f64 = 1.3;
/// Own-piece pairs closer than this (squared, in units of the contact distance) in the flat pattern
/// are neighbours and never collide.
const NEIGHBOUR_DISTANCE_SQ: f64 = 16.0;
/// Pairs closer than this (squared, mm²) are treated as coincident and skipped.
const COINCIDENT_SQ: f64 = 1e-12;
/// Slips shorter than this (mm) get no friction.
const MIN_SLIP: f64 = 1e-9;
/// Initial lowest layer when scanning (any real layer is higher).
const LAYER_SCAN_MIN: i8 = 99;
/// Initial highest layer when scanning.
const LAYER_SCAN_MAX: i8 = -99;

/// Contact pairs found by the last detection, plus the hash buffers reused between detections.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ParticleContacts {
    /// First particle of each contact.
    pub q: Vec<u32>,
    /// Second particle of each contact.
    pub o: Vec<u32>,
    cell_start: Vec<u32>,
    entries: Vec<u32>,
}

/// Hash bucket of a cell for a table of `ts` buckets.
fn bucket(ix: f64, iy: f64, iz: f64, ts: usize) -> usize {
    let h = to_int32(ix * 92_837_111.0) ^ to_int32(iy * 689_287_499.0) ^ to_int32(iz * 283_923_481.0);
    (u64::from(h.unsigned_abs()) % ts as u64) as usize
}

/// Contact distance of two particles: the mean radius, or the layer gap for different layers.
fn contact_distance(p: &Particles, q: usize, o: usize, layered: bool) -> f64 {
    let mean = (f64::from(p.radius[q]) + f64::from(p.radius[o])) * 0.5;
    if layered {
        mean.max(LAYER_GAP * f64::from(p.spacing[q].max(p.spacing[o])))
    } else {
        mean
    }
}

/// Removes the tangential slip of a touching pair (relative to the substep start) by the friction
/// fraction. `n` is the contact normal from `q` to `o`, `dn` the penetration.
fn pair_friction(p: &mut Particles, params: &SimParams, q: usize, o: usize, n: [f64; 3], dn: f64, wsum: f64) {
    let (k, ko) = (q * 3, o * 3);
    let mu = (f64::from(p.friction[q]) + f64::from(p.friction[o])) * 0.5;
    let rel = |c: usize| {
        (f64::from(p.x[ko + c]) - f64::from(p.prev[ko + c]))
            - (f64::from(p.x[k + c]) - f64::from(p.prev[k + c]))
    };
    let (mut rx, mut ry, mut rz) = (rel(0), rel(1), rel(2));
    let rn = rx * n[0] + ry * n[1] + rz * n[2];
    rx -= rn * n[0];
    ry -= rn * n[1];
    rz -= rn * n[2];
    let tl = hypot3(rx, ry, rz);
    if tl < MIN_SLIP {
        return;
    }
    let f = params.friction_fraction(mu, dn, tl) / wsum;
    let (wq, wo) = (f64::from(p.inv_mass[q]), f64::from(p.inv_mass[o]));
    nudge(&mut p.x[k], rx * f * wq);
    nudge(&mut p.x[k + 1], ry * f * wq);
    nudge(&mut p.x[k + 2], rz * f * wq);
    nudge(&mut p.x[ko], -(rx * f * wo));
    nudge(&mut p.x[ko + 1], -(ry * f * wo));
    nudge(&mut p.x[ko + 2], -(rz * f * wo));
}

/// Rebuilds the particle hash (counting sort of the particles into `ts` buckets of cells of size
/// `cs`); within a bucket, particles come in descending order.
fn rebuild_hash(contacts: &mut ParticleContacts, x: &[f32], n: usize, cs: f64, ts: usize) {
    let cell = |v: f32| (f64::from(v) / cs).floor();
    let bucket_of = |q: usize| bucket(cell(x[3 * q]), cell(x[3 * q + 1]), cell(x[3 * q + 2]), ts);
    let starts = &mut contacts.cell_start;
    starts.clear();
    starts.resize(ts + 1, 0);
    contacts.entries.clear();
    contacts.entries.resize(n, 0);
    for q in 0..n {
        starts[bucket_of(q)] += 1;
    }
    let mut start = 0;
    for v in starts.iter_mut().take(ts) {
        start += *v;
        *v = start;
    }
    starts[ts] = start;
    for q in 0..n {
        let h = bucket_of(q);
        starts[h] -= 1;
        contacts.entries[starts[h] as usize] = q as u32;
    }
}

/// What detection knows about the particle whose neighbours are being visited.
struct Visitor<'a> {
    q: usize,
    /// Flat pattern positions of the particle's piece, and its index there.
    xy: &'a [f64],
    local: usize,
    offset: usize,
    multi_layer: bool,
}

/// Handles one candidate pair during detection: records it if it is in the contact band and
/// resolves it if it overlaps.
fn visit_pair(
    p: &mut Particles,
    contacts: &mut ParticleContacts,
    grid: &SdfGrid,
    params: &SimParams,
    g: &mut [f64; 3],
    v: &Visitor<'_>,
    o: usize,
) {
    let q = v.q;
    if o <= q || !p.active[o] || (p.sewn[q] && p.sewn[o]) {
        return;
    }
    let (k, ko) = (q * 3, o * 3);
    let dx = f64::from(p.x[ko]) - f64::from(p.x[k]);
    let dy = f64::from(p.x[ko + 1]) - f64::from(p.x[k + 1]);
    let dz = f64::from(p.x[ko + 2]) - f64::from(p.x[k + 2]);
    let layered = v.multi_layer && p.layer[q] != p.layer[o];
    let min_d = contact_distance(p, q, o, layered);
    let d2 = dx * dx + dy * dy + dz * dz;
    if d2 < COINCIDENT_SQ {
        return;
    }
    let other_piece = p.piece[o] != p.piece[q];
    if layered {
        if d2 < CONTACT_BAND_SQ * min_d * min_d {
            contacts.q.push(q as u32);
            contacts.o.push(o as u32);
            layer_contact(p, grid, q, o, g, min_d);
        }
        return;
    }
    if d2 >= min_d * min_d {
        if other_piece && d2 < CONTACT_BAND_SQ * min_d * min_d {
            contacts.q.push(q as u32);
            contacts.o.push(o as u32);
        }
        return;
    }
    if other_piece {
        contacts.q.push(q as u32);
        contacts.o.push(o as u32);
    } else {
        let lo = o - v.offset;
        let ex = v.xy[2 * lo] - v.xy[2 * v.local];
        let ey = v.xy[2 * lo + 1] - v.xy[2 * v.local + 1];
        if ex * ex + ey * ey < NEIGHBOUR_DISTANCE_SQ * min_d * min_d {
            return;
        }
    }
    let (wq, wo) = (f64::from(p.inv_mass[q]), f64::from(p.inv_mass[o]));
    let wsum = wq + wo;
    if wsum == 0.0 {
        return;
    }
    let d = d2.sqrt();
    let corr = (min_d - d) / d / wsum;
    nudge(&mut p.x[k], -(dx * corr * wq));
    nudge(&mut p.x[k + 1], -(dy * corr * wq));
    nudge(&mut p.x[k + 2], -(dz * corr * wq));
    nudge(&mut p.x[ko], dx * corr * wo);
    nudge(&mut p.x[ko + 1], dy * corr * wo);
    nudge(&mut p.x[ko + 2], dz * corr * wo);
    if other_piece {
        pair_friction(p, params, q, o, [dx / d, dy / d, dz / d], min_d - d, wsum);
    }
}

/// Detects particle contacts, resolves the overlapping ones and records every pair in the contact
/// band for the following substeps.
pub(crate) fn detect(
    p: &mut Particles,
    contacts: &mut ParticleContacts,
    records: &[PieceRecord],
    grid: &SdfGrid,
    params: &SimParams,
) {
    let n = p.len();
    let (mut rmax, mut pmax) = (0.0_f64, 0.0_f64);
    let (mut lmin, mut lmax) = (LAYER_SCAN_MIN, LAYER_SCAN_MAX);
    for q in 0..n {
        rmax = rmax.max(f64::from(p.radius[q]));
        pmax = pmax.max(f64::from(p.spacing[q]));
        lmin = lmin.min(p.layer[q]);
        lmax = lmax.max(p.layer[q]);
    }
    if rmax == 0.0 || n < 2 {
        return;
    }
    let multi_layer = lmax > lmin;
    let cs = (rmax * 2.0).max(if multi_layer {
        LAYER_GAP * pmax * LAYER_CELL_FACTOR
    } else {
        0.0
    });
    let ts = 2 * n + 1;
    rebuild_hash(contacts, &p.x, n, cs, ts);
    contacts.q.clear();
    contacts.o.clear();
    let mut g = [0.0; 3];
    for q in 0..n {
        if !p.active[q] {
            continue;
        }
        let cell = |v: f32| (f64::from(v) / cs).floor();
        let (ix, iy, iz) = (cell(p.x[q * 3]), cell(p.x[q * 3 + 1]), cell(p.x[q * 3 + 2]));
        if !(ix.is_finite() && iy.is_finite() && iz.is_finite()) {
            continue;
        }
        let rq = &records[p.piece[q] as usize];
        let visitor = Visitor {
            q,
            xy: &rq.mesh.xy,
            local: q - rq.offset,
            offset: rq.offset,
            multi_layer,
        };
        for a in [ix - 1.0, ix, ix + 1.0] {
            for b in [iy - 1.0, iy, iy + 1.0] {
                for c in [iz - 1.0, iz, iz + 1.0] {
                    let h = bucket(a, b, c, ts);
                    let (e0, e1) = (contacts.cell_start[h], contacts.cell_start[h + 1]);
                    for e in e0..e1 {
                        let o = contacts.entries[e as usize] as usize;
                        visit_pair(p, contacts, grid, params, &mut g, &visitor, o);
                    }
                }
            }
        }
    }
}

/// Re-resolves the recorded contact pairs: keeps them apart and applies friction while touching.
pub(crate) fn resolve(p: &mut Particles, contacts: &ParticleContacts, grid: &SdfGrid, params: &SimParams) {
    let mut g = [0.0; 3];
    for (&q, &o) in contacts.q.iter().zip(&contacts.o) {
        let (q, o) = (q as usize, o as usize);
        let (k, ko) = (q * 3, o * 3);
        let (wq, wo) = (f64::from(p.inv_mass[q]), f64::from(p.inv_mass[o]));
        let wsum = wq + wo;
        if wsum == 0.0 {
            continue;
        }
        let dx = f64::from(p.x[ko]) - f64::from(p.x[k]);
        let dy = f64::from(p.x[ko + 1]) - f64::from(p.x[k + 1]);
        let dz = f64::from(p.x[ko + 2]) - f64::from(p.x[k + 2]);
        let layered = p.layer[q] != p.layer[o];
        let min_d = contact_distance(p, q, o, layered);
        let d2 = dx * dx + dy * dy + dz * dz;
        if d2 >= CONTACT_BAND_SQ * min_d * min_d || d2 < COINCIDENT_SQ {
            continue;
        }
        let d = d2.sqrt();
        let (nx, ny, nz) = (dx / d, dy / d, dz / d);
        if layered {
            layer_contact(p, grid, q, o, &mut g, min_d);
        } else if d < min_d {
            let corr = (min_d - d) / wsum;
            nudge(&mut p.x[k], -(nx * corr * wq));
            nudge(&mut p.x[k + 1], -(ny * corr * wq));
            nudge(&mut p.x[k + 2], -(nz * corr * wq));
            nudge(&mut p.x[ko], nx * corr * wo);
            nudge(&mut p.x[ko + 1], ny * corr * wo);
            nudge(&mut p.x[ko + 2], nz * corr * wo);
        }
        if d >= min_d {
            continue;
        }
        pair_friction(p, params, q, o, [nx, ny, nz], min_d - d, wsum);
    }
}
