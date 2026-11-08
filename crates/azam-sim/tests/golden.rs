//! Compares azam-sim with recorded reference data: settings resolution, friction, grab anchor,
//! settle detection, particle estimates, simulation builds on template projects (constraints,
//! sewing, pins, particle properties, initial positions, colour batches), solver frames, rebuilds
//! that reuse the previous drape, fit maps, the untangler and skin-vs-cloth collision.
//!
//! Regenerate the data with `node tools/golden/export-sim.mjs > crates/azam-sim/tests/golden/sim.json`.
//!
//! Tolerances: the port performs the same operations in the same order and the same precision as
//! the reference (single-precision storage, double-precision arithmetic), and matches it bit for
//! bit. Builds are still compared with a small relative tolerance because they go through `sin`,
//! `cos`, `atan2` and `pow` (arrangement, grain, folds, compliances), whose last bit may differ
//! between math libraries. Solver frames start from the recorded initial state and use only
//! correctly rounded operations, so every recorded frame is compared exactly: the solver is
//! chaotic (contacts switch on and off), and any difference would grow, so none is allowed.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use azam_avatar::{Avatar, AvatarParams};
use azam_doc::{PieceId, Project, SimSettings, normalize_value};
use azam_sim::{
    BodyTriangles, BuildOptions, FitMode, Particles, SettleTracker, SimParams, Simulation, colorize,
    estimate_particles, resolve_settings, untangle::UntangleSetup,
};
use serde_json::{Value, json};

const GOLDEN: &str = include_str!("golden/sim.json");

static DATA: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(GOLDEN).expect("golden data is valid JSON"));

/// Relative tolerance (absolute below 1) for double-precision build results that go through
/// transcendental functions.
const F64_TOL: f64 = 1e-12;
/// Relative tolerance (absolute below 1) for single-precision build results that go through
/// transcendental functions: two single-precision ulps.
const F32_TOL: f64 = 2.5e-7;
/// Relative tolerance of initial positions (placed with `sin`/`cos` around the body).
const POSITION_TOL: f64 = 1e-6;
/// Solver frames from the recorded initial state are compared exactly.
const FRAME_TOL: f64 = 0.0;

fn arr(v: &Value) -> &Vec<Value> {
    v.as_array().expect("array in golden data")
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

fn nums(v: &Value) -> Vec<f64> {
    arr(v).iter().map(num).collect()
}

fn f32s(v: &Value) -> Vec<f32> {
    arr(v).iter().map(|x| num(x) as f32).collect()
}

fn usize_of(v: &Value) -> usize {
    v.as_u64().expect("count in golden data") as usize
}

/// Scaled difference: absolute below 1, relative above.
#[allow(
    clippy::float_cmp,
    reason = "equal values (infinities included) differ by nothing"
)]
fn err(a: f64, b: f64) -> f64 {
    if a == b {
        return 0.0;
    }
    (a - b).abs() / a.abs().max(b.abs()).max(1.0)
}

#[track_caller]
fn close(actual: f64, expected: f64, tol: f64, what: &str) {
    let e = err(actual, expected);
    assert!(
        e <= tol,
        "{what}: expected {expected}, got {actual} (error {e:e} > {tol:e})"
    );
}

/// Largest error over paired lists, with its index.
fn max_err(actual: impl IntoIterator<Item = f64>, expected: impl IntoIterator<Item = f64>) -> (f64, usize) {
    actual
        .into_iter()
        .zip(expected)
        .enumerate()
        .map(|(i, (a, e))| (err(a, e), i))
        .fold((0.0, 0), |m, x| if x.0 > m.0 { x } else { m })
}

#[track_caller]
fn close_all(actual: &[f64], expected: &[f64], tol: f64, what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}: length");
    let (e, i) = max_err(actual.iter().copied(), expected.iter().copied());
    assert!(
        e <= tol,
        "{what}: worst at {i}: expected {}, got {} (error {e:e} > {tol:e})",
        expected[i],
        actual[i]
    );
}

/// Compares JSON values with numbers compared by value (no tolerance).
#[track_caller]
fn same_json(actual: &Value, expected: &Value, what: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            close(num(actual), num(expected), 0.0, &format!("{what}: {a} vs {b}"));
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                same_json(&actual[k.as_str()], &expected[k.as_str()], &format!("{what}.{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{what}"),
    }
}

fn project(doc: &Value) -> Project {
    serde_json::from_value(normalize_value(doc.clone())).expect("recorded project loads")
}

