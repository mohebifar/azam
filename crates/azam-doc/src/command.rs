//! Commands: every change the UI makes to a project, applied atomically.

use std::fmt;

use azam_geom::{PointId, SizeName, Vec2};
use serde::{Deserialize, Serialize};

use crate::edit::{
    DeletePointsError, DuplicateMode, FlipAxis, PieceTransform, add_curve_point, add_internal_line,
    create_piece, delete_pieces, delete_points, duplicate_piece, flip_piece, remap_refs, rotate_piece_90,
    set_curvature, split_segment, transform_piece,
};
use crate::hems::{mark_hems, set_hem};
use crate::ids::{ColorwayId, PartId, PieceId, SeamId};
use crate::mirror::sync_mirrors;
use crate::piece::{LinePoint, Piece};
use crate::project::{Project, SeamSide, Units};
use crate::seams::{set_all_sa, set_seam_sa, set_segment_sa};

/// One undoable change to a project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Adds a piece drawn as a polygon: `(position, curve point)` per point.
    CreatePiece {
        /// Outline points.
        points: Vec<(Vec2, bool)>,
    },
    /// Adds an internal line to a piece.
    AddInternalLine {
        /// The piece.
        piece: PieceId,
        /// Points of the line.
        points: Vec<LinePoint>,
        /// Whether the line closes on itself.
        closed: bool,
    },
    /// Inserts a corner at a fraction along a segment, keeping the curve.
    SplitSegment {
        /// The piece.
        piece: PieceId,
        /// The segment (id of its start corner).
        segment: PointId,
        /// Fraction of the segment's arc length.
        t: f64,
    },
    /// Adds a curve point to a segment.
    AddCurvePoint {
        /// The piece.
        piece: PieceId,
        /// The segment.
        segment: PointId,
        /// Fraction of the segment's arc length where the point goes among the controls.
        t: f64,
        /// Position of the new point, in mm.
        at: Vec2,
    },
    /// Reshapes a segment as a curve through a point.
    SetCurvature {
        /// The piece.
        piece: PieceId,
        /// The segment.
        segment: PointId,
        /// Point the curve passes through at its middle, in mm.
        through: Vec2,
    },
    /// Moves points of a piece.
    MovePoints {
        /// The piece.
        piece: PieceId,
        /// The points.
        points: Vec<PointId>,
        /// Offset, in mm.
        delta: Vec2,
    },
    /// Deletes points of a piece (at least three corners must remain).
    DeletePoints {
        /// The piece.
        piece: PieceId,
        /// The points.
        points: Vec<PointId>,
    },
    /// Moves, rotates or otherwise transforms whole pieces.
    TransformPieces {
        /// The pieces.
        pieces: Vec<PieceId>,
        /// The transform.
        transform: PieceTransform,
    },
    /// Flips pieces in place; linked mirrors are skipped.
    FlipPieces {
        /// The pieces.
        pieces: Vec<PieceId>,
        /// Flip axis.
        axis: FlipAxis,
    },
    /// Turns pieces a quarter turn in place; linked mirrors are skipped.
    RotatePieces90 {
        /// The pieces.
        pieces: Vec<PieceId>,
    },
    /// Copies a piece.
    DuplicatePiece {
        /// The piece.
        piece: PieceId,
    },
    /// Mirrors a piece (not itself a linked mirror).
    MirrorPiece {
        /// The piece.
        piece: PieceId,
        /// Whether the mirror follows later edits of the source.
        linked: bool,
    },
    /// Deletes pieces and everything that refers to them.
    DeletePieces {
        /// The pieces.
        pieces: Vec<PieceId>,
    },
    /// Sews sides `a` to sides `b`.
    AddSeam {
        /// First group of sides.
        a: Vec<SeamSide>,
        /// Second group of sides.
        b: Vec<SeamSide>,
    },
    /// Deletes a seam.
    DeleteSeam {
        /// The seam.
        seam: SeamId,
    },
    /// Sets the allowance of every segment of a seam, in mm.
    SetSeamAllowance {
        /// The seam.
        seam: SeamId,
        /// Allowance in mm.
        mm: f64,
    },
    /// Sets the allowance of one segment (of every seam on it, if sewn), in mm.
    SetSegmentAllowance {
        /// The piece.
        piece: PieceId,
        /// The segment.
        segment: PointId,
        /// Allowance in mm.
        mm: f64,
    },
    /// Sets the allowance of every piece and clears per-segment overrides, in mm.
    SetAllAllowances {
        /// Allowance in mm.
        mm: f64,
    },
    /// Sets (or, when not positive, removes) the hem of a segment, in mm.
    SetHem {
        /// The piece.
        piece: PieceId,
        /// The segment.
        segment: PointId,
        /// Hem allowance in mm.
        mm: f64,
    },
    /// Adds the suggested hems to free edges that have none.
    MarkHems,
    /// Renames the project.
    Rename {
        /// New name (trimmed; empty names are refused).
        name: String,
    },
    /// Sets the display units.
    SetUnits {
        /// Units.
        units: Units,
    },
    /// Shows another size.
    SetActiveSize {
        /// The size.
        size: SizeName,
    },
    /// Switches colorway.
    SetActiveColorway {
        /// The colorway.
        colorway: ColorwayId,
    },
    /// Several commands applied as one undo step.
    Batch(Vec<Command>),
}

