//! Fitting morph-target weights to tailor measurements (Levenberg–Marquardt).

use crate::math::clamp;
use crate::measurements::{FIT_TARGET_COUNT, MEASUREMENT_COUNT, MEASUREMENTS};
use crate::params::{AvatarParams, Sex};

use super::compose::MhShape;
use super::measure::Positions;
use super::parse::MhModel;

/// Maximum Levenberg–Marquardt iterations.
const MAX_ITERATIONS: usize = 40;
/// Damping retries per iteration.
const MAX_DAMPING_TRIES: usize = 8;
/// Finite-difference step of the Jacobian.
const JACOBIAN_STEP: f64 = 1e-3;
/// Bound of the measurement target weights.
const TARGET_WEIGHT_LIMIT: f64 = 1.6;
/// Residual scale: at least this many millimetres…
const MIN_RESIDUAL_SCALE_MM: f64 = 4.0;
/// …or this fraction of the wanted value.
const RELATIVE_RESIDUAL_SCALE: f64 = 0.006;
/// Regularisation pulling every target weight towards zero.
const TARGET_PRIOR: f64 = 0.35;
/// Regularisation pulling a free body weight towards average.
const WEIGHT_PRIOR: f64 = 0.6;
/// Penalty holding a locked body weight at its value.
const LOCKED_WEIGHT_PENALTY: f64 = 50.0;
/// Pivots smaller than this make a linear system singular.
const SINGULAR_PIVOT: f64 = 1e-12;

/// Result of fitting the body to measurements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MhFit {
    /// The fitted morph-target weights.
    pub shape: MhShape,
    /// Uniform scale that brings the body to the wanted height.
    pub scale: f64,
    /// The measurements the fit aimed for, in [`MEASUREMENTS`] order.
    pub wanted: [Option<f64>; MEASUREMENT_COUNT],
}

impl MhModel {
    /// Fits the body to the tailor measurements in `params`: Levenberg–Marquardt over the body
    /// weight (unless [`AvatarParams::fat`] locks it) and the [`crate::FIT_TARGETS`] weights, with
    /// a uniform scale for the height.
    #[must_use]
    pub fn fit(&self, params: &AvatarParams) -> MhFit {
        let gender = if params.sex == Sex::Male { 1.0 } else { 0.0 };
        let muscle = params.muscle.unwrap_or(0.5);
        let locked_weight = params.fat;
        let wanted = MEASUREMENTS.map(|d| params.measurement(d.key).filter(|v| v.is_finite() && *v > 0.0));
        let height = wanted[0];
        let keys: Vec<usize> = (1..MEASUREMENT_COUNT).filter(|&i| wanted[i].is_some()).collect();
        let np = FIT_TARGET_COUNT + 1;
        let mut p = vec![0.0; np];
        p[0] = locked_weight.unwrap_or(0.5);
        let mut lo = vec![-TARGET_WEIGHT_LIMIT; np];
        let mut hi = vec![TARGET_WEIGHT_LIMIT; np];
        (lo[0], hi[0]) = (0.0, 1.0);
        let shape_of = |pp: &[f64]| {
            let mut measures = [0.0; FIT_TARGET_COUNT];
            measures.copy_from_slice(&pp[1..]);
            MhShape {
                gender,
                muscle,
                weight: pp[0],
                measures,
                smooth: 1.0,
            }
        };
        let eval = |pp: &[f64]| -> (Vec<f64>, f64) {
            let pos = self.compose_probe(&shape_of(pp));
            let view = Positions::Probe(&pos);
            let range = self.height_range(view);
            let s = height.map_or(1.0, |want| want / (range[1] - range[0]));
            let mut r = Vec::with_capacity(keys.len() + np + 1);
            for &k in &keys {
                let want = wanted[k].unwrap_or_default();
                let m = self.measure_one(view, &MEASUREMENTS[k], range) * s;
                r.push((m - want) / MIN_RESIDUAL_SCALE_MM.max(want * RELATIVE_RESIDUAL_SCALE));
            }
            r.extend(pp[1..].iter().map(|w| w * TARGET_PRIOR));
            r.push(match locked_weight {
                Some(fat) => (pp[0] - fat) * LOCKED_WEIGHT_PENALTY,
                None => (pp[0] - 0.5) * WEIGHT_PRIOR,
            });
            (r, s)
        };
        let cost_of = |r: &[f64]| r.iter().fold(0.0, |a, b| a + b * b);
        let (mut r, mut scale) = eval(&p);
        let mut cost = cost_of(&r);
        let mut lambda = 1e-2;
        for _ in 0..MAX_ITERATIONS {
            let m = r.len();
            let jac: Vec<Vec<f64>> = (0..np)
                .map(|j| {
                    if j == 0 && locked_weight.is_some() {
                        return vec![0.0; m];
                    }
                    let mut pp = p.clone();
                    pp[j] += JACOBIAN_STEP;
                    let (e, _) = eval(&pp);
                    (0..m).map(|i| (e[i] - r[i]) / JACOBIAN_STEP).collect()
                })
                .collect();
            let (a, g) = normal_equations(&jac, &r);
            let neg_g: Vec<f64> = g.iter().map(|v| -v).collect();
            let mut improved = false;
            for _ in 0..MAX_DAMPING_TRIES {
                let mut damped = a.clone();
                for (i, row) in damped.iter_mut().enumerate() {
                    row[i] += lambda * (a[i][i] + 1e-6);
                }
                let Some(dx) = solve_linear(&damped, &neg_g) else {
                    lambda *= 10.0;
                    continue;
                };
                let pn: Vec<f64> = (0..np).map(|j| clamp(p[j] + dx[j], lo[j], hi[j])).collect();
                let (rn, sn) = eval(&pn);
                let nc = cost_of(&rn);
                if nc < cost {
                    p = pn;
                    (r, scale) = (rn, sn);
                    lambda = (lambda * 0.3).max(1e-6);
                    improved = cost - nc > 1e-6 * cost;
                    cost = nc;
                    break;
                }
                lambda *= 10.0;
            }
            if !improved {
                break;
            }
        }
        MhFit {
            shape: shape_of(&p),
            scale,
            wanted,
        }
    }
}

