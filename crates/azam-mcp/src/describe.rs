//! The project as text for an agent: a summary of the whole garment and every detail of one piece,
//! with the ids the editing tools take.

use azam_doc::{Piece, PieceId, Project};
use serde_json::{Value, json};

/// `v` rounded to a tenth of a millimetre.
fn mm(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Whether `p` is inside the closed polygon `poly` (even-odd rule).
#[must_use]
pub fn inside(poly: &[azam_geom::Vec2], p: azam_geom::Vec2) -> bool {
    let mut odd = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            odd = !odd;
        }
    }
    odd
}

/// The bounding box of `piece`'s outline: `[min_x, min_y, max_x, max_y]`, mm.
fn bbox(piece: &Piece) -> [f64; 4] {
    let poly = piece.outline().poly;
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for p in &poly {
        b = [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)];
    }
    if poly.is_empty() {
        b = [0.0; 4];
    }
    b.map(mm)
}

/// Name of piece `id`, or the id itself.
fn piece_name(project: &Project, id: &PieceId) -> String {
    project
        .pieces
        .get(id)
        .map_or_else(|| id.0.clone(), |p| p.name.clone())
}

/// One side of a seam as stored (`{p, a: {s, t}, b: {s, t}, dir}`), readable.
fn side(project: &Project, wire: &Value) -> Value {
    let piece = PieceId::new(wire["p"].as_str().unwrap_or_default());
    json!({
        "piece": piece.0,
        "pieceName": piece_name(project, &piece),
        "from": {"segment": wire["a"]["s"], "t": wire["a"]["t"]},
        "to": {"segment": wire["b"]["s"], "t": wire["b"]["t"]},
        "reverse": wire["dir"].as_f64().is_some_and(|d| d < 0.0),
    })
}

/// Every seam with its sides and the lengths sewn together.
#[must_use]
pub fn seams(project: &Project) -> Vec<Value> {
    let lengths = azam_doc::sew_lengths(project);
    project
        .sewing
        .values()
        .map(|seam| {
            let wire = serde_json::to_value(seam).unwrap_or_default();
            let group = |k: &str| -> Vec<Value> {
                wire[k]
                    .as_array()
                    .map(|sides| sides.iter().map(|s| side(project, s)).collect())
                    .unwrap_or_default()
            };
            let len = lengths.get(&seam.id);
            json!({
                "id": seam.id.0,
                "a": group("a"),
                "b": group("b"),
                "lengthA": len.map(|l| mm(l.a)),
                "lengthB": len.map(|l| mm(l.b)),
                "mismatch": len.map(|l| mm(l.a - l.b)),
            })
        })
        .collect()
}

/// A summary of the whole project.
#[must_use]
pub fn project(project: &Project, path: Option<&std::path::Path>, sim: &str) -> Value {
    let fabrics: Vec<Value> = project
        .fabrics
        .keys()
        .filter_map(|id| project.effective_fabric(id))
        .map(|f| json!({"id": f.id.0, "name": f.name, "preset": f.preset, "color": f.color}))
        .collect();
    let colorways: Vec<Value> = project
        .colorways
        .values()
        .map(|c| json!({"id": c.id.0, "name": c.name, "active": c.id == project.active_colorway}))
        .collect();
    let pieces: Vec<Value> = project
        .pieces
        .values()
        .map(|p| {
            let b = bbox(p);
            json!({
                "id": p.id.0,
                "name": p.name,
                "points": p.pts.len(),
                "bbox": b,
                "sizeMm": [mm(b[2] - b[0]), mm(b[3] - b[1])],
                "fabric": p.fabric.0,
                "slot": p.arr.slot,
                "mirrorOf": p.mirror.as_ref().map(|m| m.src.0.clone()),
                "state": p.state,
            })
        })
        .collect();
    json!({
        "name": project.meta.name,
        "file": path.map(|p| p.display().to_string()),
        "units": "mm (all coordinates; x right, y down)",
        "sizes": project.sizes,
        "baseSize": project.base_size,
        "activeSize": project.active_size,
        "avatar": project.avatar.body,
        "fabrics": fabrics,
        "colorways": colorways,
        "pieces": pieces,
        "seams": seams(project),
        "simulation": sim,
    })
}

