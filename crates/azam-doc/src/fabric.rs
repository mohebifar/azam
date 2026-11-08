//! Fabrics: appearance, physical properties on CLO's 0–100 scales, presets and the conversions
//! from those scales to physical units.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ids::{FabricId, IdGen, prefix};

/// Upper end of the 0–100 property scales.
const SCALE_MAX: f64 = 100.0;
/// Stretch modulus at scale 0, as a power of ten of N/m.
const E2D_LOG_MIN: f64 = 0.9;
/// Decades of stretch modulus spanned by the 0–100 scale.
const E2D_LOG_SPAN: f64 = 3.0;
/// Fraction of the modulus used by the stretch constraint (accounts for the triangle layout).
const STRETCH_MODULUS_FACTOR: f64 = 0.87;
/// N/m expressed in g/s² (mm, g, s units): 1 N/m = 1 kg/s² = 1000 g/s².
const NEWTON_PER_M_IN_G_PER_S2: f64 = 1000.0;
/// Bending compliance at scale 0, as a power of ten.
const BEND_LOG_MAX: f64 = 2.8;
/// Decades of bending compliance spanned by the 0–100 scale.
const BEND_LOG_SPAN: f64 = 3.8;
/// Thinnest cloth used when converting tension to stress, in mm.
const MIN_THICKNESS_MM: f64 = 0.1;
/// Thickness assumed when a fabric gives none, in mm.
const DEFAULT_THICKNESS_MM: f64 = 0.5;
/// Strain the stiffest cloth can take before it counts as over its limit.
const LIMIT_STRAIN_MIN: f64 = 0.02;
/// Extra strain allowed for the softest cloth.
const LIMIT_STRAIN_SPAN: f64 = 0.5;

/// Default texture repeat scale of a new fabric.
const DEFAULT_SCALE: f64 = 6.0;
/// Default base colour of a new fabric.
const DEFAULT_COLOR: &str = "#d9d6cf";
/// Default secondary colour of a new fabric.
const DEFAULT_COLOR2: &str = "#ffffff";

/// Physical fabric properties. Stretch, shear, bending, buckling, friction and damping are on 0–100
/// scales; density is in g/m² and thickness in mm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FabricPhysics {
    /// Stretch stiffness across the grain (weft), 0–100.
    pub stretch_weft: f64,
    /// Stretch stiffness along the grain (warp), 0–100.
    pub stretch_warp: f64,
    /// Shear (bias) stiffness, 0–100.
    pub shear: f64,
    /// Bending stiffness across the grain, 0–100.
    pub bend_weft: f64,
    /// Bending stiffness along the grain, 0–100.
    pub bend_warp: f64,
    /// Buckling ratio, 0–100.
    pub buckling: f64,
    /// Areal density in g/m².
    pub density: f64,
    /// Thickness in mm.
    pub thickness: f64,
    /// Friction, 0–100.
    pub friction: f64,
    /// Internal damping, 0–100.
    pub damping: f64,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for FabricPhysics {
    fn default() -> Self {
        FabricPreset::Poplin.spec().physics()
    }
}

/// One editable physical property, with its label for the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhysKey {
    /// [`FabricPhysics::stretch_weft`].
    StretchWeft,
    /// [`FabricPhysics::stretch_warp`].
    StretchWarp,
    /// [`FabricPhysics::shear`].
    Shear,
    /// [`FabricPhysics::bend_weft`].
    BendWeft,
    /// [`FabricPhysics::bend_warp`].
    BendWarp,
    /// [`FabricPhysics::buckling`].
    Buckling,
    /// [`FabricPhysics::density`].
    Density,
    /// [`FabricPhysics::thickness`].
    Thickness,
    /// [`FabricPhysics::friction`].
    Friction,
    /// [`FabricPhysics::damping`].
    Damping,
}

impl PhysKey {
    /// Every property, in panel order.
    pub const ALL: [Self; 10] = [
        Self::StretchWeft,
        Self::StretchWarp,
        Self::Shear,
        Self::BendWeft,
        Self::BendWarp,
        Self::Buckling,
        Self::Density,
        Self::Thickness,
        Self::Friction,
        Self::Damping,
    ];

