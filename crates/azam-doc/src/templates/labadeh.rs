//! Labadeh: long clerical over-robe.
//!
//! Ankle length; edge-to-edge front without buttons (collar hook and hidden hooks to just below the
//! waist); close, interfaced stand collar; long set-in sleeves; one-piece skirt with no hem vents;
//! pocket-access slits in both side seams.

use std::f64::consts::PI;

use azam_geom::{GradeDelta, GradeRules, OutlinePoint, PointId, SizeName, Vec2};
use serde_json::{Map, json};

use super::{
    DraftOptions, StitchStyle, add_pom, add_stitch, assign_fabric, body, corner, curve, name_if_untitled,
    new_piece, nonzero, outline_bbox, seg_len,
};
use crate::avatar::{Measurements, size_chart};
use crate::fabric::{FabricPreset, make_fabric};
use crate::ids::{ColorwayId, FabricId, PartId, prefix};
use crate::mirror::{mirrored_grade, mirrored_outline};
use crate::num::{negate, round_half_up};
use crate::piece::{Arrangement, MAIN_FABRIC, MirrorLink, Notch, translate_points};
use crate::project::{Colorway, MAIN_COLORWAY, Project, SeamSide, TechPack};
use crate::torso::{BodyProfile, TorsoProfile};

/// Default ease over the chest, in mm.
const EASE_MM: f64 = 130.0;
/// Iterations of the sleeve and collar solves.
const SOLVE_ITERATIONS: usize = 30;
/// Spacing of the torso samples used to find the arrangement radius, in mm.
const RING_SAMPLE_MM: f64 = 10.0;
/// Arrangement radius used when the torso gives none, in mm.
const DEFAULT_RING_RADIUS_MM: f64 = 160.0;
/// Size the pattern is drafted in.
const BASE_SIZE: &str = "M";

/// Outlines and key dimensions of the labadeh in one body size.
struct Shapes {
    front: Vec<OutlinePoint>,
    back: Vec<OutlinePoint>,
    sleeve: Vec<OutlinePoint>,
    collar: Vec<OutlinePoint>,
    top: f64,
    len: f64,
    y_waist: f64,
    fnd: f64,
    bnd: f64,
    hem_w: f64,
    hw: f64,
    cap_h: f64,
    s_len: f64,
    ce: f64,
}

