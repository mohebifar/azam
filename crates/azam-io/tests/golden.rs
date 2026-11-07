//! Compares DXF, SVG and OBJ export, DXF import, the tech-pack tables and CSV, pattern dimensions
//! and the pattern PDF plan with recorded reference data for every garment template and a sampler
//! project that uses every feature.

mod common;

use azam_doc::PieceId;
use azam_geom::{Vec2, distance_to_segment};
use azam_io::dimensions::{allowance_summary, dim_table, label_pole, piece_dims, sa_max};
use azam_io::pdf::pattern::{PlanSheets, pdf_items, plan};
use azam_io::{
    ObjMaterial, ObjMesh, ObjPiece, export_dxf, export_obj, export_svg, import_dxf, tech_data, techpack_csv,
};
use common::{assert_close, assert_text_close, load, pdf_options, projects};
use serde_json::{Value, json};

/// Numbers computed the same way agree to rounding noise.
const TOL: f64 = 1e-9;
/// Printed coordinates may differ in the last printed digit (3 decimals) where a value sits on a
/// rounding boundary and the trigonometry differs in the last bit.
const PRINTED_TOL: f64 = 1.000_001e-3;
/// Import simplifies outlines to within 0.4 mm.
const SIMPLIFY_MM: f64 = 0.4;

fn xy(p: Vec2) -> Value {
    json!([p.x, p.y])
}

#[test]
fn dxf_export_matches() {
    for case in projects() {
        let project = load(case);
        let name = case["name"].as_str().expect("name");
        assert_text_close(
            case["dxf"].as_str().expect("dxf"),
            &export_dxf(&project),
            PRINTED_TOL,
            &format!("{name} dxf"),
        );
    }
}

