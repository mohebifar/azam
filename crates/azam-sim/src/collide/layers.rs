//! Contacts between particles of different layers: separated by a gap that covers the coarser
//! mesh, and an outer-layer particle that crossed to the inner side (along the body normal) is
//! moved back out.

use azam_avatar::SdfGrid;

use super::sample_into;
use crate::numeric::{hypot3, nudge};
use crate::state::Particles;

/// Gap kept between particles of different layers, as a fraction of the coarser particle distance.
pub(crate) const LAYER_GAP: f64 = 0.6;
/// Body gradients shorter than this give no layer direction.
const MIN_GRADIENT: f64 = 1e-9;

/// Separates particles `q` and `o` of different layers to `min_d` and restores their layer order.
/// `g` is the body-gradient scratch shared by one contact pass.
pub(crate) fn layer_contact(
    p: &mut Particles,
    grid: &SdfGrid,
    q: usize,
    o: usize,
    g: &mut [f64; 3],
    min_d: f64,
) {
    let (wq, wo) = (f64::from(p.inv_mass[q]), f64::from(p.inv_mass[o]));
    let ws = wq + wo;
    if ws == 0.0 {
        return;
    }
    let (k, ko) = (q * 3, o * 3);
    let x = &mut p.x;
    let dx = f64::from(x[ko]) - f64::from(x[k]);
    let dy = f64::from(x[ko + 1]) - f64::from(x[k + 1]);
    let dz = f64::from(x[ko + 2]) - f64::from(x[k + 2]);
    let d = (dx * dx + dy * dy + dz * dz).sqrt();
    if d < min_d && d > 1e-9 {
        let c = (min_d - d) / d / ws;
        nudge(&mut x[k], -(dx * c * wq));
        nudge(&mut x[k + 1], -(dy * c * wq));
        nudge(&mut x[k + 2], -(dz * c * wq));
        nudge(&mut x[ko], dx * c * wo);
        nudge(&mut x[ko + 1], dy * c * wo);
        nudge(&mut x[ko + 2], dz * c * wo);
    }
    let inn = if p.layer[q] < p.layer[o] { q } else { o };
    let out = if inn == q { o } else { q };
    let (ki, kk) = (inn * 3, out * 3);
    let mid = |c: usize| (f64::from(x[ki + c]) + f64::from(x[kk + c])) * 0.5;
    sample_into(grid, [mid(0), mid(1), mid(2)], g);
    let l = hypot3(g[0], g[1], g[2]);
    if l < MIN_GRADIENT {
        return;
    }
    let (nx, ny, nz) = (g[0] / l, g[1] / l, g[2] / l);
    let sep = (f64::from(x[kk]) - f64::from(x[ki])) * nx
        + (f64::from(x[kk + 1]) - f64::from(x[ki + 1])) * ny
        + (f64::from(x[kk + 2]) - f64::from(x[ki + 2])) * nz;
    if sep < 0.0 {
        let (wi, wout) = (f64::from(p.inv_mass[inn]), f64::from(p.inv_mass[out]));
        let c = -sep / ws;
        nudge(&mut x[kk], nx * c * wout);
        nudge(&mut x[kk + 1], ny * c * wout);
        nudge(&mut x[kk + 2], nz * c * wout);
        nudge(&mut x[ki], -(nx * c * wi));
        nudge(&mut x[ki + 1], -(ny * c * wi));
        nudge(&mut x[ki + 2], -(nz * c * wi));
    }
}
