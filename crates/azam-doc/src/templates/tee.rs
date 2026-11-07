//! T-shirt: front and back bodice, set-in short sleeves and a neck rib.

use std::f64::consts::PI;

use azam_geom::{OutlinePoint, Vec2};

use super::{
    DraftOptions, StitchStyle, add_pom, add_stitch, body, corner, curve, name_if_untitled, new_piece,
    nonzero, seg_len,
};
use crate::fabric::{FabricPreset, make_fabric};
use crate::ids::FabricId;
use crate::num::round_half_up;
use crate::piece::{Arrangement, translate_points};
use crate::project::{Project, SeamSide};

/// Default ease over the bust, in mm.
const EASE_MM: f64 = 90.0;
/// Default length as a fraction of body height.
const LENGTH_OF_HEIGHT: f64 = 0.385;
/// Shoulder point beyond half the shoulder width, in mm.
const SHOULDER_EXTENSION_MM: f64 = 18.0;
/// Neck width beyond the neck radius, in mm.
const NECK_EASE_MM: f64 = 16.0;
/// Shoulder drop, in mm.
const SHOULDER_DROP_MM: f64 = 42.0;
/// Armhole depth as a fraction of body height.
const ARMHOLE_OF_HEIGHT: f64 = 0.10;
/// Default front neck depth, in mm.
const FRONT_NECK_DEPTH_MM: f64 = 78.0;
/// Back neck depth, in mm.
const BACK_NECK_DEPTH_MM: f64 = 20.0;
/// Default sleeve length below the cap, in mm.
const SLEEVE_LENGTH_MM: f64 = 210.0;
/// Sleeve cap height as a fraction of body height.
const CAP_OF_HEIGHT: f64 = 0.04;
/// Sleeve cap ease over the armhole, in mm.
const CAP_EASE_MM: f64 = 45.0;
/// Iterations of the sleeve width solve.
const SOLVE_ITERATIONS: usize = 30;
/// Neck rib length as a fraction of the neckline.
const RIB_LENGTH_OF_NECK: f64 = 0.86;
/// Neck rib width, in mm.
const RIB_WIDTH_MM: f64 = 22.0;
/// Gap between pieces on the board, in mm.
const BOARD_GAP_MM: f64 = 120.0;