impl Command {
    /// Label of the command for the undo menu.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::CreatePiece { .. } => "Create pattern",
            Self::AddInternalLine { .. } => "Internal line",
            Self::SplitSegment { .. } => "Split",
            Self::AddCurvePoint { .. } => "Add point",
            Self::SetCurvature { .. } => "Curvature",
            Self::MovePoints { .. } => "Nudge points",
            Self::DeletePoints { .. } => "Delete point",
            Self::TransformPieces { .. } => "Move",
            Self::FlipPieces {
                axis: FlipAxis::Horizontal,
                ..
            } => "Flip horizontally",
            Self::FlipPieces { .. } => "Flip vertically",
            Self::RotatePieces90 { .. } => "Rotate 90°",
            Self::DuplicatePiece { .. } => "Duplicate",
            Self::MirrorPiece { .. } => "Symmetric pattern",
            Self::DeletePieces { .. } => "Delete pattern",
            Self::AddSeam { .. } => "Sew",
            Self::DeleteSeam { .. } => "Delete sewing",
            Self::SetSeamAllowance { .. } | Self::SetAllAllowances { .. } => "Seam allowance",
            Self::SetSegmentAllowance { .. } => "Segment seam allowance",
            Self::SetHem { mm, .. } if *mm > 0.0 => "Hem",
            Self::SetHem { .. } => "Remove hem",
            Self::MarkHems => "Mark hems",
            Self::Rename { .. } => "Rename",
            Self::SetUnits { .. } => "Units",
            Self::SetActiveSize { .. } => "Size",
            Self::SetActiveColorway { .. } => "Switch colorway",
            Self::Batch(_) => "Edit",
        }
    }
}

/// Something a command created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Created {
    /// A piece.
    Piece(PieceId),
    /// An outline point.
    Point(PointId),
    /// A seam.
    Seam(SeamId),
    /// An internal line.
    InternalLine(PartId),
}

/// Result of applying a command.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    /// What the command created, in order.
    pub created: Vec<Created>,
    /// Whether the project changed.
    pub changed: bool,
    /// The project as it was before the command, for undo.
    pub before: Project,
}

/// Why a command was refused. A refused command leaves the project unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum EditError {
    /// No piece with this id.
    NoSuchPiece(PieceId),
    /// The piece has no segment with this id.
    NoSuchSegment(PieceId, PointId),
    /// No seam with this id.
    NoSuchSeam(SeamId),
    /// No colorway with this id.
    NoSuchColorway(ColorwayId),
    /// The size is not in the size group.
    NoSuchSize(SizeName),
    /// The piece is a linked mirror; edit its source instead.
    LinkedMirror(PieceId),
    /// The edit would leave fewer than three corners.
    TooFewCorners(PieceId),
    /// Too few points for a piece or line.
    TooFewPoints,
    /// A seam needs at least one side in each group.
    EmptySeam,
    /// The name is empty.
    EmptyName,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchPiece(p) => write!(f, "no pattern piece `{p}`"),
            Self::NoSuchSegment(p, s) => write!(f, "pattern piece `{p}` has no segment `{}`", s.0),
            Self::NoSuchSeam(s) => write!(f, "no seam `{s}`"),
            Self::NoSuchColorway(c) => write!(f, "no colorway `{c}`"),
            Self::NoSuchSize(s) => write!(f, "size `{}` is not in the size group", s.0),
            Self::LinkedMirror(p) => write!(f, "pattern piece `{p}` is a linked mirror; edit its source"),
            Self::TooFewCorners(p) => write!(f, "pattern piece `{p}` needs at least 3 corner points"),
            Self::TooFewPoints => f.write_str("not enough points"),
            Self::EmptySeam => f.write_str("a seam needs a side on each end"),
            Self::EmptyName => f.write_str("the name is empty"),
        }
    }
}

