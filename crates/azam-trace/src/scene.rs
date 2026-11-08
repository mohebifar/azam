//! The scene description: meshes with cloth materials, and the studio lighting.

use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};

use crate::Weave;

/// A surface material: cloth (Lambert, sheen, GGX specular) with an optional procedural weave.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Base colour, linear RGB 0..1.
    pub base_color: [f32; 3],
    /// Roughness of the specular lobe, 0 (glossy satin) .. 1 (matte).
    pub roughness: f32,
    /// Strength of the cloth sheen lobe (the bright rim of fibres at grazing angles), 0..1.
    pub sheen: f32,
    /// How much the sheen takes the base colour (0: white sheen).
    pub sheen_tint: f32,
    /// Strength of the GGX specular lobe, 0..1 (0.5 is a dielectric's 4 % reflectance).
    pub specular: f32,
    /// Opacity, 0..1; light passes through the rest (sheer fabrics).
    pub opacity: f32,
    /// Procedural weave drawn from the texture coordinates.
    pub weave: Weave,
    /// Repeat of the weave, in mm.
    pub weave_mm: f32,
    /// Whether back faces are shaded as the inside of the fabric (cloth is two-sided).
    pub double_sided: bool,
    /// Brightness of the inside relative to the outside, 0..1.
    pub inside: f32,
}

impl Material {
    /// A matte two-sided cloth of `color` (linear RGB).
    #[must_use]
    pub const fn cloth(color: [f32; 3]) -> Self {
        Self {
            base_color: color,
            roughness: 0.8,
            sheen: 0.5,
            sheen_tint: 0.3,
            specular: 0.15,
            opacity: 1.0,
            weave: Weave::None,
            weave_mm: 1.6,
            double_sided: true,
            inside: 0.62,
        }
    }

    /// A fabric as azam's fabric library describes it: `color` (linear RGB), surface roughness
    /// `rough` (0 glossy .. 1 matte) and shine `shine` (0 matte cotton .. 1 satin), both 0..1.
    ///
    /// Shiny fabrics get a stronger, tighter GGX lobe; matte, fuzzy ones a stronger sheen. The
    /// specular lobe is never mirror-like: woven fibres scatter even the glossiest satin's
    /// highlight.
    #[must_use]
    pub fn fabric(color: [f32; 3], rough: f32, shine: f32) -> Self {
        let rough = rough.clamp(0.0, 1.0);
        let shine = shine.clamp(0.0, 1.0);
        Self {
            roughness: 0.35 + 0.65 * rough,
            specular: 0.15 + 0.6 * shine,
            sheen: 0.25 + 0.5 * rough * (1.0 - shine),
            ..Self::cloth(color)
        }
    }

    /// A plain diffuse (Lambert) surface of `color` (linear RGB), for tests and props.
    #[must_use]
    pub const fn diffuse(color: [f32; 3]) -> Self {
        Self {
            base_color: color,
            roughness: 1.0,
            sheen: 0.0,
            sheen_tint: 0.0,
            specular: 0.0,
            opacity: 1.0,
            weave: Weave::None,
            weave_mm: 1.6,
            double_sided: false,
            inside: 1.0,
        }
    }

    /// A smooth, slightly glossy surface such as a mannequin's skin.
    #[must_use]
    pub const fn skin(color: [f32; 3]) -> Self {
        Self {
            base_color: color,
            roughness: 0.5,
            sheen: 0.15,
            sheen_tint: 0.5,
            specular: 0.35,
            opacity: 1.0,
            weave: Weave::None,
            weave_mm: 1.6,
            double_sided: false,
            inside: 1.0,
        }
    }
}

