//! Which pieces are simulated, in what order, and their meshes.

use azam_doc::{Piece, PieceState, Project};
use azam_geom::{OutlinePoint, PointId, graded_points};
use azam_mesh::{InternalLine, MeshOptions, PieceMesh, mesh_piece};
use serde_json::Value;

use crate::numeric::{or_else, round_half_up};

/// Particle distance used when a piece has none, in mm.
const DEFAULT_PIECE_SPACING_MM: f64 = 20.0;
/// Smallest particle distance after scaling, in mm.
const MIN_PIECE_SPACING_MM: f64 = 3.0;
/// Outline coordinates are compared at this resolution (per mm) when deciding mesh reuse.
const OUTLINE_KEY_SCALE: f64 = 100.0;
/// A piece needs this many outline points to be simulated.
const MIN_OUTLINE_POINTS: usize = 3;
/// A mesh needs at least one triangle (this many indices) to be simulated.
const MIN_TRIANGLE_INDICES: usize = 3;

/// Particle distance of `piece` scaled by `pd_scale` (0 means 1), in mm.
#[must_use]
pub fn piece_spacing(piece: &Piece, pd_scale: f64) -> f64 {
    (or_else(piece.pd, DEFAULT_PIECE_SPACING_MM) * or_else(pd_scale, 1.0)).max(MIN_PIECE_SPACING_MM)
}

/// Rough particle count of a project at mesh density `pd_scale` (for the settings panel).
#[must_use]
pub fn estimate_particles(project: &Project, pd_scale: f64) -> u64 {
    let mut n = 0.0;
    for piece in sorted_by_id(project) {
        if piece.pts.len() < MIN_OUTLINE_POINTS || piece.state == PieceState::Off {
            continue;
        }
        let outline = piece.outline();
        let area = azam_geom::signed_area(&outline.poly).abs();
        let d = piece_spacing(piece, pd_scale);
        n += area / (d * d * 0.866) + outline.total / d;
    }
    round_half_up(n) as u64
}

/// The project's pieces in id order.
fn sorted_by_id(project: &Project) -> Vec<&Piece> {
    let mut pieces: Vec<&Piece> = project.pieces.values().collect();
    pieces.sort_by(|a, b| a.id.cmp(&b.id));
    pieces
}

/// The pieces to simulate (at least three outline points), in pattern order (then id).
pub(crate) fn simulated_pieces(project: &Project) -> Vec<&Piece> {
    let mut pieces: Vec<&Piece> = sorted_by_id(project)
        .into_iter()
        .filter(|p| p.pts.len() >= MIN_OUTLINE_POINTS)
        .collect();
    pieces.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    pieces
}

/// The piece's outline points in the project's active size.
pub(crate) fn graded(project: &Project, piece: &Piece) -> Vec<OutlinePoint> {
    match &piece.grade {
        Some(rules) if !project.active_size.0.is_empty() => {
            graded_points(&piece.pts, rules, &project.active_size, &project.base_size).into_owned()
        }
        _ => piece.pts.clone(),
    }
}

/// Fold angle of an internal line as stored (`None` for a line that does not fold).
pub(crate) fn fold_angle(extra: &serde_json::Map<String, Value>) -> Option<f64> {
    match extra.get("fold")? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if !s.trim().is_empty() => s.trim().parse().ok(),
        _ => None,
    }
}

/// An internal line converted for meshing.
struct LineInput {
    id: String,
    points: Vec<OutlinePoint>,
    fold: Option<f64>,
}

fn internal_lines(piece: &Piece) -> Vec<LineInput> {
    piece
        .internals
        .iter()
        .map(|il| LineInput {
            id: il.id.as_ref().map(|i| i.as_str().to_owned()).unwrap_or_default(),
            points: il
                .pts
                .iter()
                .map(|p| OutlinePoint {
                    id: p.id.clone().unwrap_or_else(|| PointId::new("")),
                    pos: p.pos,
                    curve: p.curve,
                })
                .collect(),
            fold: fold_angle(&il.extra),
        })
        .collect()
}

/// Meshes a piece; `None` if the mesh has no triangle.
pub(crate) fn mesh(piece: &Piece, graded: &[OutlinePoint], pd_scale: f64) -> Option<PieceMesh> {
    let lines = internal_lines(piece);
    let refs: Vec<InternalLine<'_>> = lines
        .iter()
        .map(|l| InternalLine {
            id: &l.id,
            points: &l.points,
            fold: l.fold,
        })
        .collect();
    let options = MeshOptions {
        spacing_mm: piece_spacing(piece, pd_scale),
        grain_deg: piece.grain,
    };
    let mesh = mesh_piece(graded, &refs, options);
    (mesh.tris.len() >= MIN_TRIANGLE_INDICES).then_some(mesh)
}

/// Outline point in a [`PieceKey`]: rounded position, curve flag and id.
type OutlineKey = (i64, i64, bool, PointId);
/// Internal line in a [`PieceKey`]: id, stored fold value and rounded points.
type LineKey = (Option<String>, Value, Vec<(i64, i64, bool)>);

/// Everything that decides a piece's mesh. Two pieces with equal keys mesh identically, so a
/// rebuild can reuse the previous mesh and drape.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PieceKey {
    outline: Vec<OutlineKey>,
    internals: Vec<LineKey>,
    spacing: f64,
    grain: f64,
}

impl PieceKey {
    pub(crate) fn new(piece: &Piece, graded: &[OutlinePoint], pd_scale: f64) -> Self {
        let r = |x: f64| round_half_up(x) as i64;
        Self {
            outline: graded
                .iter()
                .map(|p| {
                    (
                        r(p.pos.x * OUTLINE_KEY_SCALE),
                        r(p.pos.y * OUTLINE_KEY_SCALE),
                        p.curve,
                        p.id.clone(),
                    )
                })
                .collect(),
            internals: piece
                .internals
                .iter()
                .map(|il| {
                    (
                        il.id.as_ref().map(|i| i.as_str().to_owned()),
                        il.extra.get("fold").cloned().unwrap_or(Value::Null),
                        il.pts.iter().map(|p| (r(p.pos.x), r(p.pos.y), p.curve)).collect(),
                    )
                })
                .collect(),
            spacing: piece_spacing(piece, pd_scale),
            grain: piece.grain,
        }
    }
}
