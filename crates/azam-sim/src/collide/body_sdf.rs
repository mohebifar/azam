//! Cloth against the body's distance grid and the floor.

use azam_avatar::{Avatar, BodyVelocity, SdfGrid};
use glam::DVec3;

use super::sample_into;
use crate::numeric::{hypot3, load3, nudge};
use crate::state::{Particles, Simulation};

/// Push-out passes per particle when a simulation starts inside the body.
const PROJECT_OUT_PASSES: usize = 4;
/// Extra clearance added by the start-up push-out, in mm.
const PROJECT_OUT_EXTRA_MM: f64 = 0.5;
/// Height of the floor, in mm (particles stay at least this high).
const FLOOR_MM: f64 = 1.0;
/// Floor clearance as a fraction of the body margin.
const FLOOR_MARGIN_FACTOR: f64 = 0.3;
/// Sliding on the floor keeps this fraction of the substep's horizontal motion.
const FLOOR_SLIDE: f64 = 0.5;
/// The end-of-substep guard only corrects particles closer than this fraction of their margin.
const GUARD_MARGIN_FACTOR: f64 = 0.35;

/// Unit direction of a gradient (a zero gradient is left as is).
fn unit(g: [f64; 3]) -> (f64, f64, f64) {
    let l = hypot3(g[0], g[1], g[2]);
    let l = if l == 0.0 || l.is_nan() { 1.0 } else { l };
    (g[0] / l, g[1] / l, g[2] / l)
}

impl Simulation {
    /// Pushes every active particle out of the body to its margin (plus a little), then makes the
    /// current positions the substep start.
    pub fn project_out(&mut self, avatar: &Avatar) {
        let p = &mut self.particles;
        let mut g = [0.0; 3];
        for q in 0..p.len() {
            if !p.active[q] {
                continue;
            }
            let k = q * 3;
            for _ in 0..PROJECT_OUT_PASSES {
                let d = sample_into(&avatar.grid, load3(&p.x, q), &mut g);
                let m = f64::from(p.margin[q]);
                if d >= m {
                    break;
                }
                let (nx, ny, nz) = unit(g);
                let push = (m - d) + PROJECT_OUT_EXTRA_MM;
                nudge(&mut p.x[k], nx * push);
                nudge(&mut p.x[k + 1], ny * push);
                nudge(&mut p.x[k + 2], nz * push);
            }
        }
        p.prev.clone_from(&p.x);
    }
}

/// Keeps free particles out of the body (with friction along the skin) and above the floor.
///
/// With a moving body (`motion`: its surface velocity and the substep length `h`), friction acts
/// on the motion relative to the skin, so the cloth is carried along, and a particle the skin
/// pushes leaves no faster (along the normal) than the skin itself moves.
pub(crate) fn body_and_floor(p: &mut Particles, grid: &SdfGrid, motion: Option<(&BodyVelocity, f64)>) {
    let mut g = [0.0; 3];
    for q in 0..p.len() {
        if p.inv_mass[q] == 0.0 {
            continue;
        }
        let k = q * 3;
        let m = f64::from(p.margin[q]);
        let d = sample_into(grid, load3(&p.x, q), &mut g);
        if d < m {
            let (nx, ny, nz) = unit(g);
            let push = m - d;
            nudge(&mut p.x[k], nx * push);
            nudge(&mut p.x[k + 1], ny * push);
            nudge(&mut p.x[k + 2], nz * push);
            let [x0, y0, z0] = load3(&p.x, q);
            let [p0, p1, p2] = load3(&p.prev, q);
            let body = motion.map(|(v, h)| v.sample(DVec3::new(x0, y0, z0)) * h);
            let b = body.unwrap_or_default();
            let (ddx, ddy, ddz) = (x0 - p0 - b.x, y0 - p1 - b.y, z0 - p2 - b.z);
            let dn = ddx * nx + ddy * ny + ddz * nz;
            let (tx, ty, tz) = (ddx - dn * nx, ddy - dn * ny, ddz - dn * nz);
            let mu = f64::from(p.friction[q]);
            nudge(&mut p.x[k], -(tx * mu));
            nudge(&mut p.x[k + 1], -(ty * mu));
            nudge(&mut p.x[k + 2], -(tz * mu));
            if body.is_some() {
                let [x0, y0, z0] = load3(&p.x, q);
                let along = (x0 - p0) * nx + (y0 - p1) * ny + (z0 - p2) * nz;
                let allowed = (b.x * nx + b.y * ny + b.z * nz).max(0.0);
                if along > allowed {
                    let e = along - allowed;
                    nudge(&mut p.prev[k], nx * e);
                    nudge(&mut p.prev[k + 1], ny * e);
                    nudge(&mut p.prev[k + 2], nz * e);
                }
            }
        }
        let floor = FLOOR_MM + m * FLOOR_MARGIN_FACTOR;
        if f64::from(p.x[k + 1]) < floor {
            p.x[k + 1] = floor as f32;
            for c in [k, k + 2] {
                let (xc, pc) = (f64::from(p.x[c]), f64::from(p.prev[c]));
                p.x[c] = (pc + (xc - pc) * FLOOR_SLIDE) as f32;
            }
        }
    }
}

/// End-of-substep guard: corrects only true penetration (inside a fraction of the margin),
/// without friction, and keeps particles above the floor.
pub(crate) fn guard(p: &mut Particles, grid: &SdfGrid) {
    let mut g = [0.0; 3];
    for q in 0..p.len() {
        if p.inv_mass[q] == 0.0 {
            continue;
        }
        let k = q * 3;
        let d = sample_into(grid, load3(&p.x, q), &mut g);
        let m2 = f64::from(p.margin[q]) * GUARD_MARGIN_FACTOR;
        if d < m2 {
            let (nx, ny, nz) = unit(g);
            let push = m2 - d;
            nudge(&mut p.x[k], nx * push);
            nudge(&mut p.x[k + 1], ny * push);
            nudge(&mut p.x[k + 2], nz * push);
        }
        if f64::from(p.x[k + 1]) < FLOOR_MM {
            p.x[k + 1] = FLOOR_MM as f32;
        }
    }
}
