//! Deriving the measuring loops, pairs and landmarks on the CC0 base mesh.
//!
//! MakeHuman's CC0 measure targets (`targets/measure/*.target`) say *where* each girth is taken:
//! the vertices an `-incr` target moves most cluster around the girth. Each loop is therefore
//! placed at the magnitude-weighted centre of the strongest half of its target (left side), then
//! cut out of the base mesh by a plane through that centre — horizontal for the torso girths,
//! perpendicular to the bone (from the CC0 joint helpers) for the limbs. Every mesh edge the plane
//! crosses contributes its endpoint nearer the plane; the loop is ordered by angle around its
//! centroid, which is all the tape measure needs (it reads the convex hull in the loop's plane).
//!
//! Lengths and landmarks use anatomical rules on the same mesh:
//!
//! - `shoulderL` / `shoulderR`: the outer top corner of the shoulder — the skin vertex furthest
//!   up-and-out (60° above horizontal) from the shoulder joint in a frontal slab through it — mirrored for the
//!   right side;
//! - `crotch`: the lowest vertex on the body's midline (x = 0);
//! - upper arm length: shoulder point → elbow point; lower arm length: elbow point → wrist point,
//!   where the elbow and wrist points are the outermost skin vertices (away from the body,
//!   perpendicular to the bone) in a thin slab through the elbow and wrist joints;
//! - shoulder (`measure-shoulder-dist`): side of the neck loop → shoulder point;
//! - across front: the outermost front-facing torso vertices, left and right, at the height of
//!   the across-front target;
//! - nape to waist: back midline vertex of the neck loop → back midline vertex of the waist loop;
//! - waist to hip: side of the waist loop → side of the hip loop;
//! - upper leg height: side of the hip loop → outer knee point; lower leg height (knee to floor):
//!   outer knee point → lowest vertex of the foot.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use glam::DVec3;

use crate::{BuildError, err};

/// Skin weight above which a vertex belongs to a limb when slicing.
const LIMB_KEEP: f64 = 0.5;
/// Arm weight below which a vertex belongs to the torso when slicing.
const TORSO_ARM_LIMIT: f64 = 0.35;

/// The body mesh and rig data the derivation reads.
#[derive(Clone, Copy, Debug)]
pub struct BodyGeometry<'a> {
    /// Body vertex positions (millimetres).
    pub positions: &'a [DVec3],
    /// Body faces (quads or triangles).
    pub faces: &'a [Vec<u32>],
    /// Left and right arm weight (0..1) per body vertex.
    pub arm: [&'a [f64]; 2],
    /// Left and right leg weight (0..1) per body vertex.
    pub leg: [&'a [f64]; 2],
    /// Joint centres by joint helper name (`joint-l-shoulder`, …).
    pub joints: &'a HashMap<String, DVec3>,
}

/// Measures and landmarks derived from the mesh.
#[derive(Clone, Debug, Default)]
pub struct Derived {
    /// Vertex loops (girths) and vertex pairs (lengths) by measure name.
    pub measures: BTreeMap<String, Vec<u32>>,
    /// Landmark vertices by name.
    pub landmarks: BTreeMap<String, u32>,
    /// Human-readable notes on each derived item.
    pub notes: Vec<String>,
}

/// Magnitude-weighted centre of the vertices an `-incr` target moves by at least half its
/// largest displacement, on the left side (x ≥ 0). `None` if the target moves no body vertex.
#[must_use]
pub fn target_peak(target: &[(u32, DVec3)], positions: &[DVec3]) -> Option<DVec3> {
    let body: Vec<(DVec3, f64)> = target
        .iter()
        .filter_map(|&(v, d)| positions.get(v as usize).map(|&p| (p, d.length())))
        .collect();
    let max = body.iter().map(|b| b.1).fold(0.0, f64::max);
    if max <= 0.0 {
        return None;
    }
    let (mut sum, mut w) = (DVec3::ZERO, 0.0);
    for &(p, m) in &body {
        if m >= 0.5 * max && p.x >= 0.0 {
            sum += p * m;
            w += m;
        }
    }
    (w > 0.0).then(|| sum / w)
}

/// Area-weighted vertex normals of the faces (fan-triangulated).
#[must_use]
pub fn vertex_normals(positions: &[DVec3], faces: &[Vec<u32>]) -> Vec<DVec3> {
    let mut n = vec![DVec3::ZERO; positions.len()];
    for f in faces {
        for k in 1..f.len() - 1 {
            let (a, b, c) = (f[0] as usize, f[k] as usize, f[k + 1] as usize);
            let fnorm = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
            for i in [a, b, c] {
                n[i] += fnorm;
            }
        }
    }
    n.iter().map(|v| v.normalize_or_zero()).collect()
}

struct Geo<'a> {
    g: BodyGeometry<'a>,
    edges: Vec<(u32, u32)>,
    normals: Vec<DVec3>,
}

