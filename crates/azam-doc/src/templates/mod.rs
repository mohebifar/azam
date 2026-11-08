//! Garment templates drafted from the avatar's measurements.

mod aba;
mod dress;
mod labadeh;
mod skirt;
mod tee;

use std::collections::BTreeMap;

use azam_geom::{BBox, Outline, OutlinePoint, PointId, bbox};
use serde_json::{Map, Value, json};

use crate::avatar::Sex;
use crate::ids::{PieceId, PomId, StitchId, prefix};
use crate::num::round_half_up;
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

/// A new project drafted from `template` on the default avatar of `sex`, with azam's
/// refinements of the reference drafts (see [`refine`]).
#[must_use]
pub fn from_template(
    template: Template,
    sex: Sex,
    name: Option<&str>,
    profile: Option<&dyn BodyProfile>,
    now_ms: f64,
) -> Project {
    let mut project = reference_template(template, sex, name, profile, now_ms);
    refine(&mut project, template);
    project
}

/// A new project drafted from `template` exactly as the reference engine drafts it, without
/// azam's refinements (for checking the drafts against recorded reference data).
#[must_use]
pub fn reference_template(
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

/// Name of the skirt template's waistband piece.
const WAISTBAND_NAME: &str = "Waistband";

/// What azam adds to a fresh reference draft of `template`:
/// - pieces with no grading get it from the size chart: the template is drafted again on each
///   size's body and every point takes its offset from the base size (centred on the piece, so
///   the pieces grow about their middles rather than drift across the board);
/// - the skirt's waistband is interfaced, so it holds the skirt at the waist;
/// - the skirt's and dress's hems are trued, square to the side seams (see [`true_hems`]);
/// - the dress front gets a side bust dart each side (see [`dress::add_bust_darts`]).
pub fn refine(project: &mut Project, template: Template) {
    if template == Template::Skirt {
        for pc in project.pieces.values_mut().filter(|pc| pc.name == WAISTBAND_NAME) {
            pc.strengthen = Some(true);
        }
    }
    if matches!(template, Template::Skirt | Template::Dress) {
        true_hems(project);
    }
    grade_from_chart(project, template);
    if template == Template::Dress {
        dress::add_bust_darts(project);
    }
}

/// Trues the curved hems of flared panels (outline points `hipL`, `hemL`, `hemC`, `hemR`, `hipR`):
/// the hem's middle control point moves to where the lines square off the side seams at the hem
/// corners meet, so the hem leaves each corner at a right angle to the side seam. Two panels sewn
/// at the side then meet in a straight hem; with the reference drafts' flatter hem each corner is
/// a few degrees short of square and the sewn side seam ends in a downward point.
fn true_hems(project: &mut Project) {
    for pc in project.pieces.values_mut() {
        let at = |id: &str| pc.pts.iter().position(|p| p.id.0 == id);
        let (Some(hip_l), Some(hem_l), Some(hem_c), Some(hem_r)) =
            (at("hipL"), at("hemL"), at("hemC"), at("hemR"))
        else {
            continue;
        };
        let (a, b, r) = (pc.pts[hip_l].pos, pc.pts[hem_l].pos, pc.pts[hem_r].pos);
        let side = b - a;
        // square to the side seam, turned towards the other corner
        let mut n = azam_geom::Vec2::new(-side.y, side.x);
        if n.x * (r.x - b.x) < 0.0 {
            n *= -1.0;
        }
        let centre_x = f64::midpoint(b.x, r.x);
        if n.x.abs() < 1e-9 {
            continue;
        }
        let s = (centre_x - b.x) / n.x;
        pc.pts[hem_c].pos = azam_geom::Vec2::new(centre_x, b.y + s * n.y);
    }
}

/// Grades every piece of `project` that has no grading by drafting `template` on the body of
/// each size in the project's size run (parametric torso, as the base it is compared with).
fn grade_from_chart(project: &mut Project, template: Template) {
    if project.pieces.values().all(|pc| pc.grade.is_some()) {
        return;
    }
    let sex = project.avatar.body.sex;
    let chart = crate::avatar::size_chart(sex);
    let base_name = if project.base_size.0.is_empty() {
        crate::project::DEFAULT_BASE_SIZE.to_owned()
    } else {
        project.base_size.0.clone()
    };
    let Some(base_spec) = chart.iter().find(|c| c.name == base_name) else {
        return;
    };
    let draft_on = |m: crate::avatar::Measurements| {
        let mut p = Project::new(None, sex, 0.0);
        p.avatar.body = m;
        template.draft(&mut p, &DraftOptions::default());
        p
    };
    let body = project.avatar.body;
    let base = draft_on(body);
    let mut rules: BTreeMap<String, azam_geom::GradeRules> = BTreeMap::new();
    for spec in chart {
        if spec.name == base_spec.name || !project.sizes.iter().any(|s| s.0 == spec.name) {
            continue;
        }
        let sized = draft_on(body.graded(base_spec, spec));
        for (b, s) in base.pieces.values().zip(sized.pieces.values()) {
            if b.name != s.name || b.pts.len() != s.pts.len() {
                continue;
            }
            let centre = |pc: &crate::piece::Piece| {
                azam_geom::bbox(pc.pts.iter().map(|p| p.pos)).map_or(azam_geom::Vec2::ZERO, |bb| bb.center())
            };
            let shift = centre(s) - centre(b);
            let r = rules.entry(b.name.clone()).or_default();
            for (pb, ps) in b.pts.iter().zip(&s.pts) {
                let d = ps.pos - pb.pos - shift;
                r.entry(pb.id.clone()).or_default().insert(
                    azam_geom::SizeName::from(spec.name),
                    azam_geom::GradeDelta {
                        dx: round_half_up(d.x * 10.0) / 10.0,
                        dy: round_half_up(d.y * 10.0) / 10.0,
                    },
                );
            }
        }
    }
    for pc in project.pieces.values_mut().filter(|pc| pc.grade.is_none()) {
        if let Some(r) = rules.remove(&pc.name)
            && !r.is_empty()
        {
            pc.grade = Some(r);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::pom_value;

    #[test]
    fn refined_templates_grade_and_interface_the_waistband() {
        for template in [Template::Tee, Template::Dress, Template::Skirt] {
            let p = from_template(template, Sex::Female, None, None, 0.0);
            assert!(
                p.pieces.values().all(|pc| pc.grade.is_some()),
                "{template:?}: every piece grades"
            );
            for pom in p.poms.values() {
                let at = |s: &str| pom_value(&p, pom, &azam_geom::SizeName::from(s));
                assert!(
                    at("XS") < at("M") && at("M") < at("XL"),
                    "{template:?} {}: grows with size",
                    pom.name
                );
            }
            let reference = reference_template(template, Sex::Female, None, None, 0.0);
            assert!(
                reference.pieces.values().all(|pc| pc.grade.is_none()),
                "the reference draft has none"
            );
        }
        let skirt = from_template(Template::Skirt, Sex::Female, None, None, 0.0);
        let band = skirt
            .pieces
            .values()
            .find(|pc| pc.name == WAISTBAND_NAME)
            .expect("waistband");
        assert_eq!(band.strengthen, Some(true));
    }

    #[test]
    fn the_dress_front_has_bust_darts_and_its_side_seams_still_match() {
        let p = from_template(Template::Dress, Sex::Female, None, None, 0.0);
        let reference = reference_template(Template::Dress, Sex::Female, None, None, 0.0);
        let front = p.pieces.values().find(|pc| pc.name == "Front").expect("front");
        let reference_front = reference
            .pieces
            .values()
            .find(|pc| pc.name == "Front")
            .expect("front");
        assert!(
            front.pts.len() >= reference_front.pts.len() + 6,
            "two legs and an apex per dart (and the curve split around them)"
        );
        assert_eq!(
            p.sewing.len(),
            reference.sewing.len() + 2,
            "a seam closing each dart"
        );
        let lengths = crate::sew_lengths(&p);
        for seam in p.sewing.values() {
            let l = &lengths[&seam.id];
            // the side seams: the front's two stretches against the back's
            if seam
                .a
                .iter()
                .chain(&seam.b)
                .filter(|s| s.piece == front.id)
                .count()
                >= 2
                && seam.a.iter().chain(&seam.b).any(|s| s.piece != front.id)
            {
                assert!((l.a - l.b).abs() < 2.0, "side seam {:.1} vs {:.1}", l.a, l.b);
            }
        }
        // each dart's legs are the same length, so it closes flat
        for seam in p
            .sewing
            .values()
            .filter(|s| s.a.iter().chain(&s.b).all(|x| x.piece == front.id))
        {
            let l = &lengths[&seam.id];
            assert!((l.a - l.b).abs() < 0.5, "dart legs {:.1} vs {:.1}", l.a, l.b);
        }
        // and it grades with every size
        for size in ["XS", "XL"] {
            let ol = front.graded_outline(&azam_geom::SizeName::from(size), &p.base_size);
            assert!(!ol.poly.is_empty());
        }
    }

    #[test]
    fn trued_hems_leave_the_side_seams_square() {
        for template in [Template::Skirt, Template::Dress] {
            let p = from_template(template, Sex::Female, None, None, 0.0);
            for pc in p
                .pieces
                .values()
                .filter(|pc| pc.point_index(&azam_geom::PointId::new("hemC")).is_some())
            {
                let pos = |id: &str| pc.pts[pc.point_index(&azam_geom::PointId::new(id)).expect("point")].pos;
                let (hip, hem, c) = (pos("hipL"), pos("hemL"), pos("hemC"));
                let (side, hem_start) = (hip - hem, c - hem);
                let cos =
                    (side.x * hem_start.x + side.y * hem_start.y) / (side.length() * hem_start.length());
                assert!(
                    cos.abs() < 1e-9,
                    "{template:?} {}: the hem leaves the corner square ({cos})",
                    pc.name
                );
            }
        }
    }
}
