//! Gauss–Seidel projection of the distance-like constraints: stretch (XPBD), sewing, bending and
//! fasteners. Constraints are solved one after another in array order.

use crate::numeric::nudge;
use crate::state::{BendConstraints, Fasteners, Particles, SewingConstraints, StretchConstraints};

/// Lengths below this (mm) give no direction and are skipped.
const MIN_LENGTH: f64 = 1e-9;
/// Seam gaps below this (mm) are closed.
const MIN_SEAM_GAP: f64 = 1e-6;
/// Seams closer than this (mm) are pulled shut in one go; wider seams close at the sewing speed.
const SEAM_SNAP_MM: f64 = 10.0;
/// Share of a wide seam's gap always allowed per iteration, on top of the sewing speed.
const SEAM_GAP_SHARE: f64 = 0.02;
/// Gap a fastened button keeps from its buttonhole, in mm.
const FASTENER_GAP_MM: f64 = 2.0;

/// Moves particles `i` and `j` apart along `d` by `dl` (weighted by their inverse masses).
#[inline]
fn apply_pair(x: &mut [f32], i: usize, j: usize, wi: f64, wj: f64, dl: f64, d: [f64; 3]) {
    let (a, b) = (i * 3, j * 3);
    nudge(&mut x[a], -(wi * dl * d[0]));
    nudge(&mut x[a + 1], -(wi * dl * d[1]));
    nudge(&mut x[a + 2], -(wi * dl * d[2]));
    nudge(&mut x[b], wj * dl * d[0]);
    nudge(&mut x[b + 1], wj * dl * d[1]);
    nudge(&mut x[b + 2], wj * dl * d[2]);
}

#[inline]
fn delta(x: &[f32], i: usize, j: usize) -> [f64; 3] {
    let (a, b) = (i * 3, j * 3);
    [
        f64::from(x[b]) - f64::from(x[a]),
        f64::from(x[b + 1]) - f64::from(x[a + 1]),
        f64::from(x[b + 2]) - f64::from(x[a + 2]),
    ]
}

/// One XPBD pass over the stretch constraints. `ih2` is `1 / h²`.
pub(crate) fn stretch(p: &mut Particles, s: &StretchConstraints, ih2: f64) {
    for c in 0..s.len() {
        let (i, j) = (s.i[c] as usize, s.j[c] as usize);
        let (wi, wj) = (f64::from(p.inv_mass[i]), f64::from(p.inv_mass[j]));
        let ws = wi + wj;
        if ws == 0.0 {
            continue;
        }
        let d = delta(&p.x, i, j);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < MIN_LENGTH {
            continue;
        }
        let cval = len - f64::from(s.rest[c]);
        let compliance = f64::from(s.compliance[c])
            * if cval < 0.0 {
                f64::from(s.compression[c])
            } else {
                1.0
            };
        let al = compliance * ih2;
        let dl = -cval / (ws + al) / len;
        apply_pair(&mut p.x, i, j, wi, wj, dl, d);
    }
}

/// One pass over the sewing constraints. Closed seams are held hard; open ones close by at most
/// `max_sew` mm (plus a share of the gap) per pass.
pub(crate) fn sew(p: &mut Particles, sw: &SewingConstraints, max_sew: f64) {
    for c in 0..sw.len() {
        let (ia, i0, i1) = (sw.a[c] as usize, sw.b0[c] as usize, sw.b1[c] as usize);
        let u = f64::from(sw.u[c]);
        let u0 = 1.0 - u;
        let (wa, wb0, wb1) = (
            f64::from(p.inv_mass[ia]),
            f64::from(p.inv_mass[i0]),
            f64::from(p.inv_mass[i1]),
        );
        let den = wa + u0 * u0 * wb0 + u * u * wb1;
        if den == 0.0 {
            continue;
        }
        let x = &mut p.x;
        let (a, b0, b1) = (ia * 3, i0 * 3, i1 * 3);
        let t = |o: usize| u0 * f64::from(x[b0 + o]) + u * f64::from(x[b1 + o]);
        let (dx, dy, dz) = (
            t(0) - f64::from(x[a]),
            t(1) - f64::from(x[a + 1]),
            t(2) - f64::from(x[a + 2]),
        );
        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        if len < MIN_SEAM_GAP {
            continue;
        }
        let cval = if len <= SEAM_SNAP_MM {
            len
        } else {
            len.min(SEAM_SNAP_MM.max(max_sew + len * SEAM_GAP_SHARE))
        };
        let s2 = cval / len / den;
        nudge(&mut x[a], wa * s2 * dx);
        nudge(&mut x[a + 1], wa * s2 * dy);
        nudge(&mut x[a + 2], wa * s2 * dz);
        nudge(&mut x[b0], -(wb0 * u0 * s2 * dx));
        nudge(&mut x[b0 + 1], -(wb0 * u0 * s2 * dy));
        nudge(&mut x[b0 + 2], -(wb0 * u0 * s2 * dz));
        nudge(&mut x[b1], -(wb1 * u * s2 * dx));
        nudge(&mut x[b1 + 1], -(wb1 * u * s2 * dy));
        nudge(&mut x[b1 + 2], -(wb1 * u * s2 * dz));
    }
}

/// One pass over the bending constraints: plain ones only resist folding (the distance between
/// the opposite vertices may grow freely); fold lines hold their distance both ways.
pub(crate) fn bend(p: &mut Particles, b: &BendConstraints) {
    for c in 0..b.len() {
        let (i, j) = (b.i[c] as usize, b.j[c] as usize);
        let (wi, wj) = (f64::from(p.inv_mass[i]), f64::from(p.inv_mass[j]));
        let ws = wi + wj;
        if ws == 0.0 {
            continue;
        }
        let d = delta(&p.x, i, j);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < MIN_LENGTH {
            continue;
        }
        let cval = len - f64::from(b.rest[c]);
        let rb = f64::from(b.compliance[c]);
        if rb >= 0.0 && cval > 0.0 {
            continue;
        }
        let dl = -cval / (ws + rb.abs() * ws) / len;
        apply_pair(&mut p.x, i, j, wi, wj, dl, d);
    }
}

/// One pass over the fasteners: buttons are pulled towards their holes by at most `max_sew` mm.
pub(crate) fn fasten(p: &mut Particles, f: &Fasteners, max_sew: f64) {
    for c in 0..f.a.len() {
        let (i, j) = (f.a[c] as usize, f.b[c] as usize);
        let (wi, wj) = (f64::from(p.inv_mass[i]), f64::from(p.inv_mass[j]));
        let ws = wi + wj;
        if ws == 0.0 {
            continue;
        }
        let d = delta(&p.x, i, j);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < FASTENER_GAP_MM {
            continue;
        }
        let cval = (len - FASTENER_GAP_MM).min(max_sew);
        let k = cval / len / ws;
        apply_pair(&mut p.x, i, j, wi, wj, -k, d);
    }
}
