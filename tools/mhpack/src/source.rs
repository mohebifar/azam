//! Reading MakeHuman's asset files: the base mesh (OBJ), morph targets, skeleton and weights.

use std::collections::{BTreeSet, HashMap};

use glam::DVec3;
use serde_json::Value;

use crate::{BuildError, err};

/// MakeHuman's files are in decimetres; the pack is in millimetres.
pub const MM_PER_UNIT: f64 = 100.0;

/// The base mesh: vertex positions and the faces of each named group.
#[derive(Clone, Debug, Default)]
pub struct BaseMesh {
    /// Vertex positions in millimetres.
    pub positions: Vec<DVec3>,
    /// Faces (vertex indices, 0-based) of each group; repeated group names are merged.
    pub groups: HashMap<String, Vec<Vec<u32>>>,
}

impl BaseMesh {
    /// Parses `base.obj`.
    ///
    /// # Errors
    ///
    /// Returns an error on malformed vertex or face lines.
    pub fn parse(text: &str) -> Result<Self, BuildError> {
        let mut mesh = Self::default();
        let mut group = String::from("default");
        for (n, line) in text.lines().enumerate() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => {
                    let c: Vec<f64> = it
                        .take(3)
                        .map(str::parse)
                        .collect::<Result<_, _>>()
                        .map_err(|e| BuildError(format!("base.obj line {}: {e}", n + 1)))?;
                    if c.len() != 3 {
                        return err(format!("base.obj line {}: vertex needs 3 coordinates", n + 1));
                    }
                    mesh.positions.push(DVec3::new(c[0], c[1], c[2]) * MM_PER_UNIT);
                }
                Some("g") => it.next().unwrap_or("default").clone_into(&mut group),
                Some("f") => {
                    let face: Vec<u32> = it
                        .map(|t| {
                            t.split('/')
                                .next()
                                .and_then(|i| i.parse::<u32>().ok())
                                .and_then(|i| i.checked_sub(1))
                        })
                        .collect::<Option<_>>()
                        .ok_or_else(|| BuildError(format!("base.obj line {}: bad face", n + 1)))?;
                    if face.len() < 3 {
                        return err(format!("base.obj line {}: face has fewer than 3 corners", n + 1));
                    }
                    mesh.groups.entry(group.clone()).or_default().push(face);
                }
                _ => {}
            }
        }
        let count = mesh.positions.len();
        if mesh
            .groups
            .values()
            .flatten()
            .flatten()
            .any(|&v| v as usize >= count)
        {
            return err("base.obj: face vertex out of range");
        }
        Ok(mesh)
    }

    /// Faces of a group (empty if the group is missing).
    #[must_use]
    pub fn faces(&self, group: &str) -> &[Vec<u32>] {
        self.groups.get(group).map_or(&[], Vec::as_slice)
    }

    /// Sorted vertices used by a group's faces.
    #[must_use]
    pub fn group_vertices(&self, group: &str) -> Vec<u32> {
        self.faces(group)
            .iter()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// Parses a `.target` file: sparse vertex displacements, in millimetres.
///
/// # Errors
///
/// Returns an error on malformed lines.
pub fn parse_target(text: &str) -> Result<Vec<(u32, DVec3)>, BuildError> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let bad = || BuildError(format!("target line {}: `{line}`", n + 1));
        if f.len() < 4 {
            return Err(bad());
        }
        let v: u32 = f[0].parse().map_err(|_| bad())?;
        let mut d = [0.0; 3];
        for (k, s) in f[1..4].iter().enumerate() {
            d[k] = s.parse::<f64>().map_err(|_| bad())? * MM_PER_UNIT;
        }
        out.push((v, DVec3::from_array(d)));
    }
    Ok(out)
}

/// The bone hierarchy of a `.mhskel` file and its joint helper names.
#[derive(Clone, Debug, Default)]
pub struct Skeleton {
    /// Parent of each bone (`None` for the root).
    pub parents: HashMap<String, Option<String>>,
    /// Head joint vertex list of each bone.
    pub heads: HashMap<String, Vec<u32>>,
}

