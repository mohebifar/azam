//! Reading a morph-target avatar pack.
//!
//! Layout: the magic `ATMH`, a little-endian `u32` header length, a JSON header of that many
//! bytes, then the array data. The header lists each array's element type, byte offset (from the
//! end of the header) and element count, plus the morph target names, the vertex loops and pairs
//! used for measuring, landmark vertices and joint vertex groups.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use serde::Deserialize;

use crate::measurements::{FIT_TARGETS, LOWER_ARM_LENGTH, MEASUREMENTS, MeasureKind, UPPER_ARM_LENGTH};

/// Magic bytes at the start of a pack.
const MAGIC: &[u8; 4] = b"ATMH";
/// Number of lowest and of highest body vertices added to the fitting probe (they decide the
/// body height).
const PROBE_EXTREMES: usize = 260;

/// Why a pack could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MhParseError {
    /// The data does not start with the pack magic.
    NotAPack,
    /// The data ends before the header or an array does.
    Truncated,
    /// The JSON header is malformed.
    Header(String),
    /// A required array is missing from the header.
    MissingArray(String),
    /// An array has an unknown or unsuitable element type.
    BadArrayType {
        /// Array name.
        name: String,
        /// Its element type.
        dtype: String,
    },
    /// The pack is internally inconsistent (wrong sizes, out-of-range indices, missing measures,
    /// landmarks or joints).
    Invalid(String),
    /// The pack file could not be read.
    Io(String),
}

impl fmt::Display for MhParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAPack => write!(f, "not an avatar pack"),
            Self::Truncated => write!(f, "avatar pack is truncated"),
            Self::Header(e) => write!(f, "avatar pack header: {e}"),
            Self::MissingArray(name) => write!(f, "avatar pack has no array `{name}`"),
            Self::BadArrayType { name, dtype } => {
                write!(f, "avatar pack array `{name}` has unsuitable type `{dtype}`")
            }
            Self::Invalid(e) => write!(f, "invalid avatar pack: {e}"),
            Self::Io(e) => write!(f, "avatar pack: {e}"),
        }
    }
}

impl std::error::Error for MhParseError {}

/// Sparse vertex displacements of one morph target, quantised to `i16` (multiply by
/// [`MhModel::quantum`] for millimetres).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MorphTarget {
    /// Displaced vertices.
    pub indices: Vec<u32>,
    /// Displacement `x, y, z` per displaced vertex.
    pub deltas: Vec<i16>,
}

/// Skinning weights (0–255 per vertex) of the four limbs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LimbWeights {
    /// Left arm.
    pub arm_left: Vec<u8>,
    /// Right arm.
    pub arm_right: Vec<u8>,
    /// Left leg.
    pub leg_left: Vec<u8>,
    /// Right leg.
    pub leg_right: Vec<u8>,
}

/// Optional skin weights (0–255 per vertex) for posing beyond the limb roots: the forearms and
/// shins (elbow and knee bends) and the upper body above the lower back (forward lean). Each
/// array is empty when the pack does not provide it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoseWeights {
    /// Left forearm and hand.
    pub lower_arm_left: Vec<u8>,
    /// Right forearm and hand.
    pub lower_arm_right: Vec<u8>,
    /// Left shin and foot.
    pub lower_leg_left: Vec<u8>,
    /// Right shin and foot.
    pub lower_leg_right: Vec<u8>,
    /// Upper body (chest, arms, neck and head) above the lower back.
    pub spine: Vec<u8>,
}

/// The vertices that measuring and the height range need, with targets restricted to them, so
/// the fit composes a few thousand vertices instead of the whole body.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Probe {
    /// Probe vertex `i` is model vertex `vertices[i]` (ascending).
    pub vertices: Vec<u32>,
    /// Probe index of each model vertex, or -1.
    pub local: Vec<i32>,
    /// Base positions of the probe vertices.
    pub base: Vec<f32>,
    /// Targets restricted to probe vertices, indices local.
    pub targets: HashMap<String, MorphTarget>,
    /// Whether each probe vertex belongs to the body (not the eyes).
    pub body: Vec<bool>,
}