impl std::error::Error for EditError {}

/// Applies `command` to `project`. Linked mirrors are re-synced afterwards.
///
/// # Errors
/// Returns an [`EditError`] and leaves the project as it was when the command refers to something
/// missing or would make the pattern invalid.
pub fn apply(project: &mut Project, command: Command) -> Result<Applied, EditError> {
    let before = project.clone();
    let lengths = crate::link_lengths(project);
    let symmetric = crate::symmetry_snapshot(project);
    let mut created = Vec::new();
    match run(project, command, &mut created) {
        Ok(()) => {
            crate::sync_symmetry(project, &symmetric);
            crate::sync_length_links(project, &lengths);
            sync_mirrors(project);
            let changed = !same_content(project, &before);
            Ok(Applied {
                created,
                changed,
                before,
            })
        }
        Err(e) => {
            *project = before;
            Err(e)
        }
    }
}

/// Whether two projects hold the same document (the id generator state is ignored).
fn same_content(a: &mut Project, b: &Project) -> bool {
    let ids = std::mem::replace(&mut a.ids, b.ids.clone());
    let same = a == b;
    a.ids = ids;
    same
}

fn piece<'a>(project: &'a Project, id: &PieceId) -> Result<&'a Piece, EditError> {
    project
        .pieces
        .get(id)
        .ok_or_else(|| EditError::NoSuchPiece(id.clone()))
}

/// The piece, refusing linked mirrors (their outline follows the source).
fn editable_piece<'a>(project: &'a Project, id: &PieceId) -> Result<&'a Piece, EditError> {
    let pc = piece(project, id)?;
    if pc.mirror.is_some() {
        return Err(EditError::LinkedMirror(id.clone()));
    }
    Ok(pc)
}

fn require_segment(pc: &Piece, segment: &PointId) -> Result<(), EditError> {
    if pc.outline().segment_index(segment).is_some() {
        Ok(())
    } else {
        Err(EditError::NoSuchSegment(pc.id.clone(), segment.clone()))
    }
}

/// Runs an outline edit on piece `id` and re-anchors what referred to its edges.
fn edit_outline<T>(
    project: &mut Project,
    id: &PieceId,
    segment: &PointId,
    f: impl FnOnce(&mut Piece, &mut crate::ids::IdGen) -> T,
) -> Result<T, EditError> {
    require_segment(editable_piece(project, id)?, segment)?;
    project.ensure_ids_seeded();
    let Project { pieces, ids, .. } = &mut *project;
    let pc = pieces
        .get_mut(id)
        .ok_or_else(|| EditError::NoSuchPiece(id.clone()))?;
    let old = pc.pts.clone();
    let out = f(pc, ids);
    remap_refs(project, id, &old);
    Ok(out)
}

