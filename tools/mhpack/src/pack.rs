//! Assembling and writing the pack.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use azam_avatar::FIT_TARGETS;
use glam::DVec3;
use serde_json::{Value, json};

use crate::derive::{BodyGeometry, derive, target_peak};
use crate::source::{BaseMesh, Skeleton, parse_target, parse_weights, subtree_weight};
use crate::{BuildError, err};

/// Default millimetres per quantised displacement unit.
pub const DEFAULT_QUANTUM_MM: f64 = 0.01;

/// Joint helper cubes copied into the pack (after the eye vertices). The first thirteen are the
/// ones the avatar requires; the rest serve posing.
pub const JOINTS: [&str; 16] = [
    "joint-l-shoulder",
    "joint-l-elbow",
    "joint-l-hand",
    "joint-l-upper-leg",
    "joint-l-knee",
    "joint-l-ankle",
    "joint-r-shoulder",
    "joint-r-elbow",
    "joint-r-hand",
    "joint-r-upper-leg",
    "joint-r-knee",
    "joint-r-ankle",
    "joint-head",
    "joint-neck",
    "joint-pelvis",
    "joint-spine-3",
];

/// Skin weight arrays: pack name and the root bone of the subtree it sums. The first four are
/// required; the rest are optional extras for posing.
pub const WEIGHTS: [(&str, &str); 9] = [
    ("w_armL", "upperarm01.L"),
    ("w_armR", "upperarm01.R"),
    ("w_legL", "upperleg01.L"),
    ("w_legR", "upperleg01.R"),
    ("w_lowerarmL", "lowerarm01.L"),
    ("w_lowerarmR", "lowerarm01.R"),
    ("w_lowerlegL", "lowerleg01.L"),
    ("w_lowerlegR", "lowerleg01.R"),
    ("w_spine", "spine03"),
];

/// Ethnic groups MakeHuman mixes equally by default.
const ETHNIC: [&str; 3] = ["african", "asian", "caucasian"];

/// What to build from.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// MakeHuman's `makehuman/data` directory (holding `3dobjs`, `targets` and `rigs`).
    pub data_dir: PathBuf,
    /// Provenance recorded in the header (repository URL and commit).
    pub source: String,
    /// Millimetres per quantised displacement unit.
    pub quantum: f64,
}

/// A built pack.
#[derive(Clone, Debug)]
pub struct Built {
    /// The pack bytes.
    pub bytes: Vec<u8>,
    /// Notes on how measures and landmarks were derived, and pack statistics.
    pub notes: Vec<String>,
}

fn read(dir: &Path, rel: &str) -> Result<String, BuildError> {
    let path = dir.join(rel);
    fs::read_to_string(&path).map_err(|e| BuildError(format!("{}: {e}", path.display())))
}

/// Triangles of `faces` (fans), remapped, wound so that the enclosed volume is positive.
fn triangles(faces: &[Vec<u32>], remap: &[Option<u32>], positions: &[DVec3]) -> Result<Vec<u32>, BuildError> {
    let mut tris = Vec::new();
    for f in faces {
        for k in 1..f.len() - 1 {
            for v in [f[0], f[k], f[k + 1]] {
                match remap[v as usize] {
                    Some(n) => tris.push(n),
                    None => return err("face uses a vertex outside the pack"),
                }
            }
        }
    }
    let volume: f64 = tris
        .chunks_exact(3)
        .map(|t| {
            let [a, b, c] = [0, 1, 2].map(|i| positions[t[i] as usize]);
            a.dot(b.cross(c))
        })
        .sum();
    if volume < 0.0 {
        for t in tris.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
    }
    Ok(tris)
}

struct Arrays {
    specs: BTreeMap<String, Value>,
    data: Vec<u8>,
}

impl Arrays {
    fn push(&mut self, name: &str, dtype: &str, length: usize, bytes: &[u8]) {
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        self.specs.insert(
            name.to_owned(),
            json!({ "dtype": dtype, "offset": self.data.len(), "length": length }),
        );
        self.data.extend_from_slice(bytes);
    }

    fn u16s(&mut self, name: &str, v: &[u32]) -> Result<(), BuildError> {
        let mut b = Vec::with_capacity(v.len() * 2);
        for &x in v {
            let x = u16::try_from(x).map_err(|_| BuildError(format!("{name}: index {x} exceeds u16")))?;
            b.extend_from_slice(&x.to_le_bytes());
        }
        self.push(name, "uint16", v.len(), &b);
        Ok(())
    }
}

/// A target remapped to pack vertices and quantised.
struct Quantised {
    indices: Vec<u32>,
    deltas: Vec<i16>,
}