/// A parsed morph-target avatar model.
#[derive(Clone, Debug, PartialEq)]
pub struct MhModel {
    /// Number of vertices (body first, then eyes).
    pub vertex_count: usize,
    /// Number of body vertices.
    pub body_vertex_count: usize,
    /// Millimetres per quantised displacement unit.
    pub quantum: f64,
    /// Base positions, `x, y, z` per vertex.
    pub base: Vec<f32>,
    /// Body triangles.
    pub body_triangles: Vec<u32>,
    /// Eye triangles.
    pub eye_triangles: Vec<u32>,
    /// Limb skinning weights.
    pub weights: LimbWeights,
    /// Optional weights for elbow, knee and spine bends.
    pub pose_weights: PoseWeights,
    pub(crate) targets: HashMap<String, MorphTarget>,
    pub(crate) measures: HashMap<String, Vec<u32>>,
    pub(crate) landmarks: HashMap<String, u32>,
    pub(crate) joints: HashMap<String, Vec<u32>>,
    pub(crate) probe: Probe,
}

#[derive(Deserialize)]
struct ArraySpec {
    dtype: String,
    offset: usize,
    length: usize,
}

#[derive(Deserialize)]
struct Header {
    #[serde(rename = "nV")]
    vertex_count: usize,
    #[serde(rename = "bodyV")]
    body_vertex_count: usize,
    q: f64,
    arrays: HashMap<String, ArraySpec>,
    targets: Vec<String>,
    measures: HashMap<String, Vec<u32>>,
    #[serde(default)]
    landmarks: HashMap<String, serde_json::Value>,
    joints: HashMap<String, Vec<u32>>,
}

/// Joint vertex groups the pose and the landmarks need.
pub(crate) const JOINT_NAMES: [&str; 13] = [
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
];
/// Landmark vertices the measurements need.
const LANDMARK_NAMES: [&str; 3] = ["shoulderL", "shoulderR", "crotch"];

/// Array element values decoded from the pack.
enum Values {
    F32(Vec<f32>),
    Ints(Vec<i64>),
}

struct Arrays<'a> {
    specs: &'a HashMap<String, ArraySpec>,
    data: &'a [u8],
}

impl Arrays<'_> {
    fn get(&self, name: &str) -> Result<Option<Values>, MhParseError> {
        let Some(spec) = self.specs.get(name) else {
            return Ok(None);
        };
        let size = match spec.dtype.as_str() {
            "uint8" => 1,
            "uint16" | "int16" => 2,
            "float32" | "int32" | "uint32" => 4,
            _ => {
                return Err(MhParseError::BadArrayType {
                    name: name.to_owned(),
                    dtype: spec.dtype.clone(),
                });
            }
        };
        let end = spec
            .length
            .checked_mul(size)
            .and_then(|n| n.checked_add(spec.offset))
            .ok_or(MhParseError::Truncated)?;
        let bytes = self.data.get(spec.offset..end).ok_or(MhParseError::Truncated)?;
        let ints = |f: fn(&[u8]) -> i64| Values::Ints(bytes.chunks_exact(size).map(f).collect());
        Ok(Some(match spec.dtype.as_str() {
            "float32" => Values::F32(
                bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect(),
            ),
            "uint8" => ints(|c| i64::from(c[0])),
            "uint16" => ints(|c| i64::from(u16::from_le_bytes([c[0], c[1]]))),
            "int16" => ints(|c| i64::from(i16::from_le_bytes([c[0], c[1]]))),
            "int32" => ints(|c| i64::from(i32::from_le_bytes([c[0], c[1], c[2], c[3]]))),
            _ => ints(|c| i64::from(u32::from_le_bytes([c[0], c[1], c[2], c[3]]))),
        }))
    }

    fn require(&self, name: &str) -> Result<Values, MhParseError> {
        self.get(name)?
            .ok_or_else(|| MhParseError::MissingArray(name.to_owned()))
    }

    fn floats(&self, name: &str) -> Result<Vec<f32>, MhParseError> {
        match self.require(name)? {
            Values::F32(v) => Ok(v),
            Values::Ints(_) => Err(self.bad_type(name)),
        }
    }

    fn ints<T: TryFrom<i64>>(&self, name: &str, required: bool) -> Result<Vec<T>, MhParseError> {
        let values = if required {
            Some(self.require(name)?)
        } else {
            self.get(name)?
        };
        match values {
            None => Ok(Vec::new()),
            Some(Values::F32(_)) => Err(self.bad_type(name)),
            Some(Values::Ints(v)) => v
                .into_iter()
                .map(|x| {
                    T::try_from(x)
                        .map_err(|_| MhParseError::Invalid(format!("array `{name}` value {x} out of range")))
                })
                .collect(),
        }
    }

    fn bad_type(&self, name: &str) -> MhParseError {
        let dtype = self.specs.get(name).map(|s| s.dtype.clone()).unwrap_or_default();
        MhParseError::BadArrayType {
            name: name.to_owned(),
            dtype,
        }
    }
}

