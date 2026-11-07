//! Aba: sleeveless open clerical cloak.
//!
//! One wide body whose sides are straight (the fold of the cloth), a long straight shoulder line,
//! an open front with no closure, and an arm slit at the top of each side. Worn over the robe,
//! slightly shorter so the robe shows at the hem.

use std::f64::consts::PI;

use azam_geom::{GradeDelta, GradeRules, OutlinePoint, SizeName, Vec2};
use serde_json::json;

use super::{
    DraftOptions, StitchStyle, add_pom, add_stitch, body, corner, curve, flag, name_if_untitled, new_piece,
    nonzero, outline_bbox,
};
use crate::avatar::{Measurements, SizeSpec, size_chart};
use crate::fabric::{FabricPreset, make_fabric};
use crate::ids::{FabricId, PieceId, SeamId};
use crate::mirror::{mirrored_grade, mirrored_outline};
use crate::num::{negate, round_half_up};
use crate::piece::{Arrangement, MirrorLink, translate_points};
use crate::project::{DEFAULT_BASE_SIZE, Project, SeamSide, TechPack};
use crate::torso::{BodyProfile, TorsoProfile};

/// Default fabric id of the aba.
const ABA_FABRIC: &str = "fab_aba";
/// Default arrangement distance from the body, in mm.
const DEFAULT_OFFSET_MM: f64 = 180.0;
/// Gap left on the board to the right of existing pieces, in mm.
const BOARD_GAP_MM: f64 = 300.0;
/// Tech-pack note of the aba; notes already starting an aba entry are left alone.
const NOTE: &str = "Aba: sleeveless open cloak in light wool, worn over the robe and 60 mm shorter. Straight sides (cloth fold) closed from the arm slits to the hem; 360 mm arm slits at the top of each side; long shoulder line sloping 40° with the arm; open front, no closure; narrow braid along the neck and front edges.";
/// Marker of the aba note.
const NOTE_MARKER: &str = "Aba:";

/// Ids of what [`draft_aba`] added.
#[derive(Clone, Debug, PartialEq)]
pub struct AbaPieces {
    /// Left front, right front (linked mirror) and back.
    pub pieces: Vec<PieceId>,
    /// The seams.
    pub sewing: Vec<SeamId>,
}

struct Shapes {
    front: Vec<OutlinePoint>,
    back: Vec<OutlinePoint>,
    top: f64,
    w: f64,
    e0: f64,
}

fn shapes(m: &Measurements, tp: &dyn BodyProfile, opts: &DraftOptions<'_>) -> Shapes {
    let h = m.height;
    // the robe's neckline, used as the shared reference
    let nk_robe = m.neck / (2.0 * PI) + 9.0;
    let top = tp.shoulder_y(nk_robe) + 10.0;
    let len = nonzero(opts.length).unwrap_or_else(|| round_half_up(top - h * 0.068) - 60.0);
    // the neck opening wraps loosely around the robe's collar so the aba sits on the shoulders
    let nk = nk_robe + 35.0;
    let bnd = nonzero(opts.back_neck).unwrap_or(70.0);
    let e0 = 45.0;
    let fnd = nonzero(opts.front_neck).unwrap_or(110.0);
    let sh = m.shoulder / 2.0 + 25.0;
    let drop = 48.0;
    // kimono-style long shoulder line sloping with the arm, so each arm passes out through its slit
    let w = nonzero(opts.half_width).unwrap_or_else(|| round_half_up(m.shoulder / 2.0 + m.arm * 0.62));
    let slope = nonzero(opts.slope).unwrap_or(40.0);
    let s_t = drop + (w - sh) * (slope * (PI / 180.0)).tan();
    let slit = nonzero(opts.slit).unwrap_or(360.0);
    let sl_b = s_t + slit;
    let front = vec![
        corner("hps", nk, 0.0),
        curve("n1", nk * 0.92, fnd * 0.62),
        curve("n2", e0 + (nk - e0) * 0.35, fnd),
        corner("fN", e0, fnd),
        corner("fH", e0, len),
        curve("hc", (e0 + w) * 0.5, len),
        corner("sH", w, len - 10.0),
        corner("slB", w, sl_b),
        corner("sT", w, s_t),
        corner("shp", sh, drop),
    ];
    let back = vec![
        corner("hpsR", nk, 0.0),
        curve("bnR", nk * 0.5, bnd),
        corner("cbk", 0.0, bnd),
        curve("bnL", -nk * 0.5, bnd),
        corner("hpsL", -nk, 0.0),
        corner("shpL", -sh, drop),
        corner("sTL", -w, s_t),
        corner("slBL", -w, sl_b),
        corner("sHL", -w, len - 10.0),
        curve("bh", 0.0, len + 10.0),
        corner("sHR", w, len - 10.0),
        corner("slBR", w, sl_b),
        corner("sTR", w, s_t),
        corner("shpR", sh, drop),
    ];
    Shapes {
        front,
        back,
        top,
        w,
        e0,
    }
}

