//! U3M fabric import: the Unified 3D Material format (schema 1.0) that fabric scanners and
//! material libraries exchange, as a `.u3m` JSON file next to its images or a `.u3ma` ZIP archive
//! holding both.
//!
//! The front side's principled channels give the fabric's look: base colour and its image, the
//! normal map, roughness (constant and map), sheen and opacity. Physics measurements (a separate
//! `physics.json` of raw bending and stretch curves) are not read; the fabric keeps the physics
//! of the preset it starts from.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value;

/// Millimetres per inch.
const MM_PER_INCH: f64 = 25.4;

/// An image a U3M channel tiles over the fabric.
#[derive(Clone, Debug, PartialEq)]
pub struct U3mImage {
    /// Path of the image, relative to the `.u3m` file (or inside the archive).
    pub path: String,
    /// Width of one tile on the fabric, mm (0 when the file gives only a resolution).
    pub width_mm: f64,
    /// Resolution, dots per inch (0 when not given).
    pub dpi: f64,
}

impl U3mImage {
    /// Width of one tile on the fabric in mm: as the file gives it, else from the image's width
    /// in pixels and its resolution.
    #[must_use]
    pub fn tile_mm(&self, pixel_width: Option<u32>) -> Option<f64> {
        if self.width_mm > 0.0 {
            return Some(self.width_mm);
        }
        let px = f64::from(pixel_width?);
        (self.dpi > 0.0 && px > 0.0).then(|| px / self.dpi * MM_PER_INCH)
    }
}

/// The look of a U3M material's front side.
#[derive(Clone, Debug, PartialEq)]
pub struct U3mMaterial {
    /// Name of the material.
    pub name: String,
    /// Base colour, linear RGB 0..1.
    pub base_color: [f64; 3],
    /// Image of the base colour (the fabric's print or weave photo).
    pub base_image: Option<U3mImage>,
    /// Normal map.
    pub normal_image: Option<U3mImage>,
    /// Strength of the normal map (its texture factor), 1 when not given.
    pub normal_strength: f64,
    /// Roughness, 0..1.
    pub roughness: f64,
    /// Roughness map.
    pub roughness_image: Option<U3mImage>,
    /// Sheen, 0..1.
    pub sheen: f64,
    /// Opacity, 0..1.
    pub alpha: f64,
}

/// Why a U3M file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum U3mError {
    /// The file is not JSON.
    NotJson(String),
    /// The JSON has no `material` with a `front` side.
    NoMaterial,
    /// The archive is not a ZIP file, or holds no `.u3m` file.
    BadArchive,
}

impl fmt::Display for U3mError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJson(e) => write!(f, "not a U3M file: {e}"),
            Self::NoMaterial => f.write_str("the U3M file describes no material front side"),
            Self::BadArchive => f.write_str("not a U3M archive (a ZIP file with a .u3m file in it)"),
        }
    }
}

impl std::error::Error for U3mError {}

/// Reads a `.u3m` file's JSON.
///
/// # Errors
/// [`U3mError::NotJson`] or [`U3mError::NoMaterial`].
pub fn read_u3m(json: &str) -> Result<U3mMaterial, U3mError> {
    let doc: Value = serde_json::from_str(json).map_err(|e| U3mError::NotJson(e.to_string()))?;
    let material = doc
        .get("material")
        .filter(|m| m.is_object())
        .ok_or(U3mError::NoMaterial)?;
    let front = material
        .get("front")
        .filter(|f| f.is_object())
        .or_else(|| material.get("back").filter(|b| b.is_object()))
        .ok_or(U3mError::NoMaterial)?;
    let channel = |name: &str| front.get(name).filter(|c| c.is_object());
    let constant = |name: &str, default: f64| {
        channel(name)
            .and_then(|c| c.get("constant"))
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map_or(default, |v| v.clamp(0.0, 1.0))
    };
    let image = |name: &str| {
        let img = channel(name)?.get("texture")?.get("image")?;
        let path = img.get("path")?.as_str().filter(|p| !p.is_empty())?;
        let num = |k: &str| {
            img.get(k)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite() && *v > 0.0)
        };
        Some(U3mImage {
            path: path.replace('\\', "/"),
            width_mm: num("width").unwrap_or(0.0),
            dpi: num("dpi").unwrap_or(0.0),
        })
    };
    let base_color = channel("basecolor")
        .and_then(|c| c.get("constant"))
        .map_or([0.8; 3], |c| {
            ["r", "g", "b"].map(|k| c.get(k).and_then(Value::as_f64).unwrap_or(0.8).clamp(0.0, 1.0))
        });
    let normal_strength = channel("normal")
        .and_then(|c| c.get("texture"))
        .and_then(|t| t.get("factor"))
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .unwrap_or(1.0)
        .clamp(0.0, 2.0);
    Ok(U3mMaterial {
        name: material
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.trim().is_empty())
            .unwrap_or("U3M Fabric")
            .trim()
            .to_owned(),
        base_color,
        base_image: image("basecolor"),
        normal_image: image("normal"),
        normal_strength,
        roughness: constant("roughness", 0.8),
        roughness_image: image("roughness"),
        sheen: constant("sheen_value", 0.0),
        alpha: constant("alpha", 1.0),
    })
}