    /// Stored field name.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::StretchWeft => "stretchWeft",
            Self::StretchWarp => "stretchWarp",
            Self::Shear => "shear",
            Self::BendWeft => "bendWeft",
            Self::BendWarp => "bendWarp",
            Self::Buckling => "buckling",
            Self::Density => "density",
            Self::Thickness => "thickness",
            Self::Friction => "friction",
            Self::Damping => "damping",
        }
    }

    /// Label shown in the fabric panel.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::StretchWeft => "Stretch-Weft",
            Self::StretchWarp => "Stretch-Warp",
            Self::Shear => "Shear (bias)",
            Self::BendWeft => "Bending-Weft",
            Self::BendWarp => "Bending-Warp",
            Self::Buckling => "Buckling ratio",
            Self::Density => "Density (g/m²)",
            Self::Thickness => "Thickness (mm)",
            Self::Friction => "Friction",
            Self::Damping => "Internal damping",
        }
    }

    /// The value of this property in `phys`.
    #[must_use]
    pub fn get(self, phys: &FabricPhysics) -> f64 {
        match self {
            Self::StretchWeft => phys.stretch_weft,
            Self::StretchWarp => phys.stretch_warp,
            Self::Shear => phys.shear,
            Self::BendWeft => phys.bend_weft,
            Self::BendWarp => phys.bend_warp,
            Self::Buckling => phys.buckling,
            Self::Density => phys.density,
            Self::Thickness => phys.thickness,
            Self::Friction => phys.friction,
            Self::Damping => phys.damping,
        }
    }

    /// Mutable access to this property in `phys`.
    pub fn get_mut(self, phys: &mut FabricPhysics) -> &mut f64 {
        match self {
            Self::StretchWeft => &mut phys.stretch_weft,
            Self::StretchWarp => &mut phys.stretch_warp,
            Self::Shear => &mut phys.shear,
            Self::BendWeft => &mut phys.bend_weft,
            Self::BendWarp => &mut phys.bend_warp,
            Self::Buckling => &mut phys.buckling,
            Self::Density => &mut phys.density,
            Self::Thickness => &mut phys.thickness,
            Self::Friction => &mut phys.friction,
            Self::Damping => &mut phys.damping,
        }
    }
}

/// Built-in fabric presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FabricPreset {
    /// Cotton poplin.
    Poplin,
    /// Cotton jersey.
    Jersey,
    /// 1×1 rib knit.
    Rib,
    /// 12 oz denim.
    Denim,
    /// Silk charmeuse.
    Silk,
    /// Polyester chiffon.
    Chiffon,
    /// Wool suiting.
    Wool,
    /// Linen.
    Linen,
    /// Duchess satin.
    Satin,
    /// Lamb leather.
    Leather,
    /// Scuba neoprene.
    Scuba,
    /// Fleece.
    Fleece,
    /// Corduroy.
    Corduroy,
}

/// Appearance and physics of a preset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresetSpec {
    /// Display name.
    pub name: &'static str,
    /// Weave used for the procedural texture.
    pub weave: &'static str,
    /// Surface roughness, 0–1.
    pub rough: f64,
    /// Sheen, 0–1.
    pub sheen: f64,
    /// Opacity, 0–1, for sheer fabrics.
    pub opacity: Option<f64>,
    /// Physics values in [`PhysKey::ALL`] order.
    pub phys: [f64; 10],
}

impl PresetSpec {
    /// The preset's physics.
    #[must_use]
    pub fn physics(&self) -> FabricPhysics {
        let p = self.phys;
        FabricPhysics {
            stretch_weft: p[0],
            stretch_warp: p[1],
            shear: p[2],
            bend_weft: p[3],
            bend_warp: p[4],
            buckling: p[5],
            density: p[6],
            thickness: p[7],
            friction: p[8],
            damping: p[9],
            extra: Map::new(),
        }
    }
}

const fn spec(
    name: &'static str,
    weave: &'static str,
    rough: f64,
    sheen: f64,
    opacity: Option<f64>,
    phys: [f64; 10],
) -> PresetSpec {
    PresetSpec {
        name,
        weave,
        rough,
        sheen,
        opacity,
        phys,
    }
}

impl FabricPreset {
    /// Every preset, in menu order.
    pub const ALL: [Self; 13] = [
        Self::Poplin,
        Self::Jersey,
        Self::Rib,
        Self::Denim,
        Self::Silk,
        Self::Chiffon,
        Self::Wool,
        Self::Linen,
        Self::Satin,
        Self::Leather,
        Self::Scuba,
        Self::Fleece,
        Self::Corduroy,
    ];