/// Avatars by their parameters (building one takes a while in debug builds).
static AVATARS: LazyLock<Mutex<HashMap<String, &'static Avatar>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn avatar(doc: &Value) -> &'static Avatar {
    let key = doc["avatar"].to_string();
    let mut cache = AVATARS.lock().expect("avatar cache");
    cache.entry(key).or_insert_with(|| {
        let params: AvatarParams = serde_json::from_value(doc["avatar"].clone()).expect("avatar params");
        Box::leak(Box::new(Avatar::parametric(&params)))
    })
}

/// Substeps of each preset in the reference engine. azam's presets take more (see
/// `SimPreset::changes`: stiff cloth does not converge with these), so the recorded runs are
/// replayed with the reference's.
fn reference_substeps(preset: azam_sim::SimPreset) -> f64 {
    match preset {
        azam_sim::SimPreset::Draft => 8.0,
        azam_sim::SimPreset::Normal => 10.0,
        azam_sim::SimPreset::Fine => 14.0,
        azam_sim::SimPreset::Layered => 16.0,
        azam_sim::SimPreset::Final => 20.0,
    }
}

/// Particle-distance scale of each preset in the reference engine (azam's Normal and Fine mesh
/// finer).
fn reference_pd_scale(preset: azam_sim::SimPreset) -> f64 {
    match preset {
        azam_sim::SimPreset::Draft => 1.3,
        azam_sim::SimPreset::Normal | azam_sim::SimPreset::Layered => 1.0,
        azam_sim::SimPreset::Fine => 0.8,
        azam_sim::SimPreset::Final => 0.65,
    }
}

/// `r` as the reference engine resolves it: the preset's substeps and mesh scale as the
/// reference has them, unless the project sets its own.
fn as_reference(mut r: azam_sim::ResolvedSettings) -> azam_sim::ResolvedSettings {
    let (substeps, pd) = (reference_substeps(r.preset), reference_pd_scale(r.preset));
    r.base.substeps = substeps;
    r.base.pd_scale = pd;
    if !r.overrides.contains_key("substeps") {
        r.values.substeps = substeps;
    }
    if !r.overrides.contains_key("pdScale") {
        r.values.pd_scale = pd;
    }
    r
}

fn params_for(case: &Value, project: &Project) -> SimParams {
    let mut p = SimParams::from(&as_reference(resolve_settings(&project.sim)));
    let extra = &case["extraParams"];
    if extra["cpuTriCollision"] == json!(true) {
        p.cpu_triangle_collision = true;
    }
    if let Some(n) = extra["ccEvery"].as_u64() {
        p.triangle_detect_every = n as u32;
    }
    p
}

fn case(name: &str) -> &'static Value {
    arr(&DATA["projects"])
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no recorded project {name}"))
}

fn build_case(name: &str) -> (&'static Value, Project, &'static Avatar, Simulation) {
    let c = case(name);
    let project = project(&c["doc"]);
    let av = avatar(&c["doc"]);
    let params = params_for(c, &project);
    let sim = Simulation::build(&project, av, None, &params);
    (c, project, av, sim)
}

/* ---------- settings ---------- */

#[test]
fn settings_resolve_like_the_reference() {
    for (i, c) in arr(&DATA["settings"]).iter().enumerate() {
        let stored: SimSettings = serde_json::from_value(c["sim"].clone()).expect("stored settings");
        let r = as_reference(resolve_settings(&stored));
        let e = &c["resolved"];
        let what = format!("settings case {i} ({})", c["sim"]);
        assert_eq!(r.preset.key(), e["preset"], "{what}: preset");
        same_json(
            &serde_json::to_value(r.values).expect("values"),
            &e["v"],
            &format!("{what}: values"),
        );
        same_json(
            &serde_json::to_value(r.base).expect("base"),
            &e["base"],
            &format!("{what}: base"),
        );
        same_json(
            &Value::Object(r.overrides.clone()),
            &e["over"],
            &format!("{what}: overrides"),
        );
        assert_eq!(r.modified, e["modified"], "{what}: modified");
        let p = SimParams::from(&r);
        let ep = &c["params"];
        let got = json!({
            "pdScale": p.pd_scale, "substeps": p.substeps, "iterations": p.iterations, "framesPerStep": p.frames_per_step,
            "gravity": p.gravity, "air": p.air, "sewSpeed": p.sew_speed, "grabSpeed": p.grab_speed,
            "selfCollision": p.self_collision, "ccEveryGPU": p.cc_every_gpu, "ccIters": p.cc_iters,
            "clothThickness": p.cloth_thickness, "ccFric": p.cc_fric, "ccRepair": p.cc_repair, "untangle": p.untangle,
            "ccFricMode": i32::from(p.friction_model != azam_sim::FrictionModel::Slide), "ccMuCap": p.cc_mu_cap,
        });
        for (k, v) in got.as_object().expect("object") {
            match (v.as_f64(), ep[k].as_f64()) {
                (Some(a), Some(b)) => close(a, b, 0.0, &format!("{what}: param {k}")),
                _ => assert_eq!(v, &ep[k], "{what}: param {k}"),
            }
        }
    }
}

