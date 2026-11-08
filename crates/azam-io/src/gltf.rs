//! Binary glTF 2.0 (`.glb`) export of the draped garment: one mesh per piece with its fabric's
//! colour as a PBR material, normals and texture coordinates, in metres with +Y up (glTF's own
//! conventions, which azam's 3D view shares). Optionally the avatar too.

use serde_json::{Value, json};

use crate::format::hex_rgb;
use crate::obj::{ObjMesh, ObjPiece};

/// glTF coordinates are metres; meshes are in mm.
const METRES_PER_MM: f32 = 0.001;
/// glTF component types and buffer targets.
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
/// GLB chunk types.
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;
/// Skin colour of an exported avatar (linear RGB).
const SKIN: [f32; 3] = [0.48, 0.45, 0.42];

/// Accumulates the binary buffer and the glTF objects that point into it.
#[derive(Default)]
struct Builder {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Builder {
    /// Appends `bytes` (4-byte aligned) as a buffer view; returns its index.
    fn view(&mut self, bytes: &[u8], target: u32) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        self.views.push(json!({
            "buffer": 0, "byteOffset": offset, "byteLength": bytes.len(), "target": target
        }));
        self.views.len() - 1
    }

    /// A float VEC2 or VEC3 accessor over `data` (`width` floats per element), with bounds when
    /// asked (glTF requires them for positions).
    fn floats(&mut self, data: &[f32], width: usize, bounds: bool) -> usize {
        let view = self.view(&f32_bytes(data), ARRAY_BUFFER);
        let mut acc = json!({
            "bufferView": view, "componentType": FLOAT, "count": data.len() / width,
            "type": if width == 2 { "VEC2" } else { "VEC3" },
        });
        if bounds {
            let (mut lo, mut hi) = (vec![f32::MAX; width], vec![f32::MIN; width]);
            for e in data.chunks_exact(width) {
                for k in 0..width {
                    lo[k] = lo[k].min(e[k]);
                    hi[k] = hi[k].max(e[k]);
                }
            }
            acc["min"] = json!(lo);
            acc["max"] = json!(hi);
        }
        self.accessors.push(acc);
        self.accessors.len() - 1
    }

    fn indices(&mut self, data: &[u32]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|i| i.to_le_bytes()).collect();
        let view = self.view(&bytes, ELEMENT_ARRAY_BUFFER);
        self.accessors.push(json!({
            "bufferView": view, "componentType": UNSIGNED_INT, "count": data.len(), "type": "SCALAR"
        }));
        self.accessors.len() - 1
    }
}