fn imported_json(pieces: &[azam_io::ImportedPiece]) -> Value {
    Value::Array(
        pieces
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "pts": p.points.iter().map(|q| xy(q.pos)).collect::<Vec<_>>(),
                    "internals": p.internals.iter().map(|il| json!({
                        "pts": il.points.iter().map(|&q| xy(q)).collect::<Vec<_>>(),
                        "closed": il.closed,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

#[test]
fn dxf_import_matches_and_round_trips() {
    for case in projects() {
        let project = load(case);
        let name = case["name"].as_str().expect("name");
        let text = export_dxf(&project);
        let imported = match import_dxf(&text) {
            Ok(p) => p,
            Err(azam_io::DxfError::NoPieces) if project.pieces.is_empty() => Vec::new(),
            Err(e) => panic!("{name}: {e}"),
        };
        assert_close(
            &case["dxfImport"],
            &imported_json(&imported),
            1e-9,
            &format!("{name} import"),
        );

        // Every imported corner lies on the exported sewing line (within the simplification).
        let mut pieces: Vec<_> = project.pieces.values().collect();
        pieces.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        assert_eq!(pieces.len(), imported.len(), "{name}: piece count");
        for (pc, imp) in pieces.iter().zip(&imported) {
            assert_eq!(imp.name, pc.name, "{name}: imported name");
            let ol = pc.graded_outline(&project.active_size, &project.base_size);
            let n = ol.poly.len();
            for q in &imp.points {
                let d = (0..n)
                    .map(|i| distance_to_segment(q.pos, ol.poly[i], ol.poly[(i + 1) % n]).distance)
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    d < 2e-3,
                    "{name}/{}: imported point {:?} is {d} mm off",
                    pc.name,
                    q.pos
                );
            }
            // ...and the sewing line stays within the simplification tolerance of the import.
            let m = imp.points.len();
            for &p in &ol.poly {
                let d = (0..m)
                    .map(|i| distance_to_segment(p, imp.points[i].pos, imp.points[(i + 1) % m].pos).distance)
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    d <= SIMPLIFY_MM + 2e-3,
                    "{name}/{}: outline point {p:?} is {d} mm from the import",
                    pc.name
                );
            }
        }
    }
}

#[test]
fn foreign_dxf_import_matches() {
    for case in common::golden()["foreignDxf"].as_array().expect("foreign") {
        let name = case["name"].as_str().expect("name");
        let pieces = import_dxf(case["text"].as_str().expect("text")).expect("imports");
        assert_close(&case["pieces"], &imported_json(&pieces), 1e-9, name);
    }
}

#[test]
fn svg_export_matches() {
    for case in projects() {
        let project = load(case);
        let name = case["name"].as_str().expect("name");
        assert_text_close(
            case["svg"].as_str().expect("svg"),
            &export_svg(&project),
            1.000_001e-2,
            &format!("{name} svg"),
        );
    }
}

#[test]
fn tech_data_matches() {
    for case in projects() {
        let project = load(case);
        let name = case["name"].as_str().expect("name");
        let actual = serde_json::to_value(tech_data(&project)).expect("serializes");
        assert_close(&case["techData"], &actual, TOL, &format!("{name} techData"));
    }
}

#[test]
fn techpack_csv_matches() {
    for case in projects() {
        let project = load(case);
        let name = case["name"].as_str().expect("name");
        assert_eq!(
            techpack_csv(&project),
            case["csv"].as_str().expect("csv"),
            "{name} csv"
        );
    }
}

#[test]
fn dimensions_match() {
    for case in projects() {
        let project = load(case);
        let name = case["name"].as_str().expect("name");
        let nums = azam_doc::seam_numbers(&project);
        let ids: Vec<PieceId> = project.pieces.keys().cloned().collect();
        let opts = azam_io::PatternPdfOptions {
            piece_ids: ids,
            ..Default::default()
        };
        let actual: Vec<Value> = pdf_items(&project, &opts)
            .iter()
            .map(|it| {
                let pc = it.piece;
                let d = piece_dims(&project, pc, None);
                let t = dim_table(&project, pc, &it.geom.outline, &nums);
                let pole = label_pole(&it.geom.outline);
                let m = &t.model;
                json!({
                    "id": pc.id,
                    "dims": { "w": d.w, "h": d.h, "cw": d.cw, "ch": d.ch, "area": d.area, "cutArea": d.cut_area, "perim": d.perim },
                    "allowance": allowance_summary(pc),
                    "saMax": sa_max(pc),
                    "box": [it.bounds.x0, it.bounds.y0, it.bounds.x1, it.bounds.y1],
                    "sym": m.symmetric,
                    "cx": m.cx,
                    "corners": m.corners.iter().map(|c| json!({
                        "L": c.letter, "x": c.pos.x, "y": c.pos.y, "X": c.ordinate.x, "Y": c.ordinate.y,
                        "ang": c.angle, "smooth": c.smooth,
                    })).collect::<Vec<_>>(),
                    "edges": m.edges.iter().map(|e| json!({
                        "from": e.from, "to": e.to, "curve": e.curve, "len": e.len, "chord": e.chord,
                        "allow": e.allow, "hem": e.hem,
                    })).collect::<Vec<_>>(),
                    "seams": t.seams,
                    "notches": t.notches.iter().map(|n| json!({ "k": n.k, "from": n.from, "to": n.to, "a": n.a, "b": n.b })).collect::<Vec<_>>(),
                    "pole": {
                        "x": pole.pos.x, "y": pole.pos.y, "s": pole.score, "d": pole.clearance,
                        "cands": pole.candidates.iter().take(10).map(|(p, s)| json!([p.x, p.y, s])).collect::<Vec<_>>(),
                    },
                })
            })
            .collect();
        assert_close(
            &case["dimensions"],
            &Value::Array(actual),
            TOL,
            &format!("{name} dimensions"),
        );
    }
}

#[test]
fn pdf_plans_match() {
    for case in projects() {
        let project = load(case);
        let name = case["name"].as_str().expect("name");
        for p in case["plans"].as_array().expect("plans") {
            let o = pdf_options(&p["options"]);
            let pl = plan(&project, &o);
            let pack_json = |pk: &azam_io::pdf::pattern::Pack| {
                json!({
                    "w": pk.w, "h": pk.h,
                    "pos": pk.pos.iter().map(|q| json!([q.x, q.y, q.w, q.h])).collect::<Vec<_>>(),
                    "rows": pk.rows.iter().map(|r| json!({ "y": r.y, "h": r.h, "idx": r.idx })).collect::<Vec<_>>(),
                })
            };
            let mut actual = json!({
                "pages": pl.pages, "note": pl.note, "sewPages": pl.sew_pages, "dimPages": pl.dim_pages,
                "items": pl.items.iter().map(|it| json!({
                    "id": it.piece.id,
                    "k": it.fit.map(|f| f.k),
                    "land": it.fit.map(|f| f.landscape),
                })).collect::<Vec<_>>(),
                "pack": null, "cols": null, "rows": null, "tw": null, "th": null, "used": null, "roll": null, "sheets": null,
            });
            match &pl.sheets {
                PlanSheets::Tiled(t) => {
                    actual["pack"] = pack_json(&t.pack);
                    actual["cols"] = json!(t.cols);
                    actual["rows"] = json!(t.rows);
                    actual["tw"] = json!(t.tile_w);
                    actual["th"] = json!(t.tile_h);
                    actual["used"] = json!(t.used.iter().map(|&(r, c)| [r, c]).collect::<Vec<_>>());
                }
                PlanSheets::Plotter(pp) => {
                    actual["pack"] = pack_json(&pp.pack);
                    actual["roll"] = json!(pp.roll);
                    actual["sheets"] = json!(
                        pp.sheets
                            .iter()
                            .map(|s| json!({ "y0": s.y0, "h": s.h, "idx": s.idx }))
                            .collect::<Vec<_>>()
                    );
                }
                _ => {}
            }
            assert_close(&p["plan"], &actual, TOL, &format!("{name} plan {}", p["options"]));
        }
    }
}

/// Mesh buffers of one recorded piece.
struct Buffers {
    name: String,
    material: ObjMaterial,
    hidden: bool,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f64; 2]>,
    triangles: Vec<[u32; 3]>,
}

#[test]
fn obj_export_matches() {
    for case in projects() {
        let project = load(case);
        let name = case["name"].as_str().expect("name");
        let rec = &case["obj"];
        let f32s = |v: &Value| -> Vec<[f32; 3]> {
            let flat: Vec<f32> = v
                .as_array()
                .expect("array")
                .iter()
                .map(|x| x.as_f64().expect("number") as f32)
                .collect();
            flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
        };
        let tris = |v: &Value| -> Vec<[u32; 3]> {
            let flat: Vec<u32> = v
                .as_array()
                .expect("array")
                .iter()
                .map(|x| x.as_u64().expect("index") as u32)
                .collect();
            flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
        };
        let buffers: Vec<Buffers> = rec["pieces"]
            .as_array()
            .expect("pieces")
            .iter()
            .map(|p| {
                let pc = &project.pieces[&PieceId::from(p["id"].as_str().expect("id"))];
                let uv: Vec<f64> = p["uvs"]
                    .as_array()
                    .expect("uvs")
                    .iter()
                    .map(|x| x.as_f64().expect("number"))
                    .collect();
                Buffers {
                    name: pc.name.clone(),
                    material: ObjMaterial::for_fabric(&project, &pc.fabric),
                    hidden: p["hidden"].as_bool().unwrap_or(false),
                    positions: f32s(&p["positions"]),
                    normals: f32s(&p["normals"]),
                    uvs: uv.chunks(2).map(|c| [c[0], c[1]]).collect(),
                    triangles: tris(&p["triangles"]),
                }
            })
            .collect();
        let pieces: Vec<ObjPiece> = buffers
            .iter()
            .map(|b| ObjPiece {
                name: &b.name,
                material: b.material.clone(),
                hidden: b.hidden,
                positions: &b.positions,
                normals: &b.normals,
                uvs: &b.uvs,
                triangles: &b.triangles,
            })
            .collect();
        let (ap, an, at) = (
            f32s(&rec["avatar"]["positions"]),
            f32s(&rec["avatar"]["normals"]),
            tris(&rec["avatar"]["triangles"]),
        );
        let avatar = ObjMesh {
            positions: &ap,
            normals: &an,
            triangles: &at,
        };
        let out = export_obj(&pieces, Some(&avatar));
        assert_eq!(out.obj, rec["obj"].as_str().expect("obj"), "{name} obj");
        assert_eq!(out.mtl, rec["mtl"].as_str().expect("mtl"), "{name} mtl");
    }
}
