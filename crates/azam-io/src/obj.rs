//! Wavefront OBJ export of the draped garment, with a material library.

use std::fmt::Write as _;

use azam_doc::{FabricId, Project};

use crate::format::{fixed, hex_rgb, number, slug};

/// OBJ coordinates are metres; meshes are in mm.
const METRES_PER_MM: f64 = 0.001;
/// Decimals of positions and texture coordinates.
const POSITION_DIGITS: usize = 5;
/// Decimals of normals and colours.
const NORMAL_DIGITS: usize = 4;
/// Colour of a piece whose fabric is missing.
const FALLBACK_COLOR: &str = "#cccccc";
/// Material name stem of a piece whose fabric is missing.
const FALLBACK_MATERIAL: &str = "fabric";
/// Material file the OBJ refers to.
pub const MTL_FILE_NAME: &str = "garment.mtl";

/// Appearance of one material.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjMaterial {
    /// Material name (unique per fabric).
    pub name: String,
    /// Diffuse colour, `#rrggbb`.
    pub color: String,
    /// Opacity, 0–1.
    pub opacity: f64,
}

impl ObjMaterial {
    /// The material of fabric `fabric` in the active colorway: named after the fabric and its id,
    /// or a grey fallback if the fabric does not exist.
    #[must_use]
    pub fn for_fabric(project: &Project, fabric: &FabricId) -> Self {
        let id = if fabric.as_str().is_empty() {
            "x"
        } else {
            fabric.as_str()
        };
        match project.effective_fabric(fabric) {
            Some(f) => Self {
                name: format!("{}_{id}", slug(&f.name)),
                color: if f.color.is_empty() {
                    FALLBACK_COLOR.to_owned()
                } else {
                    f.color
                },
                opacity: if f.opacity == 0.0 || f.opacity.is_nan() {
                    1.0
                } else {
                    f.opacity
                },
            },
            None => Self {
                name: format!("{}_{id}", slug(FALLBACK_MATERIAL)),
                color: FALLBACK_COLOR.to_owned(),
                opacity: 1.0,
            },
        }
    }
}

/// One simulated piece to export.
#[derive(Clone, Debug)]
pub struct ObjPiece<'a> {
    /// Piece name (written as a slug).
    pub name: &'a str,
    /// Material of the piece.
    pub material: ObjMaterial,
    /// Pieces hidden in 3D are left out (their pattern still sets the texture-space bounds).
    pub hidden: bool,
    /// Vertex positions in mm.
    pub positions: &'a [[f32; 3]],
    /// Unit vertex normals.
    pub normals: &'a [[f32; 3]],
    /// Vertex positions on the 2D pattern in mm; texture coordinates are derived from them.
    pub uvs: &'a [[f64; 2]],
    /// Triangles as vertex indices.
    pub triangles: &'a [[u32; 3]],
}

/// The avatar mesh, exported with a plain skin material.
#[derive(Clone, Copy, Debug)]
pub struct ObjMesh<'a> {
    /// Vertex positions in mm.
    pub positions: &'a [[f32; 3]],
    /// Unit vertex normals.
    pub normals: &'a [[f32; 3]],
    /// Triangles as vertex indices.
    pub triangles: &'a [[u32; 3]],
}

/// An exported OBJ file and its material library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjExport {
    /// The OBJ text (refers to [`MTL_FILE_NAME`]).
    pub obj: String,
    /// The MTL text.
    pub mtl: String,
}