/// A triangle mesh in mm.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Vertex normals, one per position (empty: flat shading from the triangles).
    pub normals: Vec<[f32; 3]>,
    /// Texture coordinates in mm (a garment piece's pattern coordinates), one per position
    /// (empty: no weave).
    pub uvs: Vec<[f32; 2]>,
    /// Triangle list, three indices per triangle.
    pub indices: Vec<u32>,
    /// The surface material.
    pub material: Material,
    /// An image printed on the surface (a piece's graphics), placed by the texture coordinates.
    pub decal: Option<Arc<Decal>>,
    /// The fabric's normal and roughness maps, tiled over the texture coordinates.
    pub relief: Option<Relief>,
}

/// A surface map tiled over a mesh's texture coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Relief {
    /// The packed normal and roughness maps.
    pub map: Arc<azam_render::SurfaceMap>,
    /// Width of one tile in texture coordinates (mm).
    pub tile_mm: f32,
    /// Strength of the normal map, 0 (flat) .. 2.
    pub strength: f32,
}

/// An image printed on a mesh, placed in its texture coordinates (pattern mm).
#[derive(Clone, Debug, PartialEq)]
pub struct Decal {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// sRGB RGBA bytes, row by row, top row first (the smallest v).
    pub rgba: Vec<u8>,
    /// Texture coordinates of the image's top-left corner.
    pub min: [f32; 2],
    /// Size of the image in texture coordinates.
    pub size: [f32; 2],
}

impl Decal {
    /// The linear colour and alpha (0..1) printed at texture coordinates `uv` (nearest texel),
    /// or `None` outside the image.
    #[must_use]
    pub fn sample(&self, uv: Vec2) -> Option<(Vec3, f32)> {
        if self.width == 0 || self.height == 0 || self.size[0] <= 0.0 || self.size[1] <= 0.0 {
            return None;
        }
        let s = (uv.x - self.min[0]) / self.size[0];
        let t = (uv.y - self.min[1]) / self.size[1];
        if !(0.0..1.0).contains(&s) || !(0.0..1.0).contains(&t) {
            return None;
        }
        let x = ((s * self.width as f32) as u32).min(self.width - 1);
        let y = ((t * self.height as f32) as u32).min(self.height - 1);
        let at = ((y * self.width + x) * 4) as usize;
        let px = self.rgba.get(at..at + 4)?;
        let lin = |c: u8| azam_render::srgb_to_linear(f32::from(c) / 255.0);
        Some((
            Vec3::new(lin(px[0]), lin(px[1]), lin(px[2])),
            f32::from(px[3]) / 255.0,
        ))
    }
}

/// A one-sided rectangular area light (a softbox).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaLight {
    /// Centre of the rectangle, in mm.
    pub center: Vec3,
    /// Half of one side, as a vector in mm.
    pub half_u: Vec3,
    /// Half of the other side, as a vector in mm; the light shines along `half_u × half_v`.
    pub half_v: Vec3,
    /// Emitted radiance, linear RGB.
    pub radiance: [f32; 3],
}

impl AreaLight {
    /// A `width` × `height` mm softbox at `center` aimed at `target`, emitting `radiance`.
    #[must_use]
    pub fn aimed(center: Vec3, target: Vec3, width: f32, height: f32, radiance: [f32; 3]) -> Self {
        let forward = (target - center).normalize_or(Vec3::NEG_Y);
        let side = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = side.cross(forward).normalize_or(Vec3::Z);
        // half_u × half_v must point along `forward`: side × up = −forward, so swap.
        Self {
            center,
            half_u: up * (height / 2.0),
            half_v: side * (width / 2.0),
            radiance,
        }
    }

    /// Area of the rectangle, in mm².
    #[must_use]
    pub fn area(&self) -> f32 {
        4.0 * self.half_u.cross(self.half_v).length()
    }

    /// Unit normal: the direction the light shines.
    #[must_use]
    pub fn normal(&self) -> Vec3 {
        self.half_u.cross(self.half_v).normalize_or(Vec3::NEG_Y)
    }
}

/// The environment seen by rays that leave the scene: a sky gradient, linear radiance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    /// Radiance straight up.
    pub zenith: [f32; 3],
    /// Radiance at the horizon.
    pub horizon: [f32; 3],
    /// Radiance straight down.
    pub ground: [f32; 3],
}

