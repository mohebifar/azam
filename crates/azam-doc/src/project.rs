//! The project document: every piece, seam, fabric and setting of one garment.

use azam_geom::{PointId, SewingSide, SideDirection, SideEnd, SizeName};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::avatar::{Avatar, Sex};
use crate::fabric::{Fabric, FabricPreset, make_fabric};
use crate::ids::{ColorwayId, FabricId, IdGen, ItemId, PieceId, PomId, SeamId, StitchId, prefix};
use crate::piece::{MAIN_FABRIC, Piece};

/// Name of a project that has not been named yet.
pub const UNTITLED: &str = "Untitled garment";
/// File-format version written in [`Meta::version`].
pub const FORMAT_VERSION: u32 = 1;
/// Colours given to new seams, in turn.
pub const SEW_COLORS: [&str; 12] = [
    "#E8654A", "#3B82F6", "#10B981", "#F59E0B", "#8B5CF6", "#EC4899", "#14B8A6", "#EF4444", "#6366F1",
    "#84CC16", "#06B6D4", "#F97316",
];
/// Id of a new project's colorway.
pub const MAIN_COLORWAY: &str = "cw_main";
/// Name of a new project's colorway.
pub const MAIN_COLORWAY_NAME: &str = "Colorway 1";
/// Sizes of a new project.
pub const DEFAULT_SIZES: [&str; 5] = ["XS", "S", "M", "L", "XL"];
/// Base size of a new project.
pub const DEFAULT_BASE_SIZE: &str = "M";
/// Name and colour of a new project's main fabric.
const MAIN_FABRIC_NAME: &str = "Cotton Jersey";
const MAIN_FABRIC_COLOR: &str = "#efeae1";
/// Simulation quality of a new project.
const DEFAULT_SIM_QUALITY: &str = "normal";

/// Project metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    /// Garment name.
    pub name: String,
    /// Creation time, in ms since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<f64>,
    /// Last edit time, in ms since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<f64>,
    /// File-format version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Units lengths are shown in. Stored values are always mm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    /// Millimetres.
    Mm,
    /// Centimetres.
    #[default]
    Cm,
    /// Inches.
    In,
}

/// One side of a seam: a stretch of a piece outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "SeamSideWire", into = "SeamSideWire")]
pub struct SeamSide {
    /// The piece.
    pub piece: PieceId,
    /// The stretch of its outline.
    pub side: SewingSide,
    /// Fields this version does not interpret, kept as stored.
    pub extra: Map<String, Value>,
}

impl SeamSide {
    /// The stretch of `piece` from fraction `t0` to `t1` of the segment that starts at `segment`,
    /// walked backwards when `reverse` is set.
    #[must_use]
    pub fn new(piece: &PieceId, segment: &str, reverse: bool, t0: f64, t1: f64) -> Self {
        let (a, b, direction) = if reverse {
            (t1, t0, SideDirection::Backward)
        } else {
            (t0, t1, SideDirection::Forward)
        };
        let end = |t| SideEnd {
            segment: PointId::new(segment),
            t,
        };
        Self {
            piece: piece.clone(),
            side: SewingSide {
                a: end(a),
                b: end(b),
                direction,
            },
            extra: Map::new(),
        }
    }

    /// The whole segment that starts at `segment`, walked backwards when `reverse` is set.
    #[must_use]
    pub fn segment(piece: &PieceId, segment: &str, reverse: bool) -> Self {
        Self::new(piece, segment, reverse, 0.0, 1.0)
    }
}

#[derive(Serialize, Deserialize)]
struct SideEndWire {
    s: PointId,
    #[serde(default)]
    t: f64,
}

#[derive(Serialize, Deserialize)]
struct SeamSideWire {
    p: PieceId,
    a: SideEndWire,
    b: SideEndWire,
    dir: f64,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl From<SeamSideWire> for SeamSide {
    fn from(w: SeamSideWire) -> Self {
        Self {
            piece: w.p,
            side: SewingSide {
                a: SideEnd {
                    segment: w.a.s,
                    t: w.a.t,
                },
                b: SideEnd {
                    segment: w.b.s,
                    t: w.b.t,
                },
                direction: if w.dir > 0.0 {
                    SideDirection::Forward
                } else {
                    SideDirection::Backward
                },
            },
            extra: w.extra,
        }
    }
}

impl From<SeamSide> for SeamSideWire {
    fn from(s: SeamSide) -> Self {
        let end = |e: SideEnd| SideEndWire { s: e.segment, t: e.t };
        Self {
            p: s.piece,
            dir: match s.side.direction {
                SideDirection::Forward => 1.0,
                SideDirection::Backward => -1.0,
            },
            a: end(s.side.a),
            b: end(s.side.b),
            extra: s.extra,
        }
    }
}

/// A seam: the sides in `a` are sewn to the sides in `b` (M:N when either has several).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seam {
    /// Id of the seam.
    pub id: SeamId,
    /// First group of sides, sewn end to end.
    #[serde(default)]
    pub a: Vec<SeamSide>,
    /// Second group of sides.
    #[serde(default)]
    pub b: Vec<SeamSide>,
    /// Display colour, `#rrggbb`.
    #[serde(default)]
    pub color: String,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Seam {
    /// Both groups of sides, tagged with the group they belong to.
    pub fn sides(&self) -> impl Iterator<Item = (SeamGroup, &SeamSide)> {
        self.a
            .iter()
            .map(|s| (SeamGroup::A, s))
            .chain(self.b.iter().map(|s| (SeamGroup::B, s)))
    }

    /// The sides of `group`.
    #[must_use]
    pub fn group(&self, group: SeamGroup) -> &[SeamSide] {
        match group {
            SeamGroup::A => &self.a,
            SeamGroup::B => &self.b,
        }
    }
}

/// One of the two groups of sides of a seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeamGroup {
    /// [`Seam::a`].
    A,
    /// [`Seam::b`].
    B,
}