    /// Stored key of the preset.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Poplin => "poplin",
            Self::Jersey => "jersey",
            Self::Rib => "rib",
            Self::Denim => "denim",
            Self::Silk => "silk",
            Self::Chiffon => "chiffon",
            Self::Wool => "wool",
            Self::Linen => "linen",
            Self::Satin => "satin",
            Self::Leather => "leather",
            Self::Scuba => "scuba",
            Self::Fleece => "fleece",
            Self::Corduroy => "corduroy",
        }
    }

    /// The preset with stored key `key`.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    /// Appearance and physics of the preset.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one table row per preset")]
    pub fn spec(self) -> PresetSpec {
        match self {
            Self::Poplin => spec(
                "Cotton Poplin",
                "plain",
                0.82,
                0.1,
                None,
                [72.0, 76.0, 60.0, 34.0, 36.0, 50.0, 120.0, 0.3, 30.0, 30.0],
            ),
            Self::Jersey => spec(
                "Cotton Jersey",
                "jersey",
                0.9,
                0.05,
                None,
                [16.0, 30.0, 12.0, 12.0, 14.0, 70.0, 165.0, 0.55, 35.0, 30.0],
            ),
            Self::Rib => spec(
                "Rib Knit 1×1",
                "rib",
                0.9,
                0.05,
                None,
                [6.0, 40.0, 8.0, 14.0, 16.0, 70.0, 230.0, 0.8, 40.0, 30.0],
            ),
            Self::Denim => spec(
                "Denim 12 oz",
                "denim",
                0.88,
                0.05,
                None,
                [80.0, 84.0, 70.0, 62.0, 66.0, 40.0, 400.0, 0.9, 40.0, 35.0],
            ),
            Self::Silk => spec(
                "Silk Charmeuse",
                "satin",
                0.35,
                0.8,
                None,
                [60.0, 64.0, 28.0, 4.0, 5.0, 60.0, 80.0, 0.2, 15.0, 20.0],
            ),
            Self::Chiffon => spec(
                "Polyester Chiffon",
                "plain",
                0.6,
                0.3,
                Some(0.82),
                [55.0, 58.0, 25.0, 2.0, 2.0, 60.0, 45.0, 0.15, 15.0, 20.0],
            ),
            Self::Wool => spec(
                "Wool Suiting",
                "twill",
                0.85,
                0.1,
                None,
                [70.0, 72.0, 55.0, 42.0, 44.0, 45.0, 260.0, 0.6, 35.0, 35.0],
            ),
            Self::Linen => spec(
                "Linen",
                "plain",
                0.92,
                0.05,
                None,
                [76.0, 78.0, 66.0, 46.0, 48.0, 40.0, 180.0, 0.4, 40.0, 30.0],
            ),
            Self::Satin => spec(
                "Duchess Satin",
                "satin",
                0.3,
                0.9,
                None,
                [66.0, 70.0, 40.0, 30.0, 32.0, 45.0, 200.0, 0.35, 15.0, 25.0],
            ),
            Self::Leather => spec(
                "Lamb Leather",
                "leather",
                0.55,
                0.4,
                None,
                [86.0, 86.0, 80.0, 74.0, 74.0, 30.0, 700.0, 1.0, 45.0, 40.0],
            ),
            Self::Scuba => spec(
                "Scuba Neoprene",
                "none",
                0.7,
                0.15,
                None,
                [30.0, 34.0, 30.0, 58.0, 58.0, 30.0, 300.0, 2.0, 40.0, 40.0],
            ),
            Self::Fleece => spec(
                "Fleece",
                "fleece",
                0.98,
                0.0,
                None,
                [25.0, 35.0, 22.0, 30.0, 30.0, 50.0, 280.0, 2.5, 50.0, 40.0],
            ),
            Self::Corduroy => spec(
                "Corduroy",
                "corduroy",
                0.9,
                0.1,
                None,
                [74.0, 78.0, 60.0, 52.0, 56.0, 40.0, 320.0, 1.1, 40.0, 35.0],
            ),
        }
    }
}

