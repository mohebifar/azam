//! The path tracer: the scene flattened for tracing, and the radiance along one camera ray.

use std::f32::consts::PI;

use glam::{Vec2, Vec3};

use crate::bsdf::{Bsdf, Frame, cosine_hemisphere, luminance};
use crate::rng::mix64;
use crate::scene::{AreaLight, Backdrop, Floor, Sky};
use crate::weave::weave;
use crate::{Bvh, Material, Ray, Rng, Scene, Weave};

/// Opacity at or above which a surface is treated as fully opaque.
const OPAQUE: f32 = 0.999;
/// Bounces after which Russian roulette may end a path.
const ROULETTE_AFTER: u32 = 2;
/// Most a light's selection weight can drop relative to its full value, so every light keeps a
/// chance of being sampled (the weights only guide, they never exclude).
const MIN_LIGHT_FACTOR: f32 = 0.1;

/// A light prepared for sampling.
#[derive(Clone, Copy, Debug)]
struct Light {
    center: Vec3,
    half_u: Vec3,
    half_v: Vec3,
    normal: Vec3,
    area: f32,
    radiance: Vec3,
    /// Luminous power over π: radiance luminance × area.
    power: f32,
}

impl Light {
    fn new(l: &AreaLight) -> Self {
        let radiance = Vec3::from(l.radiance).max(Vec3::ZERO);
        let area = l.area();
        Self {
            center: l.center,
            half_u: l.half_u,
            half_v: l.half_v,
            normal: l.normal(),
            area,
            radiance,
            power: luminance(radiance) * area,
        }
    }

    /// Where `ray` meets the emitting side of the light, if before `t_max`.
    fn hit(&self, ray: &Ray, t_max: f32) -> Option<f32> {
        let denom = ray.dir.dot(self.normal);
        if denom >= -1e-6 {
            return None;
        }
        let t = (self.center - ray.origin).dot(self.normal) / denom;
        if !(t > 0.0 && t < t_max) {
            return None;
        }
        let d = ray.at(t) - self.center;
        let a = d.dot(self.half_u) / self.half_u.length_squared();
        let b = d.dot(self.half_v) / self.half_v.length_squared();
        (a.abs() <= 1.0 && b.abs() <= 1.0).then_some(t)
    }

    /// How strongly this light is expected to light a point `p` with normal `n` (a selection
    /// weight; never zero).
    fn weight(&self, p: Vec3, n: Vec3) -> f32 {
        let to = self.center - p;
        let d2 = to.length_squared().max(1.0);
        let dir = to / d2.sqrt();
        let soft = |c: f32| c.max(0.0) * (1.0 - MIN_LIGHT_FACTOR) + MIN_LIGHT_FACTOR;
        self.power * soft(n.dot(dir)) * soft(-self.normal.dot(dir)) / d2
    }
}

/// The scene flattened for tracing.
pub(crate) struct Prepared {
    bvh: Bvh,
    /// Global vertex indices of each triangle.
    tris: Vec<[u32; 3]>,
    /// Material of each triangle.
    tri_material: Vec<u32>,
    /// Unit geometric normal of each triangle (along `(b − a) × (c − a)`).
    tri_normal: Vec<Vec3>,
    /// Vertex positions (for texture-space frames).
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<Vec2>,
    materials: Vec<Material>,
    /// Printed image of each material's mesh, if any.
    decals: Vec<Option<std::sync::Arc<crate::Decal>>>,
    /// Normal and roughness maps of each material's mesh, if any.
    reliefs: Vec<Option<crate::Relief>>,
    lights: Vec<Light>,
    sky: Sky,
    floor: Option<Floor>,
    backdrop: Option<Backdrop>,
}

/// What a camera sample returns: radiance and the guides for the denoiser.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PathSample {
    pub radiance: Vec3,
    pub albedo: Vec3,
    pub normal: Vec3,
    pub depth: f32,
}