#[test]
fn friction_fraction_matches() {
    for c in arr(&DATA["fricK"]) {
        let p = SimParams {
            friction_model: if c["params"]["ccFricMode"] == 0 {
                azam_sim::FrictionModel::Slide
            } else {
                azam_sim::FrictionModel::Grip
            },
            cc_mu_cap: num(&c["params"]["ccMuCap"]),
            cc_fric: num(&c["params"]["ccFric"]),
            ..SimParams::default()
        };
        let got = p.friction_fraction(num(&c["mu0"]), num(&c["dn"]), num(&c["tl"]));
        close(got, num(&c["out"]), 0.0, &format!("friction {c}"));
    }
}

#[test]
fn grab_anchor_chases_the_target_like_the_reference() {
    let g = &DATA["grabAnchor"];
    let mut grab = azam_sim::Grab {
        vertices: vec![],
        weights: vec![],
        offsets: vec![],
        target: [0.0; 3],
        current: None,
    };
    for (i, s) in arr(&g["steps"]).iter().enumerate() {
        let t = nums(&s["target"]);
        grab.target = [t[0], t[1], t[2]];
        let a = grab.advance_anchor(num(&g["speed"]), num(&s["dt"]));
        close_all(&a, &nums(&s["anchor"]), 0.0, &format!("grab anchor step {i}"));
    }
}

#[test]
fn settle_tracker_matches() {
    for c in arr(&DATA["settle"]) {
        let mut t = SettleTracker::new();
        for (i, p) in arr(&c["pushes"]).iter().enumerate() {
            let p = arr(p);
            let got = t.push(num(&p[0]), num(&p[1]), num(&p[2]), num(&p[3]));
            let got = got.map(|r| match r {
                azam_sim::SettleReason::Threshold => "threshold",
                azam_sim::SettleReason::Plateau => "plateau",
            });
            assert_eq!(got, p[4].as_str(), "settle '{}' push {i}", c["name"]);
        }
    }
}

#[test]
fn particle_estimates_match() {
    for c in arr(&DATA["estimate"]) {
        let sex = if c["sex"] == "male" {
            azam_doc::Sex::Male
        } else {
            azam_doc::Sex::Female
        };
        // the reference also records templates azam no longer offers (labadeh, aba)
        let Some(template) = azam_doc::Template::from_id(c["template"].as_str().expect("template id")) else {
            continue;
        };
        // the recording was made from the reference engine's drafts (no azam refinements)
        let project = azam_doc::reference_template(template, sex, None, None, 0.0);
        for v in arr(&c["values"]) {
            let v = arr(v);
            assert_eq!(
                estimate_particles(&project, num(&v[0])),
                v[1].as_u64().expect("count"),
                "estimate {} at {}",
                c["template"],
                v[0]
            );
        }
    }
}

/* ---------- builds ---------- */

/// Compares a build with the recorded one.
fn check_build(sim: &Simulation, e: &Value, what: &str) {
    check_records(sim, e, what);
    check_constraint_sums(sim, e, what);
    check_constraint_samples(sim, e, what);
    check_sewing_and_anchors(sim, e, what);
    check_particles(sim, e, what);
    check_coloring(sim, e, what);
}

