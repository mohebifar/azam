//! Fit maps: per-particle strain, stress, fit (strain over the fabric's limit) or body contact.

use azam_avatar::Avatar;
use azam_doc::{FabricPreset, Project, limit_strain, stress_kpa};

use crate::collide::sample_into;
use crate::numeric::load3;
use crate::state::Simulation;

/// Particles within this distance beyond their margin (mm) count as touching the body.
const CONTACT_BAND_MM: f64 = 1.2;
/// Triangles whose flat determinant is below this are skipped.
const DEGENERATE_DET: f64 = 1e-9;
/// Fit values at or above this are tight.
const TIGHT_FIT: f64 = 0.6;
/// Fit values at or above this exceed the fabric's limit.
const OVER_LIMIT_FIT: f64 = 1.0;

/// What a fit map shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FitMode {
    /// Largest principal stretch ratio (1 = unstretched).
    Strain,
    /// Stress in kPa.
    Stress,
    /// Strain as a fraction of the fabric's limit (1 = at the limit).
    Fit,
    /// 1 where the cloth touches the body, else 0.
    Pressure,
}

/// Share of the active particles that are tight or over the limit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FitSummary {
    /// Fraction with a fit value in [0.6, 1).
    pub tight: f64,
    /// Fraction with a fit value of 1 or more.
    pub over_limit: f64,
}

impl Simulation {
    /// Per-particle fit values: triangle values averaged over each particle's triangles.
    #[must_use]
    pub fn fit_values(&self, project: &Project, avatar: &Avatar, mode: FitMode) -> Vec<f32> {
        let n = self.particle_count();
        let x = &self.particles.x;
        let mut out = vec![0.0_f32; n];
        if mode == FitMode::Pressure {
            let mut g = [0.0; 3];
            for (q, o) in out.iter_mut().enumerate() {
                let d = sample_into(&avatar.grid, load3(x, q), &mut g);
                *o = if d < f64::from(self.particles.margin[q]) + CONTACT_BAND_MM {
                    1.0
                } else {
                    0.0
                };
            }
            return out;
        }
        let mut cnt = vec![0.0_f32; n];
        for r in &self.records {
            let ph = project
                .fabrics
                .get(&r.fabric)
                .map_or_else(|| FabricPreset::Poplin.spec().physics(), |f| f.phys.clone());
            let k_avg = f64::midpoint(ph.stretch_weft, ph.stretch_warp);
            let lim = limit_strain(k_avg);
            let th = if ph.thickness == 0.0 || ph.thickness.is_nan() {
                0.5
            } else {
                ph.thickness
            };
            let xy = &r.mesh.xy;
            for t in r.mesh.tris.chunks_exact(3) {
                let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
                let (u1, v1) = (xy[2 * b] - xy[2 * a], xy[2 * b + 1] - xy[2 * a + 1]);
                let (u2, v2) = (xy[2 * c] - xy[2 * a], xy[2 * c + 1] - xy[2 * a + 1]);
                let det = u1 * v2 - u2 * v1;
                if det.abs() < DEGENERATE_DET {
                    continue;
                }
                let (i00, i01, i10, i11) = (v2 / det, -u2 / det, -v1 / det, u1 / det);
                let (pa, pb, pc) = (
                    load3(x, r.offset + a),
                    load3(x, r.offset + b),
                    load3(x, r.offset + c),
                );
                let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
                let f0 = [
                    e1[0] * i00 + e2[0] * i10,
                    e1[1] * i00 + e2[1] * i10,
                    e1[2] * i00 + e2[2] * i10,
                ];
                let f1 = [
                    e1[0] * i01 + e2[0] * i11,
                    e1[1] * i01 + e2[1] * i11,
                    e1[2] * i01 + e2[2] * i11,
                ];
                let c00 = f0[0] * f0[0] + f0[1] * f0[1] + f0[2] * f0[2];
                let c11 = f1[0] * f1[0] + f1[1] * f1[1] + f1[2] * f1[2];
                let c01 = f0[0] * f1[0] + f0[1] * f1[1] + f0[2] * f1[2];
                let tr = c00 + c11;
                let dt2 = ((c00 - c11) * (c00 - c11) / 4.0 + c01 * c01).max(0.0).sqrt();
                let lmax = (tr / 2.0 + dt2).max(0.0).sqrt();
                let val = match mode {
                    FitMode::Strain => lmax,
                    FitMode::Stress => stress_kpa(k_avg, (lmax - 1.0).max(0.0), Some(th)),
                    FitMode::Fit | FitMode::Pressure => (lmax - 1.0).max(0.0) / lim,
                };
                for v in [a, b, c] {
                    let q = r.offset + v;
                    out[q] = (f64::from(out[q]) + val) as f32;
                    cnt[q] += 1.0;
                }
            }
        }
        let empty = if mode == FitMode::Strain { 1.0 } else { 0.0 };
        for (o, &c) in out.iter_mut().zip(&cnt) {
            *o = if c > 0.0 {
                (f64::from(*o) / f64::from(c)) as f32
            } else {
                empty
            };
        }
        out
    }

    /// Share of the active particles that are tight or over the limit, from fit values
    /// ([`FitMode::Fit`]).
    #[must_use]
    pub fn fit_summary(&self, values: &[f32]) -> FitSummary {
        let (mut tight, mut over, mut total) = (0_usize, 0_usize, 0_usize);
        for (q, &v) in values.iter().enumerate().take(self.particle_count()) {
            if !self.particles.active[q] {
                continue;
            }
            total += 1;
            let v = f64::from(v);
            if v >= OVER_LIMIT_FIT {
                over += 1;
            } else if v >= TIGHT_FIT {
                tight += 1;
            }
        }
        if total == 0 {
            return FitSummary::default();
        }
        FitSummary {
            tight: tight as f64 / total as f64,
            over_limit: over as f64 / total as f64,
        }
    }
}
