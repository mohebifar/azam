//! The cloth BSDF: Lambertian diffuse, a Charlie sheen lobe (Estevez & Kulla) with Neubelt's
//! visibility term, and a GGX specular lobe for satin and leather, sampled by lobe with the
//! visible-normal distribution for GGX.

use std::f32::consts::{FRAC_1_PI, PI, TAU};

use glam::Vec3;

use crate::Material;

/// The luminance of a linear RGB colour.
#[must_use]
pub(crate) fn luminance(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

/// An orthonormal frame around a unit normal (Duff et al., "Building an Orthonormal Basis,
/// Revisited").
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    t: Vec3,
    b: Vec3,
    n: Vec3,
}

impl Frame {
    pub fn new(n: Vec3) -> Self {
        let sign = 1.0f32.copysign(n.z);
        let a = -1.0 / (sign + n.z);
        let b = n.x * n.y * a;
        Self {
            t: Vec3::new(1.0 + sign * n.x * n.x * a, sign * b, -sign * n.x),
            b: Vec3::new(b, sign + n.y * n.y * a, -n.y),
            n,
        }
    }

    pub fn to_local(self, v: Vec3) -> Vec3 {
        Vec3::new(v.dot(self.t), v.dot(self.b), v.dot(self.n))
    }

    pub fn to_world(self, v: Vec3) -> Vec3 {
        self.t * v.x + self.b * v.y + self.n * v.z
    }
}

/// A cosine-weighted direction on the hemisphere around +z.
pub(crate) fn cosine_hemisphere(u1: f32, u2: f32) -> Vec3 {
    let r = u1.sqrt();
    let phi = TAU * u2;
    let (s, c) = phi.sin_cos();
    Vec3::new(r * c, r * s, (1.0 - u1).max(0.0).sqrt())
}

/// A uniform direction on the hemisphere around +z.
fn uniform_hemisphere(u1: f32, u2: f32) -> Vec3 {
    let z = u1;
    let r = (1.0 - z * z).max(0.0).sqrt();
    let (s, c) = (TAU * u2).sin_cos();
    Vec3::new(r * c, r * s, z)
}

/// The BSDF at one surface point, in the local frame (z along the shading normal).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bsdf {
    diffuse: Vec3,
    sheen: Vec3,
    sheen_alpha: f32,
    spec_alpha: f32,
    f0: f32,
    p_diffuse: f32,
    p_sheen: f32,
}

/// A sampled direction.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    /// Incident direction, local.
    pub wi: Vec3,
    /// BSDF value (without the cosine).
    pub f: Vec3,
    /// Probability density per solid angle.
    pub pdf: f32,
}

impl Bsdf {
    /// The BSDF of `material` where its colour (after the weave and the inside darkening) is
    /// `albedo` and the light on it is dimmed by `brightness` (1 outside, the inside factor inside).
    pub fn new(material: &Material, albedo: Vec3, brightness: f32) -> Self {
        let albedo = albedo.clamp(Vec3::ZERO, Vec3::ONE);
        let tint_color = {
            let l = luminance(albedo);
            if l > 1e-4 {
                (albedo / l).min(Vec3::splat(1.0))
            } else {
                Vec3::ONE
            }
        };
        let sheen = material.sheen.clamp(0.0, 1.0)
            * brightness
            * Vec3::ONE.lerp(tint_color, material.sheen_tint.clamp(0.0, 1.0));
        let roughness = material.roughness.clamp(0.0, 1.0);
        let f0 = 0.08 * material.specular.clamp(0.0, 1.0) * brightness;
        let diffuse = albedo * (1.0 - f0);
        // Pick lobes roughly by how much light each returns; glossy lobes get a floor since their
        // peaks are what light sampling misses.
        let w_diffuse = luminance(diffuse);
        let w_sheen = luminance(sheen) * 0.3;
        let w_spec = if f0 > 0.0 {
            0.12 + 0.4 * (1.0 - roughness)
        } else {
            0.0
        };
        let total = w_diffuse + w_sheen + w_spec;
        let (p_diffuse, p_sheen) = if total > 0.0 {
            (w_diffuse / total, w_sheen / total)
        } else {
            (1.0, 0.0)
        };
        Self {
            diffuse,
            sheen,
            sheen_alpha: 0.3 + 0.4 * roughness,
            spec_alpha: (roughness * roughness).max(0.003),
            f0,
            p_diffuse,
            p_sheen,
        }
    }

    fn p_spec(&self) -> f32 {
        (1.0 - self.p_diffuse - self.p_sheen).max(0.0)
    }