fn check_records(sim: &Simulation, e: &Value, what: &str) {
    let n = usize_of(&e["N"]);
    assert_eq!(sim.particle_count(), n, "{what}: particles");
    for (r, er) in sim.records.iter().zip(arr(&e["recs"])) {
        assert_eq!(r.id.as_str(), er["id"], "{what}: record id");
        assert_eq!(r.offset, usize_of(&er["off"]), "{what}: {} offset", r.id);
        assert_eq!(r.count, usize_of(&er["n"]), "{what}: {} particles", r.id);
        assert_eq!(
            r.mesh.boundary_count(),
            usize_of(&er["nB"]),
            "{what}: {} boundary",
            r.id
        );
        assert_eq!(
            r.mesh.triangle_count(),
            usize_of(&er["tris"]),
            "{what}: {} triangles",
            r.id
        );
        close(
            r.mesh.spacing,
            num(&er["pd"]),
            0.0,
            &format!("{what}: {} spacing", r.id),
        );
    }
    assert_eq!(sim.records.len(), arr(&e["recs"]).len(), "{what}: records");
    let c = &e["counts"];
    assert_eq!(
        sim.stretch.len(),
        usize_of(&c["S"]),
        "{what}: stretch constraints"
    );
    assert_eq!(sim.bend.len(), usize_of(&c["B"]), "{what}: bending constraints");
    assert_eq!(sim.sewing.len(), usize_of(&c["SW"]), "{what}: sewing constraints");
    assert_eq!(sim.fasteners.a.len(), usize_of(&c["FA"]), "{what}: fasteners");
    assert_eq!(sim.pins.vertex.len(), usize_of(&c["pins"]), "{what}: pins");
}

fn check_constraint_sums(sim: &Simulation, e: &Value, what: &str) {
    let sum = |v: &[f32]| v.iter().fold(0.0, |s, &x| s + f64::from(x));
    let s = &e["sums"];
    close(
        sum(&sim.stretch.rest),
        num(&s["SL"]),
        F64_TOL,
        &format!("{what}: stretch rest sum"),
    );
    close(
        sum(&sim.stretch.compliance),
        num(&s["Sr"]),
        F64_TOL,
        &format!("{what}: stretch compliance sum"),
    );
    close(
        sum(&sim.stretch.compression),
        num(&s["Scm"]),
        F64_TOL,
        &format!("{what}: compression sum"),
    );
    close(
        sum(&sim.bend.rest),
        num(&s["BL"]),
        F64_TOL,
        &format!("{what}: bend rest sum"),
    );
    close(
        sum(&sim.bend.compliance),
        num(&s["Br"]),
        F64_TOL,
        &format!("{what}: bend compliance sum"),
    );
    close(
        sum(&sim.sewing.u),
        num(&s["SWu"]),
        F64_TOL,
        &format!("{what}: sewing u sum"),
    );
}

fn check_constraint_samples(sim: &Simulation, e: &Value, what: &str) {
    for row in arr(&e["stretch"]) {
        let r = nums(row);
        let k = r[0] as usize;
        let st = &sim.stretch;
        assert_eq!(
            (st.i[k], st.j[k]),
            (r[1] as u32, r[2] as u32),
            "{what}: stretch {k} particles"
        );
        close(
            f64::from(st.rest[k]),
            r[3],
            F32_TOL,
            &format!("{what}: stretch {k} rest"),
        );
        close(
            f64::from(st.compliance[k]) * 1e9,
            r[4] * 1e9,
            F32_TOL,
            &format!("{what}: stretch {k} compliance"),
        );
        close(
            f64::from(st.compression[k]),
            r[5],
            F32_TOL,
            &format!("{what}: stretch {k} compression"),
        );
    }
    for row in arr(&e["bend"]).iter().chain(arr(&e["folds"])) {
        let r = nums(row);
        let k = r[0] as usize;
        let b = &sim.bend;
        assert_eq!(
            (b.i[k], b.j[k]),
            (r[1] as u32, r[2] as u32),
            "{what}: bend {k} particles"
        );
        close(
            f64::from(b.rest[k]),
            r[3],
            F32_TOL,
            &format!("{what}: bend {k} rest"),
        );
        close(
            f64::from(b.compliance[k]),
            r[4],
            F32_TOL,
            &format!("{what}: bend {k} compliance"),
        );
    }
    let folds = sim.bend.compliance.iter().filter(|&&r| r < 0.0).count();
    assert_eq!(folds, arr(&e["folds"]).len(), "{what}: fold constraints");
}