/// Every detail of `piece`: its points and segments with their ids, seams, hems and arrangement.
#[must_use]
pub fn piece(project: &Project, piece: &Piece) -> Value {
    let outline = piece.outline();
    let n = piece.pts.len();
    let points: Vec<Value> = piece
        .pts
        .iter()
        .enumerate()
        .map(|(i, p)| json!({"i": i, "id": p.id.0, "x": mm(p.pos.x), "y": mm(p.pos.y), "curve": p.curve}))
        .collect();
    // which seams start or end on each segment
    let mut sewn: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for seam in project.sewing.values() {
        let wire = serde_json::to_value(seam).unwrap_or_default();
        for k in ["a", "b"] {
            for s in wire[k].as_array().into_iter().flatten() {
                if s["p"].as_str() != Some(piece.id.0.as_str()) {
                    continue;
                }
                for end in ["a", "b"] {
                    if let Some(seg) = s[end]["s"].as_str() {
                        let list = sewn.entry(seg.to_owned()).or_default();
                        if !list.contains(&seam.id.0) {
                            list.push(seam.id.0.clone());
                        }
                    }
                }
            }
        }
    }
    let segments: Vec<Value> = outline
        .segments
        .iter()
        .map(|s| {
            let end = &piece.pts[s.end_point % n.max(1)];
            // how far the middle of the curve sits from its chord, positive out of the piece
            let bulge = match (s.poly.first(), s.poly.last()) {
                (Some(&a), Some(&b)) if s.ctrl.len() > 2 => {
                    let mid = s.point_at(0.5).pos;
                    let dir = (b - a).normalize_or_zero();
                    let mut normal = azam_geom::Vec2::new(-dir.y, dir.x);
                    if inside(&outline.poly, (a + b) / 2.0 + normal) {
                        normal = -normal;
                    }
                    mm((mid - (a + b) / 2.0).dot(normal))
                }
                _ => 0.0,
            };
            json!({
                "id": s.id.0,
                "from": s.id.0,
                "to": end.id.0,
                "lengthMm": mm(s.len),
                "curved": s.ctrl.len() > 2,
                "bulgeMm": bulge,
                "seams": sewn.get(&s.id.0).cloned().unwrap_or_default(),
                "hemMm": piece.hem.as_ref().and_then(|h| h.get(&s.id)).copied(),
                "allowanceMm": piece.sa_seg.as_ref().and_then(|m| m.get(&s.id)).copied().unwrap_or(piece.sa),
            })
        })
        .collect();
    let internals: Vec<Value> = piece
        .internals
        .iter()
        .enumerate()
        .map(|(i, l)| {
            json!({
                "index": i,
                "id": l.id.as_ref().map(|id| id.0.clone()),
                "points": l.pts.iter().map(|p| json!([mm(p.pos.x), mm(p.pos.y), p.curve])).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "id": piece.id.0,
        "name": piece.name,
        "fabric": piece.fabric.0,
        "bbox": bbox(piece),
        "areaCm2": mm(azam_doc::piece_area(project, piece, None, false) / 100.0),
        "perimeterMm": mm(outline.total),
        "seamAllowanceMm": piece.sa,
        "grainDeg": piece.grain,
        "layer": piece.layer,
        "state": piece.state,
        "arrangement": piece.arr,
        "mirror": piece.mirror.as_ref().map(|m| json!({"of": m.src.0, "axisX": m.axis})),
        "points": points,
        "segments": segments,
        "internalLines": internals,
        "notches": piece.notches.len(),
        "pins": piece.pins.len(),
    })
}