/// Per-render constants the integrator needs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PathParams {
    pub max_bounces: u32,
    pub clamp: f32,
    /// Width of a pixel at distance 1 (perspective), for the weave footprint.
    pub pixel_angle: f32,
    /// Width of a pixel in mm for an orthographic camera (0 for perspective).
    pub pixel_mm: f32,
}

/// A surface point being shaded.
struct Surface {
    p: Vec3,
    /// Geometric normal on the side the ray came from.
    ng: Vec3,
    /// Shading normal on the side the ray came from.
    ns: Vec3,
    albedo: Vec3,
    bsdf: Bsdf,
}

/// The power heuristic (β = 2) weight of a strategy with density `a` against one with `b`.
fn power_heuristic(a: f32, b: f32) -> f32 {
    let (a2, b2) = (a * a, b * b);
    if a2 + b2 > 0.0 { a2 / (a2 + b2) } else { 0.0 }
}

/// A point offset off a surface with normal `n`, against self-intersection.
fn offset(p: Vec3, n: Vec3) -> Vec3 {
    let eps = 0.02 + 2e-5 * p.abs().max_element();
    p + n * eps
}

/// Scales `c` down so its luminance is at most `max`.
fn clamp_luminance(c: Vec3, max: f32) -> Vec3 {
    let l = luminance(c);
    if l > max && l.is_finite() {
        c * (max / l)
    } else {
        c
    }
}

impl Prepared {
    /// Flattens `scene`, dropping degenerate or non-finite triangles.
    pub fn new(scene: &Scene) -> Self {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut tris = Vec::new();
        let mut tri_material = Vec::new();
        let mut tri_normal = Vec::new();
        let mut corners = Vec::new();
        let mut materials = Vec::new();
        let mut decals = Vec::new();
        let mut reliefs = Vec::new();
        for (mi, mesh) in scene.meshes.iter().enumerate() {
            let mut material = mesh.material;
            if mesh.uvs.len() != mesh.positions.len() {
                material.weave = Weave::None;
            }
            materials.push(material);
            decals.push(
                mesh.decal
                    .clone()
                    .filter(|_| mesh.uvs.len() == mesh.positions.len()),
            );
            reliefs.push(
                mesh.relief
                    .clone()
                    .filter(|r| mesh.uvs.len() == mesh.positions.len() && r.tile_mm > 0.0),
            );
            let base = positions.len() as u32;
            let n = mesh.positions.len();
            positions.extend(mesh.positions.iter().map(|p| Vec3::from(*p)));
            normals.extend((0..n).map(|i| {
                mesh.normals
                    .get(i)
                    .map(|v| Vec3::from(*v).normalize_or_zero())
                    .filter(|v| v.is_finite())
                    .unwrap_or(Vec3::ZERO)
            }));
            uvs.extend((0..n).map(|i| mesh.uvs.get(i).map_or(Vec2::ZERO, |uv| Vec2::from(*uv))));
            for t in mesh.indices.chunks_exact(3) {
                if t.iter().any(|&i| i as usize >= n) {
                    continue;
                }
                let idx = [base + t[0], base + t[1], base + t[2]];
                let [a, b, c] = idx.map(|i| positions[i as usize]);
                let cross = (b - a).cross(c - a);
                if !(a.is_finite() && b.is_finite() && c.is_finite()) || cross.length_squared() < 1e-12 {
                    continue;
                }
                tris.push(idx);
                tri_material.push(mi as u32);
                tri_normal.push(cross.normalize());
                corners.push([a, b, c]);
            }
        }
        let lights: Vec<Light> = scene
            .lighting
            .lights
            .iter()
            .map(Light::new)
            .filter(|l| l.power > 0.0 && l.area > 0.0)
            .collect();
        Self {
            bvh: Bvh::new(&corners),
            tris,
            tri_material,
            tri_normal,
            positions,
            normals,
            uvs,
            materials,
            decals,
            reliefs,
            lights,
            sky: scene.lighting.sky,
            floor: scene.lighting.floor,
            backdrop: scene.lighting.backdrop,
        }
    }

