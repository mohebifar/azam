//! Compares azam-avatar with recorded reference data: presets, numeric helpers, three parametric
//! avatars (torso profile, primitives, joints, distance grid, display mesh, arrangement of every
//! garment template piece) and a morph-target avatar built from a small synthetic body pack.
//!
//! Regenerate the data (and the synthetic pack) with
//! `node tools/golden/export-avatar.mjs > crates/azam-avatar/tests/golden/avatar.json`.

#![allow(clippy::float_cmp)]

use std::collections::BTreeMap;
use std::sync::OnceLock;

use azam_avatar::makehuman::{MhShape, solve_linear, tape_measure};
use azam_avatar::{
    Arrangement, Avatar, AvatarKind, AvatarParams, BodyPrimitive, Ellipsoid, FIT_TARGET_COUNT, FIT_TARGETS,
    MEASUREMENTS, MhModel, ParametricBody, Pchip, RoundCone, SdfGrid, Sex, Shape, SlotFrame, TorsoProfile,
    arrange_positions, arrangement_points, ellipse_axes, ellipse_perimeter, size_chart, slot_frame,
    smooth_min,
};
use azam_geom::{BBox, Vec2};
use glam::DVec3;
use serde::Deserialize;

const GOLDEN: &str = include_str!("golden/avatar.json");
const PACK: &[u8] = include_bytes!("golden/synthetic-body.atmh");

/// Relative tolerance for double-precision results. The port performs the same operations in the
/// same order, so results agree to within a few ulps (transcendental functions may differ in the
/// last bit).
const F64_TOL: f64 = 1e-9;
/// Relative tolerance for values stored in single precision.
const F32_TOL: f64 = 1e-5;