/// Drafts the tee into `project`.
#[allow(
    clippy::too_many_lines,
    reason = "a draft reads best as one sequence of steps"
)]
pub fn draft_tee(project: &mut Project, opts: &DraftOptions<'_>) {
    let m = project.avatar.body;
    let ease = opts.ease.unwrap_or(EASE_MM);
    let len = nonzero(opts.length).unwrap_or_else(|| round_half_up(m.height * LENGTH_OF_HEIGHT));
    let body = body(project, opts);
    let tp = body.get();
    let w = (m.bust + ease) / 4.0;
    let sh = m.shoulder / 2.0 + SHOULDER_EXTENSION_MM;
    let nk = m.neck / (2.0 * PI) + NECK_EASE_MM;
    let drop = SHOULDER_DROP_MM;
    let ah = round_half_up(m.height * ARMHOLE_OF_HEIGHT);
    let hem_w = w.max((m.hip + ease * 0.6) / 4.0);
    let bodice = |depth: f64, ox: f64, back: bool| -> Vec<OutlinePoint> {
        let c = depth / 0.75;
        let mut pts = if back {
            vec![
                corner("hpsR", nk, 0.0),
                curve("nkR", nk * 0.5, depth),
                corner("cbk", 0.0, depth),
                curve("nkL", -nk * 0.5, depth),
            ]
        } else {
            vec![
                corner("hpsR", nk, 0.0),
                curve("nkR", nk * 0.82, c),
                curve("nkL", -nk * 0.82, c),
            ]
        };
        pts.extend([
            corner("hpsL", -nk, 0.0),
            corner("shL", -sh, drop),
            curve("ahL1", -sh + 6.0, drop + ah * 0.55),
            curve("ahL2", -w + 14.0, drop + ah * 0.95),
            corner("uaL", -w, drop + ah),
            corner("hemL", -hem_w, len),
            corner("hemR", hem_w, len),
            corner("uaR", w, drop + ah),
            curve("ahR2", w - 14.0, drop + ah * 0.95),
            curve("ahR1", sh - 6.0, drop + ah * 0.55),
            corner("shR", sh, drop),
        ]);
        translate_points(&pts, Vec2::new(ox, 0.0))
    };
    let chest_h = tp.shoulder_y(nk) + 10.0;
    let mut f = new_piece(
        project,
        "Front",
        bodice(
            nonzero(opts.neck_depth).unwrap_or(FRONT_NECK_DEPTH_MM),
            0.0,
            false,
        ),
    );
    f.arr = Arrangement::slot("front", 70.0).at_height(chest_h - len / 2.0);
    let mut bk = new_piece(
        project,
        "Back",
        bodice(BACK_NECK_DEPTH_MM, 2.0 * w + BOARD_GAP_MM, true),
    );
    bk.arr = Arrangement::slot("back", 70.0).at_height(chest_h - len / 2.0);
    let armhole = |pts: &[OutlinePoint]| seg_len(pts, "uaR") + seg_len(pts, "shL");
    let ah_len = armhole(&f.pts) / 2.0 + armhole(&bk.pts) / 2.0;
    let neck_len = seg_len(&f.pts, "hpsR") + seg_len(&bk.pts, "hpsR") + seg_len(&bk.pts, "cbk");
    let f = project.add_piece(f);
    let bk = project.add_piece(bk);

    let s_len = nonzero(opts.sleeve).unwrap_or(SLEEVE_LENGTH_MM);
    let cap_h = round_half_up(m.height * CAP_OF_HEIGHT);
    let cap_pts = |hw: f64| {
        vec![
            corner("uaR", hw, cap_h),
            curve("cR", hw * 0.55, -cap_h * 0.42),
            curve("cL", -hw * 0.55, -cap_h * 0.42),
            corner("uaL", -hw, cap_h),
            corner("hemL", -hw * 0.9, cap_h + s_len),
            corner("hemR", hw * 0.9, cap_h + s_len),
        ]
    };
    let mut sw = ah_len * 0.42;
    for _ in 0..SOLVE_ITERATIONS {
        sw *= (ah_len + CAP_EASE_MM) / seg_len(&cap_pts(sw), "uaR");
    }
    let sx = 4.0 * w + 300.0;
    let sleeve_t = 15.0 + f64::midpoint(cap_h, s_len);
    let mut sl = new_piece(
        project,
        "Sleeve L",
        translate_points(&cap_pts(sw), Vec2::new(sx, 0.0)),
    );
    sl.arr = Arrangement::slot("armL", 55.0).along(sleeve_t);
    let sl = project.add_piece(sl);
    let mut sr = new_piece(
        project,
        "Sleeve R",
        translate_points(&cap_pts(sw), Vec2::new(sx + 2.0 * sw + 100.0, 0.0)),
    );
    sr.arr = Arrangement::slot("armR", 55.0).along(sleeve_t);
    let sr = project.add_piece(sr);

    let rib_l = neck_len * RIB_LENGTH_OF_NECK;
    let ry = len + 80.0;
    let mut rib = new_piece(
        project,
        "Neck Rib",
        vec![
            corner("a", 0.0, ry),
            corner("b", rib_l, ry),
            corner("c", rib_l, RIB_WIDTH_MM + ry),
            corner("d", 0.0, RIB_WIDTH_MM + ry),
        ],
    );
    rib.pd = 10.0;
    rib.fabric = FabricId::new("fab_rib");
    rib.arr = Arrangement::slot("neck", 12.0)
        .at_height(chest_h + 8.0)
        .angle(0.0);
    let rib = project.add_piece(rib);
    if !project.fabrics.contains_key(&FabricId::new("fab_rib")) {
        let mut fab = make_fabric(project.id_gen(), FabricPreset::Rib);
        fab.id = FabricId::new("fab_rib");
        "Rib Knit 1×1".clone_into(&mut fab.name);
        "#efeae1".clone_into(&mut fab.color);
        project.fabrics.insert(fab.id.clone(), fab);
    }

    let side = SeamSide::segment;
    project.add_seam(vec![side(&f, "hpsL", false)], vec![side(&bk, "shR", true)]);
    project.add_seam(vec![side(&f, "shR", true)], vec![side(&bk, "hpsL", false)]);
    project.add_seam(vec![side(&f, "uaL", false)], vec![side(&bk, "hemR", true)]);
    project.add_seam(vec![side(&f, "hemR", true)], vec![side(&bk, "uaL", false)]);
    project.add_seam(vec![side(&sl, "uaL", false)], vec![side(&sl, "hemR", true)]);
    project.add_seam(vec![side(&sr, "uaL", false)], vec![side(&sr, "hemR", true)]);
    project.add_seam(
        vec![side(&sl, "uaR", true)],
        vec![side(&f, "uaR", false), side(&bk, "shL", false)],
    );
    project.add_seam(
        vec![side(&sr, "uaR", false)],
        vec![side(&f, "shL", true), side(&bk, "uaR", true)],
    );
    project.add_seam(
        vec![side(&rib, "c", false)],
        vec![
            side(&bk, "cbk", false),
            side(&f, "hpsR", false),
            side(&bk, "hpsR", false),
        ],
    );
    project.add_seam(vec![side(&rib, "b", false)], vec![side(&rib, "d", true)]);

    let hem_stitch = StitchStyle {
        off: 18.0,
        len: 3.0,
        gap: 1.6,
        w: 0.9,
        color: "#3b3a36",
        rows: 2,
        row_gap: 6.0,
    };
    add_stitch(project, &f, "hemL", &hem_stitch);
    add_stitch(project, &bk, "hemL", &hem_stitch);
    add_pom(project, "Front neckline", &[(&f, "hpsR")]);
    add_pom(project, "Shoulder seam", &[(&f, "hpsL")]);
    add_pom(project, "Side seam", &[(&f, "uaL")]);
    add_pom(project, "Front hem width", &[(&f, "hemL")]);
    add_pom(project, "Sleeve cap", &[(&sl, "uaR")]);
    add_pom(project, "Sleeve hem", &[(&sl, "hemL")]);
    name_if_untitled(project, "Essential Tee");
}