    /// Whether the triangle `prim` stops a ray with transparency seed `seed` (sheer fabrics let
    /// some rays through, the same ones every time for one ray).
    fn accept(&self, prim: u32, seed: u64) -> bool {
        let opacity = self.materials[self.tri_material[prim as usize] as usize].opacity;
        if opacity >= OPAQUE {
            return true;
        }
        let h = mix64(seed ^ (u64::from(prim) << 20));
        ((h >> 40) as f32 / 16_777_216.0) < opacity
    }

    /// Where `ray` meets the floor, if before `t_max`.
    fn floor_hit(&self, ray: &Ray, t_max: f32) -> Option<f32> {
        let floor = self.floor?;
        if ray.dir.y.abs() < 1e-8 {
            return None;
        }
        let t = (floor.height - ray.origin.y) / ray.dir.y;
        if !(t > 1e-3 && t < t_max) {
            return None;
        }
        let p = ray.at(t);
        (p.x * p.x + p.z * p.z <= floor.radius * floor.radius).then_some(t)
    }

    /// Whether anything blocks the segment from `p` along `dir` for `dist`.
    fn occluded(&self, p: Vec3, dir: Vec3, dist: f32, rng: &mut Rng) -> bool {
        let ray = Ray::new(p, dir);
        let seed = u64::from(rng.next_u32());
        self.floor_hit(&ray, dist).is_some() || self.bvh.occluded(&ray, dist, |prim| self.accept(prim, seed))
    }

    /// The selection weights of the lights at `p` with normal `n`, and their sum.
    fn light_weights(&self, p: Vec3, n: Vec3) -> ([f32; 8], f32) {
        let mut w = [0.0; 8];
        let mut sum = 0.0;
        for (i, l) in self.lights.iter().take(8).enumerate() {
            w[i] = l.weight(p, n);
            sum += w[i];
        }
        (w, sum)
    }

    /// The probability of picking light `index` at `p` with normal `n`.
    fn light_select_pdf(&self, index: usize, p: Vec3, n: Vec3) -> f32 {
        let (w, sum) = self.light_weights(p, n);
        if sum > 0.0 { w[index] / sum } else { 0.0 }
    }