fn check_sewing_and_anchors(sim: &Simulation, e: &Value, what: &str) {
    for (k, row) in arr(&e["sewing"]).iter().enumerate() {
        let r = nums(row);
        let sw = &sim.sewing;
        assert_eq!(
            (sw.a[k], sw.b0[k], sw.b1[k], sw.seam[k]),
            (r[0] as u32, r[1] as u32, r[2] as u32, r[4] as u32),
            "{what}: sewing {k}"
        );
        close(
            f64::from(sw.u[k]),
            r[3],
            F32_TOL,
            &format!("{what}: sewing {k} u"),
        );
    }
    for (k, row) in arr(&e["fasteners"]).iter().enumerate() {
        let r = nums(row);
        assert_eq!(
            (sim.fasteners.a[k], sim.fasteners.b[k]),
            (r[0] as u32, r[1] as u32),
            "{what}: fastener {k}"
        );
    }
    let pins: Vec<f64> = sim.pins.vertex.iter().map(|&q| f64::from(q)).collect();
    assert_eq!(pins, nums(&e["pins"]), "{what}: pinned particles");
    let pin_pos: Vec<f64> = sim.pins.pos.iter().map(|&v| f64::from(v)).collect();
    close_all(
        &pin_pos,
        &nums(&e["pinPos"]),
        POSITION_TOL,
        &format!("{what}: pin positions"),
    );
}

fn check_particles(sim: &Simulation, e: &Value, what: &str) {
    let p = &sim.particles;
    for row in arr(&e["particles"]) {
        let r = arr(row);
        let q = usize_of(&r[0]);
        let w = format!("{what}: particle {q}");
        close(
            f64::from(p.inv_mass[q]),
            num(&r[1]),
            F32_TOL,
            &format!("{w} inverse mass"),
        );
        close(
            f64::from(p.inv_mass_free[q]),
            num(&r[2]),
            F32_TOL,
            &format!("{w} free inverse mass"),
        );
        close(f64::from(p.damping[q]), num(&r[3]), 0.0, &format!("{w} damping"));
        close(f64::from(p.margin[q]), num(&r[4]), 0.0, &format!("{w} margin"));
        close(f64::from(p.radius[q]), num(&r[5]), 0.0, &format!("{w} radius"));
        close(
            f64::from(p.friction[q]),
            num(&r[6]),
            0.0,
            &format!("{w} friction"),
        );
        close(f64::from(p.layer[q]), num(&r[7]), 0.0, &format!("{w} layer"));
        close(f64::from(p.spacing[q]), num(&r[8]), 0.0, &format!("{w} spacing"));
        close(
            f64::from(p.thickness[q]),
            num(&r[9]),
            0.0,
            &format!("{w} thickness"),
        );
        close(f64::from(p.piece[q]), num(&r[10]), 0.0, &format!("{w} piece"));
        assert_eq!(
            u8::from(p.active[q]),
            r[11].as_u64().expect("flag") as u8,
            "{w} active"
        );
        close(
            f64::from(p.pressure[q]),
            num(&r[12]),
            0.0,
            &format!("{w} pressure"),
        );
        assert_eq!(
            u8::from(p.sewn[q]),
            r[13].as_u64().expect("flag") as u8,
            "{w} sewn"
        );
    }

    let x: Vec<f64> = p.x.iter().map(|&v| f64::from(v)).collect();
    let ex = arr(&e["x"]);
    if ex.first().is_some_and(Value::is_array) {
        for row in ex {
            let r = nums(row);
            let q = r[0] as usize;
            close_all(
                &x[q * 3..q * 3 + 3],
                &r[1..4],
                POSITION_TOL,
                &format!("{what}: initial position {q}"),
            );
        }
    } else {
        close_all(
            &x,
            &nums(&e["x"]),
            POSITION_TOL,
            &format!("{what}: initial positions"),
        );
    }
}

fn check_coloring(sim: &Simulation, e: &Value, what: &str) {
    let col = &e["coloring"];
    for (name, got) in [
        ("stretch", &sim.coloring.stretch),
        ("bend", &sim.coloring.bend),
        ("fasteners", &sim.coloring.fasteners),
        ("sewing", &sim.coloring.sewing),
    ] {
        let ranges: Vec<Vec<f64>> = arr(&col[name]["ranges"]).iter().map(nums).collect();
        let got_ranges: Vec<Vec<f64>> = got
            .ranges
            .iter()
            .map(|&(s, n)| vec![f64::from(s), f64::from(n)])
            .collect();
        assert_eq!(got_ranges, ranges, "{what}: {name} colour batches");
        if let Some(order) = col[name].get("order") {
            let got_order: Vec<f64> = got.order.iter().map(|&v| f64::from(v)).collect();
            assert_eq!(got_order, nums(order), "{what}: {name} colour order");
        }
    }
}