fn quantise(target: &HashMap<u32, DVec3>, remap: &[Option<u32>], q: f64) -> Result<Quantised, BuildError> {
    let mut rows: Vec<(u32, [i16; 3])> = Vec::new();
    for (&v, &d) in target {
        let Some(n) = remap.get(v as usize).copied().flatten() else {
            continue;
        };
        let mut c = [0_i16; 3];
        for (k, x) in d.to_array().into_iter().enumerate() {
            let r = (x / q).round();
            if r.abs() > f64::from(i16::MAX) {
                return err(format!("displacement {x} mm does not fit the quantum {q}"));
            }
            c[k] = r as i16;
        }
        if c != [0; 3] {
            rows.push((n, c));
        }
    }
    rows.sort_unstable_by_key(|r| r.0);
    Ok(Quantised {
        indices: rows.iter().map(|r| r.0).collect(),
        deltas: rows.iter().flat_map(|r| r.1).collect(),
    })
}

/// Builds the pack from MakeHuman's data directory.
///
/// # Errors
///
/// Returns an error if a source file is missing or malformed, or a measure cannot be derived.
#[allow(clippy::too_many_lines)]
pub fn build_pack(opts: &BuildOptions) -> Result<Built, BuildError> {
    let dir = &opts.data_dir;
    let q = opts.quantum;
    let mesh = BaseMesh::parse(&read(dir, "3dobjs/base.obj")?)?;
    let skeleton = Skeleton::parse(&read(dir, "rigs/default.mhskel")?)?;
    let bone_weights = parse_weights(&read(dir, "rigs/default_weights.mhw")?)?;
    let total = mesh.positions.len();
    let mut notes = Vec::new();

    // vertex order: body, eyes, joint helpers
    let body = mesh.group_vertices("body");
    let body_count = body.len();
    if body.iter().enumerate().any(|(i, &v)| v as usize != i) {
        return err("base.obj: body vertices are not the first vertices");
    }
    let mut order: Vec<u32> = body.clone();
    let eyes: Vec<u32> = ["helper-l-eye", "helper-r-eye"]
        .iter()
        .flat_map(|g| mesh.group_vertices(g))
        .collect();
    if eyes.is_empty() {
        return err("base.obj: no eye helpers");
    }
    order.extend(&eyes);
    let mut joint_groups: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    for name in JOINTS {
        let g = mesh.group_vertices(name);
        if g.is_empty() {
            return err(format!("base.obj: no joint helper {name}"));
        }
        joint_groups.insert(name, g);
    }
    let mut joint_new: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for name in JOINTS {
        let start = order.len() as u32;
        let g = &joint_groups[name];
        order.extend(g);
        joint_new.insert(name.to_owned(), (start..start + g.len() as u32).collect());
    }
    let mut remap = vec![None; total];
    for (n, &v) in order.iter().enumerate() {
        if remap[v as usize].replace(n as u32).is_some() {
            return err(format!("vertex {v} is in more than one pack section"));
        }
    }
    let n = order.len();
    let positions: Vec<DVec3> = order.iter().map(|&v| mesh.positions[v as usize]).collect();

    let body_tris = triangles(mesh.faces("body"), &remap, &positions)?;
    let mut eye_tris = Vec::new();
    for g in ["helper-l-eye", "helper-r-eye"] {
        eye_tris.extend(triangles(mesh.faces(g), &remap, &positions)?);
    }

    // skin weights
    let mut weight_arrays: Vec<(&str, Vec<u8>, Vec<f64>)> = Vec::new();
    for (name, root) in WEIGHTS {
        if !skeleton.parents.contains_key(root) {
            return err(format!("skeleton has no bone {root}"));
        }
        let w = subtree_weight(&skeleton, &bone_weights, root, total);
        let subtree: BTreeSet<&str> = skeleton.subtree(root).into_iter().collect();
        let mut bytes: Vec<u8> = order
            .iter()
            .map(|&v| (w[v as usize] * 255.0).round() as u8)
            .collect();
        // joint helpers follow their limb rigidly: fully weighted if the joint heads a bone of
        // the subtree, else not at all
        for (jname, verts) in &joint_groups {
            let inside = skeleton.heads.iter().any(|(bone, head)| {
                subtree.contains(bone.as_str())
                    && head.len() == verts.len()
                    && head.iter().all(|h| verts.contains(h))
            });
            for &nv in &joint_new[*jname] {
                bytes[nv as usize] = if inside { 255 } else { 0 };
            }
        }
        let body_w: Vec<f64> = (0..body_count).map(|v| w[v]).collect();
        weight_arrays.push((name, bytes, body_w));
    }

    // targets
    let load = |rel: &str| -> Result<HashMap<u32, DVec3>, BuildError> {
        Ok(parse_target(&read(dir, rel)?)?.into_iter().collect())
    };
    let mut targets: Vec<(String, HashMap<u32, DVec3>)> = Vec::new();
    for (pack_name, gender) in [("g_female", "female"), ("g_male", "male")] {
        let mut sum: HashMap<u32, DVec3> = HashMap::new();
        for race in ETHNIC {
            for (v, d) in load(&format!("targets/macrodetails/{race}-{gender}-young.target"))? {
                *sum.entry(v).or_default() += d / ETHNIC.len() as f64;
            }
        }
        targets.push((pack_name.to_owned(), sum));
    }
    for gender in ["female", "male"] {
        for muscle in ["minmuscle", "averagemuscle", "maxmuscle"] {
            for weight in ["minweight", "averageweight", "maxweight"] {
                let name = format!("universal-{gender}-young-{muscle}-{weight}");
                let t = load(&format!("targets/macrodetails/{name}.target"))?;
                targets.push((name, t));
            }
        }
    }
    let mut peaks = BTreeMap::new();
    for m in FIT_TARGETS {
        for dir_name in ["incr", "decr"] {
            let name = format!("{m}-{dir_name}");
            let t = load(&format!("targets/measure/{name}.target"))?;
            if dir_name == "incr" {
                let list: Vec<(u32, DVec3)> = t.iter().map(|(&v, &d)| (v, d)).collect();
                if let Some(p) = target_peak(&list, &positions[..body_count]) {
                    peaks.insert(m.to_owned(), p);
                }
            }
            targets.push((name, t));
        }
    }
    for name in ["nipple-point-decr", "nipple-size-decr"] {
        targets.push((name.to_owned(), load(&format!("targets/breast/{name}.target"))?));
    }

    // measures and landmarks
    let mut joint_centres = HashMap::new();
    for (name, verts) in &joint_new {
        let c = verts.iter().map(|&v| positions[v as usize]).sum::<DVec3>() / verts.len() as f64;
        joint_centres.insert(name.clone(), c);
    }
    let body_faces: Vec<Vec<u32>> = mesh.faces("body").to_vec();
    let w = |name: &str| -> &[f64] {
        weight_arrays
            .iter()
            .find(|a| a.0 == name)
            .map_or(&[], |a| a.2.as_slice())
    };
    let derived = derive(
        BodyGeometry {
            positions: &positions[..body_count],
            faces: &body_faces,
            arm: [w("w_armL"), w("w_armR")],
            leg: [w("w_legL"), w("w_legR")],
            joints: &joint_centres,
        },
        &peaks,
    )?;
    notes.extend(derived.notes.iter().cloned());

    // arrays
    let mut arrays = Arrays {
        specs: BTreeMap::new(),
        data: Vec::new(),
    };
    let base: Vec<u8> = positions
        .iter()
        .flat_map(|p| p.to_array().map(|c| c as f32))
        .flat_map(f32::to_le_bytes)
        .collect();
    arrays.push("base", "float32", n * 3, &base);
    arrays.u16s("bodyTris", &body_tris)?;
    arrays.u16s("eyeTris", &eye_tris)?;
    for (name, bytes, _) in &weight_arrays {
        arrays.push(name, "uint8", bytes.len(), bytes);
    }
    let mut names = Vec::new();
    let mut displaced = 0;
    for (name, t) in &targets {
        let qt = quantise(t, &remap, q)?;
        displaced += qt.indices.len();
        arrays.u16s(&format!("ti_{name}"), &qt.indices)?;
        let d: Vec<u8> = qt.deltas.iter().flat_map(|x| x.to_le_bytes()).collect();
        arrays.push(&format!("td_{name}"), "int16", qt.deltas.len(), &d);
        names.push(name.clone());
    }
    let header = json!({
        "nV": n,
        "bodyV": body_count,
        "q": q,
        "arrays": arrays.specs,
        "targets": names,
        "measures": derived.measures,
        "landmarks": derived.landmarks,
        "joints": joint_new,
        "units": "mm",
        "license": "CC0-1.0",
        "source": opts.source,
        "generator": "tools/mhpack",
    });
    let mut text = serde_json::to_string(&header).map_err(|e| BuildError(e.to_string()))?;
    while (8 + text.len()) % 4 != 0 {
        text.push(' ');
    }
    let header_len = u32::try_from(text.len()).map_err(|_| BuildError("header too large".into()))?;
    let mut bytes = Vec::with_capacity(8 + text.len() + arrays.data.len());
    bytes.extend_from_slice(b"ATMH");
    bytes.extend_from_slice(&header_len.to_le_bytes());
    bytes.extend_from_slice(text.as_bytes());
    bytes.extend_from_slice(&arrays.data);
    notes.push(format!(
        "{n} vertices ({body_count} body, {} eye, {} joint helper), {} body and {} eye triangles, {} targets with {displaced} displaced vertices, {} bytes",
        eyes.len(),
        n - body_count - eyes.len(),
        body_tris.len() / 3,
        eye_tris.len() / 3,
        targets.len(),
        bytes.len()
    ));
    Ok(Built { bytes, notes })
}