    /// The nearest light hit along `ray` before `t_max`.
    fn light_hit(&self, ray: &Ray, t_max: f32) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for (i, l) in self.lights.iter().take(8).enumerate() {
            if let Some(t) = l.hit(ray, best.map_or(t_max, |b| b.1)) {
                best = Some((i, t));
            }
        }
        best
    }

    /// The light `ray` (a BSDF sample from the previous bounce with density, position and normal
    /// `prev`) reaches before `t_max`, weighted against having sampled that light directly.
    fn emitted(&self, ray: &Ray, t_max: f32, prev: (f32, Vec3, Vec3)) -> Option<Vec3> {
        let (pdf, p, n) = prev;
        let (li, t) = self.light_hit(ray, t_max)?;
        let light = &self.lights[li];
        let cos_l = -light.normal.dot(ray.dir);
        let light_pdf = self.light_select_pdf(li, p, n) * t * t / (cos_l * light.area).max(1e-12);
        Some(light.radiance * power_heuristic(pdf, light_pdf))
    }

    /// Shades the triangle hit `hit` of `ray`.
    fn surface(&self, ray: &Ray, hit: &crate::Hit, footprint: f32) -> Surface {
        let prim = hit.prim as usize;
        let [i0, i1, i2] = self.tris[prim].map(|i| i as usize);
        let (u, v) = (hit.u, hit.v);
        let w = 1.0 - u - v;
        let p = ray.at(hit.t);
        let mut ng = self.tri_normal[prim];
        let mut ns = (self.normals[i0] * w + self.normals[i1] * u + self.normals[i2] * v).normalize_or_zero();
        if ns == Vec3::ZERO {
            ns = ng;
        }
        let mi = self.tri_material[prim] as usize;
        let material = &self.materials[mi];
        let wo = -ray.dir;
        // The vertex normals say which side is the outside; seen from the other side it's the inside.
        if ns.dot(ng) < 0.0 {
            ng = -ng;
        }
        let front = ng.dot(wo) >= 0.0;
        if !front {
            ng = -ng;
            ns = -ns;
        }
        if ns.dot(wo) < 1e-4 {
            ns = ng;
        }
        let brightness = if front || !material.double_sided {
            1.0
        } else {
            material.inside.clamp(0.0, 1.0)
        };
        let mut albedo = Vec3::from(material.base_color) * brightness;
        let uv = self.uvs[i0] * w + self.uvs[i1] * u + self.uvs[i2] * v;
        if material.weave != Weave::None {
            // A pixel's footprint stretches on surfaces seen at a glancing angle.
            let stretched = footprint / ng.dot(wo).abs().max(0.25);
            albedo *= weave(uv, material.weave, material.weave_mm, stretched);
        }
        // graphics printed on the piece, over the weave
        if let Some((print, alpha)) = self.decals[mi].as_ref().and_then(|d| d.sample(uv)) {
            albedo = albedo.lerp(print * brightness, alpha);
        }
        let mut shaded = *material;
        if let Some(relief) = &self.reliefs[mi] {
            let (tn, rough) = relief.map.sample(uv.x / relief.tile_mm, uv.y / relief.tile_mm);
            if relief.map.has_roughness {
                shaded.roughness = rough.clamp(0.02, 1.0);
            }
            if relief.map.has_normal && relief.strength > 0.0 {
                let bent = self.bend(prim, ns, tn, relief.strength);
                // a bump never turns the surface away from the viewer
                if bent.dot(wo) > 1e-3 {
                    ns = bent;
                }
            }
        }
        Surface {
            p,
            ng,
            ns,
            albedo,
            bsdf: Bsdf::new(&shaded, albedo, brightness),
        }
    }

    /// `ns` bent by the tangent-space normal `tn` (scaled by `strength`) in the frame triangle
    /// `prim`'s texture coordinates make. Pattern y runs down the image while a normal map's green
    /// points up it, hence the flipped bitangent.
    fn bend(&self, prim: usize, ns: Vec3, tn: [f32; 3], strength: f32) -> Vec3 {
        let [i0, i1, i2] = self.tris[prim].map(|i| i as usize);
        let (e1, e2) = (
            self.positions[i1] - self.positions[i0],
            self.positions[i2] - self.positions[i0],
        );
        let (d1, d2) = (self.uvs[i1] - self.uvs[i0], self.uvs[i2] - self.uvs[i0]);
        let det = d1.x * d2.y - d2.x * d1.y;
        if det.abs() < 1e-12 {
            return ns;
        }
        let t = (e1 * d2.y - e2 * d1.y) / det;
        // Gram-Schmidt against the shading normal; the bitangent follows the normal's side
        let t = (t - ns * ns.dot(t)).normalize_or_zero();
        if t == Vec3::ZERO {
            return ns;
        }
        let mut b = ns.cross(t);
        let uv_b = (e2 * d1.x - e1 * d2.x) / det;
        if b.dot(uv_b) < 0.0 {
            b = -b;
        }
        let tn = Vec3::new(tn[0] * strength, tn[1] * strength, tn[2].max(0.05)).normalize();
        (t * tn.x - b * tn.y + ns * tn.z).normalize_or_zero()
    }

    /// The floor as a surface (for rays that bounce off it).
    fn floor_surface(&self, ray: &Ray, t: f32) -> Surface {
        let albedo = self.floor.map_or(Vec3::splat(0.3), |f| Vec3::from(f.albedo));
        let n = if ray.dir.y < 0.0 { Vec3::Y } else { Vec3::NEG_Y };
        let material = Material::diffuse(albedo.into());
        Surface {
            p: ray.at(t),
            ng: n,
            ns: n,
            albedo,
            bsdf: Bsdf::new(&material, albedo, 1.0),
        }
    }

    /// How much of the light reaching floor point `p` is not blocked by the scene, 0..1: the
    /// shadow catcher's estimate, from one sample of every light and of the sky.
    fn catcher_visibility(&self, p: Vec3, rng: &mut Rng) -> f32 {
        let origin = p + Vec3::Y * 0.05;
        let (mut lit, mut total) = (0.0, 0.0);
        for light in &self.lights {
            let (u1, u2) = rng.next_2d();
            let q = light.center + light.half_u * (2.0 * u1 - 1.0) + light.half_v * (2.0 * u2 - 1.0);
            let to = q - origin;
            let d2 = to.length_squared();
            let dir = to / d2.sqrt();
            let (cos_f, cos_l) = (dir.y, -light.normal.dot(dir));
            if cos_f <= 0.0 || cos_l <= 0.0 {
                continue;
            }
            let w = light.power * cos_f * cos_l / d2;
            total += w;
            if !self.bvh.occluded(&Ray::new(origin, dir), d2.sqrt(), |prim| {
                self.accept(prim, u64::from(rng.next_u32()))
            }) {
                lit += w;
            }
        }
        let (u1, u2) = rng.next_2d();
        let local = cosine_hemisphere(u1, u2);
        let dir = Vec3::new(local.x, local.z, local.y);
        let w = luminance(self.sky.radiance(dir)) * PI;
        total += w;
        let seed = u64::from(rng.next_u32());
        if !self.bvh.occluded(&Ray::new(origin, dir), f32::INFINITY, |prim| {
            self.accept(prim, seed)
        }) {
            lit += w;
        }
        if total > 0.0 { lit / total } else { 1.0 }
    }

    /// Light from one sampled light reaching `s` and reflected towards `wo` (local), weighted
    /// against BSDF sampling.
    fn direct(&self, s: &Surface, frame: &Frame, wo: Vec3, rng: &mut Rng) -> Vec3 {
        let (weights, sum) = self.light_weights(s.p, s.ns);
        if sum <= 0.0 {
            return Vec3::ZERO;
        }
        let pick = rng.next_f32() * sum;
        let (mut index, mut acc) = (0, 0.0);
        for (i, w) in weights.iter().take(self.lights.len().min(8)).enumerate() {
            acc += w;
            index = i;
            if pick < acc {
                break;
            }
        }
        let light = &self.lights[index];
        let select = weights[index] / sum;
        let (u1, u2) = rng.next_2d();
        let q = light.center + light.half_u * (2.0 * u1 - 1.0) + light.half_v * (2.0 * u2 - 1.0);
        let to = q - s.p;
        let d2 = to.length_squared();
        let dist = d2.sqrt();
        let wi = to / dist;
        let cos_l = -light.normal.dot(wi);
        if cos_l <= 0.0 || wi.dot(s.ng) <= 0.0 {
            return Vec3::ZERO;
        }
        let wi_local = frame.to_local(wi);
        let f = s.bsdf.eval(wo, wi_local);
        if f == Vec3::ZERO {
            return Vec3::ZERO;
        }
        let origin = offset(s.p, s.ng);
        if self.occluded(origin, wi, dist * (1.0 - 1e-4), rng) {
            return Vec3::ZERO;
        }
        let light_pdf = select * d2 / (cos_l * light.area);
        let weight = power_heuristic(light_pdf, s.bsdf.pdf(wo, wi_local));
        f * light.radiance * (wi_local.z * weight / light_pdf)
    }

    /// Traces one camera ray; `screen` is its position in 0..1 (for the backdrop).
    pub fn trace(&self, camera_ray: Ray, screen: Vec2, params: &PathParams, rng: &mut Rng) -> PathSample {
        let mut ray = camera_ray;
        let mut beta = Vec3::ONE;
        let mut radiance = Vec3::ZERO;
        let mut guides: Option<(Vec3, Vec3, f32)> = None;
        // The previous bounce: its BSDF density, position and normal (for weighting light hits).
        let mut prev: Option<(f32, Vec3, Vec3)> = None;
        let mut bounce = 0;
        let mut travelled = 0.0;
        loop {
            let seed = u64::from(rng.next_u32());
            let hit = self
                .bvh
                .intersect(&ray, f32::INFINITY, |prim| self.accept(prim, seed));
            let t_surface = hit.map_or(f32::INFINITY, |h| h.t);
            let floor_t = self.floor_hit(&ray, t_surface);
            let t_near = floor_t.unwrap_or(t_surface);
            if let Some(prev) = prev
                && let Some(emitted) = self.emitted(&ray, t_near, prev)
            {
                radiance += clamp_luminance(beta * emitted, params.clamp);
                break;
            }
            let surface = match (floor_t, hit) {
                (Some(t), _) => {
                    if bounce == 0
                        && let Some(floor) = self.floor.filter(|f| f.shadow_catcher)
                    {
                        let p = ray.at(t);
                        let visible = self.catcher_visibility(p, rng);
                        let shade = 1.0 - floor.shadow_strength.clamp(0.0, 1.0) * (1.0 - visible);
                        radiance = self.background(ray.dir, screen) * shade;
                        guides = Some((Vec3::ONE, Vec3::Y, travelled + t));
                        break;
                    }
                    self.floor_surface(&ray, t)
                }
                (None, Some(h)) => {
                    let footprint = if bounce == 0 {
                        params.pixel_mm + params.pixel_angle * (travelled + h.t)
                    } else {
                        f32::INFINITY
                    };
                    self.surface(&ray, &h, footprint)
                }
                (None, None) => {
                    if bounce == 0 {
                        radiance = self.background(ray.dir, screen);
                        guides = Some((Vec3::ONE, Vec3::ZERO, 1e9));
                    } else {
                        radiance += clamp_luminance(beta * self.sky.radiance(ray.dir), params.clamp);
                    }
                    break;
                }
            };
            travelled += t_near;
            if guides.is_none() {
                guides = Some((surface.albedo, surface.ns, travelled));
            }
            let frame = Frame::new(surface.ns);
            let wo = frame.to_local(-ray.dir);
            let direct = beta * self.direct(&surface, &frame, wo, rng);
            radiance += if bounce == 0 {
                direct
            } else {
                clamp_luminance(direct, params.clamp)
            };
            if bounce >= params.max_bounces {
                break;
            }
            let Some((throughput, next, pdf)) = Self::scatter(&surface, &frame, wo, rng) else {
                break;
            };
            beta *= throughput;
            prev = Some((pdf, surface.p, surface.ns));
            ray = next;
            bounce += 1;
            if bounce > ROULETTE_AFTER {
                let q = beta.max_element().min(0.95);
                if rng.next_f32() >= q {
                    break;
                }
                beta /= q;
            }
        }
        let (albedo, normal, depth) = guides.unwrap_or((Vec3::ONE, Vec3::ZERO, 1e9));
        let radiance = if radiance.is_finite() {
            radiance
        } else {
            Vec3::ZERO
        };
        PathSample {
            radiance,
            albedo,
            normal,
            depth,
        }
    }

    /// Samples the BSDF at `s` for the next bounce: the throughput factor `f · cos / pdf`, the
    /// ray and the density, or `None` if the path ends.
    fn scatter(s: &Surface, frame: &Frame, wo: Vec3, rng: &mut Rng) -> Option<(Vec3, Ray, f32)> {
        let u = rng.next_f32();
        let (u1, u2) = rng.next_2d();
        let sample = s.bsdf.sample(wo, u, u1, u2)?;
        let wi = frame.to_world(sample.wi);
        let throughput = sample.f * (sample.wi.z / sample.pdf);
        (wi.dot(s.ng) > 0.0 && throughput.is_finite())
            .then(|| (throughput, Ray::new(offset(s.p, s.ng), wi), sample.pdf))
    }

    /// What a camera ray leaving the scene sees.
    fn background(&self, dir: Vec3, screen: Vec2) -> Vec3 {
        match self.backdrop {
            Some(b) => b.radiance(screen.x, screen.y),
            None => self.sky.radiance(dir),
        }
    }

    /// Number of triangles traced.
    pub fn triangle_count(&self) -> usize {
        self.bvh.len()
    }
}