/// [`Fabric::extra`] key: path of an image tiled over the fabric (its print or weave photo).
pub const FABRIC_TEXTURE_KEY: &str = "texture";
/// [`Fabric::extra`] key: width of one tile of the texture on the cloth, mm.
pub const FABRIC_TEXTURE_MM_KEY: &str = "textureMm";
/// Tile width of a texture when the fabric gives none, mm.
pub const DEFAULT_TEXTURE_MM: f64 = 100.0;
/// [`Fabric::extra`] key: path of a tangent-space normal map tiled over the fabric (OpenGL
/// convention: green points up the image).
pub const FABRIC_NORMAL_MAP_KEY: &str = "normalMap";
/// [`Fabric::extra`] key: path of a roughness map tiled over the fabric (white is matte).
pub const FABRIC_ROUGHNESS_MAP_KEY: &str = "roughnessMap";
/// [`Fabric::extra`] key: strength of the normal map, 0 (flat) .. 2.
pub const FABRIC_NORMAL_STRENGTH_KEY: &str = "normalStrength";
/// [`Fabric::extra`] key: width of one tile of the normal and roughness maps on the cloth, mm.
pub const FABRIC_MAP_MM_KEY: &str = "mapMm";
/// Normal map strength when the fabric gives none.
pub const DEFAULT_NORMAL_STRENGTH: f64 = 1.0;
/// Strongest normal map a fabric may ask for.
pub const MAX_NORMAL_STRENGTH: f64 = 2.0;

/// The surface relief of a fabric: images tiled over the cloth that bend its shading normal and
/// set its roughness texel by texel.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceMaps<'a> {
    /// Path of the normal map, if any.
    pub normal: Option<&'a str>,
    /// Path of the roughness map, if any.
    pub roughness: Option<&'a str>,
    /// Width of one tile on the cloth, mm.
    pub tile_mm: f64,
    /// Normal map strength, 0..=[`MAX_NORMAL_STRENGTH`].
    pub strength: f64,
}

/// A fabric of the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fabric {
    /// Id of the fabric.
    pub id: FabricId,
    /// Display name.
    pub name: String,
    /// Key of the preset it was made from.
    pub preset: String,
    /// Base colour, `#rrggbb`.
    pub color: String,
    /// Secondary colour, `#rrggbb`.
    pub color2: String,
    /// Weave used for the procedural texture.
    pub weave: String,
    /// Texture repeat scale.
    pub scale: f64,
    /// Surface roughness, 0–1.
    pub rough: f64,
    /// Sheen, 0–1.
    pub sheen: f64,
    /// Opacity, 0–1.
    pub opacity: f64,
    /// Physical properties.
    pub phys: FabricPhysics,
    /// Fields this version does not interpret (content, price, width, texture…), kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Fabric {
    fn default() -> Self {
        Self::from_preset(FabricPreset::Poplin, FabricId::default())
    }
}

impl Fabric {
    /// The image tiled over the fabric and the width of one tile in mm, if it has one.
    #[must_use]
    pub fn texture(&self) -> Option<(&str, f64)> {
        let path = self
            .extra
            .get(FABRIC_TEXTURE_KEY)?
            .as_str()
            .filter(|p| !p.is_empty())?;
        let mm = self
            .extra
            .get(FABRIC_TEXTURE_MM_KEY)
            .and_then(Value::as_f64)
            .filter(|v| *v > 0.0)
            .unwrap_or(DEFAULT_TEXTURE_MM);
        Some((path, mm))
    }

    /// The fabric's normal and roughness maps, if it has either. Their tile width is their own,
    /// else the texture's, else [`DEFAULT_TEXTURE_MM`].
    #[must_use]
    pub fn surface_maps(&self) -> Option<SurfaceMaps<'_>> {
        let path = |key: &str| self.extra.get(key)?.as_str().filter(|p| !p.is_empty());
        let (normal, roughness) = (path(FABRIC_NORMAL_MAP_KEY), path(FABRIC_ROUGHNESS_MAP_KEY));
        if normal.is_none() && roughness.is_none() {
            return None;
        }
        let num = |key: &str| {
            self.extra
                .get(key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
        };
        let tile_mm = num(FABRIC_MAP_MM_KEY)
            .filter(|v| *v > 0.0)
            .or_else(|| self.texture().map(|(_, mm)| mm))
            .unwrap_or(DEFAULT_TEXTURE_MM);
        let strength = num(FABRIC_NORMAL_STRENGTH_KEY)
            .unwrap_or(DEFAULT_NORMAL_STRENGTH)
            .clamp(0.0, MAX_NORMAL_STRENGTH);
        Some(SurfaceMaps {
            normal,
            roughness,
            tile_mm,
            strength,
        })
    }