impl Sky {
    /// A sky of one radiance in every direction.
    #[must_use]
    pub const fn uniform(radiance: [f32; 3]) -> Self {
        Self {
            zenith: radiance,
            horizon: radiance,
            ground: radiance,
        }
    }

    /// The radiance arriving from direction `dir` (unit).
    #[must_use]
    pub fn radiance(&self, dir: Vec3) -> Vec3 {
        let (z, h, g) = (
            Vec3::from(self.zenith),
            Vec3::from(self.horizon),
            Vec3::from(self.ground),
        );
        if dir.y >= 0.0 {
            h.lerp(z, dir.y.sqrt())
        } else {
            h.lerp(g, (-dir.y).sqrt())
        }
    }
}

/// The floor: the plane y = `height`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Floor {
    /// Height of the plane, in mm.
    pub height: f32,
    /// Diffuse albedo, linear RGB (what bounce light from the floor sees).
    pub albedo: [f32; 3],
    /// When set, the camera sees the floor as the backdrop darkened by the shadows on it (a
    /// shadow catcher), so floor and backdrop meet without a horizon line.
    pub shadow_catcher: bool,
    /// How dark a full shadow makes the shadow catcher, 0..1.
    pub shadow_strength: f32,
    /// Radius of the floor around the origin, in mm.
    pub radius: f32,
}

/// The studio backdrop the camera sees behind the subject: a soft radial gradient in screen space,
/// linear radiance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Backdrop {
    /// Radiance at the centre of the frame.
    pub center: [f32; 3],
    /// Radiance at the corners.
    pub edge: [f32; 3],
}

impl Backdrop {
    /// The radiance at screen position `x`, `y` in 0..1 from the top-left.
    #[must_use]
    pub fn radiance(&self, x: f32, y: f32) -> Vec3 {
        // Centred a little high, as a lit cyclorama looks behind a standing figure.
        let r = Vec2::new(x - 0.5, (y - 0.42) * 1.15).length() / 0.75;
        let t = (r * r).min(1.0);
        Vec3::from(self.center).lerp(Vec3::from(self.edge), t)
    }
}

/// How the scene is lit and what surrounds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Lighting {
    /// Area lights (the first eight are used).
    pub lights: Vec<AreaLight>,
    /// The environment.
    pub sky: Sky,
    /// The floor, if any.
    pub floor: Option<Floor>,
    /// What camera rays that leave the scene see (`None`: the sky).
    pub backdrop: Option<Backdrop>,
}

/// Irradiance the key light gives the subject: a white diffuse surface facing it has radiance
/// 0.6, which leaves headroom for white fabrics under the tone curve.
const KEY_IRRADIANCE: f32 = 0.6 * std::f32::consts::PI;