#[test]
fn avatar_grid_matches_the_recorded_one() {
    for c in arr(&DATA["projects"]) {
        let g = &avatar(&c["doc"]).grid;
        let e = &c["grid"];
        assert_eq!(
            g.dims,
            [usize_of(&e["nx"]), usize_of(&e["ny"]), usize_of(&e["nz"])],
            "grid dims"
        );
        close(g.cell, num(&e["g"]), 0.0, "grid cell");
        close_all(
            &[g.origin.x, g.origin.y, g.origin.z],
            &[num(&e["x0"]), num(&e["y0"]), num(&e["z0"])],
            0.0,
            "grid origin",
        );
        let sum = g.data.iter().fold(0.0, |s, &v| s + f64::from(v));
        close(sum, num(&e["sum"]), F64_TOL, &format!("{}: grid sum", c["name"]));
    }
}

#[test]
fn builds_match_the_reference() {
    for c in arr(&DATA["projects"]) {
        let name = c["name"].as_str().expect("name");
        let (c, _, _, sim) = build_case(name);
        check_build(&sim, &c["build"], name);
    }
}

/* ---------- frames ---------- */

fn aggregates(sim: &Simulation) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut mean = [0.0; 3];
    let x = &sim.particles.x;
    let n = sim.particle_count();
    for q in 0..n {
        for c in 0..3 {
            let v = f64::from(x[q * 3 + c]);
            lo[c] = lo[c].min(v);
            hi[c] = hi[c].max(v);
            mean[c] += v;
        }
    }
    (lo, hi, mean.map(|m| m / n as f64))
}

/// The grab the 3D view starts on pointer-down, scripted as recorded.
fn variety_script(sim: &mut Simulation, f: u64) {
    if f == 4 {
        let q = sim.records[0].offset + 100;
        sim.grab_at(q);
    }
    if (4..9).contains(&f)
        && let Some(g) = &sim.grab
    {
        let t = g.target;
        sim.set_grab_target([t[0] + 8.0, t[1] + 15.0, t[2] + 25.0]);
    }
    if f == 9 {
        sim.release_grab();
    }
}

fn widen(v: &[f32]) -> Vec<f64> {
    v.iter().map(|&x| f64::from(x)).collect()
}

/// Compares the simulation after a frame with the recorded frame `e`.
fn check_frame(sim: &Simulation, e: &Value, what: &str) {
    close(sim.time, num(&e["time"]), FRAME_TOL, &format!("{what}: time"));
    close(
        sim.energy,
        num(&e["energy"]),
        FRAME_TOL,
        &format!("{what}: energy"),
    );
    close(
        sim.sew_gap,
        num(&e["sewGap"]),
        FRAME_TOL,
        &format!("{what}: seam gap"),
    );
    close(
        sim.gravity_scale.unwrap_or(f64::NAN),
        num(&e["gScale"]),
        FRAME_TOL,
        &format!("{what}: gravity scale"),
    );
    if let Some(gp) = e.get("gP") {
        close_all(
            &widen(&sim.piece_gravity),
            &nums(gp),
            FRAME_TOL,
            &format!("{what}: piece gravity"),
        );
    }
    if let Some(sample) = e.get("sample") {
        let (x, v) = (&sim.particles.x, &sim.particles.v);
        for row in arr(sample) {
            let r = nums(row);
            let k = r[0] as usize * 3;
            close_all(
                &widen(&x[k..k + 3]),
                &r[1..4],
                FRAME_TOL,
                &format!("{what}: position {}", r[0]),
            );
            close_all(
                &widen(&v[k..k + 3]),
                &r[4..7],
                FRAME_TOL,
                &format!("{what}: velocity {}", r[0]),
            );
        }
        let (lo, hi, mean) = aggregates(sim);
        close_all(
            &lo,
            &nums(&e["lo"]),
            FRAME_TOL,
            &format!("{what}: bounding box min"),
        );
        close_all(
            &hi,
            &nums(&e["hi"]),
            FRAME_TOL,
            &format!("{what}: bounding box max"),
        );
        close_all(
            &mean,
            &nums(&e["mean"]),
            FRAME_TOL,
            &format!("{what}: mean position"),
        );
    }
    if let Some(x) = e.get("x") {
        close_all(
            &widen(&sim.particles.x),
            &nums(x),
            FRAME_TOL,
            &format!("{what}: positions"),
        );
        close_all(
            &widen(&sim.particles.v),
            &nums(&e["v"]),
            FRAME_TOL,
            &format!("{what}: velocities"),
        );
    }
}