    /// A fabric with the appearance and physics of `preset` and the default colours.
    #[must_use]
    pub fn from_preset(preset: FabricPreset, id: FabricId) -> Self {
        let p = preset.spec();
        Self {
            id,
            name: p.name.to_owned(),
            preset: preset.key().to_owned(),
            color: DEFAULT_COLOR.to_owned(),
            color2: DEFAULT_COLOR2.to_owned(),
            weave: p.weave.to_owned(),
            scale: DEFAULT_SCALE,
            rough: p.rough,
            sheen: p.sheen,
            opacity: p.opacity.unwrap_or(1.0),
            phys: p.physics(),
            extra: Map::new(),
        }
    }
}

/// A new fabric from `preset`, with a generated id (callers usually replace the id and name).
///
/// An id is generated even when the caller then replaces it, so id sequences stay the same as in
/// the recorded reference data.
pub fn make_fabric(ids: &mut IdGen, preset: FabricPreset) -> Fabric {
    Fabric::from_preset(preset, FabricId::new(ids.fresh(prefix::FABRIC)))
}

/// 2D tensile modulus in N/m for a stretch value on the 0–100 scale.
#[must_use]
pub fn e2d(k: f64) -> f64 {
    10f64.powf(E2D_LOG_MIN + E2D_LOG_SPAN * k.clamp(0.0, SCALE_MAX) / SCALE_MAX)
}

/// Stretch compliance in s²/g (mm, g, s units) for a stretch value on the 0–100 scale.
#[must_use]
pub fn r_stretch(k: f64) -> f64 {
    1.0 / (STRETCH_MODULUS_FACTOR * e2d(k) * NEWTON_PER_M_IN_G_PER_S2)
}

/// Bending compliance for a bending value on the 0–100 scale.
#[must_use]
pub fn r_bend(k: f64) -> f64 {
    10f64.powf(BEND_LOG_MAX - BEND_LOG_SPAN * k.clamp(0.0, SCALE_MAX) / SCALE_MAX)
}

/// Stress in kPa at `strain` for a stretch value on the 0–100 scale and a thickness in mm
/// (`None` or 0 uses the default thickness).
#[must_use]
pub fn stress_kpa(k: f64, strain: f64, thickness_mm: Option<f64>) -> f64 {
    let thickness = thickness_mm
        .filter(|&t| t != 0.0 && !t.is_nan())
        .unwrap_or(DEFAULT_THICKNESS_MM);
    e2d(k) * strain / thickness.max(MIN_THICKNESS_MM)
}

/// Strain at which cloth with this stretch value counts as over its limit in fit maps.
#[must_use]
pub fn limit_strain(k: f64) -> f64 {
    let q = 1.0 - k.clamp(0.0, SCALE_MAX) / SCALE_MAX;
    LIMIT_STRAIN_MIN + q * q * LIMIT_STRAIN_SPAN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_maps_fall_back_to_the_texture_tile_and_clamp_their_strength() {
        let mut f = Fabric::default();
        assert!(f.surface_maps().is_none());
        f.extra
            .insert(FABRIC_ROUGHNESS_MAP_KEY.into(), "rough.png".into());
        let m = f.surface_maps().expect("a roughness map is enough");
        assert_eq!((m.normal, m.roughness), (None, Some("rough.png")));
        assert!((m.tile_mm - DEFAULT_TEXTURE_MM).abs() < 1e-12);
        assert!((m.strength - DEFAULT_NORMAL_STRENGTH).abs() < 1e-12);
        f.extra.insert(FABRIC_TEXTURE_KEY.into(), "print.png".into());
        f.extra.insert(FABRIC_TEXTURE_MM_KEY.into(), 30.0.into());
        f.extra.insert(FABRIC_NORMAL_STRENGTH_KEY.into(), 9.0.into());
        let m = f.surface_maps().expect("maps");
        assert!((m.tile_mm - 30.0).abs() < 1e-12, "the texture's tile");
        assert!((m.strength - MAX_NORMAL_STRENGTH).abs() < 1e-12);
        f.extra.insert(FABRIC_MAP_MM_KEY.into(), 12.0.into());
        assert!(
            (f.surface_maps().expect("maps").tile_mm - 12.0).abs() < 1e-12,
            "their own tile"
        );
    }

    #[test]
    fn scales_are_clamped() {
        assert!((e2d(-5.0) - e2d(0.0)).abs() < 1e-12);
        assert!((e2d(150.0) - 10f64.powf(3.9)).abs() < 1e-9);
        assert!((limit_strain(100.0) - LIMIT_STRAIN_MIN).abs() < 1e-15);
    }

    #[test]
    fn presets_round_trip_their_keys() {
        for p in FabricPreset::ALL {
            assert_eq!(FabricPreset::from_key(p.key()), Some(p));
        }
    }
}