#[allow(
    clippy::too_many_lines,
    reason = "a draft reads best as one sequence of steps"
)]
fn shapes(m: &Measurements, tp: &dyn BodyProfile, opts: &DraftOptions<'_>) -> Shapes {
    let h = m.height;
    let ease = opts.ease.unwrap_or(EASE_MM);
    let w = (m.bust + ease) / 4.0;
    let sh = m.shoulder / 2.0 + 20.0;
    let nk = m.neck / (2.0 * PI) + 9.0;
    let fnd = round_half_up(nk * 1.1);
    let bnd = 22.0;
    let drop = 44.0;
    let ah = round_half_up(h * 0.118);
    let y_ua = drop + ah;
    let top = tp.shoulder_y(nk) + 10.0;
    let len = nonzero(opts.length).unwrap_or_else(|| round_half_up(top - h * 0.068));
    let lm = tp.landmarks();
    let y_w = round_half_up(top - lm.waist);
    let y_h = round_half_up(top - lm.hip);
    let w_w = (m.waist + 160.0) / 4.0;
    let h_w = (w - 4.0).max((m.hip + 170.0) / 4.0);
    let hem_w = h_w + (len - y_h) * 0.16;
    let side_x = |y: f64| {
        if y <= y_h {
            w_w + (h_w - w_w) * ((y - y_w) / (y_h - y_w).max(1.0)).clamp(0.0, 1.0).sqrt()
        } else {
            h_w + (hem_w - h_w) * (y - y_h) / (len - y_h)
        }
    };
    // pocket-access slit
    let y_t = y_w + 40.0;
    let y_b = y_t + 175.0;
    let front = vec![
        corner("hps", nk, 0.0),
        curve("n1", nk, fnd * 0.55),
        curve("n2", nk * 0.55, fnd),
        corner("cfN", 0.0, fnd),
        corner("cfH", 0.0, len),
        curve("hc", hem_w * 0.5, len),
        corner("hm", hem_w, len - 8.0),
        corner("slB", side_x(y_b), y_b),
        corner("slT", side_x(y_t), y_t),
        curve("wc", w_w - 6.0, y_w),
        corner("ua", w, y_ua),
        curve("a2", w - 16.0, drop + ah * 0.95),
        curve("a1", sh - 8.0, drop + ah * 0.52),
        corner("sh", sh, drop),
    ];
    let back = vec![
        corner("hpsR", nk, 0.0),
        curve("bnR", nk * 0.5, bnd),
        corner("cbk", 0.0, bnd),
        curve("bnL", -nk * 0.5, bnd),
        corner("hpsL", -nk, 0.0),
        corner("shL", -sh, drop),
        curve("ahL1", -sh + 8.0, drop + ah * 0.52),
        curve("ahL2", -w + 16.0, drop + ah * 0.95),
        corner("uaL", -w, y_ua),
        curve("wL", -(w_w - 6.0), y_w),
        corner("slTL", -side_x(y_t), y_t),
        corner("slBL", -side_x(y_b), y_b),
        corner("hmL", -hem_w, len - 8.0),
        curve("bh", 0.0, len + 8.0),
        corner("hmR", hem_w, len - 8.0),
        corner("slBR", side_x(y_b), y_b),
        corner("slTR", side_x(y_t), y_t),
        curve("wR", w_w - 6.0, y_w),
        corner("uaR", w, y_ua),
        curve("ahR2", w - 16.0, drop + ah * 0.95),
        curve("ahR1", sh - 8.0, drop + ah * 0.52),
        corner("shR", sh, drop),
    ];
    // long set-in sleeve: solve the bicep width so the cap matches the armholes plus a little ease
    let armhole = seg_len(&front, "ua") + seg_len(&back, "shL");
    let cap_h = round_half_up(h * 0.064);
    let cap_ease = 18.0;
    let s_len = m.arm + 5.0 - cap_h * 1.065;
    let hw_hem = 140f64.max((if m.upper_arm == 0.0 { 300.0 } else { m.upper_arm }) * 0.5);
    let cap_pts = |hw: f64| {
        vec![
            corner("uaR", hw, cap_h),
            curve("cR", hw * 0.55, -cap_h * 0.42),
            curve("cL", -hw * 0.55, -cap_h * 0.42),
            corner("uaL", -hw, cap_h),
            corner("hemL", -hw_hem, cap_h + s_len),
            corner("hemR", hw_hem, cap_h + s_len),
        ]
    };
    let mut hw = armhole * 0.4;
    for _ in 0..SOLVE_ITERATIONS {
        hw *= (armhole + cap_ease) / seg_len(&cap_pts(hw), "uaR");
    }
    let sleeve = cap_pts(hw);
    // stand collar: an annular-sector band whose lower edge equals the neckline
    let neck_len = 2.0 * seg_len(&front, "hps") + seg_len(&back, "hpsR") + seg_len(&back, "cbk");
    let (ch, ce, cd) = (40.0, 12.0, 8.0);
    let collar = |l: f64| {
        vec![
            corner("tl", cd, -ce),
            curve("tc", l / 2.0, ce),
            corner("tr", l - cd, -ce),
            corner("br", l, ch - ce),
            curve("bc", l / 2.0, ch + ce),
            corner("bl", 0.0, ch - ce),
        ]
    };
    let mut l = neck_len;
    for _ in 0..SOLVE_ITERATIONS {
        l *= neck_len / seg_len(&collar(l), "br");
    }
    Shapes {
        front,
        back,
        sleeve,
        collar: collar(l),
        top,
        len,
        y_waist: y_w,
        fnd,
        bnd,
        hem_w,
        hw,
        cap_h,
        s_len,
        ce,
    }
}