impl SeamGroup {
    /// The other group.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// Segment id a topstitch uses to run around the whole outline.
pub const WHOLE_OUTLINE: &str = "*";

/// A topstitch along one segment of a piece (or the whole outline, [`WHOLE_OUTLINE`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stitch {
    /// Id of the stitch.
    pub id: StitchId,
    /// The piece.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<PieceId>,
    /// The segment.
    pub seg: PointId,
    /// Fields this version does not interpret (offset, length, colour…), kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One segment measured by a point of measure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PomRef {
    /// The piece.
    pub p: PieceId,
    /// The segment.
    pub s: PointId,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A point of measure: the summed length of some segments, tracked across sizes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pom {
    /// Id of the measurement.
    pub id: PomId,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Segments measured.
    #[serde(default)]
    pub refs: Vec<PomRef>,
    /// Fields this version does not interpret (tolerance…), kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A button, buttonhole or comment, optionally anchored on a piece at `(x, y)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// Id of the item.
    pub id: ItemId,
    /// The piece it sits on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<PieceId>,
    /// x in piece coordinates, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    /// y in piece coordinates, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    /// Fields this version does not interpret (size, style, text…), kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A colorway: per-fabric overrides of appearance fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Colorway {
    /// Id of the colorway.
    pub id: ColorwayId,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Position in the colorway list.
    #[serde(default)]
    pub order: u32,
    /// Fabric id → fields that replace the fabric's own.
    #[serde(default)]
    pub over: IndexMap<FabricId, Map<String, Value>>,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Colorway {
    /// A colorway without overrides.
    #[must_use]
    pub fn new(id: &str, name: &str, order: u32) -> Self {
        Self {
            id: ColorwayId::new(id),
            name: name.to_owned(),
            order,
            over: IndexMap::new(),
            extra: Map::new(),
        }
    }
}

/// Stored simulation settings. The solver interprets them; the document only keeps them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimSettings {
    /// Preset name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Quality name written by older versions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    /// Cloth self-collision switch written by older versions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_collision: Option<bool>,
    /// Per-setting overrides of the preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub over: Option<Map<String, Value>>,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Tech-pack text fields.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TechPack {
    /// Season.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
    /// Free-form notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A garment project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// Metadata.
    pub meta: Meta,
    /// Display units.
    #[serde(default)]
    pub units: Units,
    /// Avatar settings.
    pub avatar: Avatar,
    /// Pattern pieces by id.
    #[serde(default)]
    pub pieces: IndexMap<PieceId, Piece>,
    /// Seams by id.
    #[serde(default)]
    pub sewing: IndexMap<SeamId, Seam>,
    /// Fabrics by id.
    #[serde(default)]
    pub fabrics: IndexMap<FabricId, Fabric>,
    /// Buttons by id.
    #[serde(default)]
    pub buttons: IndexMap<ItemId, Item>,
    /// Buttonholes by id.
    #[serde(default)]
    pub holes: IndexMap<ItemId, Item>,
    /// Topstitches by id.
    #[serde(default)]
    pub stitches: IndexMap<StitchId, Stitch>,
    /// Points of measure by id.
    #[serde(default)]
    pub poms: IndexMap<PomId, Pom>,
    /// Comments by id.
    #[serde(default)]
    pub comments: IndexMap<ItemId, Item>,
    /// Colorways by id.
    #[serde(default)]
    pub colorways: IndexMap<ColorwayId, Colorway>,
    /// Colorway in use.
    pub active_colorway: ColorwayId,
    /// Sizes of the size group, smallest first.
    pub sizes: Vec<SizeName>,
    /// Size the pattern is drafted in.
    pub base_size: SizeName,
    /// Size shown.
    pub active_size: SizeName,
    /// Simulation settings.
    #[serde(default)]
    pub sim: SimSettings,
    /// Project-level graphics library, kept as stored.
    #[serde(default)]
    pub graphics: Map<String, Value>,
    /// Tech-pack text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub techpack: Option<TechPack>,
    /// Segments kept the same length (see [`crate::LengthLink`]).
    #[serde(rename = "lengthLinks", default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<crate::LengthLink>,
    /// Generator for new ids. Not stored; seeded from the stored ids when first needed.
    #[serde(skip)]
    pub ids: IdGen,
    /// Fields this version does not interpret, kept as stored.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Project {
    /// A new, empty project with the default avatar of `sex`, one jersey fabric and one colorway.
    ///
    /// `now_ms` is stored as the creation and edit time.
    #[must_use]
    pub fn new(name: Option<&str>, sex: Sex, now_ms: f64) -> Self {
        let mut ids = IdGen::after(0);
        let mut main = make_fabric(&mut ids, FabricPreset::Jersey);
        main.id = FabricId::new(MAIN_FABRIC);
        MAIN_FABRIC_NAME.clone_into(&mut main.name);
        MAIN_FABRIC_COLOR.clone_into(&mut main.color);
        let colorway = Colorway::new(MAIN_COLORWAY, MAIN_COLORWAY_NAME, 0);
        Self {
            meta: Meta {
                name: name.unwrap_or(UNTITLED).to_owned(),
                created_at: Some(now_ms),
                updated_at: Some(now_ms),
                version: Some(FORMAT_VERSION),
                extra: Map::new(),
            },
            units: Units::Cm,
            avatar: Avatar::preset(sex),
            pieces: IndexMap::new(),
            sewing: IndexMap::new(),
            fabrics: IndexMap::from([(main.id.clone(), main)]),
            buttons: IndexMap::new(),
            holes: IndexMap::new(),
            stitches: IndexMap::new(),
            poms: IndexMap::new(),
            comments: IndexMap::new(),
            colorways: IndexMap::from([(colorway.id.clone(), colorway)]),
            active_colorway: ColorwayId::new(MAIN_COLORWAY),
            sizes: DEFAULT_SIZES.map(SizeName::from).to_vec(),
            base_size: SizeName::from(DEFAULT_BASE_SIZE),
            active_size: SizeName::from(DEFAULT_BASE_SIZE),
            sim: SimSettings {
                quality: Some(DEFAULT_SIM_QUALITY.to_owned()),
                self_collision: Some(true),
                ..SimSettings::default()
            },
            graphics: Map::new(),
            techpack: None,
            links: Vec::new(),
            ids,
            extra: Map::new(),
        }
    }

    /// The id generator, seeded past every stored id if it was not seeded yet.
    pub fn id_gen(&mut self) -> &mut IdGen {
        self.ensure_ids_seeded();
        &mut self.ids
    }

    /// Seeds the id generator past every stored id if it was not seeded yet.
    pub fn ensure_ids_seeded(&mut self) {
        if !self.ids.is_seeded() {
            // Serializing a document of plain data cannot fail.
            let value = serde_json::to_value(&*self).unwrap_or(Value::Null);
            self.ids = IdGen::seeded_from(&value);
        }
    }

    /// A new id with the given kind prefix.
    pub fn fresh_id(&mut self, prefix: &str) -> String {
        self.id_gen().fresh(prefix)
    }

    /// Adds `piece` at the end of the pattern list and returns its id.
    pub fn add_piece(&mut self, mut piece: Piece) -> PieceId {
        piece.order = self.pieces.len() as u32;
        let id = piece.id.clone();
        self.pieces.insert(id.clone(), piece);
        id
    }

    /// Adds a seam joining sides `a` to sides `b`, coloured in turn, and returns its id.
    pub fn add_seam(&mut self, a: Vec<SeamSide>, b: Vec<SeamSide>) -> SeamId {
        let id = SeamId::new(self.fresh_id(prefix::SEAM));
        let color = SEW_COLORS[self.sewing.len() % SEW_COLORS.len()].to_owned();
        self.sewing.insert(
            id.clone(),
            Seam {
                id: id.clone(),
                a,
                b,
                color,
                extra: Map::new(),
            },
        );
        id
    }

    /// The colorway in use, if it exists.
    #[must_use]
    pub fn current_colorway(&self) -> Option<&Colorway> {
        self.colorways.get(&self.active_colorway)
    }

    /// Fabric `id` with the active colorway's overrides applied, or `None` if there is no such
    /// fabric (or the overrides do not fit a fabric).
    #[must_use]
    pub fn effective_fabric(&self, id: &FabricId) -> Option<Fabric> {
        let fabric = self.fabrics.get(id)?;
        let Some(over) = self.current_colorway().and_then(|cw| cw.over.get(id)) else {
            return Some(fabric.clone());
        };
        let Ok(Value::Object(mut merged)) = serde_json::to_value(fabric) else {
            return None;
        };
        for (k, v) in over {
            merged.insert(k.clone(), v.clone());
        }
        serde_json::from_value(Value::Object(merged)).ok()
    }
}
