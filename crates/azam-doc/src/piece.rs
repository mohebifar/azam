//! Pattern pieces and the parts placed on them.

use std::collections::BTreeMap;

use azam_geom::{EdgeAllowances, GradeRules, Outline, OutlinePoint, PointId, SizeName, Vec2, graded_points};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ids::{FabricId, PartId, PieceId};
use crate::wire::{CurveFlag, lenient_mm_map, outline_points};

/// Seam allowance of a new piece, in mm.
pub const DEFAULT_SEAM_ALLOWANCE_MM: f64 = 10.0;
/// Simulation particle distance of a new piece, in mm.
pub const DEFAULT_PARTICLE_DISTANCE_MM: f64 = 20.0;
/// Fabric of a new project's pieces.
pub const MAIN_FABRIC: &str = "fab_main";
/// Arrangement slot of a new piece.
pub const DEFAULT_SLOT: &str = "front";
/// Distance of a new piece's arrangement from the body, in mm.
pub const DEFAULT_ARRANGEMENT_OFFSET_MM: f64 = 60.0;

/// Whether a piece takes part in simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PieceState {
    /// Simulated normally.
    #[default]
    Active,
    /// Shown in 3D but held still.
    Frozen,
    /// Left out of the simulation.
    Off,
}

/// Where a piece is placed around the avatar before sewing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Arrangement {
    /// Arrangement slot id, such as `"front"` or `"armL"`.
    pub slot: String,
    /// Distance from the body surface, in mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off: Option<f64>,
    /// Height of the piece centre above the floor, in mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<f64>,
    /// Position along a limb slot, in mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t: Option<f64>,
    /// Rotation around the body, in degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ang: Option<f64>,
    /// Fields this version does not interpret (flat, shift…), kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Arrangement {
    fn default() -> Self {
        Self::slot(DEFAULT_SLOT, DEFAULT_ARRANGEMENT_OFFSET_MM)
    }
}

impl Arrangement {
    /// An arrangement in `slot` at distance `off` mm.
    #[must_use]
    pub fn slot(slot: &str, off: f64) -> Self {
        Self {
            slot: slot.to_owned(),
            off: Some(off),
            h: None,
            t: None,
            ang: None,
            extra: Map::new(),
        }
    }

    /// Sets the height of the piece centre.
    #[must_use]
    pub fn at_height(mut self, h: f64) -> Self {
        self.h = Some(h);
        self
    }

    /// Sets the position along a limb slot.
    #[must_use]
    pub fn along(mut self, t: f64) -> Self {
        self.t = Some(t);
        self
    }

    /// Sets the rotation around the body.
    #[must_use]
    pub fn angle(mut self, ang: f64) -> Self {
        self.ang = Some(ang);
        self
    }

    /// Sets a field this version stores without interpreting.
    #[must_use]
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.extra.insert(key.to_owned(), value.into());
        self
    }
}

/// A piece that mirrors another piece across a vertical axis and follows its edits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MirrorLink {
    /// The source piece.
    pub src: PieceId,
    /// x of the mirror axis, in mm.
    pub axis: f64,
}

/// A point of an internal line. Internal-line points usually have no id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "LinePointWire", into = "LinePointWire")]
pub struct LinePoint {
    /// Optional point id.
    pub id: Option<PointId>,
    /// Position in mm.
    pub pos: Vec2,
    /// `true` for a Bézier control point.
    pub curve: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct LinePointWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<PointId>,
    x: f64,
    y: f64,
    #[serde(default, skip_serializing_if = "CurveFlag::is_corner")]
    c: CurveFlag,
}

impl From<LinePointWire> for LinePoint {
    fn from(w: LinePointWire) -> Self {
        Self {
            id: w.id,
            pos: Vec2::new(w.x, w.y),
            curve: w.c.0,
        }
    }
}

impl From<LinePoint> for LinePointWire {
    fn from(p: LinePoint) -> Self {
        Self {
            id: p.id,
            x: p.pos.x,
            y: p.pos.y,
            c: CurveFlag(p.curve),
        }
    }
}

/// An open line drawn inside a piece (fold line, dart leg, internal shape…).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InternalLine {
    /// Id of the line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<PartId>,
    /// Points of the line; curve points are Bézier controls between corners.
    #[serde(default)]
    pub pts: Vec<LinePoint>,
    /// Fields this version does not interpret (closed, fold…), kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A notch on the outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Notch {
    /// Id of the notch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<PartId>,
    /// Segment the notch is on (id of its start corner).
    pub s: PointId,
    /// Fraction along the segment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t: Option<f64>,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A simulation pin, in piece coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    /// x in mm.
    pub x: f64,
    /// y in mm.
    pub y: f64,
    /// Where the pin holds the cloth in 3D, in mm (avatar space), when it was set on the draped
    /// garment; without it the cloth is held wherever the arrangement puts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 3]>,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An image placed on a piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graphic {
    /// Id of the graphic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<PartId>,
    /// x of the graphic centre, in mm.
    pub x: f64,
    /// y of the graphic centre, in mm.
    pub y: f64,
    /// Fields this version does not interpret (path, size, rotation…), kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_sa() -> f64 {
    DEFAULT_SEAM_ALLOWANCE_MM
}

