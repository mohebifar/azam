//! Creating, copying, mirroring and deleting pieces and their points.

use std::collections::{HashMap, HashSet};

use azam_geom::{GradeRules, OutlinePoint, PointId, Vec2, bbox};
use serde_json::Value;

use crate::ids::{FabricId, IdGen, PartId, PieceId, prefix};
use crate::mirror::{mirrored_grade, mirrored_line, mirrored_point_id};
use crate::num::negate;
use crate::piece::{InternalLine, LinePoint, MirrorLink, Notch, Piece};
use crate::project::Project;
use crate::slots::{SlotKind, mirror_slot, slot_kind};

/// Gap between a piece and its copy or mirror, in mm.
const COPY_GAP_MM: f64 = 40.0;
/// Fewest corners a piece outline may have.
pub const MIN_CORNERS: usize = 3;
/// Name suffix of a copied piece.
const COPY_SUFFIX: &str = " copy";
/// Name suffix of a mirrored piece.
const MIRROR_SUFFIX: &str = " (mirror)";
/// Name prefix of a drawn piece.
const DRAWN_NAME: &str = "Pattern";

/// Deletes pieces and everything that refers to them: their sides of seams (a seam left without
/// one of its groups is deleted), buttons, buttonholes, topstitches, comments and measurements.
/// Mirrors of deleted pieces become ordinary pieces.
pub fn delete_pieces(project: &mut Project, ids: &[PieceId]) {
    let del: HashSet<&PieceId> = ids.iter().collect();
    project.pieces.retain(|id, _| !del.contains(id));
    project.sewing.retain(|_, sw| {
        sw.a.retain(|s| !del.contains(&s.piece));
        sw.b.retain(|s| !del.contains(&s.piece));
        !sw.a.is_empty() && !sw.b.is_empty()
    });
    let keep = |p: &Option<PieceId>| p.as_ref().is_none_or(|p| !del.contains(p));
    project.buttons.retain(|_, it| keep(&it.p));
    project.holes.retain(|_, it| keep(&it.p));
    project.comments.retain(|_, it| keep(&it.p));
    project.stitches.retain(|_, st| keep(&st.p));
    project.poms.retain(|_, pm| {
        pm.refs.retain(|r| !del.contains(&r.p));
        !pm.refs.is_empty()
    });
    for pc in project.pieces.values_mut() {
        if pc.mirror.as_ref().is_some_and(|m| del.contains(&m.src)) {
            pc.mirror = None;
        }
    }
}

/// How [`duplicate_piece`] copies a piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DuplicateMode {
    /// A plain copy placed to the right.
    Copy,
    /// A mirror image placed to the right; `linked` keeps it following the source.
    Mirror {
        /// Whether the mirror follows later edits of the source.
        linked: bool,
    },
}

/// Copies or mirrors piece `piece` next to it and returns the new piece's id.
///
/// A copy gets new point ids; a mirror gets mirrored ids (`m` + source id), mirrored grain,
/// arrangement and grading, and no notches, pins or graphics.
pub fn duplicate_piece(project: &mut Project, piece: &PieceId, mode: DuplicateMode) -> Option<PieceId> {
    project.ensure_ids_seeded();
    let order = project.pieces.len() as u32;
    let src = project.pieces.get(piece)?;
    let ids = &mut project.ids;
    let mut pc = src.clone();
    pc.id = PieceId::new(ids.fresh(prefix::PIECE));
    let mirror = matches!(mode, DuplicateMode::Mirror { .. });
    pc.name = format!("{}{}", src.name, if mirror { MIRROR_SUFFIX } else { COPY_SUFFIX });
    let bb = bbox(src.pts.iter().map(|p| p.pos))?;
    let map = match mode {
        DuplicateMode::Mirror { linked } => mirror_into(&mut pc, src, linked.then_some(piece), bb.max.x, ids),
        DuplicateMode::Copy => copy_into(&mut pc, src, bb.size().x, ids),
    };
    // Notches always consume ids, even when a mirror then drops them.
    let notches: Vec<Notch> = src
        .notches
        .iter()
        .map(|n| Notch {
            id: Some(PartId::new(ids.fresh(prefix::NOTCH))),
            s: map.get(&n.s).cloned().unwrap_or_else(|| n.s.clone()),
            t: if mirror {
                Some(1.0 - n.t.unwrap_or(0.0))
            } else {
                n.t
            },
            extra: serde_json::Map::new(),
        })
        .collect();
    pc.notches = if mirror { Vec::new() } else { notches };
    pc.order = order;
    let id = pc.id.clone();
    project.pieces.insert(id.clone(), pc);
    Some(id)
}

/// Makes `pc` a mirror image of `src` across `x = right + gap`, linked to `link` if given. Returns
/// the source → mirror point id map.
fn mirror_into(
    pc: &mut Piece,
    src: &Piece,
    link: Option<&PieceId>,
    right: f64,
    ids: &mut IdGen,
) -> HashMap<PointId, PointId> {
    let mut map = HashMap::new();
    let axis = right + COPY_GAP_MM;
    pc.pts = src
        .pts
        .iter()
        .rev()
        .map(|p| {
            let id = mirrored_point_id(&p.id);
            map.insert(p.id.clone(), id.clone());
            OutlinePoint {
                id,
                pos: Vec2::new(2.0 * axis - p.pos.x + 0.0, p.pos.y),
                curve: p.curve,
            }
        })
        .collect();
    pc.internals = src
        .internals
        .iter()
        .map(|il| mirrored_line(il, axis, PartId::new(ids.fresh(prefix::INTERNAL))))
        .collect();
    pc.grain = negate(src.grain);
    pc.arr = src.arr.clone();
    if let Some(slot) = mirror_slot(&src.arr.slot) {
        slot.clone_into(&mut pc.arr.slot);
    } else if slot_kind(&src.arr.slot) == SlotKind::Body
        && let Some(ang) = pc.arr.ang.filter(|&a| a != 0.0 && !a.is_nan())
    {
        pc.arr.ang = Some(-ang);
    }
    if let Some(src_id) = link {
        pc.mirror = Some(MirrorLink {
            src: src_id.clone(),
            axis,
        });
    }
    pc.graphics.clear();
    pc.pins.clear();
    pc.grade = src.grade.as_ref().map(mirrored_grade);
    map
}

