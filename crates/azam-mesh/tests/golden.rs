//! Compares azam-mesh with recorded reference data for every garment template piece.

use azam_geom::{OutlinePoint, PointId, Vec2};
use azam_mesh::{DEFAULT_SPACING_MM, InternalLine, MeshOptions, PieceMesh, mesh_piece};
use serde::Deserialize;

const GOLDEN: &str = include_str!("golden/mesh.json");
/// Relative tolerance for positions and weights. The port does the same f64 arithmetic in the
/// same order; `hypot`, `sin` and `cos` implementations may differ in the last bit.
const TOLERANCE: f64 = 1e-9;

#[derive(Deserialize)]
struct Golden {
    cases: Vec<MeshCase>,
}

/// `[x, y, curve (0 or 1), id]` as written by the exporter.
type RawPoint = (f64, f64, u8, String);
type XY = [f64; 2];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshCase {
    name: String,
    pts: Vec<RawPoint>,
    internal_lines: Vec<LineCase>,
    pd: Option<f64>,
    grain: Option<f64>,
    mesh: MeshData,
    /// Flat `[a, b, opposite, other opposite or -1]` per edge.
    edges: Option<Vec<i64>>,
    locate: Option<Vec<LocateCase>>,
}

#[derive(Deserialize)]
struct LineCase {
    id: String,
    pts: Vec<RawPoint>,
    fold: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshData {
    n: usize,
    xy: Vec<f64>,
    tris: Vec<u32>,
    n_b: usize,
    bnd: Vec<u32>,
    bk: Vec<u32>,
    bt: Vec<f64>,
    internals: Vec<SamplesCase>,
    pd: f64,
}

#[derive(Deserialize)]
struct SamplesCase {
    id: String,
    verts: Vec<u32>,
    fold: Option<f64>,
}

#[derive(Deserialize)]
struct LocateCase {
    q: XY,
    hit: Option<HitCase>,
}

#[derive(Deserialize)]
struct HitCase {
    tri: usize,
    abc: [u32; 3],
    l: [f64; 3],
}

/// A golden case with its inputs converted, and the mesh computed from them.
struct Run {
    case: MeshCase,
    mesh: PieceMesh,
}

fn golden() -> Golden {
    serde_json::from_str(GOLDEN).expect("golden file parses")
}

fn points(raw: &[RawPoint]) -> Vec<OutlinePoint> {
    raw.iter()
        .map(|(x, y, c, id)| OutlinePoint {
            id: PointId::new(id.as_str()),
            pos: Vec2::new(*x, *y),
            curve: *c != 0,
        })
        .collect()
}

fn run(case: MeshCase) -> Run {
    let line_points: Vec<Vec<OutlinePoint>> = case.internal_lines.iter().map(|l| points(&l.pts)).collect();
    let lines: Vec<InternalLine<'_>> = case
        .internal_lines
        .iter()
        .zip(&line_points)
        .map(|(l, pts)| InternalLine {
            id: &l.id,
            points: pts,
            fold: l.fold,
        })
        .collect();
    let options = MeshOptions {
        spacing_mm: case.pd.unwrap_or(DEFAULT_SPACING_MM),
        grain_deg: case.grain.unwrap_or(0.0),
    };
    let mesh = mesh_piece(&points(&case.pts), &lines, options);
    Run { case, mesh }
}

fn runs() -> Vec<Run> {
    golden().cases.into_iter().map(run).collect()
}

#[track_caller]
fn assert_num(actual: f64, expected: f64, what: &str) {
    let scale = actual.abs().max(expected.abs()).max(1.0);
    assert!(
        (actual - expected).abs() <= TOLERANCE * scale,
        "{what}: {actual} != {expected} (golden)"
    );
}

#[test]
fn golden_file_covers_every_template() {
    let g = golden();
    for prefix in ["tee/", "dress/", "skirt/"] {
        assert!(
            g.cases.iter().any(|c| c.name.starts_with(prefix)),
            "missing {prefix}"
        );
    }
    assert!(
        g.cases
            .iter()
            .any(|c| c.mesh.internals.iter().any(|s| !s.verts.is_empty()))
    );
    assert!(g.cases.iter().any(|c| c.pd.is_none()));
}

#[test]
fn spacing_matches() {
    for Run { case, mesh } in runs() {
        assert_num(mesh.spacing, case.mesh.pd, &format!("{} spacing", case.name));
    }
}

#[test]
fn vertices_match() {
    for Run { case, mesh } in runs() {
        let name = &case.name;
        assert_eq!(mesh.vertex_count(), case.mesh.n, "{name} vertex count");
        assert_eq!(mesh.xy.len(), case.mesh.xy.len(), "{name} coordinate count");
        for (i, (a, e)) in mesh.xy.iter().zip(&case.mesh.xy).enumerate() {
            assert_num(*a, *e, &format!("{name} vertex {} {}", i / 2, ["x", "y"][i % 2]));
        }
    }
}

#[test]
fn boundary_matches() {
    for Run { case, mesh } in runs() {
        let name = &case.name;
        assert_eq!(mesh.boundary_count(), case.mesh.n_b, "{name} boundary count");
        assert_eq!(mesh.boundary, case.mesh.bnd, "{name} boundary vertices");
        assert_eq!(mesh.boundary_segment, case.mesh.bk, "{name} boundary segments");
        let expected_t: Vec<f32> = case.mesh.bt.iter().map(|&t| t as f32).collect();
        assert_eq!(mesh.boundary_t, expected_t, "{name} boundary fractions");
    }
}

#[test]
fn internal_line_samples_match() {
    for Run { case, mesh } in runs() {
        let name = &case.name;
        assert_eq!(
            mesh.internals.len(),
            case.mesh.internals.len(),
            "{name} sampled lines"
        );
        for (got, exp) in mesh.internals.iter().zip(&case.mesh.internals) {
            assert_eq!(got.id, exp.id, "{name} line id");
            assert_eq!(case.internal_lines[got.line].id, exp.id, "{name} line index");
            assert_eq!(got.fold, exp.fold, "{name} line {} fold", exp.id);
            assert_eq!(got.verts, exp.verts, "{name} line {} vertices", exp.id);
        }
    }
}

#[test]
fn triangles_match() {
    for Run { case, mesh } in runs() {
        assert_eq!(mesh.tris, case.mesh.tris, "{} triangles", case.name);
    }
}

#[test]
fn topology_matches() {
    let mut checked = 0;
    for Run { case, mesh } in runs() {
        let Some(expected) = &case.edges else { continue };
        let topo = mesh.topology();
        let got: Vec<i64> = topo
            .edges
            .iter()
            .flat_map(|e| {
                [
                    i64::from(e.a),
                    i64::from(e.b),
                    i64::from(e.opposite),
                    e.other_opposite.map_or(-1, i64::from),
                ]
            })
            .collect();
        assert_eq!(&got, expected, "{} edges", case.name);
        for (index, e) in topo.edges.iter().enumerate() {
            assert_eq!(
                topo.edge_index(e.b, e.a),
                Some(index),
                "{} edge lookup",
                case.name
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no topology cases in the golden data");
}

#[test]
fn locate_matches() {
    let mut checked = 0;
    for Run { case, mesh } in runs() {
        for exp in case.locate.iter().flatten() {
            let what = format!("{} locate {:?}", case.name, exp.q);
            let got = mesh.locate(Vec2::from(exp.q));
            assert_eq!(got.is_some(), exp.hit.is_some(), "{what} found");
            let (Some(got), Some(hit)) = (got, &exp.hit) else {
                continue;
            };
            assert_eq!(got.triangle, hit.tri, "{what} triangle");
            assert_eq!(got.vertices, hit.abc, "{what} vertices");
            for k in 0..3 {
                assert_num(got.barycentric[k], hit.l[k], &format!("{what} weight {k}"));
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no locate cases in the golden data");
}