/// A U3M archive: its material and every file in it by path.
#[derive(Clone, Debug, PartialEq)]
pub struct U3mArchive {
    /// The material.
    pub material: U3mMaterial,
    /// Path of the `.u3m` file inside the archive; image paths are relative to its folder.
    pub u3m_path: String,
    /// The archive's files.
    pub files: BTreeMap<String, Vec<u8>>,
}

impl U3mArchive {
    /// The bytes of image `image`, looked up beside the `.u3m` file.
    #[must_use]
    pub fn image_bytes(&self, image: &U3mImage) -> Option<&[u8]> {
        let dir = self.u3m_path.rsplit_once('/').map_or("", |(d, _)| d);
        let joined = if dir.is_empty() {
            image.path.clone()
        } else {
            format!("{dir}/{}", image.path)
        };
        let clean = joined.trim_start_matches("./").replace("/./", "/");
        self.files
            .get(&clean)
            .or_else(|| self.files.get(&image.path))
            .map(Vec::as_slice)
    }
}

/// Reads a `.u3ma` archive.
///
/// # Errors
/// [`U3mError::BadArchive`] if it is not a ZIP with a `.u3m` file, else what [`read_u3m`] finds.
pub fn read_u3ma(bytes: &[u8]) -> Result<U3mArchive, U3mError> {
    let files = crate::zip::read_zip(bytes).ok_or(U3mError::BadArchive)?;
    let u3m_path = files
        .keys()
        .find(|k| k.to_ascii_lowercase().ends_with(".u3m"))
        .cloned()
        .ok_or(U3mError::BadArchive)?;
    let json = String::from_utf8_lossy(&files[&u3m_path]).into_owned();
    Ok(U3mArchive {
        material: read_u3m(&json)?,
        u3m_path,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "schema": "1.0",
      "material": {
        "id": "{0b6c1f9e-1111-2222-3333-444455556666}",
        "name": "Washed Twill ",
        "description": "", "created": "2026-01-01T00:00:00Z", "modified": "2026-01-01T00:00:00Z",
        "front": {
          "shader": "principled",
          "basecolor": {
            "constant": {"r": 0.2, "g": 0.3, "b": 0.5},
            "texture": {"mode": "multiply", "factor": {"r": 1, "g": 1, "b": 1},
              "image": {"path": "textures/front_basecolor.png", "width": 50, "height": 50, "dpi": 0, "repeat": null}}
          },
          "normal": {"constant": 0, "texture": {"factor": 0.6, "offset": 0,
              "image": {"path": "textures\\front_normal.png", "width": 0, "height": 0, "dpi": 300, "repeat": {"rotation": 0, "mode": "normal"}}}},
          "roughness": {"constant": 0.65, "texture": null},
          "sheen_value": {"constant": 0.3, "texture": null},
          "alpha": {"constant": 1, "texture": null}
        },
        "back": null
      },
      "custom": null
    }"#;

    #[test]
    #[allow(clippy::float_cmp, reason = "values copied from the file")]
    fn the_front_side_gives_colour_maps_and_finish() {
        let m = read_u3m(SAMPLE).expect("readable");
        assert_eq!(m.name, "Washed Twill");
        assert_eq!(m.base_color, [0.2, 0.3, 0.5]);
        let base = m.base_image.as_ref().expect("base image");
        assert_eq!(base.path, "textures/front_basecolor.png");
        assert_eq!(base.tile_mm(Some(1024)), Some(50.0));
        let normal = m.normal_image.as_ref().expect("normal map");
        assert_eq!(normal.path, "textures/front_normal.png");
        // 600 px at 300 dpi is two inches
        assert!((normal.tile_mm(Some(600)).expect("from dpi") - 50.8).abs() < 1e-9);
        assert_eq!(normal.tile_mm(None), None);
        assert!((m.normal_strength - 0.6).abs() < 1e-12);
        assert!((m.roughness - 0.65).abs() < 1e-12 && m.roughness_image.is_none());
        assert!((m.sheen - 0.3).abs() < 1e-12 && (m.alpha - 1.0).abs() < 1e-12);
        assert_eq!(read_u3m("{}"), Err(U3mError::NoMaterial));
        assert!(matches!(read_u3m("nope"), Err(U3mError::NotJson(_))));
    }

    #[test]
    fn an_archive_finds_its_u3m_and_images() {
        let zip = crate::zip::write_zip(&[
            ("Twill/material.u3m", SAMPLE.as_bytes()),
            ("Twill/textures/front_basecolor.png", b"base"),
            ("Twill/textures/front_normal.png", b"normal"),
        ]);
        let a = read_u3ma(&zip).expect("readable archive");
        assert_eq!(a.u3m_path, "Twill/material.u3m");
        let base = a.material.base_image.as_ref().expect("base image");
        assert_eq!(a.image_bytes(base), Some(&b"base"[..]));
        let normal = a.material.normal_image.as_ref().expect("normal map");
        assert_eq!(a.image_bytes(normal), Some(&b"normal"[..]));
        assert_eq!(read_u3ma(b"plain bytes"), Err(U3mError::BadArchive));
    }
}
