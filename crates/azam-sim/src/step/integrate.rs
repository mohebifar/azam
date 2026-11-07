//! Gravity hold-back while seams are open, explicit integration of each substep and the velocity
//! update at its end.

use crate::numeric::{clamp, hypot3, nudge};
use crate::state::{Particles, SewingConstraints};

/// Velocities are clamped to this speed, in mm/s.
pub(crate) const MAX_SPEED_MM_S: f64 = 6000.0;
/// Mean seam gap (mm) at which gravity starts to be held back.
const HOLD_START_GAP_MM: f64 = 6.0;
/// Gap range (mm) over which gravity fades to its minimum.
const HOLD_SPAN_MM: f64 = 30.0;
/// Smallest gravity scale while seams are open.
const MIN_GRAVITY_SCALE: f64 = 0.03;
/// Share of the way gravity recovers per frame once seams close.
const RECOVERY_RATE: f64 = 0.04;
/// Inflation acceleration per unit pressure, in mm/s² (along the vertex normal).
const PRESSURE_ACCEL: f64 = 40.0;

/// Gravity scale for a mean seam gap.
fn hold_target(gap: f64) -> f64 {
    clamp(
        1.0 - (gap - HOLD_START_GAP_MM) / HOLD_SPAN_MM,
        MIN_GRAVITY_SCALE,
        1.0,
    )
}

/// Moves a hold-back scale towards `target`: down at once, up gradually.
fn approach(current: f64, target: f64) -> f64 {
    if target < current {
        target
    } else {
        current + (target - current) * RECOVERY_RATE
    }
}

/// Distance between sewing constraint `c`'s particle and its target point.
fn seam_gap(x: &[f32], sw: &SewingConstraints, c: usize) -> f64 {
    let (a, b0, b1) = (sw.a[c] as usize * 3, sw.b0[c] as usize * 3, sw.b1[c] as usize * 3);
    let u = f64::from(sw.u[c]);
    let d = |o: usize| (1.0 - u) * f64::from(x[b0 + o]) + u * f64::from(x[b1 + o]) - f64::from(x[a + o]);
    let (dx, dy, dz) = (d(0), d(1), d(2));
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Gravity hold-back state of a simulation.
pub(crate) struct Holdback<'a> {
    pub gravity_scale: &'a mut Option<f64>,
    pub piece_gravity: &'a mut Vec<f32>,
    pub sew_gap: &'a mut f64,
}

/// Updates the hold-back from the current seam gaps and returns each piece's vertical
/// acceleration (mm/s²).
pub(crate) fn piece_gravity(
    hold: &mut Holdback<'_>,
    x: &[f32],
    piece_of: &[u32],
    sw: &SewingConstraints,
    pieces: usize,
    gravity: f64,
) -> Vec<f32> {
    let n = sw.len();
    let mut gap = 0.0;
    if n > 0 {
        let acc: f64 = (0..n)
            .step_by(2)
            .map(|c| seam_gap(x, sw, c))
            .fold(0.0, |s, v| s + v);
        gap = acc / n.div_ceil(2) as f64;
    }
    *hold.sew_gap = gap;
    let target = hold_target(gap);
    let scale = hold.gravity_scale.map_or(target, |s| approach(s, target));
    *hold.gravity_scale = Some(scale);
    if hold.piece_gravity.len() != pieces {
        *hold.piece_gravity = vec![1.0; pieces];
    }
    if n > 0 {
        let (mut sum, mut count) = (vec![0.0_f64; pieces], vec![0.0_f64; pieces]);
        for c in (0..n).step_by(2) {
            let len = seam_gap(x, sw, c);
            let (pa, pb) = (
                piece_of[sw.a[c] as usize] as usize,
                piece_of[sw.b0[c] as usize] as usize,
            );
            sum[pa] += len;
            count[pa] += 1.0;
            if pb != pa {
                sum[pb] += len;
                count[pb] += 1.0;
            }
        }
        for r in 0..pieces {
            let mean = if count[r] > 0.0 { sum[r] / count[r] } else { 0.0 };
            let current = f64::from(hold.piece_gravity[r]);
            hold.piece_gravity[r] = approach(current, hold_target(mean)) as f32;
        }
    }
    hold.piece_gravity
        .iter()
        .map(|&g| (-gravity * scale.min(f64::from(g))) as f32)
        .collect()
}

/// Applies damping, gravity and pressure to the velocities, remembers the substep start and moves
/// the particles. Fixed particles only record their start.
pub(crate) fn predict(p: &mut Particles, gy: &[f32], air: f64, h: f64, pressure: bool, normals: &[f32]) {
    for q in 0..p.len() {
        let k = q * 3;
        if p.inv_mass[q] == 0.0 {
            p.prev[k..k + 3].copy_from_slice(&p.x[k..k + 3]);
            continue;
        }
        let dmp = (1.0 - (f64::from(p.damping[q]) + air) * h).max(0.0);
        let v = &mut p.v;
        v[k] = (f64::from(v[k]) * dmp) as f32;
        v[k + 1] = ((f64::from(v[k + 1]) + f64::from(gy[p.piece[q] as usize]) * h) * dmp) as f32;
        v[k + 2] = (f64::from(v[k + 2]) * dmp) as f32;
        if pressure && p.pressure[q] != 0.0 {
            let f = f64::from(p.pressure[q]) * PRESSURE_ACCEL * h;
            for d in 0..3 {
                nudge(&mut v[k + d], f64::from(normals[k + d]) * f);
            }
        }
        let sp = hypot3(f64::from(v[k]), f64::from(v[k + 1]), f64::from(v[k + 2]));
        if sp > MAX_SPEED_MM_S {
            let f = MAX_SPEED_MM_S / sp;
            for d in 0..3 {
                v[k + d] = (f64::from(v[k + d]) * f) as f32;
            }
        }
        p.prev[k..k + 3].copy_from_slice(&p.x[k..k + 3]);
        for d in 0..3 {
            p.x[k + d] = (f64::from(p.x[k + d]) + f64::from(p.v[k + d]) * h) as f32;
        }
    }
}

/// Velocities from the substep's displacement.
pub(crate) fn update_velocities(p: &mut Particles, h: f64) {
    let ih = 1.0 / h;
    for k in 0..p.x.len() {
        p.v[k] = ((f64::from(p.x[k]) - f64::from(p.prev[k])) * ih) as f32;
    }
}
