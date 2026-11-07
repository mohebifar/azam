//! Garment templates drafted from the avatar's measurements.

mod aba;
mod dress;
mod labadeh;
mod skirt;
mod tee;

use azam_geom::{BBox, Outline, OutlinePoint, PointId, bbox};
use serde_json::{Map, Value, json};

use crate::avatar::Sex;
use crate::ids::{PieceId, PomId, StitchId, prefix};
use crate::piece::Piece;
use crate::project::{Pom, PomRef, Project, Stitch, UNTITLED};
use crate::torso::{BodyProfile, TorsoProfile};

pub use aba::{AbaPieces, draft_aba};
pub use dress::draft_dress;
pub use labadeh::draft_labadeh;
pub use skirt::draft_skirt;
pub use tee::draft_tee;

/// Options for drafting a template. Unset fields use the template's defaults; the avatar's own
/// parametric torso is used when no body profile is given.
#[derive(Clone, Copy, Default)]
pub struct DraftOptions<'a> {
    /// Body the garment is drafted on.
    pub profile: Option<&'a dyn BodyProfile>,
    /// Total ease added to the bust girth, in mm.
    pub ease: Option<f64>,
    /// Garment length, in mm.
    pub length: Option<f64>,
    /// Front neck depth, in mm.
    pub neck_depth: Option<f64>,
    /// Sleeve length below the cap, in mm.
    pub sleeve: Option<f64>,
    /// Back neck depth, in mm.
    pub back_neck: Option<f64>,
    /// Front neck depth of a cloak, in mm.
    pub front_neck: Option<f64>,
    /// Half width of a cloak, in mm.
    pub half_width: Option<f64>,
    /// Shoulder slope of a cloak, in degrees.
    pub slope: Option<f64>,
    /// Arm slit length, in mm.
    pub slit: Option<f64>,
    /// Arrangement distance from the body, in mm.
    pub off: Option<f64>,
    /// Fabric id for the drafted pieces.
    pub fabric: Option<&'a str>,
}

impl std::fmt::Debug for DraftOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DraftOptions")
            .field("profile", &self.profile.map(|_| "custom"))
            .field("ease", &self.ease)
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

/// A set value, treating 0 as unset like the template defaults do.
pub(crate) fn nonzero(v: Option<f64>) -> Option<f64> {
    v.filter(|&x| x != 0.0 && !x.is_nan())
}

/// The garment templates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Template {
    /// Set-in sleeve jersey tee with neck rib.
    Tee,
    /// Sleeveless A-line linen dress.
    Dress,
    /// Wool A-line skirt with waistband.
    Skirt,
    /// Clerical over-robe: stand collar, long sleeves, ankle length.
    Labadeh,
    /// Sleeveless open clerical cloak with arm slits.
    Aba,
    /// Empty board.
    Blank,
}

impl Template {
    /// Every template, in menu order.
    pub const ALL: [Self; 6] = [
        Self::Tee,
        Self::Dress,
        Self::Skirt,
        Self::Labadeh,
        Self::Aba,
        Self::Blank,
    ];

    /// Stored id of the template.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Tee => "tee",
            Self::Dress => "dress",
            Self::Skirt => "skirt",
            Self::Labadeh => "labadeh",
            Self::Aba => "aba",
            Self::Blank => "blank",
        }
    }

    /// The template with id `id`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    /// Display name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Tee => "Essential Tee",
            Self::Dress => "Shift Dress",
            Self::Skirt => "A-line Skirt",
            Self::Labadeh => "Labadeh",
            Self::Aba => "Aba",
            Self::Blank => "Blank",
        }
    }

    /// One-line description.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Tee => "Set-in sleeve jersey tee with neck rib",
            Self::Dress => "Sleeveless A-line linen dress",
            Self::Skirt => "Wool skirt with waistband",
            Self::Labadeh => "Shia clerical over-robe: stand collar, long sleeves, ankle length",
            Self::Aba => "Sleeveless open clerical cloak with arm slits",
            Self::Blank => "Empty board — draft from scratch",
        }
    }

    /// Body type the template is meant for, if any.
    #[must_use]
    pub fn sex(self) -> Option<Sex> {
        match self {
            Self::Labadeh | Self::Aba => Some(Sex::Male),
            _ => None,
        }
    }

    /// Drafts the template into `project`.
    pub fn draft(self, project: &mut Project, opts: &DraftOptions<'_>) {
        match self {
            Self::Tee => draft_tee(project, opts),
            Self::Dress => draft_dress(project, opts),
            Self::Skirt => draft_skirt(project, opts),
            Self::Labadeh => draft_labadeh(project, opts),
            Self::Aba => {
                draft_aba(project, opts);
            }
            Self::Blank => {}
        }
    }
}

/// A new project drafted from `template` on the default avatar of `sex`.
#[must_use]
pub fn from_template(
    template: Template,
    sex: Sex,
    name: Option<&str>,
    profile: Option<&dyn BodyProfile>,
    now_ms: f64,
) -> Project {
    let mut project = Project::new(name, sex, now_ms);
    template.draft(
        &mut project,
        &DraftOptions {
            profile,
            ..DraftOptions::default()
        },
    );
    project
}