impl Geo<'_> {
    fn p(&self, v: u32) -> DVec3 {
        self.g.positions[v as usize]
    }

    fn joint(&self, name: &str) -> Result<DVec3, BuildError> {
        match self.g.joints.get(name) {
            Some(&j) => Ok(j),
            None => err(format!("missing joint helper {name}")),
        }
    }

    /// Vertices nearest a plane along every edge it crosses, within `radius` of the plane's centre
    /// line, ordered by angle around their centroid.
    fn slice(&self, center: DVec3, normal: DVec3, radius: f64, keep: &dyn Fn(usize) -> bool) -> Vec<u32> {
        let n = normal.normalize();
        let mut picked = BTreeSet::new();
        for &(a, b) in &self.edges {
            if !keep(a as usize) || !keep(b as usize) {
                continue;
            }
            let (pa, pb) = (self.p(a), self.p(b));
            let (sa, sb) = ((pa - center).dot(n), (pb - center).dot(n));
            if sa * sb > 0.0 || (sa - sb).abs() < 1e-12 {
                continue;
            }
            let x = pa + (pb - pa) * (sa / (sa - sb));
            let off = x - center;
            if (off - n * off.dot(n)).length() > radius {
                continue;
            }
            picked.insert(if sa.abs() <= sb.abs() { a } else { b });
        }
        let pts: Vec<u32> = picked.into_iter().collect();
        let c = pts.iter().map(|&v| self.p(v)).sum::<DVec3>() / pts.len().max(1) as f64;
        let reference = if n.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let e1 = reference.cross(n).normalize();
        let e2 = n.cross(e1);
        let mut keyed: Vec<(f64, u32)> = pts
            .iter()
            .map(|&v| {
                let d = self.p(v) - c;
                (d.dot(e2).atan2(d.dot(e1)), v)
            })
            .collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        keyed.into_iter().map(|(_, v)| v).collect()
    }

    /// The vertex of `set` maximising `score`.
    fn best(&self, set: impl IntoIterator<Item = u32>, score: impl Fn(DVec3) -> f64) -> Option<u32> {
        set.into_iter()
            .map(|v| (score(self.p(v)), v))
            .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)))
            .map(|(_, v)| v)
    }

    /// The vertex nearest the mirror image (x → −x) of `v`.
    fn mirror(&self, v: u32) -> u32 {
        let m = self.p(v) * DVec3::new(-1.0, 1.0, 1.0);
        self.best(0..self.g.positions.len() as u32, |p| -(p - m).length_squared())
            .unwrap_or(v)
    }

    /// Outermost skin vertex in a slab through `joint` perpendicular to `axis`, measured along the
    /// direction away from the body (perpendicular to the axis, towards +x).
    fn outer_point(&self, joint: DVec3, axis: DVec3, keep: &dyn Fn(usize) -> bool) -> Option<u32> {
        let axis = axis.normalize();
        let out = (DVec3::X - axis * axis.x).normalize();
        let slab = (0..self.g.positions.len() as u32)
            .filter(|&v| keep(v as usize) && (self.p(v) - joint).dot(axis).abs() < 8.0);
        self.best(slab, |p| (p - joint).dot(out))
    }
}

/// A limb girth cut: measure name, upper and lower joint, search radius and vertex filter.
type LimbCut<'a> = (&'a str, DVec3, DVec3, f64, &'a dyn Fn(usize) -> bool);

