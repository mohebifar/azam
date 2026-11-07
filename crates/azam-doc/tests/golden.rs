//! Compares azam-doc with recorded reference data: fabrics, curve helpers, loading, every garment
//! template, measurements, scripted edit sequences and a file round trip.
//!
//! Regenerate the data with `node tools/golden/export-doc.mjs > crates/azam-doc/tests/golden/doc.json`.
//!
//! Generated ids are compared exactly: the recorded data was produced with the same `<prefix>_<n>`
//! id scheme as [`IdGen`], so the same edits give the same ids.

use std::sync::LazyLock;

use azam_doc::{
    Command, Created, FabricId, FabricPreset, IdGen, PhysKey, Project, Sex, SizeName, Template, apply,
    de_casteljau_split, default_hem, detect_hems, e2d, edge_coverage, limit_strain, make_fabric,
    mirror_seg_id, normalize_value, piece_area, pom_value, r_bend, r_stretch, rigid_map, seam_numbers,
    seam_sa, seg_bez_param, sew_lengths, side_seg_ids, src_seg_id, stress_kpa,
};
use azam_geom::{PointId, Vec2};
use serde_json::{Map, Value, json};

const GOLDEN: &str = include_str!("golden/doc.json");
/// Relative tolerance (absolute below 1). The port does the same f64 arithmetic in the same order;
/// iterative solves and libm functions may differ in the last bits.
const TOLERANCE: f64 = 1e-9;

static DATA: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(GOLDEN).expect("golden data is valid JSON"));

fn data(key: &str) -> &'static Value {
    &DATA[key]
}

fn arr(v: &Value) -> &Vec<Value> {
    v.as_array().expect("array in golden data")
}

fn num(v: &Value) -> f64 {
    v.as_f64().expect("number in golden data")
}

fn text(v: &Value) -> &str {
    v.as_str().expect("string in golden data")
}

/// Compares JSON values: numbers within [`TOLERANCE`], object members that are `null` treated as
/// missing. Differences are collected with their paths.
fn diff(path: &str, expected: &Value, actual: &Value, out: &mut Vec<String>) {
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap_or(f64::NAN), b.as_f64().unwrap_or(f64::NAN));
            if (a - b).abs() > TOLERANCE * a.abs().max(b.abs()).max(1.0) {
                out.push(format!("{path}: expected {a}, got {b}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() == b.len() {
                for (i, (x, y)) in a.iter().zip(b).enumerate() {
                    diff(&format!("{path}[{i}]"), x, y, out);
                }
            } else {
                out.push(format!("{path}: expected {} items, got {}", a.len(), b.len()));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let keys = a.keys().chain(b.keys().filter(|k| !a.contains_key(*k)));
            for k in keys {
                let (x, y) = (a.get(k).unwrap_or(&Value::Null), b.get(k).unwrap_or(&Value::Null));
                diff(&format!("{path}.{k}"), x, y, out);
            }
        }
        (a, b) if a == b => {}
        (a, b) => out.push(format!("{path}: expected {a}, got {b}")),
    }
}