/// Normal equations `JᵀJ` and `Jᵀr` of a Jacobian given as columns.
fn normal_equations(jac: &[Vec<f64>], r: &[f64]) -> (Vec<Vec<f64>>, Vec<f64>) {
    let np = jac.len();
    let mut a = vec![vec![0.0; np]; np];
    let mut g = vec![0.0; np];
    for ia in 0..np {
        for ib in 0..=ia {
            let s = jac[ia].iter().zip(&jac[ib]).fold(0.0, |s, (x, y)| s + x * y);
            a[ia][ib] = s;
            a[ib][ia] = s;
        }
        g[ia] = jac[ia].iter().zip(r).fold(0.0, |s, (x, y)| s + x * y);
    }
    (a, g)
}

/// Solves `a x = b` by Gaussian elimination with partial pivoting. Returns `None` if the matrix
/// is singular (or not square with `b.len()` rows).
#[must_use]
pub fn solve_linear(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    if a.len() != n || a.iter().any(|row| row.len() < n) {
        return None;
    }
    let mut m: Vec<Vec<f64>> = a
        .iter()
        .zip(b)
        .map(|(row, &bi)| row[..n].iter().copied().chain([bi]).collect())
        .collect();
    for c in 0..n {
        let mut piv = c;
        for r in c + 1..n {
            if m[r][c].abs() > m[piv][c].abs() {
                piv = r;
            }
        }
        if m[piv][c].abs() < SINGULAR_PIVOT {
            return None;
        }
        m.swap(c, piv);
        for r in c + 1..n {
            let f = m[r][c] / m[c][c];
            if f == 0.0 || f.is_nan() {
                continue;
            }
            for k in c..=n {
                m[r][k] -= f * m[c][k];
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = m[r][n];
        for (mk, xk) in m[r][r + 1..n].iter().zip(&x[r + 1..]) {
            s -= mk * xk;
        }
        x[r] = s / m[r][r];
    }
    Some(x)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn solves_a_system_that_needs_pivoting() {
        let a = vec![vec![0.0, 2.0, 1.0], vec![1.0, 1.0, 0.0], vec![3.0, 0.0, 1.0]];
        let x = solve_linear(&a, &[5.0, 3.0, 6.0]).unwrap();
        let expect = [1.4, 1.6, 1.8];
        for (xi, ei) in x.iter().zip(expect) {
            assert!((xi - ei).abs() < 1e-12, "{x:?}");
        }
        assert!(solve_linear(&[vec![1.0, 2.0], vec![2.0, 4.0]], &[1.0, 2.0]).is_none());
        assert!(solve_linear(&[vec![1.0]], &[1.0, 2.0]).is_none());
    }
}