fn invalid(msg: impl Into<String>) -> MhParseError {
    MhParseError::Invalid(msg.into())
}

fn check_indices(what: &str, indices: &[u32], limit: usize) -> Result<(), MhParseError> {
    match indices.iter().find(|&&i| i as usize >= limit) {
        Some(i) => Err(invalid(format!("{what}: vertex {i} out of range"))),
        None => Ok(()),
    }
}

/// Checks the named measures, landmarks and joints the avatar needs; returns the landmarks.
fn validate_lookups(h: &Header, n: usize) -> Result<HashMap<String, u32>, MhParseError> {
    for (name, loop_) in &h.measures {
        check_indices(name, loop_, n)?;
    }
    for d in &MEASUREMENTS {
        let (name, min_len) = match d.kind {
            MeasureKind::Circumference(name) => (name, 1),
            MeasureKind::Distance(name) => (name, 2),
            _ => continue,
        };
        if h.measures.get(name).is_none_or(|l| l.len() < min_len) {
            return Err(invalid(format!("missing measure {name}")));
        }
    }
    for name in [UPPER_ARM_LENGTH, LOWER_ARM_LENGTH] {
        if h.measures.get(name).is_none_or(|l| l.len() < 2) {
            return Err(invalid(format!("missing measure {name}")));
        }
    }
    let landmarks: HashMap<String, u32> = h
        .landmarks
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), u32::try_from(v.as_u64()?).ok()?)))
        .collect();
    for name in LANDMARK_NAMES {
        match landmarks.get(name) {
            Some(&v) if (v as usize) < n => {}
            _ => return Err(invalid(format!("missing landmark {name}"))),
        }
    }
    check_indices("landmarks", &landmarks.values().copied().collect::<Vec<_>>(), n)?;
    for name in JOINT_NAMES {
        match h.joints.get(name) {
            Some(group) if !group.is_empty() => check_indices(name, group, n)?,
            _ => return Err(invalid(format!("missing joint {name}"))),
        }
    }
    for (name, group) in &h.joints {
        check_indices(name, group, n)?;
    }
    Ok(landmarks)
}

impl MhModel {
    /// Reads and validates a pack (uncompressed bytes).
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are not a well-formed pack or the pack lacks data the avatar
    /// needs.
    pub fn parse(bytes: &[u8]) -> Result<Self, MhParseError> {
        if bytes.len() < 8 || &bytes[..4] != MAGIC {
            return Err(MhParseError::NotAPack);
        }
        let header_len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let header_bytes = bytes.get(8..8 + header_len).ok_or(MhParseError::Truncated)?;
        let h: Header =
            serde_json::from_slice(header_bytes).map_err(|e| MhParseError::Header(e.to_string()))?;
        let arrays = Arrays {
            specs: &h.arrays,
            data: &bytes[8 + header_len..],
        };
        let n = h.vertex_count;
        if h.body_vertex_count == 0 || h.body_vertex_count > n {
            return Err(invalid("body vertex count out of range"));
        }
        let base = arrays.floats("base")?;
        if base.len() != n * 3 {
            return Err(invalid("base positions do not match the vertex count"));
        }
        let body_triangles: Vec<u32> = arrays.ints("bodyTris", true)?;
        let eye_triangles: Vec<u32> = arrays.ints("eyeTris", false)?;
        for (what, tris) in [("bodyTris", &body_triangles), ("eyeTris", &eye_triangles)] {
            if tris.len() % 3 != 0 {
                return Err(invalid(format!("{what}: length is not a multiple of 3")));
            }
            check_indices(what, tris, n)?;
        }
        let weight = |name: &str| -> Result<Vec<u8>, MhParseError> {
            let w: Vec<u8> = arrays.ints(name, true)?;
            if w.len() < n {
                return Err(invalid(format!("{name}: fewer weights than vertices")));
            }
            Ok(w)
        };
        let weights = LimbWeights {
            arm_left: weight("w_armL")?,
            arm_right: weight("w_armR")?,
            leg_left: weight("w_legL")?,
            leg_right: weight("w_legR")?,
        };
        let optional = |name: &str| -> Result<Vec<u8>, MhParseError> {
            let w: Vec<u8> = arrays.ints(name, false)?;
            if !w.is_empty() && w.len() < n {
                return Err(invalid(format!("{name}: fewer weights than vertices")));
            }
            Ok(w)
        };
        let pose_weights = PoseWeights {
            lower_arm_left: optional("w_lowerarmL")?,
            lower_arm_right: optional("w_lowerarmR")?,
            lower_leg_left: optional("w_lowerlegL")?,
            lower_leg_right: optional("w_lowerlegR")?,
            spine: optional("w_spine")?,
        };
        let mut targets = HashMap::with_capacity(h.targets.len());
        for name in &h.targets {
            let indices: Vec<u32> = arrays.ints(&format!("ti_{name}"), true)?;
            let deltas: Vec<i16> = arrays.ints(&format!("td_{name}"), true)?;
            check_indices(name, &indices, n)?;
            if deltas.len() < indices.len() * 3 {
                return Err(invalid(format!("target {name}: fewer deltas than indices")));
            }
            targets.insert(name.clone(), MorphTarget { indices, deltas });
        }
        let landmarks = validate_lookups(&h, n)?;
        let mut model = Self {
            vertex_count: n,
            body_vertex_count: h.body_vertex_count,
            quantum: h.q,
            base,
            body_triangles,
            eye_triangles,
            weights,
            pose_weights,
            targets,
            measures: h.measures,
            landmarks,
            joints: h.joints,
            probe: Probe::default(),
        };
        model.probe = model.build_probe();
        Ok(model)
    }

