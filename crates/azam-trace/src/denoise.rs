//! An edge-avoiding À-trous wavelet filter (Dammertz et al. 2010, with SVGF's variance-guided
//! luminance weight) guided by the first hit's albedo, normal and depth.
//!
//! The filter works on demodulated illumination (radiance ÷ albedo), so weaves and piece colours
//! stay sharp while the lighting noise is smoothed.

use glam::Vec3;
use rayon::prelude::*;

use crate::bsdf::luminance;

/// The guide buffers and the noisy image, all per pixel.
pub(crate) struct Guides<'a> {
    pub width: usize,
    pub height: usize,
    /// Mean radiance.
    pub color: &'a [Vec3],
    /// Mean first-hit albedo.
    pub albedo: &'a [Vec3],
    /// Mean first-hit normal (zero for the background).
    pub normal: &'a [Vec3],
    /// Mean first-hit depth, in mm.
    pub depth: &'a [f32],
    /// Variance of the mean luminance.
    pub variance: &'a [f32],
}

/// The B3-spline kernel.
const KERNEL: [f32; 5] = [1.0 / 16.0, 1.0 / 4.0, 3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0];
/// Filter passes, with step sizes 1, 2, 4, 8, 16.
const PASSES: u32 = 5;
/// Luminance edge-stopping strength (in standard deviations).
const SIGMA_LUMINANCE: f32 = 4.0;
/// Normal edge-stopping exponent.
const SIGMA_NORMAL: f32 = 64.0;
/// Albedo edge-stopping width.
const SIGMA_ALBEDO: f32 = 0.08;
/// Relative depth edge-stopping width, per pixel of step.
const SIGMA_DEPTH: f32 = 0.004;

/// Albedo below which a channel is not demodulated.
const MIN_ALBEDO: f32 = 0.02;

fn demodulate(c: Vec3, a: Vec3) -> Vec3 {
    let a = a.max(Vec3::splat(MIN_ALBEDO));
    c / a
}

/// Returns the filtered radiance.
pub(crate) fn denoise(g: &Guides) -> Vec<Vec3> {
    let (w, h) = (g.width, g.height);
    let mut illum: Vec<Vec3> = g
        .color
        .iter()
        .zip(g.albedo)
        .map(|(&c, &a)| demodulate(c, a))
        .collect();
    // Variance of the demodulated luminance, approximately.
    let mut var: Vec<f32> = g
        .variance
        .iter()
        .zip(g.albedo)
        .map(|(&v, &a)| {
            let l = luminance(a.max(Vec3::splat(MIN_ALBEDO)));
            v / (l * l)
        })
        .collect();
    for pass in 0..PASSES {
        let step = 1usize << pass;
        // Pre-blur the variance a little (3×3), as SVGF does, so its estimate is less noisy.
        let blurred = blur_variance(&var, w, h);
        let (next_illum, next_var): (Vec<Vec3>, Vec<f32>) = (0..w * h)
            .into_par_iter()
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let (cp, np, ap, zp) = (illum[i], g.normal[i], g.albedo[i], g.depth[i]);
                let lp = luminance(cp);
                let sigma_l = SIGMA_LUMINANCE * blurred[i].max(0.0).sqrt() + 1e-4;
                let mut sum = Vec3::ZERO;
                let mut sum_var = 0.0;
                let mut total = 0.0;
                for (ky, &hy) in KERNEL.iter().enumerate() {
                    let Some(qy) = tap(y, ky, step, h) else { continue };
                    for (kx, &hx) in KERNEL.iter().enumerate() {
                        let Some(qx) = tap(x, kx, step, w) else { continue };
                        let q = qy * w + qx;
                        let cq = illum[q];
                        let nq = g.normal[q];
                        // Surfaces only blend with surfaces facing the same way; background with
                        // background.
                        let wn = if np == Vec3::ZERO || nq == Vec3::ZERO {
                            if np == nq { 1.0 } else { 0.0 }
                        } else {
                            np.dot(nq).max(0.0).powf(SIGMA_NORMAL)
                        };
                        if wn <= 0.0 {
                            continue;
                        }
                        let dz = (zp - g.depth[q]).abs() / (zp.max(1.0) * SIGMA_DEPTH * step as f32);
                        let da = (ap - g.albedo[q]).length_squared() / (SIGMA_ALBEDO * SIGMA_ALBEDO);
                        let dl = (lp - luminance(cq)).abs() / sigma_l;
                        let weight = hx * hy * wn * (-(dz + da + dl)).exp();
                        sum += cq * weight;
                        sum_var += var[q] * weight * weight;
                        total += weight;
                    }
                }
                if total > 0.0 {
                    (sum / total, sum_var / (total * total))
                } else {
                    (cp, var[i])
                }
            })
            .unzip();
        illum = next_illum;
        var = next_var;
    }
    illum
        .iter()
        .zip(g.albedo)
        .map(|(&i, &a)| i * a.max(Vec3::splat(MIN_ALBEDO)))
        .collect()
}

/// The coordinate of kernel tap `k` (0..5, centred on 2) around `x` with `step`, if inside 0..`n`.
fn tap(x: usize, k: usize, step: usize, n: usize) -> Option<usize> {
    let q = (x + k * step).checked_sub(2 * step)?;
    (q < n).then_some(q)
}

/// A 3×3 box blur of the variance.
fn blur_variance(var: &[f32], w: usize, h: usize) -> Vec<f32> {
    (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let mut sum = 0.0;
            let mut n = 0.0;
            for qy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for qx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    sum += var[qy * w + qx];
                    n += 1.0;
                }
            }
            sum / n
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rng;

    #[test]
    fn smooths_noise_and_keeps_edges() {
        // Left half dark, right half bright, both noisy; one flat surface.
        let (w, h) = (64, 32);
        let mut rng = Rng::new(5, 5);
        let truth = |x: usize| if x < w / 2 { 0.2 } else { 0.8 };
        let color: Vec<Vec3> = (0..w * h)
            .map(|i| Vec3::splat(truth(i % w) * (0.6 + 0.8 * rng.next_f32())))
            .collect();
        let albedo: Vec<Vec3> = (0..w * h).map(|i| Vec3::splat(truth(i % w))).collect();
        let normal = vec![Vec3::Z; w * h];
        let depth = vec![1000.0; w * h];
        let variance: Vec<f32> = (0..w * h).map(|i| (truth(i % w) * 0.23f32).powi(2)).collect();
        let out = denoise(&Guides {
            width: w,
            height: h,
            color: &color,
            albedo: &albedo,
            normal: &normal,
            depth: &depth,
            variance: &variance,
        });
        let err = |img: &[Vec3]| {
            img.iter()
                .enumerate()
                .map(|(i, c)| (c.x - truth(i % w)).powi(2))
                .sum::<f32>()
                / img.len() as f32
        };
        assert!(err(&out) < err(&color) * 0.1, "{} vs {}", err(&out), err(&color));
        // The edge stays sharp: pixels right next to it keep their side's value.
        let row = 16 * w;
        assert!(
            (out[row + w / 2 - 1].x - 0.2).abs() < 0.05,
            "{}",
            out[row + w / 2 - 1].x
        );
        assert!((out[row + w / 2].x - 0.8).abs() < 0.1, "{}", out[row + w / 2].x);
        assert!(out.iter().all(|c| c.is_finite()));
    }
}