fn grade_outline(rules: &mut GradeRules, size: &str, pts: &[OutlinePoint], base: &[OutlinePoint]) {
    for (p, b) in pts.iter().zip(base) {
        rules.entry(p.id.clone()).or_default().insert(
            SizeName::from(size),
            GradeDelta {
                dx: round_half_up((p.pos.x - b.pos.x) * 10.0) / 10.0,
                dy: round_half_up((p.pos.y - b.pos.y) * 10.0) / 10.0,
            },
        );
    }
}

/// Drafts the aba into `project`, to the right of any pieces already there, with grading for the
/// project's sizes. Returns the ids of the added pieces and seams.
#[allow(
    clippy::too_many_lines,
    reason = "a draft reads best as one sequence of steps"
)]
pub fn draft_aba(project: &mut Project, opts: &DraftOptions<'_>) -> AbaPieces {
    let m = project.avatar.body;
    let body = body(project, opts);
    let tp = body.get();
    let s = shapes(&m, tp, opts);
    // place to the right of whatever is already on the board
    let mut x0 = project
        .pieces
        .values()
        .flat_map(|pc| pc.pts.iter())
        .fold(0.0_f64, |acc, p| acc.max(p.pos.x));
    if !project.pieces.is_empty() {
        x0 += BOARD_GAP_MM;
    }
    let ox = x0 + s.w + 40.0;
    // the fronts meet the axis with a 40 mm gap
    let axis = ox - 20.0 - s.e0;
    let fb_pts = translate_points(&s.front, Vec2::new(ox - s.e0, 0.0));
    let fa_pts = mirrored_outline(&fb_pts, axis);
    let bx = ox - s.e0 + s.w + 160.0 + s.w;
    let bk_pts = translate_points(&s.back, Vec2::new(bx, 0.0));
    // flat panels in front of and behind the A-posed arms
    let off_b = nonzero(opts.off).unwrap_or(DEFAULT_OFFSET_MM);
    let (bb_f, bb_b) = (outline_bbox(&fb_pts), outline_bbox(&bk_pts));
    let h = s.top + 50.0 - bb_f.center().y;
    let h_b = s.top + 50.0 - bb_b.center().y;
    let shift = round_half_up(bb_f.center().x - (ox - s.e0));
    let fabric = FabricId::new(opts.fabric.unwrap_or(ABA_FABRIC));

    let mut fb = new_piece(project, "عبا · جلو چپ", fb_pts);
    fb.fabric = fabric.clone();
    fb.pd = 34.0;
    fb.layer = 1;
    fb.arr = Arrangement::slot("front", off_b)
        .at_height(h)
        .with("flat", flag(true))
        .with("shift", json!(shift));
    let mut fa = new_piece(project, "عبا · جلو راست", fa_pts);
    fa.fabric = fabric.clone();
    fa.pd = 34.0;
    fa.layer = 1;
    fa.arr = Arrangement::slot("front", off_b)
        .at_height(h)
        .with("flat", flag(true))
        .with("shift", json!(negate(shift)));
    fa.mirror = Some(MirrorLink {
        src: fb.id.clone(),
        axis,
    });
    let mut bk = new_piece(project, "عبا · پشت", bk_pts);
    bk.fabric = fabric.clone();
    bk.pd = 34.0;
    bk.layer = 1;
    bk.arr = Arrangement::slot("back", round_half_up(off_b * 0.65))
        .at_height(h_b)
        .with("flat", flag(true));

    // grading from the size chart, for the sizes the project uses
    let chart = size_chart(m.sex);
    let sizes: Vec<SizeName> = if project.sizes.is_empty() {
        chart.iter().map(|c| SizeName::from(c.name)).collect()
    } else {
        project.sizes.clone()
    };
    let base_name = if project.base_size.0.is_empty() {
        DEFAULT_BASE_SIZE
    } else {
        project.base_size.0.as_str()
    };
    let base_spec = chart
        .iter()
        .find(|c| c.name == base_name)
        .unwrap_or(&chart[chart.len() / 2]);
    let slit_only = DraftOptions {
        slit: opts.slit,
        ..DraftOptions::default()
    };
    let shapes_for = |spec: &SizeSpec| {
        let mm = m.with_size(spec);
        shapes(&mm, &TorsoProfile::new(&mm), &slit_only)
    };
    let b0 = shapes_for(base_spec);
    let (mut g_fb, mut g_bk) = (GradeRules::new(), GradeRules::new());
    for c in chart {
        if c.name == base_spec.name || !sizes.iter().any(|s| s.0 == c.name) {
            continue;
        }
        let sz = shapes_for(c);
        grade_outline(&mut g_fb, c.name, &sz.front, &b0.front);
        grade_outline(&mut g_bk, c.name, &sz.back, &b0.back);
    }
    fa.grade = Some(mirrored_grade(&g_fb));
    fb.grade = Some(g_fb);
    bk.grade = Some(g_bk);

    let fb = project.add_piece(fb);
    let fa = project.add_piece(fa);
    let bk = project.add_piece(bk);
    let side = SeamSide::segment;
    let sewing = vec![
        // long shoulder lines (neck to shoulder point, then out to the arm slit)
        project.add_seam(vec![side(&fb, "shp", true)], vec![side(&bk, "hpsL", false)]),
        project.add_seam(vec![side(&fb, "sT", true)], vec![side(&bk, "shpL", false)]),
        project.add_seam(vec![side(&fa, "mhps", false)], vec![side(&bk, "shpR", true)]),
        project.add_seam(vec![side(&fa, "mshp", false)], vec![side(&bk, "sTR", true)]),
        // sides closed from the slit to the hem (the fold of the cloth); slits stay open for the arms
        project.add_seam(vec![side(&fb, "sH", true)], vec![side(&bk, "slBL", false)]),
        project.add_seam(vec![side(&fa, "mslB", false)], vec![side(&bk, "sHR", true)]),
    ];
    if !project.fabrics.contains_key(&fabric) {
        let mut fab = make_fabric(project.id_gen(), FabricPreset::Wool);
        fab.id = fabric.clone();
        "پشم عبایی".clone_into(&mut fab.name);
        "#1f1e1c".clone_into(&mut fab.color);
        "twill".clone_into(&mut fab.weave);
        fab.sheen = 0.14;
        fab.extra
            .insert("content".to_owned(), json!("Light wool (aba cloth)"));
        let ph = &mut fab.phys;
        (ph.stretch_weft, ph.stretch_warp, ph.shear) = (62.0, 66.0, 40.0);
        (ph.bend_weft, ph.bend_warp, ph.buckling) = (22.0, 24.0, 55.0);
        (ph.density, ph.thickness, ph.damping, ph.friction) = (170.0, 0.45, 30.0, 60.0);
        project.fabrics.insert(fabric.clone(), fab);
    }
    // colorways: black by default, dark brown with the brown robe, translucent with the summer robe
    for cw in project.colorways.values_mut() {
        let over = match cw.id.as_str() {
            "cw_brown" => Some(json!({ "color": "#2e241d" })),
            "cw_cream" => Some(json!({ "color": "#c9b48e", "opacity": 0.88 })),
            _ => None,
        };
        if let Some(serde_json::Value::Object(o)) = over {
            cw.over.insert(fabric.clone(), o);
        }
    }
    let braid = StitchStyle {
        off: 8.0,
        len: 2.4,
        gap: 1.2,
        w: 0.8,
        color: "#4a4136",
        rows: 2,
        row_gap: 5.0,
    };
    for (pc, seg) in [
        (&fb, "fN"),
        (&fa, "mfH"),
        (&fb, "hps"),
        (&fa, "mfN"),
        (&bk, "hpsR"),
        (&bk, "cbk"),
    ] {
        add_stitch(project, pc, seg, &braid);
    }
    add_pom(project, "Aba front edge", &[(&fb, "fN")]);
    add_pom(project, "Aba arm slit", &[(&fb, "slB")]);
    add_pom(project, "Aba shoulder line", &[(&fb, "shp"), (&fb, "sT")]);
    if project.sizes.is_empty() {
        project.sizes = sizes;
        project.base_size = SizeName::from(base_spec.name);
        project.active_size = SizeName::from(base_spec.name);
    }
    let tech = project.techpack.get_or_insert_with(TechPack::default);
    tech.notes = Some(match tech.notes.take().filter(|n| !n.is_empty()) {
        Some(n) if n.contains(NOTE_MARKER) => n,
        Some(n) => format!("{n}\n{NOTE}"),
        None => NOTE.to_owned(),
    });
    name_if_untitled(project, "عبا");
    AbaPieces {
        pieces: vec![fb, fa, bk],
        sewing,
    }
}