/// Unique undirected edges of the faces.
fn face_edges(faces: &[Vec<u32>]) -> Vec<(u32, u32)> {
    let mut set = BTreeSet::new();
    for f in faces {
        for k in 0..f.len() {
            let (a, b) = (f[k], f[(k + 1) % f.len()]);
            set.insert((a.min(b), a.max(b)));
        }
    }
    set.into_iter().collect()
}

/// Derives every measure and landmark the pack needs. `peaks` maps measure names to
/// [`target_peak`] of their `-incr` target.
///
/// # Errors
///
/// Returns an error if a joint helper, target peak or loop is missing.
#[allow(clippy::too_many_lines)]
pub fn derive(g: BodyGeometry<'_>, peaks: &BTreeMap<String, DVec3>) -> Result<Derived, BuildError> {
    let geo = Geo {
        edges: face_edges(g.faces),
        normals: vertex_normals(g.positions, g.faces),
        g,
    };
    let mut out = Derived::default();
    let peak = |name: &str| match peaks.get(name) {
        Some(&p) => Ok(p),
        None => err(format!("no peak for {name}")),
    };
    let [arm_l, arm_r] = g.arm;
    let leg_l = g.leg[0];
    let torso = |v: usize| arm_l[v] < TORSO_ARM_LIMIT && arm_r[v] < TORSO_ARM_LIMIT;
    let left_arm = |v: usize| arm_l[v] > LIMB_KEEP;
    let left_leg = |v: usize| leg_l[v] > LIMB_KEEP;
    let add_loop = |out: &mut Derived, name: &str, l: Vec<u32>, how: String| -> Result<(), BuildError> {
        if l.len() < 8 {
            return err(format!("{name}: only {} loop vertices", l.len()));
        }
        out.notes.push(format!("{name}: {} vertices, {how}", l.len()));
        out.measures.insert(name.to_owned(), l);
        Ok(())
    };

    // torso girths: horizontal cuts at the target's height
    for (name, radius) in [
        ("measure-neck-circ", 90.0),
        ("measure-bust-circ", 320.0),
        ("measure-underbust-circ", 320.0),
        ("measure-waist-circ", 320.0),
        ("measure-hips-circ", 320.0),
    ] {
        let p = peak(name)?;
        let l = geo.slice(DVec3::new(0.0, p.y, p.z), DVec3::Y, radius, &torso);
        add_loop(&mut out, name, l, format!("horizontal cut at y = {:.1} mm", p.y))?;
    }
    // limb girths: cuts perpendicular to the bone through the target's centre
    let s = geo.joint("joint-l-shoulder")?;
    let e = geo.joint("joint-l-elbow")?;
    let h = geo.joint("joint-l-hand")?;
    let hip = geo.joint("joint-l-upper-leg")?;
    let k = geo.joint("joint-l-knee")?;
    let a = geo.joint("joint-l-ankle")?;
    let limbs: [LimbCut<'_>; 6] = [
        ("measure-upperarm-circ", s, e, 90.0, &left_arm),
        ("measure-wrist-circ", e, h, 60.0, &left_arm),
        ("measure-thigh-circ", hip, k, 120.0, &left_leg),
        ("measure-knee-circ", k, a, 90.0, &left_leg),
        ("measure-calf-circ", k, a, 90.0, &left_leg),
        ("measure-ankle-circ", k, a, 70.0, &left_leg),
    ];
    for (name, j0, j1, radius, keep) in limbs {
        let axis = (j1 - j0).normalize();
        let c = j0 + axis * (peak(name)? - j0).dot(axis);
        let l = geo.slice(c, axis, radius, keep);
        let how = format!(
            "cut perpendicular to the bone at {:.0}% from the upper joint",
            100.0 * (c - j0).length() / (j1 - j0).length()
        );
        add_loop(&mut out, name, l, how)?;
    }
    let lp = |out: &Derived, name: &str| -> Vec<u32> { out.measures.get(name).cloned().unwrap_or_default() };
    let missing = |what: &str| BuildError(format!("could not place {what}"));

    // landmarks
    // the shoulder point: the outer top corner of the shoulder in a frontal slab through the
    // shoulder joint (furthest along up-and-out, 60° above horizontal)
    let corner = DVec3::new(0.5, 0.866, 0.0).normalize();
    let slab = (0..g.positions.len() as u32).filter(|&v| {
        let p = geo.p(v);
        (p.z - s.z).abs() < 15.0 && (p - s).length() < 90.0
    });
    let shoulder_l = geo
        .best(slab, |p| (p - s).dot(corner))
        .ok_or_else(|| missing("shoulder point"))?;
    let shoulder_r = geo.mirror(shoulder_l);
    let midline = (0..g.positions.len() as u32).filter(|&v| geo.p(v).x.abs() < 0.5);
    let crotch = geo.best(midline, |p| -p.y).ok_or_else(|| missing("crotch"))?;
    out.landmarks.insert("shoulderL".into(), shoulder_l);
    out.landmarks.insert("shoulderR".into(), shoulder_r);
    out.landmarks.insert("crotch".into(), crotch);
    out.notes.push(format!(
        "landmarks: shoulderL {shoulder_l}, shoulderR {shoulder_r} (outer top corner of the shoulder), crotch {crotch} (lowest midline vertex)"
    ));

    // lengths
    let elbow = geo
        .outer_point(e, e - s, &left_arm)
        .ok_or_else(|| missing("elbow point"))?;
    let wrist = geo
        .outer_point(h, h - e, &left_arm)
        .ok_or_else(|| missing("wrist point"))?;
    let knee = geo
        .outer_point(k, a - hip, &left_leg)
        .ok_or_else(|| missing("knee point"))?;
    let floor = geo
        .best(
            (0..g.positions.len() as u32).filter(|&v| left_leg(v as usize)),
            |p| -p.y,
        )
        .ok_or_else(|| missing("floor point"))?;
    let neck = lp(&out, "measure-neck-circ");
    let waist = lp(&out, "measure-waist-circ");
    let hips = lp(&out, "measure-hips-circ");
    let side = |l: &[u32]| geo.best(l.iter().copied(), |p| p.x);
    let back_mid = |l: &[u32]| geo.best(l.iter().copied(), |p| -p.x.abs() * 4.0 - p.z);
    let neck_side = side(&neck).ok_or_else(|| missing("neck side"))?;
    let nape = back_mid(&neck).ok_or_else(|| missing("nape"))?;
    let back_waist = back_mid(&waist).ok_or_else(|| missing("back waist"))?;
    let waist_side = side(&waist).ok_or_else(|| missing("waist side"))?;
    let hip_side = side(&hips).ok_or_else(|| missing("hip side"))?;
    let front_y = peak("measure-frontchest-dist")?.y;
    let front = |v: usize, sign: f64| {
        let p = geo.p(v as u32);
        torso(v) && (p.y - front_y).abs() < 8.0 && geo.normals[v].z > 0.5 && p.x * sign > 0.0
    };
    let across_l = geo
        .best(
            (0..g.positions.len() as u32).filter(|&v| front(v as usize, 1.0)),
            |p| p.x,
        )
        .ok_or_else(|| missing("across front"))?;
    let across_r = geo.mirror(across_l);
    let pairs = [
        (
            "measure-upperarm-length",
            [shoulder_l, elbow],
            "shoulder point to outer elbow",
        ),
        (
            "measure-lowerarm-length",
            [elbow, wrist],
            "outer elbow to outer wrist",
        ),
        (
            "measure-shoulder-dist",
            [neck_side, shoulder_l],
            "side of the neck loop to shoulder point",
        ),
        (
            "measure-frontchest-dist",
            [across_l, across_r],
            "front arm creases at the across-front target height",
        ),
        (
            "measure-napetowaist-dist",
            [nape, back_waist],
            "back of the neck loop to back of the waist loop",
        ),
        (
            "measure-waisttohip-dist",
            [waist_side, hip_side],
            "side of the waist loop to side of the hip loop",
        ),
        (
            "measure-upperleg-height",
            [hip_side, knee],
            "side of the hip loop to outer knee",
        ),
        (
            "measure-lowerleg-height",
            [knee, floor],
            "outer knee to lowest foot vertex",
        ),
    ];
    for (name, pair, how) in pairs {
        let len = (geo.p(pair[0]) - geo.p(pair[1])).length();
        out.notes
            .push(format!("{name}: {pair:?}, {how} ({len:.1} mm on the base mesh)"));
        out.measures.insert(name.to_owned(), pair.to_vec());
    }
    Ok(out)
}