/// Arrangement radius around the torso over the height of a piece centred at `h` (the radius the
/// body arrangement uses), plus `off`.
fn ring_radius(tp: &dyn BodyProfile, bb: &azam_geom::BBox, h: f64, off: f64) -> f64 {
    let (y0, y1) = tp.torso_range();
    let cy = bb.center().y;
    let mut r = 0.0_f64;
    let mut y = h - (bb.max.y - cy);
    while y <= h + (cy - bb.min.y) {
        let e = tp.torso_at(y.clamp(y0, y1));
        r = r.max(e[0].max(e[1] + e[2].abs()));
        y += RING_SAMPLE_MM;
    }
    (if r == 0.0 { DEFAULT_RING_RADIUS_MM } else { r }) + off
}

/// Grade offsets of one outline: each point's offset from the base size, rounded to 0.1 mm.
fn add_grades(rules: &mut GradeRules, size: &str, pts: &[OutlinePoint], base: &[OutlinePoint]) {
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

fn notch(project: &mut Project, segment: &str) -> Notch {
    Notch {
        id: Some(PartId::new(project.fresh_id(prefix::NOTCH))),
        s: PointId::new(segment),
        t: Some(0.5),
        extra: Map::new(),
    }
}

/// Drafts the labadeh into `project`, with grading for every size of the size chart.
#[allow(
    clippy::too_many_lines,
    reason = "a draft reads best as one sequence of steps"
)]
pub fn draft_labadeh(project: &mut Project, opts: &DraftOptions<'_>) {
    let m = project.avatar.body;
    let body = body(project, opts);
    let tp = body.get();
    let s = shapes(&m, tp, opts);
    // 2D layout: right front | left front | back | sleeves, collar below the sleeves
    let ox = s.hem_w + 80.0;
    let axis = ox - 40.0;
    let fb_pts = translate_points(&s.front, Vec2::new(ox, 0.0));
    let fa_pts = mirrored_outline(&fb_pts, axis);
    let bx = ox + 2.0 * s.hem_w + 120.0;
    let bk_pts = translate_points(&s.back, Vec2::new(bx, 0.0));
    let sx = bx + s.hem_w + 150.0 + s.hw;
    let sl_pts = translate_points(&s.sleeve, Vec2::new(sx, 0.0));
    let sr_pts = translate_points(&s.sleeve, Vec2::new(sx + 2.0 * s.hw + 100.0, 0.0));
    let co_pts = translate_points(&s.collar, Vec2::new(sx - s.hw, s.cap_h + s.s_len + 160.0));
    let off_b = 75.0;
    let bb_f = outline_bbox(&fb_pts);
    let h_f = s.top - bb_f.center().y;
    let ang_f = (bb_f.center().x - ox) / ring_radius(tp, &bb_f, h_f, off_b) / (PI / 180.0);
    let ang_rounded = round_half_up(ang_f * 10.0);
    let (bb_b, bb_s, bb_c) = (
        outline_bbox(&bk_pts),
        outline_bbox(&sl_pts),
        outline_bbox(&co_pts),
    );

    let mut fb = new_piece(project, "جلو چپ", fb_pts);
    fb.pd = 26.0;
    fb.arr = Arrangement::slot("front", off_b)
        .at_height(h_f)
        .angle(ang_rounded / 10.0);
    let fb_id = fb.id.clone();
    let mut fa = new_piece(project, "جلو راست", fa_pts);
    fa.pd = 26.0;
    fa.arr = Arrangement::slot("front", off_b)
        .at_height(h_f)
        .angle(negate(ang_rounded) / 10.0);
    fa.mirror = Some(MirrorLink {
        src: fb_id.clone(),
        axis,
    });
    let mut bk = new_piece(project, "پشت", bk_pts);
    bk.pd = 26.0;
    bk.arr = Arrangement::slot("back", off_b).at_height(s.top - bb_b.center().y);
    let t_s = 15.0 + (bb_s.center().y - bb_s.min.y);
    let sl_notch = notch(project, "uaR");
    let mut sl = new_piece(project, "آستین چپ", sl_pts);
    sl.pd = 20.0;
    sl.arr = Arrangement::slot("armL", 55.0).along(t_s);
    sl.notches = vec![sl_notch];
    let sr_notch = notch(project, "uaR");
    let mut sr = new_piece(project, "آستین راست", sr_pts);
    sr.pd = 20.0;
    sr.arr = Arrangement::slot("armR", 55.0).along(t_s);
    sr.notches = vec![sr_notch];
    let co_notch = notch(project, "br");
    let collar_h = s.top - s.bnd + (co_pts[3].pos.y + s.ce) - bb_c.center().y;
    let mut co = new_piece(project, "یقه", co_pts);
    co.pd = 10.0;
    co.strengthen = Some(true);
    co.arr = Arrangement::slot("neck", 12.0).angle(180.0).at_height(collar_h);
    co.notches = vec![co_notch];

    let shapes_for = |size: &crate::avatar::SizeSpec| {
        let mm = m.with_size(size);
        shapes(&mm, &TorsoProfile::new(&mm), opts)
    };
    let chart = size_chart(m.sex);
    let base_spec = chart
        .iter()
        .find(|c| c.name == BASE_SIZE)
        .unwrap_or(&chart[chart.len() / 2]);
    let base = shapes_for(base_spec);
    let (mut g_fb, mut g_bk, mut g_sl, mut g_co) = (
        GradeRules::new(),
        GradeRules::new(),
        GradeRules::new(),
        GradeRules::new(),
    );
    for c in chart.iter().filter(|c| c.name != base_spec.name) {
        let sz = shapes_for(c);
        add_grades(&mut g_fb, c.name, &sz.front, &base.front);
        add_grades(&mut g_bk, c.name, &sz.back, &base.back);
        add_grades(&mut g_sl, c.name, &sz.sleeve, &base.sleeve);
        add_grades(&mut g_co, c.name, &sz.collar, &base.collar);
    }
    fa.grade = Some(mirrored_grade(&g_fb));
    fb.grade = Some(g_fb);
    bk.grade = Some(g_bk);
    sr.grade = Some(g_sl.clone());
    sl.grade = Some(g_sl);
    co.grade = Some(g_co);

    let fb = project.add_piece(fb);
    let fa = project.add_piece(fa);
    let bk = project.add_piece(bk);
    let sl = project.add_piece(sl);
    let sr = project.add_piece(sr);
    let co = project.add_piece(co);

    let side = SeamSide::segment;
    // shoulders and side seams (upper and lower parts; the slit between them stays open)
    project.add_seam(vec![side(&fa, "mhps", false)], vec![side(&bk, "shR", true)]);
    project.add_seam(vec![side(&fb, "sh", true)], vec![side(&bk, "hpsL", false)]);
    project.add_seam(vec![side(&fa, "mua", false)], vec![side(&bk, "slTR", true)]);
    project.add_seam(vec![side(&fa, "mslB", false)], vec![side(&bk, "hmR", true)]);
    project.add_seam(vec![side(&fb, "slT", true)], vec![side(&bk, "uaL", false)]);
    project.add_seam(vec![side(&fb, "hm", true)], vec![side(&bk, "slBL", false)]);
    // sleeves: underarm seams and set-in caps (M:N to front + back armholes)
    project.add_seam(vec![side(&sl, "uaL", false)], vec![side(&sl, "hemR", true)]);
    project.add_seam(vec![side(&sr, "uaL", false)], vec![side(&sr, "hemR", true)]);
    project.add_seam(
        vec![side(&sl, "uaR", true)],
        vec![side(&fb, "ua", false), side(&bk, "shL", false)],
    );
    project.add_seam(
        vec![side(&sr, "uaR", false)],
        vec![side(&fa, "msh", true), side(&bk, "uaR", true)],
    );
    // stand collar onto the neckline, its ends hooked at centre front
    project.add_seam(
        vec![side(&co, "br", false)],
        vec![
            side(&fa, "mcfN", false),
            side(&bk, "hpsR", false),
            side(&bk, "cbk", false),
            side(&fb, "hps", false),
        ],
    );
    project.add_seam(vec![side(&co, "tr", false)], vec![side(&co, "bl", true)]);
    // hidden hooks: front edges meet edge to edge from the collar to just below the waist
    let f = ((s.y_waist + 20.0 - s.fnd) / (s.len - s.fnd)).clamp(0.05, 0.95);
    project.add_seam(
        vec![SeamSide::new(&fb, "cfN", false, 0.0, f)],
        vec![SeamSide::new(&fa, "mcfH", true, 1.0 - f, 1.0)],
    );

    // fabric: worsted wool; the collar is interfaced via `strengthen`
    let mut fab = make_fabric(project.id_gen(), FabricPreset::Wool);
    fab.id = FabricId::new(MAIN_FABRIC);
    "فاستونی پشمی".clone_into(&mut fab.name);
    "#57544f".clone_into(&mut fab.color);
    "twill".clone_into(&mut fab.weave);
    fab.extra.insert("content".to_owned(), json!("Worsted wool"));
    assign_fabric(project, fab);
    project.colorways.clear();
    let main = Colorway::new(MAIN_COLORWAY, "طوسی ذغالی", 0);
    project.colorways.insert(main.id.clone(), main);
    let alternatives = [
        ("cw_brown", "قهوه‌ای", "#4b3c31"),
        ("cw_camel", "شتری", "#9c8466"),
        ("cw_cream", "شیری تابستانی", "#d8cfbe"),
        ("cw_navy", "سرمه‌ای", "#2b3242"),
    ];
    for (i, (id, name, color)) in (1u32..).zip(alternatives) {
        let mut cw = Colorway::new(id, name, i);
        let mut over = Map::new();
        over.insert("color".to_owned(), json!(color));
        cw.over.insert(FabricId::new(MAIN_FABRIC), over);
        project.colorways.insert(cw.id.clone(), cw);
    }
    project.active_colorway = ColorwayId::new(MAIN_COLORWAY);

    let braid = StitchStyle {
        off: 5.0,
        len: 2.6,
        gap: 1.4,
        w: 0.7,
        color: "#3a3834",
        rows: 1,
        row_gap: 6.0,
    };
    add_stitch(project, &fb, "cfN", &braid);
    add_stitch(project, &fa, "mcfH", &braid);
    add_stitch(project, &co, "tl", &braid);
    add_pom(project, "Front edge (collar to hem)", &[(&fb, "cfN")]);
    add_pom(project, "Neckline / collar", &[(&co, "br")]);
    add_pom(project, "Pocket slit", &[(&fb, "slB")]);
    add_pom(project, "Front hem (half)", &[(&fb, "cfH")]);
    add_pom(project, "Sleeve cap", &[(&sl, "uaR")]);
    add_pom(project, "Sleeve opening", &[(&sl, "hemL")]);

    project.sizes = chart.iter().map(|c| SizeName::from(c.name)).collect();
    project.base_size = SizeName::from(base_spec.name);
    project.active_size = SizeName::from(base_spec.name);
    let tech = project.techpack.take().unwrap_or_default();
    project.techpack = Some(TechPack {
        season: tech.season.or_else(|| Some("All season".to_owned())),
        notes: tech.notes.or_else(|| Some(NOTES.to_owned())),
        extra: tech.extra,
    });
    name_if_untitled(project, "لباده");
}

/// Tech-pack notes of the labadeh.
const NOTES: &str = "Labadeh: ankle-length clerical over-robe. Edge-to-edge front without buttons, closed by a hook at the collar and hidden hooks to just below the waist. Stand collar, shoulders and chest interfaced. Long set-in sleeves. One-piece skirt with no hem vents; pocket-access slits in both side seams. Fully lined (lining not modelled).";