    /// Whether the model has a morph target called `name`.
    #[must_use]
    pub fn has_target(&self, name: &str) -> bool {
        self.targets.contains_key(name)
    }

    /// Names of the fit targets missing from the model (both variants are needed).
    #[must_use]
    pub fn missing_fit_targets(&self) -> Vec<String> {
        FIT_TARGETS
            .iter()
            .flat_map(|t| [format!("{t}-incr"), format!("{t}-decr")])
            .filter(|t| !self.has_target(t))
            .collect()
    }

    /// Number of vertices the fit composes.
    #[must_use]
    pub fn probe_vertex_count(&self) -> usize {
        self.probe.vertices.len()
    }

    /// Vertex loop (girths) or vertex pair (lengths) of a named measure, empty if unknown.
    #[must_use]
    pub fn measure_vertices(&self, name: &str) -> &[u32] {
        self.measures.get(name).map_or(&[], Vec::as_slice)
    }

    /// Vertex of a named landmark (validated at parse time).
    pub(crate) fn landmark(&self, name: &str) -> u32 {
        self.landmarks.get(name).copied().unwrap_or(0)
    }

    /// Whether the model has a joint vertex group called `name`.
    #[must_use]
    pub fn has_joint(&self, name: &str) -> bool {
        self.joints.get(name).is_some_and(|g| !g.is_empty())
    }

    /// Vertex group of a named joint (validated at parse time).
    pub(crate) fn joint_vertices(&self, name: &str) -> &[u32] {
        self.joints.get(name).map_or(&[], Vec::as_slice)
    }

    #[allow(clippy::cast_possible_wrap)] // vertex counts fit in i32
    fn build_probe(&self) -> Probe {
        let mut set = BTreeSet::new();
        set.extend(self.measures.values().flatten().copied());
        set.extend(self.landmarks.values().copied());
        let mut by_height: Vec<(f32, u32)> = (0..self.body_vertex_count)
            .map(|v| (self.base[v * 3 + 1], v as u32))
            .collect();
        by_height.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let extremes = PROBE_EXTREMES.min(by_height.len());
        set.extend(by_height[..extremes].iter().map(|&(_, v)| v));
        set.extend(by_height[by_height.len() - extremes..].iter().map(|&(_, v)| v));
        let vertices: Vec<u32> = set.into_iter().collect();
        let mut local = vec![-1_i32; self.vertex_count];
        let mut base = Vec::with_capacity(vertices.len() * 3);
        for (i, &g) in vertices.iter().enumerate() {
            local[g as usize] = i as i32;
            base.extend_from_slice(&self.base[g as usize * 3..g as usize * 3 + 3]);
        }
        let targets = self
            .targets
            .iter()
            .map(|(name, t)| {
                let mut out = MorphTarget::default();
                for (e, &g) in t.indices.iter().enumerate() {
                    let l = local[g as usize];
                    if l >= 0 {
                        out.indices.push(l as u32);
                        out.deltas.extend_from_slice(&t.deltas[e * 3..e * 3 + 3]);
                    }
                }
                (name.clone(), out)
            })
            .collect();
        let body = vertices
            .iter()
            .map(|&g| (g as usize) < self.body_vertex_count)
            .collect();
        Probe {
            vertices,
            local,
            base,
            targets,
            body,
        }
    }
}
