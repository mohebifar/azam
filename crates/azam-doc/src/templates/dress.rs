//! Sleeveless A-line shift dress.

use std::f64::consts::PI;

use azam_geom::Vec2;

use super::{
    DraftOptions, add_pom, assign_fabric, body, corner, curve, name_if_untitled, new_piece, nonzero,
};
use crate::fabric::{FabricPreset, make_fabric};
use crate::ids::FabricId;
use crate::num::round_half_up;
use crate::piece::{Arrangement, MAIN_FABRIC, translate_points};
use crate::project::{Project, SeamSide};

/// Ease over the bust, in mm.
const EASE_MM: f64 = 70.0;
/// Default length as a fraction of body height.
const LENGTH_OF_HEIGHT: f64 = 0.56;
/// Shoulder point inside half the shoulder width (sleeveless), in mm.
const SHOULDER_INSET_MM: f64 = 22.0;
/// Neck width beyond the neck radius, in mm.
const NECK_EASE_MM: f64 = 22.0;
/// Shoulder drop, in mm.
const SHOULDER_DROP_MM: f64 = 40.0;
/// Armhole depth as a fraction of body height.
const ARMHOLE_OF_HEIGHT: f64 = 0.105;
/// Ease over the waist, in mm.
const WAIST_EASE_MM: f64 = 90.0;
/// Ease over the hip, in mm.
const HIP_EASE_MM: f64 = 70.0;
/// Hem flare per mm of length.
const FLARE_PER_LENGTH: f64 = 0.12;
/// Default front neck depth, in mm.
const FRONT_NECK_DEPTH_MM: f64 = 95.0;
/// Back neck depth, in mm.
const BACK_NECK_DEPTH_MM: f64 = 30.0;

/// Drafts the dress into `project`.
pub fn draft_dress(project: &mut Project, opts: &DraftOptions<'_>) {
    let m = project.avatar.body;
    let len = nonzero(opts.length).unwrap_or_else(|| round_half_up(m.height * LENGTH_OF_HEIGHT));
    let body = body(project, opts);
    let tp = body.get();
    let w = (m.bust + EASE_MM) / 4.0;
    let sh = m.shoulder / 2.0 - SHOULDER_INSET_MM;
    let nk = m.neck / (2.0 * PI) + NECK_EASE_MM;
    let drop = SHOULDER_DROP_MM;
    let ah = round_half_up(m.height * ARMHOLE_OF_HEIGHT);
    let ww = (m.waist + WAIST_EASE_MM) / 4.0;
    let hw = (m.hip + HIP_EASE_MM) / 4.0;
    let flare = hw + len * FLARE_PER_LENGTH;
    let top = tp.shoulder_y(nk) + 10.0;
    let lm = tp.landmarks();
    let yw = round_half_up(top - lm.waist);
    let yh = round_half_up(top - lm.hip);
    let panel = |depth: f64, ox: f64| {
        let c = depth / 0.75;
        translate_points(
            &[
                corner("hpsR", nk, 0.0),
                curve("nkR", nk * 0.8, c),
                curve("nkL", -nk * 0.8, c),
                corner("hpsL", -nk, 0.0),
                corner("shL", -sh, drop),
                curve("ahL1", -sh + 2.0, drop + ah * 0.6),
                curve("ahL2", -w + 18.0, drop + ah),
                corner("uaL", -w, drop + ah),
                curve("wL", -ww - 6.0, yw),
                corner("hipL", -hw, yh),
                corner("hemL", -flare, len),
                curve("hemC", 0.0, len + 8.0),
                corner("hemR", flare, len),
                corner("hipR", hw, yh),
                curve("wR", ww + 6.0, yw),
                corner("uaR", w, drop + ah),
                curve("ahR2", w - 18.0, drop + ah),
                curve("ahR1", sh - 2.0, drop + ah * 0.6),
                corner("shR", sh, drop),
            ],
            Vec2::new(ox, 0.0),
        )
    };
    let mut f = new_piece(
        project,
        "Front",
        panel(nonzero(opts.neck_depth).unwrap_or(FRONT_NECK_DEPTH_MM), 0.0),
    );
    f.arr = Arrangement::slot("front", 75.0).at_height(top - len / 2.0);
    let f = project.add_piece(f);
    let mut bk = new_piece(project, "Back", panel(BACK_NECK_DEPTH_MM, 2.0 * flare + 120.0));
    bk.arr = Arrangement::slot("back", 75.0).at_height(top - len / 2.0);
    let bk = project.add_piece(bk);

    let side = SeamSide::segment;
    project.add_seam(vec![side(&f, "hpsL", false)], vec![side(&bk, "shR", true)]);
    project.add_seam(vec![side(&f, "shR", true)], vec![side(&bk, "hpsL", false)]);
    project.add_seam(
        vec![side(&f, "uaL", false), side(&f, "hipL", false)],
        vec![side(&bk, "hipR", true), side(&bk, "hemR", true)],
    );
    project.add_seam(
        vec![side(&f, "hipR", true), side(&f, "hemR", true)],
        vec![side(&bk, "uaL", false), side(&bk, "hipL", false)],
    );
    add_pom(project, "Front neckline", &[(&f, "hpsR")]);
    add_pom(project, "Side seam", &[(&f, "uaL"), (&f, "hipL")]);
    add_pom(project, "Front hem", &[(&f, "hemL")]);
    let mut fab = make_fabric(project.id_gen(), FabricPreset::Linen);
    fab.id = FabricId::new(MAIN_FABRIC);
    "Linen".clone_into(&mut fab.name);
    "#b9c3b2".clone_into(&mut fab.color);
    assign_fabric(project, fab);
    name_if_untitled(project, "Linen Shift Dress");
}