#[allow(clippy::too_many_lines, reason = "one arm per command")]
fn run(project: &mut Project, command: Command, created: &mut Vec<Created>) -> Result<(), EditError> {
    match command {
        Command::CreatePiece { points } => {
            let id = create_piece(project, &points).ok_or(EditError::TooFewPoints)?;
            created.push(Created::Piece(id));
        }
        Command::AddInternalLine {
            piece: id,
            points,
            closed,
        } => {
            piece(project, &id)?;
            let line = add_internal_line(project, &id, &points, closed).ok_or(EditError::TooFewPoints)?;
            created.push(Created::InternalLine(line));
        }
        Command::SplitSegment {
            piece: id,
            segment,
            t,
        } => {
            let point = edit_outline(project, &id, &segment, |pc, ids| {
                split_segment(pc, &segment, t, ids)
            })?;
            created.extend(point.map(Created::Point));
        }
        Command::AddCurvePoint {
            piece: id,
            segment,
            t,
            at,
        } => {
            let point = edit_outline(project, &id, &segment, |pc, ids| {
                add_curve_point(pc, &segment, t, at, ids)
            })?;
            created.extend(point.map(Created::Point));
        }
        Command::SetCurvature {
            piece: id,
            segment,
            through,
        } => {
            require_segment(editable_piece(project, &id)?, &segment)?;
            project.ensure_ids_seeded();
            let Project { pieces, ids, .. } = &mut *project;
            if let Some(pc) = pieces.get_mut(&id) {
                set_curvature(pc, &segment, through, ids);
            }
        }
        Command::MovePoints {
            piece: id,
            points,
            delta,
        } => {
            editable_piece(project, &id)?;
            if let Some(pc) = project.pieces.get_mut(&id) {
                for p in pc.pts.iter_mut().filter(|p| points.contains(&p.id)) {
                    p.pos = Vec2::new(p.pos.x + delta.x, p.pos.y + delta.y);
                }
            }
        }
        Command::DeletePoints { piece: id, points } => {
            editable_piece(project, &id)?;
            delete_points(project, &id, &points).map_err(|e| match e {
                DeletePointsError::NoSuchPiece => EditError::NoSuchPiece(id.clone()),
                DeletePointsError::TooFewCorners => EditError::TooFewCorners(id.clone()),
            })?;
        }
        Command::TransformPieces { pieces, transform } => {
            for id in &pieces {
                piece(project, id)?;
            }
            for id in &pieces {
                transform_piece(project, id, |p| transform.apply(p));
            }
        }
        Command::FlipPieces { pieces, axis } => {
            for id in &pieces {
                if piece(project, id)?.mirror.is_none() {
                    flip_piece(project, id, axis);
                }
            }
        }
        Command::RotatePieces90 { pieces } => {
            for id in &pieces {
                if piece(project, id)?.mirror.is_none() {
                    rotate_piece_90(project, id);
                }
            }
        }
        Command::DuplicatePiece { piece: id } => {
            piece(project, &id)?;
            let new = duplicate_piece(project, &id, DuplicateMode::Copy).ok_or(EditError::TooFewPoints)?;
            created.push(Created::Piece(new));
        }
        Command::MirrorPiece { piece: id, linked } => {
            editable_piece(project, &id)?;
            let new = duplicate_piece(project, &id, DuplicateMode::Mirror { linked })
                .ok_or(EditError::TooFewPoints)?;
            created.push(Created::Piece(new));
        }
        Command::DeletePieces { pieces } => {
            for id in &pieces {
                piece(project, id)?;
            }
            delete_pieces(project, &pieces);
        }
        Command::AddSeam { a, b } => {
            if a.is_empty() || b.is_empty() {
                return Err(EditError::EmptySeam);
            }
            for sd in a.iter().chain(&b) {
                piece(project, &sd.piece)?;
            }
            created.push(Created::Seam(project.add_seam(a, b)));
        }
        Command::DeleteSeam { seam } => {
            project
                .sewing
                .shift_remove(&seam)
                .ok_or(EditError::NoSuchSeam(seam))?;
        }
        Command::SetSeamAllowance { seam, mm } => {
            if !project.sewing.contains_key(&seam) {
                return Err(EditError::NoSuchSeam(seam));
            }
            set_seam_sa(project, &seam, mm);
        }
        Command::SetSegmentAllowance {
            piece: id,
            segment,
            mm,
        } => {
            require_segment(piece(project, &id)?, &segment)?;
            set_segment_sa(project, &id, &segment, mm);
        }
        Command::SetAllAllowances { mm } => set_all_sa(project, mm),
        Command::SetHem {
            piece: id,
            segment,
            mm,
        } => {
            require_segment(piece(project, &id)?, &segment)?;
            set_hem(project, &id, &segment, mm);
        }
        Command::MarkHems => {
            mark_hems(project);
        }
        Command::Rename { name } => {
            let name = name.trim();
            if name.is_empty() {
                return Err(EditError::EmptyName);
            }
            name.clone_into(&mut project.meta.name);
        }
        Command::SetUnits { units } => project.units = units,
        Command::SetActiveSize { size } => {
            if !project.sizes.contains(&size) {
                return Err(EditError::NoSuchSize(size));
            }
            project.active_size = size;
        }
        Command::SetActiveColorway { colorway } => {
            if !project.colorways.contains_key(&colorway) {
                return Err(EditError::NoSuchColorway(colorway));
            }
            project.active_colorway = colorway;
        }
        Command::Batch(commands) => {
            for c in commands {
                run(project, c, created)?;
                sync_mirrors(project);
            }
        }
    }
    Ok(())
}