impl Skeleton {
    /// Parses a `.mhskel` (JSON) file.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is malformed or lacks bones.
    pub fn parse(text: &str) -> Result<Self, BuildError> {
        let v: Value = serde_json::from_str(text).map_err(|e| BuildError(format!("mhskel: {e}")))?;
        let bones = v
            .get("bones")
            .and_then(Value::as_object)
            .ok_or_else(|| BuildError("mhskel: no bones".into()))?;
        let joints = v.get("joints").and_then(Value::as_object);
        let mut s = Self::default();
        for (name, b) in bones {
            let parent = b.get("parent").and_then(Value::as_str).map(str::to_owned);
            s.parents.insert(name.clone(), parent);
            let head = b.get("head").and_then(Value::as_str).unwrap_or_default();
            let verts = joints
                .and_then(|j| j.get(head))
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_u64().and_then(|x| u32::try_from(x).ok()))
                        .collect()
                })
                .unwrap_or_default();
            s.heads.insert(name.clone(), verts);
        }
        Ok(s)
    }

    /// Whether `bone` is `root` or one of its descendants.
    #[must_use]
    pub fn in_subtree(&self, bone: &str, root: &str) -> bool {
        let mut cur = Some(bone.to_owned());
        while let Some(b) = cur {
            if b == root {
                return true;
            }
            cur = self.parents.get(&b).cloned().flatten();
        }
        false
    }

    /// Bones of the subtree rooted at `root`.
    #[must_use]
    pub fn subtree(&self, root: &str) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .parents
            .keys()
            .filter(|b| self.in_subtree(b, root))
            .map(String::as_str)
            .collect();
        out.sort_unstable();
        out
    }
}

/// Per-bone skin weights of a `.mhw` file.
pub type BoneWeights = HashMap<String, Vec<(u32, f64)>>;

/// Parses a `.mhw` (JSON) weights file.
///
/// # Errors
///
/// Returns an error if the JSON is malformed or lacks weights.
pub fn parse_weights(text: &str) -> Result<BoneWeights, BuildError> {
    let v: Value = serde_json::from_str(text).map_err(|e| BuildError(format!("mhw: {e}")))?;
    let w = v
        .get("weights")
        .and_then(Value::as_object)
        .ok_or_else(|| BuildError("mhw: no weights".into()))?;
    let mut out = HashMap::new();
    for (bone, list) in w {
        let pairs = list
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let p = p.as_array()?;
                        let v = u32::try_from(p.first()?.as_u64()?).ok()?;
                        Some((v, p.get(1)?.as_f64()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.insert(bone.clone(), pairs);
    }
    Ok(out)
}

/// Fraction (0..1) of each vertex's total skin weight that belongs to the subtree rooted at
/// `root` (MakeHuman normalises the weights of every vertex to sum to one).
#[must_use]
pub fn subtree_weight(
    skeleton: &Skeleton,
    weights: &BoneWeights,
    root: &str,
    vertex_count: usize,
) -> Vec<f64> {
    let mut total = vec![0.0; vertex_count];
    let mut part = vec![0.0; vertex_count];
    let bones: BTreeSet<&str> = skeleton.subtree(root).into_iter().collect();
    for (bone, list) in weights {
        let inside = bones.contains(bone.as_str());
        for &(v, w) in list {
            if let Some(t) = total.get_mut(v as usize) {
                *t += w;
                if inside {
                    part[v as usize] += w;
                }
            }
        }
    }
    part.iter()
        .zip(&total)
        .map(|(&p, &t)| if t > 0.0 { (p / t).clamp(0.0, 1.0) } else { 0.0 })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_obj_groups_targets_and_subtree_weights() {
        let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\ng body\nf 1/1 2/2 3/3\ng helper\nf 1 3 4\n";
        let mesh = BaseMesh::parse(obj).unwrap();
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.positions[1], DVec3::new(MM_PER_UNIT, 0.0, 0.0));
        assert_eq!(mesh.group_vertices("body"), vec![0, 1, 2]);
        assert_eq!(mesh.group_vertices("helper"), vec![0, 2, 3]);
        assert!(BaseMesh::parse("v 0 0 0\nf 1 2 3\n").is_err());

        let t = parse_target("# comment\n3 0.1 -0.2 0\n\n7 0 0 0.01\n").unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].0, 3);
        assert!((t[0].1 - DVec3::new(10.0, -20.0, 0.0)).length() < 1e-9);
        assert!(parse_target("3 0.1").is_err());

        let skel = Skeleton::parse(
            r#"{"bones": {"root": {"head": "r"}, "arm": {"parent": "root", "head": "a"},
                "hand": {"parent": "arm", "head": "h"}}, "joints": {"a": [5, 6]}}"#,
        )
        .unwrap();
        assert_eq!(skel.subtree("arm"), vec!["arm", "hand"]);
        assert_eq!(skel.heads["arm"], vec![5, 6]);
        let w = parse_weights(
            r#"{"weights": {"root": [[0, 1.0], [1, 0.5]], "arm": [[1, 0.25]], "hand": [[1, 0.25], [2, 2.0]]}}"#,
        )
        .unwrap();
        let share = subtree_weight(&skel, &w, "arm", 3);
        assert_eq!(share, vec![0.0, 0.5, 1.0]);
    }
}