/// The draped pieces (and optionally the avatar) as OBJ in metres.
///
/// Texture coordinates map the bounding square of every piece's 2D pattern onto 0–1, so prints
/// line up across pieces. Each piece is an object using its fabric's material.
#[must_use]
pub fn export_obj(pieces: &[ObjPiece], avatar: Option<&ObjMesh>) -> ObjExport {
    let mut obj = format!("# azam export — units: metres\nmtllib {MTL_FILE_NAME}\n");
    let mut mtl = String::new();
    let (mut x0, mut y0, mut x1, mut y1) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in pieces {
        for uv in p.uvs {
            x0 = x0.min(uv[0]);
            y0 = y0.min(uv[1]);
            x1 = x1.max(uv[0]);
            y1 = y1.max(uv[1]);
        }
    }
    let span = (x1 - x0).max(y1 - y0);
    let span = if span == 0.0 || span.is_nan() { 1.0 } else { span };
    let mut written: Vec<&str> = Vec::new();
    let mut offset = 1;
    let v3 = |out: &mut String, tag: &str, v: &[f32; 3], scale: f64, digits: usize| {
        let _ = writeln!(
            out,
            "{tag} {} {} {}",
            fixed(f64::from(v[0]) * scale, digits),
            fixed(f64::from(v[1]) * scale, digits),
            fixed(f64::from(v[2]) * scale, digits)
        );
    };
    for p in pieces.iter().filter(|p| !p.hidden) {
        let m = &p.material;
        if !written.contains(&m.name.as_str()) {
            written.push(&m.name);
            let rgb = hex_rgb(&m.color).unwrap_or([0, 0, 0]);
            let c = |i: usize| fixed(f64::from(rgb[i]) / 255.0, NORMAL_DIGITS);
            let _ = write!(
                mtl,
                "newmtl {}\nKd {} {} {}\nNs 10\nd {}\n\n",
                m.name,
                c(0),
                c(1),
                c(2),
                number(m.opacity)
            );
        }
        let _ = write!(obj, "o {}\nusemtl {}\n", slug(p.name), m.name);
        for v in p.positions {
            v3(&mut obj, "v", v, METRES_PER_MM, POSITION_DIGITS);
        }
        for uv in p.uvs {
            let _ = writeln!(
                obj,
                "vt {} {}",
                fixed((uv[0] - x0) / span, POSITION_DIGITS),
                fixed(1.0 - (uv[1] - y0) / span, POSITION_DIGITS)
            );
        }
        for n in p.normals {
            v3(&mut obj, "vn", n, 1.0, NORMAL_DIGITS);
        }
        for t in p.triangles {
            let [a, b, c] = t.map(|i| i as usize + offset);
            let _ = writeln!(obj, "f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}");
        }
        offset += p.positions.len();
    }
    if let Some(av) = avatar {
        obj.push_str("o avatar\nusemtl avatar\n");
        mtl.push_str("newmtl avatar\nKd 0.8 0.78 0.75\n\n");
        for v in av.positions {
            v3(&mut obj, "v", v, METRES_PER_MM, POSITION_DIGITS);
        }
        for n in av.normals {
            v3(&mut obj, "vn", n, 1.0, NORMAL_DIGITS);
        }
        for t in av.triangles {
            let [a, b, c] = t.map(|i| i as usize + offset);
            let _ = writeln!(obj, "f {a}//{a} {b}//{b} {c}//{c}");
        }
    }
    ObjExport { obj, mtl }
}

/// A triangle mesh read from an OBJ file.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjImport {
    /// Vertex positions, `x, y, z` per vertex, in the file's units.
    pub positions: Vec<f32>,
    /// Triangles, three vertex indices each (polygons are fanned into triangles).
    pub indices: Vec<u32>,
}

/// Why an OBJ file could not be read as a mesh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjImportError {
    /// A number in line `line` does not parse.
    InvalidNumber {
        /// 1-based line.
        line: usize,
    },
    /// A face in line `line` names a vertex the file does not have.
    BadIndex {
        /// 1-based line.
        line: usize,
    },
    /// The file has no faces.
    Empty,
}

impl std::fmt::Display for ObjImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidNumber { line } => write!(f, "line {line}: a number does not parse"),
            Self::BadIndex { line } => write!(f, "line {line}: a face names a missing vertex"),
            Self::Empty => f.write_str("the file has no faces"),
        }
    }
}

impl std::error::Error for ObjImportError {}

/// The mesh in OBJ `text`: `v` positions and `f` faces (with or without texture and normal
/// indices, negative indices counting back from the last vertex). Everything else is skipped.
///
/// # Errors
/// [`ObjImportError`] for a bad number or index, or a file without faces.
pub fn import_obj(text: &str) -> Result<ObjImport, ObjImportError> {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for (k, raw) in text.lines().enumerate() {
        let line = k + 1;
        let mut words = raw.split_whitespace();
        match words.next() {
            Some("v") => {
                for _ in 0..3 {
                    let v: f32 = words
                        .next()
                        .and_then(|w| w.parse().ok())
                        .ok_or(ObjImportError::InvalidNumber { line })?;
                    positions.push(v);
                }
            }
            Some("f") => {
                let count = positions.len() / 3;
                let corners = words
                    .map(|w| {
                        let i: i64 = w
                            .split('/')
                            .next()
                            .and_then(|n| n.parse().ok())
                            .ok_or(ObjImportError::InvalidNumber { line })?;
                        let index = if i < 0 {
                            i64::try_from(count).unwrap_or(i64::MAX) + i
                        } else {
                            i - 1
                        };
                        u32::try_from(index)
                            .ok()
                            .filter(|&v| (v as usize) < count)
                            .ok_or(ObjImportError::BadIndex { line })
                    })
                    .collect::<Result<Vec<u32>, _>>()?;
                for w in 1..corners.len().saturating_sub(1) {
                    indices.extend([corners[0], corners[w], corners[w + 1]]);
                }
            }
            _ => {}
        }
    }
    if indices.is_empty() {
        return Err(ObjImportError::Empty);
    }
    Ok(ObjImport { positions, indices })
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn obj_faces_fan_into_triangles() {
        let text = "# quad\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nf 1/1 2/1 3/1 -1/1\n";
        let mesh = import_obj(text).expect("reads");
        assert_eq!(mesh.positions.len(), 12);
        assert_eq!(mesh.indices, vec![0, 1, 2, 0, 2, 3]);
        assert_eq!(
            import_obj("v 0 0 0\nf 1 2 3\n"),
            Err(ObjImportError::BadIndex { line: 2 })
        );
        assert_eq!(import_obj("v 0 0 0\n"), Err(ObjImportError::Empty));
    }
}