/// The body profile to draft on: the given one, or the avatar's parametric torso.
pub(crate) enum Body<'a> {
    Given(&'a dyn BodyProfile),
    Own(TorsoProfile),
}

impl Body<'_> {
    pub(crate) fn get(&self) -> &dyn BodyProfile {
        match self {
            Body::Given(p) => *p,
            Body::Own(p) => p,
        }
    }
}

pub(crate) fn body<'a>(project: &Project, opts: &DraftOptions<'a>) -> Body<'a> {
    opts.profile
        .map_or_else(|| Body::Own(TorsoProfile::new(&project.avatar.body)), Body::Given)
}

/// A corner (`curve == false`) or curve point.
pub(crate) fn pt(id: &str, x: f64, y: f64, curve: bool) -> OutlinePoint {
    OutlinePoint {
        id: PointId::new(id),
        pos: azam_geom::Vec2::new(x, y),
        curve,
    }
}

/// A corner point.
pub(crate) fn corner(id: &str, x: f64, y: f64) -> OutlinePoint {
    pt(id, x, y, false)
}

/// A curve point.
pub(crate) fn curve(id: &str, x: f64, y: f64) -> OutlinePoint {
    pt(id, x, y, true)
}

/// A new piece with a generated id.
pub(crate) fn new_piece(project: &mut Project, name: &str, pts: Vec<OutlinePoint>) -> Piece {
    Piece::new(PieceId::new(project.fresh_id(prefix::PIECE)), name, pts)
}

/// Arc length of segment `id` of the outline through `pts` (0 if there is no such segment).
pub(crate) fn seg_len(pts: &[OutlinePoint], id: &str) -> f64 {
    let ol = Outline::new(pts);
    ol.segment_index(&PointId::new(id))
        .map_or(0.0, |k| ol.segments[k].len)
}

/// Bounding box of the flattened outline through `pts`.
pub(crate) fn outline_bbox(pts: &[OutlinePoint]) -> BBox {
    bbox(Outline::new(pts).poly).unwrap_or(BBox {
        min: azam_geom::Vec2::ZERO,
        max: azam_geom::Vec2::ZERO,
    })
}

/// Look of a topstitch.
pub(crate) struct StitchStyle {
    pub off: f64,
    pub len: f64,
    pub gap: f64,
    pub w: f64,
    pub color: &'static str,
    pub rows: u32,
    pub row_gap: f64,
}

/// Adds a topstitch along segment `seg` of piece `piece`.
pub(crate) fn add_stitch(project: &mut Project, piece: &PieceId, seg: &str, style: &StitchStyle) {
    let id = StitchId::new(project.fresh_id(prefix::STITCH));
    let mut extra = Map::new();
    for (k, v) in [
        ("off", json!(style.off)),
        ("len", json!(style.len)),
        ("gap", json!(style.gap)),
        ("w", json!(style.w)),
        ("color", json!(style.color)),
        ("rows", json!(style.rows)),
        ("rowGap", json!(style.row_gap)),
    ] {
        extra.insert(k.to_owned(), v);
    }
    project.stitches.insert(
        id.clone(),
        Stitch {
            id,
            p: Some(piece.clone()),
            seg: PointId::new(seg),
            extra,
        },
    );
}

/// Adds a point of measure over segments `refs` (piece, segment).
pub(crate) fn add_pom(project: &mut Project, name: &str, refs: &[(&PieceId, &str)]) {
    let id = PomId::new(project.fresh_id(prefix::POM));
    let refs = refs
        .iter()
        .map(|&(p, s)| PomRef {
            p: p.clone(),
            s: PointId::new(s),
            extra: Map::new(),
        })
        .collect();
    project.poms.insert(
        id.clone(),
        Pom {
            id,
            name: name.to_owned(),
            refs,
            extra: Map::new(),
        },
    );
}

/// Names the project `name` unless it already has a name.
pub(crate) fn name_if_untitled(project: &mut Project, name: &str) {
    if project.meta.name == UNTITLED {
        name.clone_into(&mut project.meta.name);
    }
}

/// Replaces fabric `id` with `fabric`, keeping fields of the old one the new one does not set.
pub(crate) fn assign_fabric(project: &mut Project, fabric: crate::fabric::Fabric) {
    let id = fabric.id.clone();
    match project.fabrics.get_mut(&id) {
        Some(existing) => {
            let mut extra = std::mem::take(&mut existing.extra);
            for (k, v) in fabric.extra.clone() {
                extra.insert(k, v);
            }
            *existing = crate::fabric::Fabric { extra, ..fabric };
        }
        None => {
            project.fabrics.insert(id, fabric);
        }
    }
}

/// A JSON value for an arrangement field this version stores without interpreting.
pub(crate) fn flag(v: bool) -> Value {
    Value::Bool(v)
}