    /// The BSDF value for outgoing `wo` and incident `wi` (local, unit), without the cosine.
    pub fn eval(&self, wo: Vec3, wi: Vec3) -> Vec3 {
        if wo.z <= 0.0 || wi.z <= 0.0 {
            return Vec3::ZERO;
        }
        let mut f = self.diffuse * FRAC_1_PI;
        let h = (wo + wi).normalize_or_zero();
        if h == Vec3::ZERO {
            return f;
        }
        if self.sheen != Vec3::ZERO {
            let v = 1.0 / (4.0 * (wi.z + wo.z - wi.z * wo.z)).max(1e-6);
            f += self.sheen * (charlie_d(self.sheen_alpha, h.z) * v);
        }
        if self.f0 > 0.0 {
            let a = self.spec_alpha;
            let fresnel = self.f0 + (1.0 - self.f0) * (1.0 - wo.dot(h).clamp(0.0, 1.0)).powi(5);
            let g = 1.0 / (1.0 + smith_lambda(a, wo) + smith_lambda(a, wi));
            let spec = ggx_d(a, h.z) * g * fresnel / (4.0 * wo.z * wi.z).max(1e-8);
            f += Vec3::splat(spec);
        }
        f
    }

    /// The density with which [`Bsdf::sample`] picks `wi` for `wo`.
    pub fn pdf(&self, wo: Vec3, wi: Vec3) -> f32 {
        if wo.z <= 0.0 || wi.z <= 0.0 {
            return 0.0;
        }
        let mut pdf = self.p_diffuse * wi.z * FRAC_1_PI + self.p_sheen / TAU;
        let p_spec = self.p_spec();
        if p_spec > 0.0 {
            let h = (wo + wi).normalize_or_zero();
            if h != Vec3::ZERO {
                let a = self.spec_alpha;
                let g1 = 1.0 / (1.0 + smith_lambda(a, wo));
                pdf += p_spec * g1 * ggx_d(a, h.z) / (4.0 * wo.z);
            }
        }
        pdf
    }

    /// Samples an incident direction for `wo` with three uniform numbers.
    pub fn sample(&self, wo: Vec3, u: f32, u1: f32, u2: f32) -> Option<Sample> {
        if wo.z <= 0.0 {
            return None;
        }
        let wi = if u < self.p_diffuse {
            cosine_hemisphere(u1, u2)
        } else if u < self.p_diffuse + self.p_sheen {
            uniform_hemisphere(u1, u2)
        } else {
            let h = sample_vndf(wo, self.spec_alpha, u1, u2);
            reflect(wo, h)
        };
        if wi.z <= 1e-6 {
            return None;
        }
        let pdf = self.pdf(wo, wi);
        let f = self.eval(wo, wi);
        (pdf > 1e-12 && f.is_finite()).then_some(Sample { wi, f, pdf })
    }
}

/// `v` mirrored about `n`.
fn reflect(v: Vec3, n: Vec3) -> Vec3 {
    2.0 * v.dot(n) * n - v
}

/// The Charlie sheen distribution for a half vector at `cos_h` from the normal.
fn charlie_d(alpha: f32, cos_h: f32) -> f32 {
    let inv = 1.0 / alpha;
    let sin2 = (1.0 - cos_h * cos_h).max(0.0);
    (2.0 + inv) * sin2.powf(inv * 0.5) / TAU
}

/// The GGX (Trowbridge–Reitz) normal distribution.
fn ggx_d(alpha: f32, cos_h: f32) -> f32 {
    if cos_h <= 0.0 {
        return 0.0;
    }
    let a2 = alpha * alpha;
    let d = cos_h * cos_h * (a2 - 1.0) + 1.0;
    a2 / (PI * d * d).max(1e-12)
}

/// Smith's Λ for GGX.
fn smith_lambda(alpha: f32, w: Vec3) -> f32 {
    let cos2 = w.z * w.z;
    if cos2 <= 0.0 {
        return 0.0;
    }
    let tan2 = (1.0 - cos2).max(0.0) / cos2;
    ((1.0 + alpha * alpha * tan2).sqrt() - 1.0) * 0.5
}