/// `data` as little-endian bytes (glTF's byte order).
fn f32_bytes(data: &[f32]) -> Vec<u8> {
    data.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// sRGB `#rrggbb` as linear RGB (glTF base colours are linear).
fn linear(hex: &str) -> [f32; 3] {
    let c = hex_rgb(hex).unwrap_or([204, 204, 204]);
    let lin = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    [lin(c[0]), lin(c[1]), lin(c[2])]
}

/// The draped pieces (and optionally the avatar) as a binary glTF file.
///
/// Texture coordinates map the bounding square of all the pieces' 2D patterns onto 0–1, as the
/// OBJ export does, so prints line up across pieces.
#[must_use]
pub fn export_glb(pieces: &[ObjPiece], avatar: Option<&ObjMesh>) -> Vec<u8> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for uv in pieces.iter().flat_map(|p| p.uvs) {
        (x0, y0, x1, y1) = (x0.min(uv[0]), y0.min(uv[1]), x1.max(uv[0]), y1.max(uv[1]));
    }
    let span = (x1 - x0).max(y1 - y0);
    let span = if span > 0.0 { span } else { 1.0 };
    let mut b = Builder::default();
    let (mut meshes, mut nodes, mut materials) = (Vec::new(), Vec::new(), Vec::<Value>::new());
    let mut material_of = |name: &str, color: [f32; 3], alpha: f64, roughness: f32| -> usize {
        if let Some(i) = materials.iter().position(|m| m["name"] == name) {
            return i;
        }
        let mut m = json!({
            "name": name, "doubleSided": true,
            "pbrMetallicRoughness": {
                "baseColorFactor": [color[0], color[1], color[2], alpha as f32],
                "metallicFactor": 0.0, "roughnessFactor": roughness
            }
        });
        if alpha < 1.0 {
            m["alphaMode"] = json!("BLEND");
        }
        materials.push(m);
        materials.len() - 1
    };
    for p in pieces.iter().filter(|p| !p.hidden) {
        let pos: Vec<f32> = p
            .positions
            .iter()
            .flat_map(|v| v.map(|c| c * METRES_PER_MM))
            .collect();
        let nor: Vec<f32> = p.normals.iter().flatten().copied().collect();
        let uv: Vec<f32> = p
            .uvs
            .iter()
            .flat_map(|t| [((t[0] - x0) / span) as f32, (1.0 - (t[1] - y0) / span) as f32])
            .collect();
        let idx: Vec<u32> = p.triangles.iter().flatten().copied().collect();
        let (a_pos, a_nor, a_uv, a_idx) = (
            b.floats(&pos, 3, true),
            b.floats(&nor, 3, false),
            b.floats(&uv, 2, false),
            b.indices(&idx),
        );
        let material = material_of(
            &p.material.name,
            linear(&p.material.color),
            p.material.opacity,
            0.85,
        );
        meshes.push(json!({
            "name": p.name,
            "primitives": [{
                "attributes": {"POSITION": a_pos, "NORMAL": a_nor, "TEXCOORD_0": a_uv},
                "indices": a_idx, "material": material
            }]
        }));
        nodes.push(json!({"name": p.name, "mesh": meshes.len() - 1}));
    }
    if let Some(av) = avatar {
        let pos: Vec<f32> = av
            .positions
            .iter()
            .flat_map(|v| v.map(|c| c * METRES_PER_MM))
            .collect();
        let nor: Vec<f32> = av.normals.iter().flatten().copied().collect();
        let idx: Vec<u32> = av.triangles.iter().flatten().copied().collect();
        let (a_pos, a_nor, a_idx) = (b.floats(&pos, 3, true), b.floats(&nor, 3, false), b.indices(&idx));
        let material = material_of("avatar_skin", SKIN, 1.0, 0.6);
        meshes.push(json!({
            "name": "Avatar",
            "primitives": [{"attributes": {"POSITION": a_pos, "NORMAL": a_nor}, "indices": a_idx, "material": material}]
        }));
        nodes.push(json!({"name": "Avatar", "mesh": meshes.len() - 1}));
    }
    while b.bin.len() % 4 != 0 {
        b.bin.push(0);
    }
    let doc = json!({
        "asset": {"version": "2.0", "generator": "azam"},
        "scene": 0,
        "scenes": [{"nodes": (0..nodes.len()).collect::<Vec<_>>()}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": b.accessors,
        "bufferViews": b.views,
        "buffers": [{"byteLength": b.bin.len()}],
    });
    pack(&doc, &b.bin)
}

/// The GLB container: header, the JSON chunk (space-padded) and the binary chunk.
fn pack(doc: &Value, bin: &[u8]) -> Vec<u8> {
    let mut text = doc.to_string().into_bytes();
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    let u32le = |out: &mut Vec<u8>, v: usize| out.extend_from_slice(&(v as u32).to_le_bytes());
    out.extend_from_slice(b"glTF");
    u32le(&mut out, 2);
    u32le(&mut out, total);
    u32le(&mut out, text.len());
    u32le(&mut out, JSON_CHUNK as usize);
    out.extend_from_slice(&text);
    u32le(&mut out, bin.len());
    u32le(&mut out, BIN_CHUNK as usize);
    out.extend_from_slice(bin);
    out
}

/// One mesh in one frame: vertex positions (mm) and unit normals.
pub type MeshFrame = (Vec<[f32; 3]>, Vec<[f32; 3]>);

/// The garment (and avatar) moving, as recorded frame by frame.
#[derive(Clone, Debug, Default)]
pub struct Animation {
    /// Seconds between frames.
    pub dt: f64,
    /// Per frame, per piece (in the order of the pieces given): positions and normals.
    pub pieces: Vec<Vec<MeshFrame>>,
    /// Per frame, the avatar's positions and normals, when it moves.
    pub avatar: Option<Vec<MeshFrame>>,
}

/// One mesh's frames as morph targets: per frame after the first, an accessor of position and
/// of normal offsets from the first frame.
fn morph_targets(b: &mut Builder, frames: &[MeshFrame]) -> Vec<Value> {
    let Some((p0, n0)) = frames.first() else {
        return Vec::new();
    };
    frames[1..]
        .iter()
        .map(|(p, n)| {
            let dp: Vec<f32> = p
                .iter()
                .zip(p0)
                .flat_map(|(a, z)| {
                    [
                        (a[0] - z[0]) * METRES_PER_MM,
                        (a[1] - z[1]) * METRES_PER_MM,
                        (a[2] - z[2]) * METRES_PER_MM,
                    ]
                })
                .collect();
            let dn: Vec<f32> = n
                .iter()
                .zip(n0)
                .flat_map(|(a, z)| [a[0] - z[0], a[1] - z[1], a[2] - z[2]])
                .collect();
            json!({"POSITION": b.floats(&dp, 3, true), "NORMAL": b.floats(&dn, 3, false)})
        })
        .collect()
}

/// The draped pieces moving through `anim` (their geometry as in its first frame), and the
/// avatar with them, as a binary glTF file: each mesh keeps one morph target per later frame and
/// an animation steps its weights from one frame to the next, so any glTF viewer or Blender plays
/// it back.
#[must_use]
pub fn export_glb_animated(pieces: &[ObjPiece], avatar: Option<&ObjMesh>, anim: &Animation) -> Vec<u8> {
    let first: Vec<ObjPiece> = pieces
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut p = p.clone();
            if let Some(f) = anim.pieces.first().and_then(|f| f.get(i)) {
                p.positions = &f.0;
                p.normals = &f.1;
            }
            p
        })
        .collect();
    let base = export_glb(&first, avatar);
    // take the still file apart, add the targets and the animation, and put it back together
    let json_len = u32::from_le_bytes(base[12..16].try_into().unwrap_or_default()) as usize;
    let Ok(mut doc) = serde_json::from_slice::<Value>(&base[20..20 + json_len]) else {
        return base;
    };
    let bin_at = 20 + json_len + 8;
    let mut b = Builder {
        bin: base[bin_at..].to_vec(),
        views: doc["bufferViews"].as_array().cloned().unwrap_or_default(),
        accessors: doc["accessors"].as_array().cloned().unwrap_or_default(),
    };
    let frames = anim.pieces.len();
    if frames < 2 {
        return base;
    }
    let mut series: Vec<Vec<MeshFrame>> = (0..pieces.len())
        .filter(|&i| !pieces[i].hidden)
        .map(|i| {
            anim.pieces
                .iter()
                .map(|f| f.get(i).cloned().unwrap_or_default())
                .collect()
        })
        .collect();
    if let Some(av) = &anim.avatar
        && avatar.is_some()
        && av.len() == frames
    {
        series.push(av.clone());
    }
    let times: Vec<f32> = (0..frames).map(|k| (k as f64 * anim.dt) as f32).collect();
    let input = {
        let view = b.view(&f32_bytes(&times), ARRAY_BUFFER);
        b.accessors.push(json!({
            "bufferView": view, "componentType": FLOAT, "count": frames, "type": "SCALAR",
            "min": [0.0], "max": [times[frames - 1]]
        }));
        b.accessors.len() - 1
    };
    let targets = frames - 1;
    // frame k shows target k - 1 alone (frame 0: none)
    let weights: Vec<f32> = (0..frames)
        .flat_map(|k| (0..targets).map(move |t| if k == t + 1 { 1.0 } else { 0.0 }))
        .collect();
    let output = {
        let view = b.view(&f32_bytes(&weights), ARRAY_BUFFER);
        b.accessors.push(json!({
            "bufferView": view, "componentType": FLOAT, "count": weights.len(), "type": "SCALAR"
        }));
        b.accessors.len() - 1
    };
    let (mut samplers, mut channels) = (Vec::new(), Vec::new());
    let meshes = doc["meshes"].as_array().map_or(0, Vec::len);
    for (m, frames_of) in series.iter().enumerate().take(meshes) {
        let t = morph_targets(&mut b, frames_of);
        if t.len() != targets {
            continue;
        }
        doc["meshes"][m]["primitives"][0]["targets"] = Value::Array(t);
        doc["meshes"][m]["weights"] = json!(vec![0.0; targets]);
        samplers.push(json!({"input": input, "output": output, "interpolation": "LINEAR"}));
        channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": m, "path": "weights"}}));
    }
    while b.bin.len() % 4 != 0 {
        b.bin.push(0);
    }
    doc["animations"] = json!([{"name": "Drape", "samplers": samplers, "channels": channels}]);
    doc["accessors"] = Value::Array(b.accessors);
    doc["bufferViews"] = Value::Array(b.views);
    doc["buffers"] = json!([{"byteLength": b.bin.len()}]);
    pack(&doc, &b.bin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obj::ObjMaterial;

    #[test]
    fn a_piece_becomes_a_valid_glb() {
        let positions = [[0.0, 1000.0, 0.0], [100.0, 1000.0, 0.0], [0.0, 900.0, 0.0]];
        let normals = [[0.0, 0.0, 1.0]; 3];
        let uvs = [[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]];
        let triangles = [[0, 1, 2]];
        let piece = ObjPiece {
            name: "Front",
            material: ObjMaterial {
                name: "jersey_f".into(),
                color: "#ff0000".into(),
                opacity: 1.0,
            },
            hidden: false,
            positions: &positions,
            normals: &normals,
            uvs: &uvs,
            triangles: &triangles,
        };
        let glb = export_glb(&[piece], None);
        assert_eq!(&glb[0..4], b"glTF");
        let word = |at: usize| u32::from_le_bytes(glb[at..at + 4].try_into().expect("4 bytes")) as usize;
        assert_eq!(word(4), 2, "version 2");
        assert_eq!(word(8), glb.len(), "length in the header");
        let json_len = word(12);
        assert_eq!(word(16) as u32, JSON_CHUNK);
        let doc: Value = serde_json::from_slice(&glb[20..20 + json_len]).expect("JSON chunk");
        assert_eq!(doc["meshes"][0]["name"], "Front");
        let pos = &doc["accessors"][doc["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
            .as_u64()
            .expect("index") as usize];
        assert_eq!(pos["count"], 3);
        assert!(
            (pos["max"][1].as_f64().expect("max y") - 1.0).abs() < 1e-6,
            "metres"
        );
        let bin_len = word(20 + json_len);
        assert_eq!(doc["buffers"][0]["byteLength"], bin_len);
        let base = &doc["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"];
        assert!((base[0].as_f64().expect("red") - 1.0).abs() < 1e-6 && base[1].as_f64() == Some(0.0));
    }

    #[test]
    fn recorded_frames_become_morph_targets_and_an_animation() {
        let frame = |dz: f32| {
            (
                vec![[0.0, 1000.0, dz], [100.0, 1000.0, dz], [0.0, 900.0, dz]],
                vec![[0.0, 0.0, 1.0]; 3],
            )
        };
        let (p0, n0) = frame(0.0);
        let piece = ObjPiece {
            name: "Front",
            material: ObjMaterial {
                name: "m".into(),
                color: "#808080".into(),
                opacity: 1.0,
            },
            hidden: false,
            positions: &p0,
            normals: &n0,
            uvs: &[[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]],
            triangles: &[[0, 1, 2]],
        };
        let anim = Animation {
            dt: 1.0 / 30.0,
            pieces: vec![vec![frame(0.0)], vec![frame(10.0)], vec![frame(20.0)]],
            avatar: None,
        };
        let glb = export_glb_animated(&[piece], None, &anim);
        let json_len = u32::from_le_bytes(glb[12..16].try_into().expect("4 bytes")) as usize;
        assert_eq!(
            u32::from_le_bytes(glb[8..12].try_into().expect("4 bytes")) as usize,
            glb.len()
        );
        let doc: Value = serde_json::from_slice(&glb[20..20 + json_len]).expect("JSON");
        assert_eq!(
            doc["meshes"][0]["primitives"][0]["targets"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(doc["animations"][0]["channels"][0]["target"]["path"], "weights");
        let last = &doc["accessors"][doc["meshes"][0]["primitives"][0]["targets"][1]["POSITION"]
            .as_u64()
            .expect("i") as usize];
        assert!(
            (last["max"][2].as_f64().expect("z") - 0.02).abs() < 1e-6,
            "the last frame is 20 mm forward"
        );
    }
}
