//! Compares azam-geom with recorded reference data for every garment template piece.
//!
//! Regenerate the data with `node tools/golden/export-geom.mjs > crates/azam-geom/tests/golden/geom.json`.

use std::collections::BTreeMap;

use azam_geom::{
    DEFAULT_SIDE_STEP_MM, EdgeAllowances, Outline, OutlinePoint, PointId, SewingSide, SideDirection, SideEnd,
    Vec2, contains, cut_outline, flatten_open_line, offset_outline, signed_area,
};
use serde::Deserialize;

const GOLDEN: &str = include_str!("golden/geom.json");
/// Relative tolerance. The port does the same f64 arithmetic in the same order, so results agree
/// to within a few ulps; `hypot` implementations may differ in the last bit.
const TOLERANCE: f64 = 1e-9;

#[derive(Deserialize)]
struct Golden {
    pieces: Vec<PieceCase>,
}

/// `[x, y, curve (0 or 1), id]` as written by the exporter.
type RawPoint = (f64, f64, u8, String);
type XY = [f64; 2];

#[derive(Deserialize)]
struct PieceCase {
    name: String,
    pts: Vec<RawPoint>,
    outline: OutlineCase,
    area: f64,
    perimeter: Vec<PerimeterCase>,
    nearest: Vec<NearestCase>,
    inside: Vec<InsideCase>,
    offset: OffsetCase,
    cut: CutCase,
    sides: Vec<SideCase>,
    path: PathCase,
    internals: Vec<InternalCase>,
}

#[derive(Deserialize)]
struct OutlineCase {
    total: f64,
    starts: Vec<f64>,
    segs: Vec<SegmentCase>,
    poly: Vec<XY>,
}

#[derive(Deserialize)]
struct SegmentCase {
    id: String,
    i0: usize,
    i1: usize,
    len: f64,
    poly: Vec<XY>,
}

#[derive(Deserialize)]
struct PerimeterCase {
    p: f64,
    x: f64,
    y: f64,
    tx: f64,
    ty: f64,
    k: usize,
    t: f64,
}

#[derive(Deserialize)]
struct NearestCase {
    q: XY,
    k: usize,
    t: f64,
    x: f64,
    y: f64,
    d: f64,
}

#[derive(Deserialize)]
struct InsideCase {
    q: XY,
    inside: bool,
}

#[derive(Deserialize)]
struct OffsetCase {
    widths: BTreeMap<String, f64>,
    pts: Vec<XY>,
}

#[derive(Deserialize)]
struct CutCase {
    sa: f64,
    #[serde(rename = "saSeg")]
    sa_seg: BTreeMap<String, f64>,
    hem: BTreeMap<String, f64>,
    pts: Vec<XY>,
}

#[derive(Deserialize)]
struct SideEndCase {
    s: String,
    t: f64,
}

#[derive(Deserialize)]
struct SideCase {
    a: SideEndCase,
    b: SideEndCase,
    dir: i32,
    length: Option<f64>,
    pts: Vec<XY>,
}

#[derive(Deserialize)]
struct PathCase {
    p0: f64,
    p1: f64,
    pts: Vec<XY>,
}

#[derive(Deserialize)]
struct InternalCase {
    pts: Vec<RawPoint>,
    flat: Vec<XY>,
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

fn ids(map: &BTreeMap<String, f64>) -> BTreeMap<PointId, f64> {
    map.iter().map(|(k, v)| (PointId::new(k.as_str()), *v)).collect()
}

#[track_caller]
fn assert_num(actual: f64, expected: f64, what: &str) {
    let scale = actual.abs().max(expected.abs()).max(1.0);
    assert!(
        (actual - expected).abs() <= TOLERANCE * scale,
        "{what}: {actual} != {expected} (golden)"
    );
}

#[track_caller]
fn assert_vec(actual: Vec2, expected: XY, what: &str) {
    assert_num(actual.x, expected[0], &format!("{what}.x"));
    assert_num(actual.y, expected[1], &format!("{what}.y"));
}

#[track_caller]
fn assert_poly(actual: &[Vec2], expected: &[XY], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}: point count");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert_vec(*a, *e, &format!("{what}[{i}]"));
    }
}

#[test]
fn golden_file_covers_every_template() {
    let g = golden();
    for prefix in ["tee/", "dress/", "skirt/", "labadeh/", "aba/"] {
        assert!(
            g.pieces.iter().any(|p| p.name.starts_with(prefix)),
            "missing {prefix}"
        );
    }
}