/// Samples a GGX visible normal for `wo` (Heitz 2018).
fn sample_vndf(wo: Vec3, alpha: f32, u1: f32, u2: f32) -> Vec3 {
    let vh = Vec3::new(alpha * wo.x, alpha * wo.y, wo.z).normalize();
    let lensq = vh.x * vh.x + vh.y * vh.y;
    let t1 = if lensq > 1e-12 {
        Vec3::new(-vh.y, vh.x, 0.0) / lensq.sqrt()
    } else {
        Vec3::X
    };
    let t2 = vh.cross(t1);
    let r = u1.sqrt();
    let (s, c) = (TAU * u2).sin_cos();
    let p1 = r * c;
    let mix = 0.5 * (1.0 + vh.z);
    let p2 = (1.0 - mix) * (1.0 - p1 * p1).max(0.0).sqrt() + mix * r * s;
    let nh = t1 * p1 + t2 * p2 + vh * (1.0 - p1 * p1 - p2 * p2).max(0.0).sqrt();
    Vec3::new(alpha * nh.x, alpha * nh.y, nh.z.max(1e-6)).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rng;

    /// Estimates ∫ f cos dωi two ways — uniform hemisphere sampling and the BSDF's own sampling —
    /// which must agree if `sample` and `pdf` are consistent.
    fn albedo_two_ways(bsdf: &Bsdf, wo: Vec3, n: u32) -> (f32, f32) {
        let mut rng = Rng::new(9, 9);
        let (mut uniform, mut importance) = (0.0f64, 0.0f64);
        for _ in 0..n {
            let wi = uniform_hemisphere(rng.next_f32(), rng.next_f32());
            uniform += f64::from(luminance(bsdf.eval(wo, wi)) * wi.z * TAU);
            if let Some(s) = bsdf.sample(wo, rng.next_f32(), rng.next_f32(), rng.next_f32()) {
                importance += f64::from(luminance(s.f) * s.wi.z / s.pdf);
            }
        }
        (
            (uniform / f64::from(n)) as f32,
            (importance / f64::from(n)) as f32,
        )
    }

    #[test]
    fn lambert_albedo_is_its_colour() {
        let bsdf = Bsdf::new(&Material::diffuse([0.5; 3]), Vec3::splat(0.5), 1.0);
        let wo = Vec3::new(0.3, 0.1, 0.9).normalize();
        let (a, b) = albedo_two_ways(&bsdf, wo, 200_000);
        assert!((a - 0.5).abs() < 0.01, "{a}");
        assert!((b - 0.5).abs() < 0.002, "{b}");
    }

    #[test]
    fn sampling_matches_evaluation_for_cloth_and_satin() {
        let mut satin = Material::cloth([0.6, 0.2, 0.3]);
        satin.roughness = 0.25;
        satin.specular = 1.0;
        for m in [
            Material::cloth([0.6, 0.2, 0.3]),
            satin,
            Material::skin([0.8, 0.6, 0.5]),
        ] {
            let bsdf = Bsdf::new(&m, Vec3::from(m.base_color), 1.0);
            for wo in [Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.7, 0.2, 0.3).normalize()] {
                let (a, b) = albedo_two_ways(&bsdf, wo, 400_000);
                assert!((a - b).abs() < 0.02 * a.max(0.1), "{m:?} {wo}: {a} vs {b}");
                assert!(b < 1.0, "energy gain {b}");
            }
        }
    }

    #[test]
    fn glancing_and_degenerate_directions_are_finite() {
        let mut m = Material::cloth([1.0; 3]);
        m.roughness = 0.0;
        m.specular = 1.0;
        m.sheen = 1.0;
        let bsdf = Bsdf::new(&m, Vec3::ONE, 1.0);
        let grazing = Vec3::new(1.0, 0.0, 1e-7).normalize();
        for wi in [grazing, Vec3::Z, Vec3::new(-1.0, 0.0, 1e-7).normalize()] {
            assert!(bsdf.eval(grazing, wi).is_finite());
            assert!(bsdf.pdf(grazing, wi).is_finite());
        }
        let mut rng = Rng::new(3, 4);
        for _ in 0..10_000 {
            if let Some(s) = bsdf.sample(grazing, rng.next_f32(), rng.next_f32(), rng.next_f32()) {
                assert!(s.f.is_finite() && s.pdf.is_finite() && s.wi.is_finite());
            }
        }
    }

    #[test]
    fn frame_is_orthonormal() {
        for n in [Vec3::Z, Vec3::NEG_Z, Vec3::new(0.3, -0.8, 0.2).normalize()] {
            let f = Frame::new(n);
            let v = Vec3::new(0.2, 0.5, -0.7);
            assert!((f.to_world(f.to_local(v)) - v).length() < 1e-5);
            assert!((f.to_local(n) - Vec3::Z).length() < 1e-5);
        }
    }
}