type V3 = [f64; 3];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    presets: BTreeMap<String, AvatarParams>,
    size_charts: BTreeMap<String, Vec<SizeRow>>,
    math: MathCases,
    solve_lin: Vec<SolveCase>,
    avatars: Vec<AvatarCase>,
    makehuman: MhCase,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SizeRow {
    name: String,
    height: f64,
    bust: f64,
    underbust: f64,
    waist: f64,
    hip: f64,
    neck: f64,
    shoulder: f64,
    arm: f64,
    crotch: f64,
    thigh: f64,
    upper_arm: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MathCases {
    ell_axes: Vec<[f64; 4]>,
    ell_perim: Vec<[f64; 3]>,
    pchip: Vec<PchipCase>,
    round_cone: Vec<RoundConeCase>,
    ellipsoid: Vec<EllipsoidCase>,
    smin: Vec<[f64; 4]>,
}

#[derive(Deserialize)]
struct PchipCase {
    xs: Vec<f64>,
    ys: Vec<f64>,
    at: Vec<[f64; 2]>,
}

#[derive(Deserialize)]
struct RoundConeCase {
    a: V3,
    b: V3,
    r1: f64,
    r2: f64,
    pts: Vec<[f64; 4]>,
}

#[derive(Deserialize)]
struct EllipsoidCase {
    center: V3,
    radii: V3,
    pts: Vec<[f64; 4]>,
}

#[derive(Deserialize)]
struct SolveCase {
    #[serde(rename = "A")]
    a: Vec<Vec<f64>>,
    b: Vec<f64>,
    x: Option<Vec<f64>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AvatarCase {
    name: String,
    params: AvatarParams,
    profile: ProfileCase,
    prims: Vec<PrimCase>,
    joints: BTreeMap<String, JointCase>,
    sdf: Vec<[f64; 4]>,
    grid: GridCase,
    samples: Vec<Vec<f64>>,
    mesh: MeshCase,
    arrangement_points: Vec<PointCase>,
    slot_frames: Vec<SlotFrameCase>,
    pieces: Vec<PieceCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileCase {
    #[serde(rename = "H")]
    height: f64,
    ys: LevelsCase,
    neck_r: f64,
    torso: TorsoCase,
    evals: Vec<[f64; 4]>,
    shoulder_y: Vec<[f64; 2]>,
    s: Option<f64>,
    fem: Option<bool>,
    hip: Option<[f64; 2]>,
    sh_h: Option<f64>,
}

#[derive(Deserialize)]
struct LevelsCase {
    shoulder: f64,
    neck: f64,
    chin: f64,
    bust: f64,
    under: f64,
    waist: f64,
    high: f64,
    hip: f64,
    crotch: f64,
    knee: f64,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct TorsoCase {
    y0: f64,
    y1: f64,
    step: f64,
    A: Vec<f64>,
    B: Vec<f64>,
    C: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(tag = "t")]
enum PrimCase {
    #[serde(rename = "rc")]
    RoundCone {
        a: V3,
        b: V3,
        r1: f64,
        r2: f64,
        k: f64,
        bb: [f64; 6],
    },
    #[serde(rename = "el")]
    Ellipsoid { c: V3, r: V3, k: f64, bb: [f64; 6] },
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct JointCase {
    S: V3,
    E: V3,
    W: V3,
    a: V3,
    uaR: f64,
    Hj: V3,
    K: V3,
    A: V3,
    la: V3,
    thR: f64,
}

#[derive(Deserialize)]
struct GridCase {
    x0: f64,
    y0: f64,
    z0: f64,
    g: f64,
    nx: usize,
    ny: usize,
    nz: usize,
    sum: f64,
    neg: usize,
    stride: usize,
    values: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshCase {
    vertices: usize,
    triangles: usize,
    pos_sum: f64,
    nor_sum: f64,
    idx_sum: f64,
    v_stride: usize,
    t_stride: usize,
    verts: Vec<[f64; 7]>,
    tris: Vec<[u32; 4]>,
}

#[derive(Deserialize)]
struct PointCase {
    slot: String,
    h: Option<f64>,
    t: Option<f64>,
    label: String,
    pos: V3,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SlotFrameCase {
    arr: Arrangement,
    v_range: [f64; 2],
    frame: FrameCase,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct FrameCase {
    O: V3,
    a: V3,
    n: V3,
    r: V3,
    R: f64,
    t0: f64,
    #[serde(rename = "wrapAxis")]
    wrap_axis: bool,
    chain: Vec<ChainCase>,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct ChainCase {
    A: V3,
    a: V3,
    n: V3,
    r: V3,
    s: f64,
    L: f64,
}

#[derive(Deserialize)]
struct PieceCase {
    name: String,
    arr: Option<Arrangement>,
    bbox: [f64; 4],
    xy: Vec<f64>,
    pos: Vec<f64>,
    frame: FrameCase,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MhCase {
    probe: usize,
    composed: ComposedCase,
    measure_base: Vec<f64>,
    measure_composed: Vec<f64>,
    tapes: Vec<TapeCase>,
    pose: PoseRunCase,
    fits: Vec<FitRunCase>,
    avatar: MhAvatarCase,
}

#[derive(Deserialize)]
struct ShapeCase {
    gender: f64,
    muscle: f64,
    weight: f64,
    meas: Vec<f64>,
}

#[derive(Deserialize)]
struct ComposedCase {
    shape: ShapeCase,
    pos: Vec<f64>,
}

#[derive(Deserialize)]
struct TapeCase {
    name: String,
    len: f64,
    c: V3,
    hull: Vec<V3>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoseInfoCase {
    l_arm: f64,
    l_leg: f64,
    r_arm: f64,
    r_leg: f64,
}

#[derive(Deserialize)]
struct PoseRunCase {
    arm: f64,
    leg: f64,
    info: PoseInfoCase,
    pos: Vec<f64>,
}

#[derive(Deserialize)]
struct FitCase {
    gender: f64,
    muscle: f64,
    weight: f64,
    meas: Vec<f64>,
    scale: f64,
}

#[derive(Deserialize)]
struct FitRunCase {
    spec: AvatarParams,
    fit: FitCase,
}

#[derive(Deserialize)]
struct MhTapeCase {
    k: String,
    closed: bool,
    pts: Vec<V3>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MhMeshCase {
    pos: Vec<f64>,
    nor: Vec<f64>,
    idx: Vec<u32>,
    body_v: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MhAvatarCase {
    spec: AvatarParams,
    measured: Vec<f64>,
    fit: FitCase,
    pose: PoseInfoCase,
    profile: ProfileCase,
    sh_h: f64,
    hip: [f64; 2],
    joints: BTreeMap<String, JointCase>,
    tapes: Vec<MhTapeCase>,
    mesh: MhMeshCase,
    grid: GridCase,
    samples: Vec<Vec<f64>>,
    arrangement_points: Vec<PointCase>,
}

fn golden() -> &'static Golden {
    static G: OnceLock<Golden> = OnceLock::new();
    G.get_or_init(|| serde_json::from_str(GOLDEN).expect("golden file parses"))
}

fn model() -> &'static MhModel {
    static M: OnceLock<MhModel> = OnceLock::new();
    M.get_or_init(|| MhModel::parse(PACK).expect("synthetic pack parses"))
}

#[track_caller]
fn close(actual: f64, expected: f64, tol: f64, what: &str) {
    let scale = actual.abs().max(expected.abs()).max(1.0);
    assert!(
        (actual - expected).abs() <= tol * scale,
        "{what}: {actual} != {expected} (golden)"
    );
}

#[track_caller]
fn close3(actual: DVec3, expected: V3, tol: f64, what: &str) {
    close(actual.x, expected[0], tol, &format!("{what}.x"));
    close(actual.y, expected[1], tol, &format!("{what}.y"));
    close(actual.z, expected[2], tol, &format!("{what}.z"));
}

#[track_caller]
fn close_all(actual: impl IntoIterator<Item = f64>, expected: &[f64], tol: f64, what: &str) {
    let actual: Vec<f64> = actual.into_iter().collect();
    assert_eq!(actual.len(), expected.len(), "{what}: length");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        close(*a, *e, tol, &format!("{what}[{i}]"));
    }
}

fn v3(p: V3) -> DVec3 {
    DVec3::from_array(p)
}

fn widen(v: &[f32]) -> impl Iterator<Item = f64> + '_ {
    v.iter().map(|&x| f64::from(x))
}

#[test]
fn presets_and_size_charts_match() {
    let g = golden();
    for sex in [Sex::Female, Sex::Male] {
        assert_eq!(
            g.presets[sex.as_str()],
            AvatarParams::preset(sex),
            "{} preset",
            sex.as_str()
        );
        let rows = &g.size_charts[sex.as_str()];
        let chart = size_chart(sex);
        assert_eq!(rows.len(), chart.len());
        for (r, c) in rows.iter().zip(chart) {
            assert_eq!(r.name, c.name);
            let expected = [
                r.height,
                r.bust,
                r.underbust,
                r.waist,
                r.hip,
                r.neck,
                r.shoulder,
                r.arm,
                r.crotch,
                r.thigh,
                r.upper_arm,
            ];
            let actual = [
                c.height,
                c.bust,
                c.underbust,
                c.waist,
                c.hip,
                c.neck,
                c.shoulder,
                c.arm,
                c.crotch,
                c.thigh,
                c.upper_arm,
            ];
            assert_eq!(actual, expected, "size {}", r.name);
        }
    }
}

#[test]
fn numeric_helpers_match() {
    let m = &golden().math;
    for &[c, k, a, b] in &m.ell_axes {
        let r = ellipse_axes(c, k);
        close(r[0], a, F64_TOL, "ellipse_axes a");
        close(r[1], b, F64_TOL, "ellipse_axes b");
    }
    for &[a, b, p] in &m.ell_perim {
        close(ellipse_perimeter(a, b), p, F64_TOL, "ellipse_perimeter");
    }
    for (i, case) in m.pchip.iter().enumerate() {
        let f = Pchip::new(&case.xs, &case.ys);
        for &[x, y] in &case.at {
            close(f.eval(x), y, F64_TOL, &format!("pchip {i} at {x}"));
        }
    }
    for (i, case) in m.round_cone.iter().enumerate() {
        let c = RoundCone {
            a: v3(case.a),
            b: v3(case.b),
            r1: case.r1,
            r2: case.r2,
        };
        for &[x, y, z, d] in &case.pts {
            close(
                c.distance(DVec3::new(x, y, z)),
                d,
                F64_TOL,
                &format!("round cone {i}"),
            );
        }
    }
    for (i, case) in m.ellipsoid.iter().enumerate() {
        let e = Ellipsoid {
            center: v3(case.center),
            radii: v3(case.radii),
        };
        for &[x, y, z, d] in &case.pts {
            close(
                e.distance(DVec3::new(x, y, z)),
                d,
                F64_TOL,
                &format!("ellipsoid {i}"),
            );
        }
    }
    for &[a, b, k, v] in &m.smin {
        close(smooth_min(a, b, k), v, F64_TOL, "smooth_min");
    }
}

#[test]
fn linear_solver_matches() {
    for (i, case) in golden().solve_lin.iter().enumerate() {
        let x = solve_linear(&case.a, &case.b);
        match (&x, &case.x) {
            (Some(x), Some(e)) => close_all(x.iter().copied(), e, F64_TOL, &format!("solve {i}")),
            (None, None) => {}
            _ => panic!("solve {i}: {x:?} != {:?} (golden)", case.x),
        }
    }
}

fn check_profile(p: &TorsoProfile, e: &ProfileCase, tol: f64, what: &str) {
    close(p.height, e.height, tol, &format!("{what} height"));
    close(p.neck_radius, e.neck_r, tol, &format!("{what} neck radius"));
    let l = &p.levels;
    let y = &e.ys;
    let pairs = [
        (l.shoulder, y.shoulder, "shoulder"),
        (l.neck, y.neck, "neck"),
        (l.chin, y.chin, "chin"),
        (l.bust, y.bust, "bust"),
        (l.underbust, y.under, "underbust"),
        (l.waist, y.waist, "waist"),
        (l.high_hip, y.high, "high hip"),
        (l.hip, y.hip, "hip"),
        (l.crotch, y.crotch, "crotch"),
        (l.knee, y.knee, "knee"),
    ];
    for (a, b, name) in pairs {
        close(a, b, tol, &format!("{what} level {name}"));
    }
    let t = &p.torso;
    close(t.y0, e.torso.y0, tol, &format!("{what} torso y0"));
    close(t.y1, e.torso.y1, tol, &format!("{what} torso y1"));
    close(t.step, e.torso.step, tol, &format!("{what} torso step"));
    close_all(
        widen(&t.half_width),
        &e.torso.A,
        F32_TOL,
        &format!("{what} torso width"),
    );
    close_all(
        widen(&t.half_depth),
        &e.torso.B,
        F32_TOL,
        &format!("{what} torso depth"),
    );
    close_all(
        widen(&t.center_z),
        &e.torso.C,
        F32_TOL,
        &format!("{what} torso centre"),
    );
    for &[y, a, b, c] in &e.evals {
        let s = t.eval(y);
        close(s.half_width, a, tol, &format!("{what} eval({y}) width"));
        close(s.half_depth, b, tol, &format!("{what} eval({y}) depth"));
        close(s.center_z, c, tol, &format!("{what} eval({y}) centre"));
    }
    for &[x, y] in &e.shoulder_y {
        close(p.shoulder_y(x), y, tol, &format!("{what} shoulder_y({x})"));
    }
}

fn check_joints(avatar: &Avatar, e: &BTreeMap<String, JointCase>, tol: f64, what: &str) {
    for (key, j) in [("L", &avatar.joints.left), ("R", &avatar.joints.right)] {
        let ej = &e[key];
        let w = format!("{what} joints {key}");
        close3(j.shoulder, ej.S, tol, &format!("{w} shoulder"));
        close3(j.elbow, ej.E, tol, &format!("{w} elbow"));
        close3(j.wrist, ej.W, tol, &format!("{w} wrist"));
        close3(j.arm_axis, ej.a, tol, &format!("{w} arm axis"));
        close(j.upper_arm_radius, ej.uaR, tol, &format!("{w} upper arm radius"));
        close3(j.hip, ej.Hj, tol, &format!("{w} hip"));
        close3(j.knee, ej.K, tol, &format!("{w} knee"));
        close3(j.ankle, ej.A, tol, &format!("{w} ankle"));
        close3(j.leg_axis, ej.la, tol, &format!("{w} leg axis"));
        close(j.thigh_radius, ej.thR, tol, &format!("{w} thigh radius"));
    }
}

fn check_grid(grid: &SdfGrid, e: &GridCase, what: &str) {
    close3(
        grid.origin,
        [e.x0, e.y0, e.z0],
        F64_TOL,
        &format!("{what} grid origin"),
    );
    close(grid.cell, e.g, F64_TOL, &format!("{what} grid cell"));
    assert_eq!(grid.dims, [e.nx, e.ny, e.nz], "{what} grid dims");
    let sum = grid.data.iter().fold(0.0, |s, &v| s + f64::from(v));
    close(sum, e.sum, F64_TOL, &format!("{what} grid sum"));
    assert_eq!(
        grid.data.iter().filter(|&&v| v < 0.0).count(),
        e.neg,
        "{what} grid negative cells"
    );
    let sampled = grid.data.iter().step_by(e.stride).map(|&v| f64::from(v));
    close_all(sampled, &e.values, F32_TOL, &format!("{what} grid values"));
}

fn check_samples(grid: &SdfGrid, samples: &[Vec<f64>], tol: f64, what: &str) {
    for (i, s) in samples.iter().enumerate() {
        let p = DVec3::new(s[0], s[1], s[2]);
        let w = format!("{what} sample {i}");
        if let Some(r) = grid.sample(p) {
            assert_eq!(s.len(), 7, "{w}: sampled inside, golden says outside");
            close(r.distance, s[3], tol, &w);
            close3(r.gradient, [s[4], s[5], s[6]], tol, &format!("{w} gradient"));
        } else {
            assert_eq!(s.len(), 4, "{w}: sampled outside, golden says inside");
            close(grid.distance(p), s[3], tol, &w);
        }
    }
}

fn check_frame(f: &SlotFrame, e: &FrameCase, tol: f64, what: &str) {
    close3(f.origin, e.O, tol, &format!("{what} origin"));
    close3(f.axis, e.a, tol, &format!("{what} axis"));
    close3(f.normal, e.n, tol, &format!("{what} normal"));
    close3(f.right, e.r, tol, &format!("{what} right"));
    close(f.radius, e.R, tol, &format!("{what} radius"));
    close(f.t0, e.t0, tol, &format!("{what} t0"));
    assert_eq!(f.wraps_body, e.wrap_axis, "{what} wraps body");
    assert_eq!(f.chain.len(), e.chain.len(), "{what} chain length");
    for (i, (c, ec)) in f.chain.iter().zip(&e.chain).enumerate() {
        let w = format!("{what} chain {i}");
        close3(c.start, ec.A, tol, &format!("{w} start"));
        close3(c.axis, ec.a, tol, &format!("{w} axis"));
        close3(c.normal, ec.n, tol, &format!("{w} normal"));
        close3(c.right, ec.r, tol, &format!("{w} right"));
        close(c.offset, ec.s, tol, &format!("{w} offset"));
        close(c.length, ec.L, tol, &format!("{w} length"));
    }
}

fn check_points(avatar: &Avatar, expected: &[PointCase], tol: f64, what: &str) {
    let pts = arrangement_points(avatar);
    assert_eq!(pts.len(), expected.len(), "{what} arrangement point count");
    for (p, e) in pts.iter().zip(expected) {
        assert_eq!(
            (p.slot, p.label.as_str()),
            (e.slot.as_str(), e.label.as_str()),
            "{what} arrangement point"
        );
        assert_eq!(p.h.is_some(), e.h.is_some());
        assert_eq!(p.t, e.t, "{what} {} t", e.label);
        if let (Some(a), Some(b)) = (p.h, e.h) {
            close(a, b, tol, &format!("{what} {} h", e.label));
        }
        close3(p.position, e.pos, tol, &format!("{what} {}", e.label));
    }
}

fn check_parametric(case: &AvatarCase) {
    let what = case.name.as_str();
    let body = ParametricBody::new(&case.params);
    let p = &case.profile;
    check_profile(&body.profile, p, F64_TOL, what);
    close(
        body.profile.scale,
        p.s.unwrap_or_default(),
        F64_TOL,
        &format!("{what} scale"),
    );
    assert_eq!(Some(body.profile.female), p.fem, "{what} female");
    let hip = p.hip.unwrap_or_default();
    close(
        body.profile.hip_axes[0],
        hip[0],
        F64_TOL,
        &format!("{what} hip axis x"),
    );
    close(
        body.profile.hip_axes[1],
        hip[1],
        F64_TOL,
        &format!("{what} hip axis z"),
    );
    close(
        body.profile.half_shoulder,
        p.sh_h.unwrap_or_default(),
        F64_TOL,
        &format!("{what} half shoulder"),
    );
    check_primitives(&body, case);
    for &[x, y, z, d] in &case.sdf {
        close(
            body.distance(DVec3::new(x, y, z)),
            d,
            F64_TOL,
            &format!("{what} distance at ({x}, {y}, {z})"),
        );
    }
    let avatar = Avatar::from_parametric(body);
    assert_eq!(avatar.kind, AvatarKind::Parametric);
    assert!(avatar.body_collision_mesh().is_none());
    check_joints(&avatar, &case.joints, F64_TOL, what);
    check_grid(&avatar.grid, &case.grid, what);
    check_samples(&avatar.grid, &case.samples, F64_TOL, what);

    check_mesh(&avatar, case);
    check_arrangement(&avatar, case);
}

fn check_primitives(body: &ParametricBody, case: &AvatarCase) {
    let what = case.name.as_str();
    assert_eq!(body.primitives.len(), case.prims.len(), "{what} primitive count");
    for (i, (prim, e)) in body.primitives.iter().zip(&case.prims).enumerate() {
        let w = format!("{what} primitive {i}");
        let check_common = |prim: &BodyPrimitive, k: f64, bb: &[f64; 6]| {
            close(prim.blend, k, F64_TOL, &format!("{w} blend"));
            close_all(prim.bounds, bb, F64_TOL, &format!("{w} bounds"));
        };
        match (&prim.shape, e) {
            (Shape::RoundCone(c), PrimCase::RoundCone { a, b, r1, r2, k, bb }) => {
                close3(c.a, *a, F64_TOL, &format!("{w} a"));
                close3(c.b, *b, F64_TOL, &format!("{w} b"));
                close(c.r1, *r1, F64_TOL, &format!("{w} r1"));
                close(c.r2, *r2, F64_TOL, &format!("{w} r2"));
                check_common(prim, *k, bb);
            }
            (Shape::Ellipsoid(el), PrimCase::Ellipsoid { c, r, k, bb }) => {
                close3(el.center, *c, F64_TOL, &format!("{w} centre"));
                close3(el.radii, *r, F64_TOL, &format!("{w} radii"));
                check_common(prim, *k, bb);
            }
            _ => panic!("{w}: shape kind differs from golden"),
        }
    }
}

fn check_mesh(avatar: &Avatar, case: &AvatarCase) {
    let what = case.name.as_str();
    let m = &avatar.mesh;
    let e = &case.mesh;
    assert_eq!(
        (m.vertex_count(), m.triangle_count()),
        (e.vertices, e.triangles),
        "{what} mesh size"
    );
    close(
        widen(&m.positions).sum(),
        e.pos_sum,
        F32_TOL,
        &format!("{what} mesh position sum"),
    );
    close(
        widen(&m.normals).sum(),
        e.nor_sum,
        F32_TOL,
        &format!("{what} mesh normal sum"),
    );
    close(
        m.indices.iter().map(|&i| f64::from(i)).sum(),
        e.idx_sum,
        F64_TOL,
        &format!("{what} mesh index sum"),
    );
    for (row, v) in e.verts.iter().zip((0..m.vertex_count()).step_by(e.v_stride)) {
        assert_eq!(row[0] as usize, v);
        close_all(
            widen(&m.positions[v * 3..v * 3 + 3]),
            &row[1..4],
            F32_TOL,
            &format!("{what} vertex {v}"),
        );
        close_all(
            widen(&m.normals[v * 3..v * 3 + 3]),
            &row[4..7],
            F32_TOL,
            &format!("{what} normal {v}"),
        );
    }
    for (row, t) in e.tris.iter().zip((0..m.triangle_count()).step_by(e.t_stride)) {
        assert_eq!(row[0] as usize, t);
        assert_eq!(&m.indices[t * 3..t * 3 + 3], &row[1..4], "{what} triangle {t}");
    }
}

fn check_arrangement(avatar: &Avatar, case: &AvatarCase) {
    let what = case.name.as_str();
    check_points(avatar, &case.arrangement_points, F64_TOL, what);
    for (i, sf) in case.slot_frames.iter().enumerate() {
        let f = slot_frame(avatar, &sf.arr, sf.v_range);
        check_frame(
            &f,
            &sf.frame,
            F64_TOL,
            &format!("{what} slot frame {i} ({:?})", sf.arr.slot),
        );
    }
    for pc in &case.pieces {
        let w = format!("{what} piece {}", pc.name);
        let arr = pc
            .arr
            .clone()
            .unwrap_or_else(|| Arrangement::in_slot("front", 60.0));
        let bounds = BBox {
            min: Vec2::new(pc.bbox[0], pc.bbox[1]),
            max: Vec2::new(pc.bbox[2], pc.bbox[3]),
        };
        let mut out = vec![0.0_f32; pc.xy.len() / 2 * 3];
        let f = arrange_positions(avatar, &arr, &bounds, &pc.xy, &mut out);
        check_frame(&f, &pc.frame, F64_TOL, &w);
        close_all(widen(&out), &pc.pos, F32_TOL, &format!("{w} positions"));
    }
}

fn avatar_case(name: &str) -> &'static AvatarCase {
    golden()
        .avatars
        .iter()
        .find(|a| a.name == name)
        .expect("avatar case present")
}

#[test]
fn female_parametric_avatar_matches() {
    let case = avatar_case("female");
    // the tee (5 pieces), the dress (2) and the skirt (3)
    assert!(case.pieces.len() >= 10, "every template piece is arranged");
    check_parametric(case);
}

#[test]
fn male_parametric_avatar_matches() {
    check_parametric(avatar_case("male"));
}

#[test]
fn custom_parametric_avatar_matches() {
    check_parametric(avatar_case("custom"));
}

fn shape_of(s: &ShapeCase) -> MhShape {
    let mut measures = [0.0; FIT_TARGET_COUNT];
    measures.copy_from_slice(&s.meas);
    MhShape {
        gender: s.gender,
        muscle: s.muscle,
        weight: s.weight,
        measures,
        smooth: 1.0,
    }
}

#[track_caller]
fn check_fit(fit: &azam_avatar::makehuman::MhFit, e: &FitCase, tol: f64, what: &str) {
    close(fit.shape.gender, e.gender, tol, &format!("{what} gender"));
    close(fit.shape.muscle, e.muscle, tol, &format!("{what} muscle"));
    close(fit.shape.weight, e.weight, tol, &format!("{what} weight"));
    close_all(
        fit.shape.measures,
        &e.meas,
        tol,
        &format!("{what} target weights"),
    );
    close(fit.scale, e.scale, tol, &format!("{what} scale"));
}

#[test]
fn morph_target_parts_match() {
    let g = &golden().makehuman;
    let m = model();
    assert_eq!(m.probe_vertex_count(), g.probe, "probe vertex count");
    assert!(m.missing_fit_targets().is_empty());

    let base: Vec<f32> = m.base.clone();
    close_all(
        m.measure_all(&base).0,
        &g.measure_base,
        F64_TOL,
        "base measurements",
    );
    let composed = m.compose(&shape_of(&g.composed.shape));
    close_all(widen(&composed), &g.composed.pos, F32_TOL, "composed positions");
    close_all(
        m.measure_all(&composed).0,
        &g.measure_composed,
        F64_TOL,
        "composed measurements",
    );
    for t in &g.tapes {
        let pts: Vec<DVec3> = m
            .measure_vertices(&t.name)
            .iter()
            .map(|&v| DVec3::from_array(std::array::from_fn(|a| f64::from(composed[v as usize * 3 + a]))))
            .collect();
        let tape = tape_measure(&pts);
        close(tape.length, t.len, F64_TOL, &format!("tape {}", t.name));
        close3(tape.center, t.c, F64_TOL, &format!("tape {} centre", t.name));
        assert_eq!(tape.hull.len(), t.hull.len(), "tape {} hull", t.name);
        for (h, eh) in tape.hull.iter().zip(&t.hull) {
            close3(*h, *eh, F64_TOL, &format!("tape {} hull", t.name));
        }
    }

    let mut posed = m.compose(&MhShape::default());
    let info = m.pose(&mut posed, Some(g.pose.arm), Some(g.pose.leg));
    let e = &g.pose.info;
    close_all(
        [info.left_arm, info.left_leg, info.right_arm, info.right_leg],
        &[e.l_arm, e.l_leg, e.r_arm, e.r_leg],
        F64_TOL,
        "natural limb angles",
    );
    close_all(widen(&posed), &g.pose.pos, F32_TOL, "posed positions");

    for (i, run) in g.fits.iter().enumerate() {
        check_fit(&m.fit(&run.spec), &run.fit, F64_TOL, &format!("fit {i}"));
    }
}

#[test]
fn morph_target_avatar_matches() {
    let g = &golden().makehuman.avatar;
    let m = model();
    let avatar = Avatar::makehuman(m, &g.spec, 10.0);
    assert_eq!(avatar.kind, AvatarKind::MakeHuman);
    let report = avatar.makehuman.as_ref().expect("fit report");
    close_all(report.measured.0, &g.measured, F64_TOL, "measured");
    check_fit(&report.fit, &g.fit, F64_TOL, "fit");
    let p = &report.pose;
    close_all(
        [p.left_arm, p.left_leg, p.right_arm, p.right_leg],
        &[g.pose.l_arm, g.pose.l_leg, g.pose.r_arm, g.pose.r_leg],
        F64_TOL,
        "pose",
    );
    // levels and sections come from single-precision positions
    check_profile(&avatar.profile, &g.profile, F64_TOL, "morph-target");
    close(avatar.profile.half_shoulder, g.sh_h, F64_TOL, "half shoulder");
    close(avatar.profile.hip_axes[0], g.hip[0], F64_TOL, "hip axis x");
    close(avatar.profile.hip_axes[1], g.hip[1], F64_TOL, "hip axis z");
    check_joints(&avatar, &g.joints, F64_TOL, "morph-target");
    assert_eq!(report.tapes.len(), g.tapes.len());
    for (t, e) in report.tapes.iter().zip(&g.tapes) {
        assert_eq!((t.key, t.closed), (e.k.as_str(), e.closed));
        assert_eq!(t.points.len(), e.pts.len(), "tape {}", e.k);
        for (a, b) in t.points.iter().zip(&e.pts) {
            close3(*a, *b, F64_TOL, &format!("tape {}", e.k));
        }
    }
    close_all(
        widen(&avatar.mesh.positions),
        &g.mesh.pos,
        F32_TOL,
        "mesh positions",
    );
    close_all(widen(&avatar.mesh.normals), &g.mesh.nor, F32_TOL, "mesh normals");
    assert_eq!(avatar.mesh.indices, g.mesh.idx, "mesh triangles");
    let collision = avatar.body_collision_mesh().expect("collision mesh");
    assert_eq!(collision.body_vertex_count, g.mesh.body_v);
    check_grid(&avatar.grid, &g.grid, "morph-target");
    check_samples(&avatar.grid, &g.samples, F64_TOL, "morph-target");
    check_points(&avatar, &g.arrangement_points, F64_TOL, "morph-target");
    assert_eq!(MEASUREMENTS.len(), g.measured.len());
    assert_eq!(FIT_TARGETS.len(), g.fit.meas.len());
}

#[test]
fn malformed_packs_are_rejected() {
    assert!(MhModel::parse(b"nope").is_err());
    assert!(MhModel::parse(&PACK[..PACK.len() / 2]).is_err());
    let mut bad = PACK.to_vec();
    bad[0] = b'X';
    assert!(MhModel::parse(&bad).is_err());
}