#[test]
fn outlines_match() {
    for case in golden().pieces {
        let ol = Outline::new(&points(&case.pts));
        let name = &case.name;
        assert_num(ol.total, case.outline.total, &format!("{name} total"));
        assert_eq!(ol.segments.len(), case.outline.segs.len(), "{name} segment count");
        for (k, (seg, exp)) in ol.segments.iter().zip(&case.outline.segs).enumerate() {
            let what = format!("{name} segment {k}");
            assert_eq!(seg.id.0, exp.id, "{what} id");
            assert_eq!(
                (seg.start_point, seg.end_point),
                (exp.i0, exp.i1),
                "{what} corners"
            );
            assert_num(seg.len, exp.len, &format!("{what} len"));
            assert_num(ol.starts[k], case.outline.starts[k], &format!("{what} start"));
            assert_poly(&seg.poly, &exp.poly, &format!("{what} poly"));
        }
        assert_poly(&ol.poly, &case.outline.poly, &format!("{name} poly"));
        assert_num(signed_area(&ol.poly), case.area, &format!("{name} area"));
    }
}

#[test]
fn perimeter_points_match() {
    for case in golden().pieces {
        let ol = Outline::new(&points(&case.pts));
        for exp in &case.perimeter {
            let what = format!("{} at p={}", case.name, exp.p);
            let got = ol.point_at_perimeter(exp.p);
            assert_eq!(got.segment, exp.k, "{what} segment");
            assert_num(got.t, exp.t, &format!("{what} t"));
            assert_vec(got.pos, [exp.x, exp.y], &format!("{what} pos"));
            assert_vec(got.tangent, [exp.tx, exp.ty], &format!("{what} tangent"));
        }
    }
}

#[test]
fn nearest_points_match() {
    for case in golden().pieces {
        let ol = Outline::new(&points(&case.pts));
        for exp in &case.nearest {
            let what = format!("{} nearest to {:?}", case.name, exp.q);
            let got = ol.nearest(Vec2::from(exp.q)).expect("outline is not empty");
            assert_eq!(got.segment, exp.k, "{what} segment");
            assert_num(got.t, exp.t, &format!("{what} t"));
            assert_vec(got.pos, [exp.x, exp.y], &format!("{what} pos"));
            assert_num(got.distance, exp.d, &format!("{what} distance"));
        }
    }
}

#[test]
fn inside_tests_match() {
    for case in golden().pieces {
        let ol = Outline::new(&points(&case.pts));
        for exp in &case.inside {
            assert_eq!(
                contains(&ol.poly, Vec2::from(exp.q)),
                exp.inside,
                "{} contains {:?}",
                case.name,
                exp.q
            );
        }
    }
}

#[test]
fn seam_allowance_matches() {
    for case in golden().pieces {
        let ol = Outline::new(&points(&case.pts));
        let widths = ids(&case.offset.widths);
        let offset = offset_outline(&ol, |id| widths[id]);
        assert_poly(&offset, &case.offset.pts, &format!("{} offset", case.name));

        let allowances = EdgeAllowances {
            piece: case.cut.sa,
            per_segment: ids(&case.cut.sa_seg),
            hems: ids(&case.cut.hem),
        };
        let cut = cut_outline(&ol, |id| allowances.finish(id));
        assert_poly(&cut, &case.cut.pts, &format!("{} cut line", case.name));
    }
}

#[test]
fn sewing_sides_match() {
    for case in golden().pieces {
        let ol = Outline::new(&points(&case.pts));
        for (i, exp) in case.sides.iter().enumerate() {
            let what = format!("{} side {i}", case.name);
            let side = SewingSide {
                a: SideEnd {
                    segment: PointId::new(exp.a.s.as_str()),
                    t: exp.a.t,
                },
                b: SideEnd {
                    segment: PointId::new(exp.b.s.as_str()),
                    t: exp.b.t,
                },
                direction: if exp.dir > 0 {
                    SideDirection::Forward
                } else {
                    SideDirection::Backward
                },
            };
            let range = side.range(&ol).map(|r| r.length);
            assert_eq!(range.is_some(), exp.length.is_some(), "{what} range");
            if let (Some(got), Some(want)) = (range, exp.length) {
                assert_num(got, want, &format!("{what} length"));
            }
            let pts: Vec<Vec2> = side
                .polyline(&ol, DEFAULT_SIDE_STEP_MM)
                .iter()
                .map(|p| p.pos)
                .collect();
            assert_poly(&pts, &exp.pts, &format!("{what} polyline"));
        }
    }
}

#[test]
fn outline_paths_match() {
    for case in golden().pieces {
        let ol = Outline::new(&points(&case.pts));
        let path = ol.path_between(case.path.p0, case.path.p1);
        assert_poly(&path, &case.path.pts, &format!("{} path", case.name));
    }
}

#[test]
fn internal_lines_match() {
    for case in golden().pieces {
        for (i, exp) in case.internals.iter().enumerate() {
            let flat = flatten_open_line(&points(&exp.pts));
            assert_poly(&flat, &exp.flat, &format!("{} internal {i}", case.name));
        }
    }
}