/// Makes `pc` a copy of `src` moved `width + gap` to the right, with new ids. Returns the
/// source → copy point id map.
fn copy_into(pc: &mut Piece, src: &Piece, width: f64, ids: &mut IdGen) -> HashMap<PointId, PointId> {
    let mut map = HashMap::new();
    let dx = width + COPY_GAP_MM;
    pc.pts = src
        .pts
        .iter()
        .map(|p| {
            let id = PointId::new(ids.fresh(prefix::POINT));
            map.insert(p.id.clone(), id.clone());
            OutlinePoint {
                id,
                pos: Vec2::new(p.pos.x + dx, p.pos.y),
                curve: p.curve,
            }
        })
        .collect();
    pc.internals = src
        .internals
        .iter()
        .map(|il| InternalLine {
            id: Some(PartId::new(ids.fresh(prefix::INTERNAL))),
            pts: il
                .pts
                .iter()
                .map(|p| LinePoint {
                    id: None,
                    pos: Vec2::new(p.pos.x + dx, p.pos.y),
                    curve: p.curve,
                })
                .collect(),
            extra: il.extra.clone(),
        })
        .collect();
    pc.graphics = src
        .graphics
        .iter()
        .map(|g| {
            let mut g = g.clone();
            g.id = Some(PartId::new(ids.fresh(prefix::GRAPHIC)));
            g.x += dx;
            g
        })
        .collect();
    pc.pins.clear();
    pc.grade = src.grade.as_ref().map(|rules| {
        rules
            .iter()
            .filter_map(|(k, v)| map.get(k).map(|nk| (nk.clone(), v.clone())))
            .collect::<GradeRules>()
    });
    map
}

/// Why points could not be deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeletePointsError {
    /// The piece does not exist.
    NoSuchPiece,
    /// Fewer than [`MIN_CORNERS`] corners would remain.
    TooFewCorners,
}

/// Deletes points `points` of piece `piece` and re-anchors what referred to its edges.
///
/// # Errors
/// Fails without changing anything if the piece is missing or fewer than three corners would
/// remain.
pub fn delete_points(
    project: &mut Project,
    piece: &PieceId,
    points: &[PointId],
) -> Result<(), DeletePointsError> {
    let pc = project
        .pieces
        .get_mut(piece)
        .ok_or(DeletePointsError::NoSuchPiece)?;
    let left: Vec<OutlinePoint> = pc
        .pts
        .iter()
        .filter(|p| !points.contains(&p.id))
        .cloned()
        .collect();
    if left.iter().filter(|p| !p.curve).count() < MIN_CORNERS {
        return Err(DeletePointsError::TooFewCorners);
    }
    let old = std::mem::replace(&mut pc.pts, left);
    super::remap_refs(project, piece, &old);
    Ok(())
}

/// Adds a piece drawn as a polygon (`(position, curve)` per point; the first point is always a
/// corner), named `Pattern N`, in the project's first fabric. Returns its id, or `None` for fewer
/// than three points.
pub fn create_piece(project: &mut Project, points: &[(Vec2, bool)]) -> Option<PieceId> {
    if points.len() < MIN_CORNERS {
        return None;
    }
    let ids = project.id_gen();
    let pts: Vec<OutlinePoint> = points
        .iter()
        .enumerate()
        .map(|(i, &(pos, curve))| OutlinePoint {
            id: PointId::new(ids.fresh(prefix::POINT)),
            pos,
            curve: curve && i > 0,
        })
        .collect();
    let id = PieceId::new(ids.fresh(prefix::PIECE));
    let name = format!("{DRAWN_NAME} {}", project.pieces.len() + 1);
    let mut pc = Piece::new(id, name, pts);
    if let Some(fabric) = project.fabrics.keys().next() {
        pc.fabric = FabricId::clone(fabric);
    }
    Some(project.add_piece(pc))
}

/// Adds an internal line to piece `piece` and returns its id, or `None` if the piece is missing or
/// the line has fewer than two points.
pub fn add_internal_line(
    project: &mut Project,
    piece: &PieceId,
    points: &[LinePoint],
    closed: bool,
) -> Option<PartId> {
    if points.len() < 2 || !project.pieces.contains_key(piece) {
        return None;
    }
    let id = PartId::new(project.fresh_id(prefix::INTERNAL));
    let pc = project.pieces.get_mut(piece)?;
    let mut extra = serde_json::Map::new();
    extra.insert("closed".to_owned(), Value::Bool(closed));
    pc.internals.push(InternalLine {
        id: Some(id.clone()),
        pts: points
            .iter()
            .map(|p| LinePoint {
                id: None,
                pos: p.pos,
                curve: p.curve,
            })
            .collect(),
        extra,
    });
    Some(id)
}