/// Runs the recorded frames from the recorded initial state and compares every one of them.
fn run_frames(name: &str, script: Option<fn(&mut Simulation, u64)>, advance: bool) {
    let (c, _, av, mut sim) = build_case(name);
    sim.set_positions(&f32s(&c["build"]["x"]));
    for e in arr(&c["frames"]) {
        let f = e["frame"].as_u64().expect("frame");
        if let Some(s) = script {
            s(&mut sim, f);
        }
        if advance {
            sim.advance(av, 1.0 / 60.0);
        } else {
            sim.step(av, 1.0 / 60.0);
        }
        check_frame(&sim, e, &format!("{name} frame {f}"));
    }
}

#[test]
fn tee_frames_match() {
    run_frames("tee", None, false);
}

#[test]
fn variety_frames_match() {
    run_frames("tee variety", Some(variety_script), false);
}

#[test]
fn triangle_contact_frames_match() {
    run_frames("tee triangle contacts", None, false);
}

#[test]
fn draft_dress_frames_match() {
    run_frames("dress draft", None, true);
}

/* ---------- fit maps ---------- */

#[test]
fn fit_values_match() {
    for name in ["tee", "tee variety", "dress draft"] {
        let (c, project, av, mut sim) = build_case(name);
        let last = arr(&c["frames"]).last().expect("frames");
        sim.set_positions(&f32s(&last["x"]));
        let fit = &c["fit"];
        for (mode, key) in [
            (FitMode::Strain, "strain"),
            (FitMode::Stress, "stress"),
            (FitMode::Fit, "fit"),
            (FitMode::Pressure, "pressure"),
        ] {
            let v = sim.fit_values(&project, av, mode);
            let (got, exp): (Vec<f64>, Vec<f64>) = arr(&fit[key])
                .iter()
                .map(|r| {
                    let r = nums(r);
                    (f64::from(v[r[0] as usize]), r[1])
                })
                .unzip();
            // stress goes through `pow`; the other modes are exact
            close_all(&got, &exp, F32_TOL, &format!("{name}: {key} fit values"));
            if mode == FitMode::Fit {
                let s = sim.fit_summary(&v);
                close(
                    s.tight,
                    num(&fit["summary"]["tight"]),
                    F64_TOL,
                    &format!("{name}: tight share"),
                );
                close(
                    s.over_limit,
                    num(&fit["summary"]["cant"]),
                    F64_TOL,
                    &format!("{name}: over-limit share"),
                );
            }
        }
    }
}

/* ---------- rebuilds ---------- */

#[test]
fn rebuilds_reuse_the_previous_drape() {
    let (c, _, av, mut prev) = build_case("tee");
    let last = arr(&c["frames"]).last().expect("frames");
    prev.set_positions(&f32s(&last["x"]));
    prev.gravity_scale = Some(num(&last["gScale"]));
    for rb in arr(&c["rebuilds"]) {
        let what = rb["name"].as_str().expect("name");
        let project = project(&rb["doc"]);
        let options = BuildOptions {
            rearrange: rb["rearrange"] == json!(true),
            rearrange_ids: arr(&rb["rearrangeIds"])
                .iter()
                .map(|v| PieceId::new(v.as_str().expect("id")))
                .collect(),
            project_out: true,
        };
        let sim = Simulation::build_with(&project, av, Some(&prev), &prev.params, &options);
        assert_eq!(sim.particle_count(), usize_of(&rb["N"]), "{what}: particles");
        for (r, er) in sim.records.iter().zip(arr(&rb["recs"])) {
            assert_eq!(
                (r.offset, r.count),
                (usize_of(&er["off"]), usize_of(&er["n"])),
                "{what}: {}",
                r.id
            );
        }
        assert_eq!(
            sim.stretch.len(),
            usize_of(&rb["counts"]["S"]),
            "{what}: stretch constraints"
        );
        assert_eq!(
            sim.sewing.len(),
            usize_of(&rb["counts"]["SW"]),
            "{what}: sewing constraints"
        );
        let x: Vec<f64> = sim.particles.x.iter().map(|&v| f64::from(v)).collect();
        close_all(&x, &nums(&rb["x"]), POSITION_TOL, &format!("{what}: positions"));
        match rb["gScale"].as_f64() {
            Some(g) => close(
                sim.gravity_scale.unwrap_or(f64::NAN),
                g,
                0.0,
                &format!("{what}: gravity scale"),
            ),
            None => assert_eq!(sim.gravity_scale, None, "{what}: gravity scale"),
        }
    }
}