impl Lighting {
    /// A photo-studio rig around a subject spanning `min`..`max` (mm), turned with the camera's
    /// `yaw` so the key always comes from the front right of the view: a large soft key, a dimmer
    /// fill on the other side, a rim strip from behind, a soft sky, a grey shadow-catching floor
    /// and a grey backdrop.
    #[must_use]
    pub fn studio(min: Vec3, max: Vec3, yaw: f32) -> Self {
        let center = (min + max) * 0.5;
        let size = (max - min).max_element().max(200.0);
        let turn = Mat3::from_rotation_y(yaw);
        let place = |azimuth_deg: f32, elevation_deg: f32, distance: f32| {
            let (sa, ca) = azimuth_deg.to_radians().sin_cos();
            let (se, ce) = elevation_deg.to_radians().sin_cos();
            center + turn * Vec3::new(sa * ce, se, ca * ce) * distance
        };
        // A light of area A at distance d gives about L·A/d² irradiance.
        let radiance_for = |irradiance: f32, area: f32, distance: f32, tint: [f32; 3]| {
            let l = irradiance * distance * distance / area;
            [l * tint[0], l * tint[1], l * tint[2]]
        };
        let key_d = size * 1.6;
        let key_w = size * 0.75;
        let key = AreaLight::aimed(
            place(50.0, 35.0, key_d),
            center,
            key_w,
            key_w,
            radiance_for(KEY_IRRADIANCE, key_w * key_w, key_d, [1.0, 0.97, 0.92]),
        );
        let fill_d = size * 1.8;
        let fill_w = size * 1.3;
        let fill = AreaLight::aimed(
            place(-60.0, 15.0, fill_d),
            center,
            fill_w,
            fill_w * 0.8,
            radiance_for(
                KEY_IRRADIANCE * 0.2,
                fill_w * fill_w * 0.8,
                fill_d,
                [0.92, 0.96, 1.0],
            ),
        );
        let rim_d = size * 1.4;
        let (rim_w, rim_h) = (size * 0.25, size * 0.9);
        let rim = AreaLight::aimed(
            place(-155.0, 35.0, rim_d),
            center + Vec3::Y * size * 0.1,
            rim_w,
            rim_h,
            radiance_for(KEY_IRRADIANCE * 1.1, rim_w * rim_h, rim_d, [1.0, 1.0, 1.0]),
        );
        Self {
            lights: vec![key, fill, rim],
            sky: Sky {
                zenith: [0.18, 0.19, 0.20],
                horizon: [0.13, 0.13, 0.13],
                ground: [0.05, 0.05, 0.05],
            },
            floor: Some(Floor {
                height: 0.0,
                albedo: [0.35, 0.35, 0.35],
                shadow_catcher: true,
                shadow_strength: 0.75,
                radius: size * 20.0,
            }),
            backdrop: Some(Backdrop {
                center: [0.30, 0.30, 0.30],
                edge: [0.10, 0.10, 0.10],
            }),
        }
    }
}

/// Everything to render: meshes and lighting.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// The meshes.
    pub meshes: Vec<Mesh>,
    /// The lighting.
    pub lighting: Lighting,
}

impl Scene {
    /// The bounds of every mesh vertex, or `None` for an empty scene.
    #[must_use]
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut it = self
            .meshes
            .iter()
            .flat_map(|m| m.positions.iter())
            .map(|p| Vec3::from(*p));
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(|m| m.indices.len() / 3).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aimed_light_faces_its_target() {
        let l = AreaLight::aimed(Vec3::new(0.0, 2000.0, 1000.0), Vec3::ZERO, 400.0, 200.0, [1.0; 3]);
        let to_target = (Vec3::ZERO - l.center).normalize();
        assert!(l.normal().dot(to_target) > 0.9999);
        assert!((l.area() - 80_000.0).abs() < 1.0);
    }

    #[test]
    fn sky_blends_through_the_horizon() {
        let sky = Sky {
            zenith: [1.0; 3],
            horizon: [0.5; 3],
            ground: [0.0; 3],
        };
        assert!((sky.radiance(Vec3::Y).x - 1.0).abs() < 1e-6);
        assert!((sky.radiance(Vec3::X).x - 0.5).abs() < 1e-6);
        assert!(sky.radiance(Vec3::NEG_Y).x.abs() < 1e-6);
    }

    #[test]
    fn studio_key_lights_the_front_right() {
        let rig = Lighting::studio(
            Vec3::new(-300.0, 0.0, -200.0),
            Vec3::new(300.0, 1800.0, 200.0),
            0.0,
        );
        let key = rig.lights[0];
        assert!(key.center.x > 0.0 && key.center.z > 0.0 && key.center.y > 900.0);
        // Irradiance at the subject's centre is close to the design value.
        let c = Vec3::new(0.0, 900.0, 0.0);
        let d = key.center - c;
        let e = key.radiance[0] * key.area() / d.length_squared();
        assert!((e - KEY_IRRADIANCE).abs() < 0.01, "{e}");
    }
}