fn default_pd() -> f64 {
    DEFAULT_PARTICLE_DISTANCE_MM
}

fn default_fabric() -> FabricId {
    FabricId::new(MAIN_FABRIC)
}

/// A pattern piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    /// Id of the piece (equal to its key in [`crate::Project::pieces`]).
    pub id: PieceId,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Closed outline (the sewing line).
    #[serde(with = "outline_points")]
    pub pts: Vec<OutlinePoint>,
    /// Internal lines.
    #[serde(default)]
    pub internals: Vec<InternalLine>,
    /// Notches.
    #[serde(default)]
    pub notches: Vec<Notch>,
    /// Simulation pins.
    #[serde(default)]
    pub pins: Vec<Pin>,
    /// Placed graphics.
    #[serde(default)]
    pub graphics: Vec<Graphic>,
    /// Seam allowance of edges without an override, in mm.
    #[serde(default = "default_sa")]
    pub sa: f64,
    /// Seam allowance overrides by segment id, in mm.
    #[serde(
        rename = "saSeg",
        default,
        with = "lenient_mm_map",
        skip_serializing_if = "Option::is_none"
    )]
    pub sa_seg: Option<BTreeMap<PointId, f64>>,
    /// Hem allowances by segment id, in mm.
    #[serde(default, with = "lenient_mm_map", skip_serializing_if = "Option::is_none")]
    pub hem: Option<BTreeMap<PointId, f64>>,
    /// Grain direction in degrees.
    #[serde(default)]
    pub grain: f64,
    /// Fabric of the piece.
    #[serde(default = "default_fabric")]
    pub fabric: FabricId,
    /// Simulation particle distance in mm.
    #[serde(default = "default_pd")]
    pub pd: f64,
    /// Layer for cloth-on-cloth stacking (higher is outside).
    #[serde(default)]
    pub layer: i32,
    /// Simulation state.
    #[serde(default)]
    pub state: PieceState,
    /// Arrangement around the avatar.
    #[serde(default)]
    pub arr: Arrangement,
    /// Position in the pattern list.
    #[serde(default)]
    pub order: u32,
    /// Set when this piece is a linked mirror of another piece.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<MirrorLink>,
    /// Grade rules: point id → size → offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<GradeRules>,
    /// Whether the piece is interfaced (stiffer in simulation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strengthen: Option<bool>,
    /// Elastic settings by segment id, kept as stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elastic: Option<BTreeMap<PointId, Value>>,
    /// Fields this version does not interpret (qty, shrinkage, hide3d…), kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Piece {
    /// A piece with the defaults of a newly drawn pattern.
    #[must_use]
    pub fn new(id: PieceId, name: impl Into<String>, pts: Vec<OutlinePoint>) -> Self {
        Self {
            id,
            name: name.into(),
            pts,
            internals: Vec::new(),
            notches: Vec::new(),
            pins: Vec::new(),
            graphics: Vec::new(),
            sa: DEFAULT_SEAM_ALLOWANCE_MM,
            sa_seg: None,
            hem: None,
            grain: 0.0,
            fabric: default_fabric(),
            pd: DEFAULT_PARTICLE_DISTANCE_MM,
            layer: 0,
            state: PieceState::Active,
            arr: Arrangement::default(),
            order: 0,
            mirror: None,
            grade: None,
            strengthen: None,
            elastic: None,
            extra: Map::new(),
        }
    }

    /// The outline of the piece in its base size.
    #[must_use]
    pub fn outline(&self) -> Outline {
        Outline::new(&self.pts)
    }

    /// The outline of the piece in `size`, given the project's base size.
    #[must_use]
    pub fn graded_outline(&self, size: &SizeName, base: &SizeName) -> Outline {
        match &self.grade {
            Some(rules) => Outline::new(&graded_points(&self.pts, rules, size, base)),
            None => self.outline(),
        }
    }

    /// Allowance settings for building the cut line.
    #[must_use]
    pub fn allowances(&self) -> EdgeAllowances {
        EdgeAllowances {
            piece: self.sa,
            per_segment: self.sa_seg.clone().unwrap_or_default(),
            hems: self.hem.clone().unwrap_or_default(),
        }
    }

    /// Index of the point with id `id`.
    #[must_use]
    pub fn point_index(&self, id: &PointId) -> Option<usize> {
        self.pts.iter().position(|p| &p.id == id)
    }
}

/// `pts` moved by `(dx, dy)`.
#[must_use]
pub fn translate_points(pts: &[OutlinePoint], delta: Vec2) -> Vec<OutlinePoint> {
    pts.iter()
        .map(|p| OutlinePoint {
            pos: p.pos + delta,
            ..p.clone()
        })
        .collect()
}

/// `pts` mirrored across the vertical line `x = cx`, in reverse order so the winding is kept, with
/// every id prefixed by `prefix`.
#[must_use]
pub fn mirror_points(pts: &[OutlinePoint], cx: f64, prefix: &str) -> Vec<OutlinePoint> {
    pts.iter()
        .rev()
        .map(|p| OutlinePoint {
            id: PointId::new(format!("{prefix}{}", p.id.0)),
            pos: Vec2::new(2.0 * cx - p.pos.x, p.pos.y),
            curve: p.curve,
        })
        .collect()
}