#[track_caller]
fn assert_matches(what: &str, expected: &Value, actual: &Value) {
    let mut out = Vec::new();
    diff("", expected, actual, &mut out);
    assert!(
        out.is_empty(),
        "{what}: {} differences from the recorded reference data:\n{}",
        out.len(),
        out.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
}

fn sex(v: &Value) -> Sex {
    match text(v) {
        "male" => Sex::Male,
        _ => Sex::Female,
    }
}

#[test]
fn fabric_presets_and_conversions() {
    let fab = data("fabrics");
    let keys: Vec<Value> = PhysKey::ALL.iter().map(|k| json!([k.key(), k.label()])).collect();
    assert_matches("physical property keys", &fab["physKeys"], &Value::Array(keys));
    for case in arr(&fab["functions"]) {
        let k = num(&case["k"]);
        let stress = [
            stress_kpa(k, 0.05, Some(0.3)),
            stress_kpa(k, 0.1, None),
            stress_kpa(k, 0.02, Some(0.0)),
            stress_kpa(k, 0.2, Some(0.05)),
        ];
        let actual = json!({
            "k": k, "e2d": e2d(k), "rStretch": r_stretch(k), "rBend": r_bend(k),
            "limitStrain": limit_strain(k), "stress": stress,
        });
        assert_matches(&format!("fabric scales at {k}"), case, &actual);
    }
    let presets = arr(&fab["presets"]);
    assert_eq!(presets.len(), FabricPreset::ALL.len());
    for case in presets {
        let key = text(&case["key"]);
        let preset = FabricPreset::from_key(key).expect("known preset");
        let fabric = make_fabric(&mut IdGen::after(0), preset);
        assert_matches(
            &format!("preset {key}"),
            &case["fabric"],
            &serde_json::to_value(&fabric).expect("serializable"),
        );
        let ph = &fabric.phys;
        let k_avg = f64::midpoint(ph.stretch_weft, ph.stretch_warp);
        let derived = json!({
            "e2d": [e2d(ph.stretch_weft), e2d(ph.stretch_warp), e2d(ph.shear)],
            "rStretch": [r_stretch(ph.stretch_weft), r_stretch(ph.stretch_warp), r_stretch(ph.shear)],
            "rBend": [r_bend(ph.bend_weft), r_bend(ph.bend_warp)],
            "limitStrain": limit_strain(k_avg),
            "stress": stress_kpa(k_avg, 0.1, Some(ph.thickness)),
        });
        assert_matches(&format!("preset {key} conversions"), &case["derived"], &derived);
    }
}

fn xy(v: Vec2) -> Value {
    json!([v.x, v.y])
}

#[test]
fn curve_helpers() {
    let curves = data("curves");
    let tee = arr(data("templates"))
        .iter()
        .find(|t| t["template"] == "tee" && t["sex"] == "female")
        .expect("tee is recorded");
    let project = Project::load(tee["doc"].clone()).expect("tee loads");
    for case in arr(&curves["segParams"]) {
        let pc = &project.pieces[&azam_doc::PieceId::new(text(&case["piece"]))];
        let ol = pc.outline();
        let seg = &ol.segments[ol
            .segment_index(&PointId::new(text(&case["segment"])))
            .expect("segment")];
        let values: Vec<Value> = arr(&case["values"])
            .iter()
            .map(|tv| {
                let t = num(&tv[0]);
                json!([t, seg_bez_param(seg, t)])
            })
            .collect();
        assert_matches("Bézier parameter", &case["values"], &Value::Array(values));
    }
    for case in arr(&curves["splits"]) {
        let ctrl: Vec<Vec2> = arr(&case["ctrl"])
            .iter()
            .map(|p| Vec2::new(num(&p[0]), num(&p[1])))
            .collect();
        let (left, right) = de_casteljau_split(&ctrl, num(&case["t"]));
        let actual = json!({
            "ctrl": case["ctrl"], "t": case["t"],
            "left": left.into_iter().map(xy).collect::<Vec<_>>(),
            "right": right.into_iter().map(xy).collect::<Vec<_>>(),
        });
        assert_matches("de Casteljau split", case, &actual);
    }
}

#[test]
fn rigid_fit() {
    let rigid = &data("curves")["rigid"];
    let pts = |v: &Value| -> Vec<azam_geom::OutlinePoint> {
        arr(v)
            .iter()
            .map(|p| azam_geom::OutlinePoint::corner(text(&p[0]), num(&p[1]), num(&p[2])))
            .collect()
    };
    let (old, new) = (pts(&rigid["old"]), pts(&rigid["new"]));
    let map = rigid_map(&old, &new).expect("enough shared points");
    let mapped: Vec<Value> = arr(&rigid["mapped"])
        .iter()
        .map(|q| {
            let p = map.apply(Vec2::new(num(&q[0]), num(&q[1])));
            json!([q[0], q[1], p.x, p.y])
        })
        .collect();
    assert_matches("rigid fit", &rigid["mapped"], &Value::Array(mapped));
    assert_eq!(rigid_map(&old, &new[..1]).is_none(), rigid["tooFew"] == true);
}

#[test]
fn loading_repairs_loose_projects() {
    for (i, case) in arr(data("normalize")).iter().enumerate() {
        let repaired = normalize_value(case["input"].clone());
        assert_matches(&format!("repair of case {i}"), &case["output"], &repaired);
        Project::load(case["input"].clone()).unwrap_or_else(|e| panic!("case {i} does not load: {e}"));
    }
}

#[test]
fn templates_match_reference() {
    for case in arr(data("templates")) {
        let id = text(&case["template"]);
        let template = Template::from_id(id).expect("known template");
        let project = azam_doc::from_template(template, sex(&case["sex"]), None, None, 0.0);
        let what = format!("template {id} ({})", text(&case["sex"]));
        assert_matches(&what, &case["doc"], &project.to_value());
        assert_eq!(
            project.ids.last(),
            case["lastId"].as_u64().expect("count"),
            "{what}: ids used"
        );
    }
}

/// The same measurements the exporter records, computed by this crate.
fn queries(p: &Project) -> Value {
    let nums = seam_numbers(p);
    let lengths: Map<String, Value> = sew_lengths(p)
        .into_iter()
        .map(|(id, l)| (id.0, json!({ "a": l.a, "b": l.b })))
        .collect();
    let by_size = |f: &dyn Fn(&SizeName) -> f64| -> Map<String, Value> {
        p.sizes.iter().map(|s| (s.0.clone(), json!(f(s)))).collect()
    };
    let pom_values: Vec<Value> = p
        .poms
        .values()
        .map(|pm| json!({ "id": pm.id, "values": by_size(&|s| pom_value(p, pm, s)) }))
        .collect();
    let areas: Vec<Value> = p
        .pieces
        .values()
        .map(|pc| {
            json!({
                "id": pc.id,
                "net": by_size(&|s| piece_area(p, pc, Some(s), false)),
                "cut": by_size(&|s| piece_area(p, pc, Some(s), true)),
            })
        })
        .collect();
    let coverage: Vec<Value> = p
        .pieces
        .values()
        .map(|pc| {
            let c = edge_coverage(p, pc, &pc.outline(), Some(&nums));
            json!({
                "id": pc.id,
                "sides": c.sides.iter().map(|s| json!({
                    "start": s.start, "end": s.end, "seam": s.seam, "number": s.number, "partners": s.partners,
                })).collect::<Vec<_>>(),
                "sewn": c.sewn, "free": c.free,
                "freeParts": c.free_parts.iter().map(|f| json!({
                    "start": f.start, "end": f.end, "segment": f.segment, "segmentId": f.segment_id,
                })).collect::<Vec<_>>(),
                "total": c.total,
            })
        })
        .collect();
    let side_segs: Vec<Value> = p
        .sewing
        .values()
        .map(|sw| {
            let segs = |sides: &[azam_doc::SeamSide]| -> Vec<Vec<PointId>> {
                sides
                    .iter()
                    .map(|sd| {
                        p.pieces
                            .get(&sd.piece)
                            .map_or_else(Vec::new, |pc| side_seg_ids(&pc.outline(), sd))
                    })
                    .collect()
            };
            json!({ "id": sw.id, "sa": seam_sa(p, sw), "a": segs(&sw.a), "b": segs(&sw.b) })
        })
        .collect();
    let mut fabrics = Vec::new();
    let mut q = p.clone();
    for cw in p.colorways.keys() {
        q.active_colorway = cw.clone();
        for fid in p.fabrics.keys() {
            let value = q
                .effective_fabric(fid)
                .map(|f| serde_json::to_value(f).expect("serializable"));
            fabrics.push(json!({ "colorway": cw, "fabric": fid, "value": value }));
        }
    }
    let mirrors: Vec<Value> = p
        .pieces
        .values()
        .filter_map(|pc| {
            let src = p.pieces.get(&pc.mirror.as_ref()?.src)?;
            let corners = |pts: &[azam_geom::OutlinePoint]| pts.iter().filter(|q| !q.curve).map(|q| q.id.clone()).collect::<Vec<_>>();
            Some(json!({
                "id": pc.id,
                "toMirror": corners(&src.pts).into_iter().map(|id| json!([id, mirror_seg_id(src, &id)])).collect::<Vec<_>>(),
                "toSource": corners(&pc.pts).into_iter().map(|id| json!([id, src_seg_id(src, &id)])).collect::<Vec<_>>(),
            }))
        })
        .collect();
    let default_hems: Map<String, Value> = p
        .pieces
        .values()
        .map(|pc| (pc.id.0.clone(), json!(default_hem(p, pc))))
        .collect();
    json!({
        "sewLengths": lengths, "seamNumbers": nums, "pomValues": pom_values, "areas": areas,
        "coverage": coverage, "sideSegs": side_segs, "fabrics": fabrics, "mirrors": mirrors,
        "defaultHems": default_hems, "detectHems": detect_hems(p),
    })
}

/// Recorded measurements with zero-length sewn intervals removed.
///
/// A side that ends on the outline start can come out a few ulps past it, which the reference
/// split off as a sewn interval of length ~1e-13; this crate treats such a side as ending on the
/// start.
fn without_degenerate_sewn(queries: &Value) -> Value {
    let mut q = queries.clone();
    for c in q["coverage"].as_array_mut().expect("coverage list") {
        if let Some(sewn) = c["sewn"].as_array_mut() {
            sewn.retain(|iv| num(&iv[1]) - num(&iv[0]) > 1e-9);
        }
    }
    q
}

#[test]
fn template_measurements() {
    for case in arr(data("templates")) {
        let project = Project::load(case["doc"].clone()).expect("template loads");
        let what = format!(
            "measurements of {} ({})",
            text(&case["template"]),
            text(&case["sex"])
        );
        assert_matches(
            &what,
            &without_degenerate_sewn(&case["queries"]),
            &queries(&project),
        );
    }
}

fn created_json(created: &[Created]) -> Value {
    Value::Array(
        created
            .iter()
            .map(|c| match c {
                Created::Piece(id) => json!({ "Piece": id }),
                Created::Point(id) => json!({ "Point": id }),
                Created::Seam(id) => json!({ "Seam": id }),
                Created::InternalLine(id) => json!({ "InternalLine": id }),
            })
            .collect(),
    )
}

#[test]
fn edit_sequences() {
    for seq in arr(data("edits")) {
        let name = text(&seq["name"]);
        let mut project = Project::load(seq["start"].clone()).expect("sequence start loads");
        for (i, step) in arr(&seq["steps"]).iter().enumerate() {
            let command: Command = serde_json::from_value(step["command"].clone())
                .unwrap_or_else(|e| panic!("{name} step {i}: bad command: {e}"));
            let label = command.label();
            let applied = apply(&mut project, command).unwrap_or_else(|e| panic!("{name} step {i}: {e}"));
            let what = format!("{name} step {i} ({label})");
            assert_matches(&what, &step["created"], &created_json(&applied.created));
            assert_matches(&what, &step["doc"], &project.to_value());
        }
        let expected = without_degenerate_sewn(&seq["queries"]);
        assert_matches(
            &format!("{name} final measurements"),
            &expected,
            &queries(&project),
        );
    }
}

/// Every recorded project, read and written back, keeps all its data.
#[test]
fn stored_projects_round_trip() {
    let mut docs: Vec<&Value> = arr(data("templates")).iter().map(|t| &t["doc"]).collect();
    for seq in arr(data("edits")) {
        docs.push(&seq["start"]);
        docs.extend(arr(&seq["steps"]).iter().map(|s| &s["doc"]));
    }
    for (i, doc) in docs.into_iter().enumerate() {
        let text = doc.to_string();
        let project = Project::from_json(&text).unwrap_or_else(|e| panic!("project {i}: {e}"));
        assert_matches(&format!("round trip of project {i}"), doc, &project.to_value());
        let again = Project::from_json(&project.to_json()).expect("written project loads");
        assert_eq!(
            again.to_value(),
            project.to_value(),
            "project {i}: second round trip"
        );
    }
}

#[test]
fn unknown_fields_survive() {
    let rich = arr(data("edits"))
        .iter()
        .find(|s| s["name"] == "rich")
        .expect("rich sequence is recorded");
    let project = Project::load(rich["start"].clone()).expect("rich project loads");
    let v = project.to_value();
    assert_eq!(v["xray"]["layers"], json!([1, 2]));
    assert_eq!(v["avatar"]["kind"], "makehuman");
    assert_eq!(v["fabrics"]["fab_main"]["texture"]["path"], "tex/wool.png");
    assert_eq!(v["fabrics"]["fab_main"]["phys"]["custom"], 3);
    let fab = project
        .fabrics
        .get(&FabricId::new("fab_main"))
        .expect("main fabric");
    assert!(fab.extra.contains_key("price"));
}