/* ---------- untangler ---------- */

#[test]
fn untangler_marks_the_poked_through_region() {
    let u = &DATA["untangle"];
    let project = project(&u["doc"]);
    let av = avatar(&u["doc"]);
    let options = BuildOptions {
        project_out: false,
        ..BuildOptions::default()
    };
    // The recording was made at the reference engine's Normal mesh.
    let params = SimParams {
        pd_scale: reference_pd_scale(azam_sim::SimPreset::Normal),
        ..SimParams::default()
    };
    let mut sim = Simulation::build_with(&project, av, None, &params, &options);
    sim.set_positions(&f32s(&u["x"]));
    let setup = UntangleSetup::new(&sim);
    let s = &u["setup"];
    assert_eq!(setup.particles, usize_of(&s["N"]), "untangle particles");
    assert_eq!(setup.tris.len() / 4, usize_of(&s["nT"]), "untangle triangles");
    assert_eq!(setup.edges.len() / 2, usize_of(&s["nE"]), "untangle edges");
    assert_eq!(
        setup.adj_start[setup.particles] as usize,
        usize_of(&s["adj"]),
        "untangle adjacency"
    );
    close(setup.cell, num(&s["cell"]), F64_TOL, "untangle cell");
    let r = setup.run(&sim.particles.x);
    let e = &u["result"];
    assert_eq!(r.crossings, usize_of(&e["crossings"]), "crossings");
    assert_eq!(r.groups, usize_of(&e["groups"]), "groups");
    assert_eq!(r.resolved, usize_of(&e["resolved"]), "resolved");
    assert_eq!(r.ambiguous, usize_of(&e["ambiguous"]), "ambiguous");
    assert_eq!(r.marked, usize_of(&e["marked"]), "marked");
    let marked: Vec<Vec<f64>> = r
        .tangles
        .iter()
        .enumerate()
        .filter(|(_, c)| **c != 0)
        .map(|(q, &c)| vec![q as f64, f64::from(c)])
        .collect();
    let expected: Vec<Vec<f64>> = arr(&e["tangles"]).iter().map(nums).collect();
    assert_eq!(marked, expected, "tangle codes");
}

/* ---------- skin vertices vs cloth triangles ---------- */

#[test]
fn skin_collision_matches() {
    let b = &DATA["bodyTri"];
    let x = f32s(&b["x"]);
    let n = x.len() / 3;
    let mut p = Particles {
        x,
        prev: f32s(&b["prev"]),
        inv_mass: f32s(&b["w"]),
        ..Particles::default()
    };
    assert_eq!(p.inv_mass.len(), n);
    let tris: Vec<u32> = arr(&b["tris"])
        .iter()
        .map(|v| v.as_u64().expect("index") as u32)
        .collect();
    let mut bt = BodyTriangles::from_triangles(tris, f32s(&b["margins"]));
    let skin = f32s(&b["skin"]);
    let fixes = bt.collide_with_skin(&mut p, &skin, &f32s(&b["normals"]), skin.len() / 3);
    assert_eq!(fixes, usize_of(&b["fixes"]), "corrections");
    let got: Vec<f64> = p.x.iter().map(|&v| f64::from(v)).collect();
    close_all(&got, &nums(&b["outX"]), 0.0, "corrected positions");
    let got: Vec<f64> = p.prev.iter().map(|&v| f64::from(v)).collect();
    close_all(&got, &nums(&b["outPrev"]), 0.0, "corrected substep starts");
}

/* ---------- colouring ---------- */

#[test]
fn colouring_separates_shared_particles() {
    let (_, _, _, sim) = build_case("tee");
    let st = &sim.stretch;
    let b = colorize(sim.particle_count(), st.len(), |c| [st.i[c], st.j[c], u32::MAX]);
    for &(start, count) in &b.ranges {
        let mut seen = std::collections::HashSet::new();
        for &c in &b.order[start as usize..(start + count) as usize] {
            assert!(
                seen.insert(st.i[c as usize]) && seen.insert(st.j[c as usize]),
                "batch shares a particle"
            );
        }
    }
}
