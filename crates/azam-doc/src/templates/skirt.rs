//! A-line skirt with waistband.

use azam_geom::Vec2;

use super::{
    DraftOptions, add_pom, assign_fabric, corner, curve, name_if_untitled, new_piece, nonzero, seg_len,
};
use crate::fabric::{FabricPreset, make_fabric};
use crate::ids::FabricId;
use crate::num::round_half_up;
use crate::piece::{Arrangement, MAIN_FABRIC, translate_points};
use crate::project::{Project, SeamSide};

/// Default length as a fraction of body height.
const LENGTH_OF_HEIGHT: f64 = 0.34;
/// Waist height as a fraction of body height, when no body profile is given.
const WAIST_OF_HEIGHT: f64 = 0.625;
/// Ease over the waist, in mm.
const WAIST_EASE_MM: f64 = 20.0;
/// Ease over the hip, in mm.
const HIP_EASE_MM: f64 = 50.0;
/// Hip line below the waist, in mm.
const HIP_DEPTH_MM: f64 = 190.0;
/// Hem flare per mm of length.
const FLARE_PER_LENGTH: f64 = 0.2;
/// Waistband width, in mm.
const WAISTBAND_MM: f64 = 36.0;
/// Overlap added to the waistband length, in mm.
const WAISTBAND_OVERLAP_MM: f64 = 4.0;

/// Drafts the skirt into `project`.
pub fn draft_skirt(project: &mut Project, opts: &DraftOptions<'_>) {
    let m = project.avatar.body;
    let len = nonzero(opts.length).unwrap_or_else(|| round_half_up(m.height * LENGTH_OF_HEIGHT));
    let ww = (m.waist + WAIST_EASE_MM) / 4.0;
    let hw = (m.hip + HIP_EASE_MM) / 4.0;
    let hip = HIP_DEPTH_MM;
    let flare = hw + len * FLARE_PER_LENGTH;
    let panel = |ox: f64| {
        translate_points(
            &[
                corner("wR", ww, 0.0),
                curve("wC", 0.0, 8.0),
                corner("wL", -ww, 0.0),
                curve("hL", -hw - 2.0, hip * 0.7),
                corner("hipL", -hw, hip),
                corner("hemL", -flare, len),
                curve("hemC", 0.0, len + 14.0),
                corner("hemR", flare, len),
                corner("hipR", hw, hip),
                curve("hR", hw + 2.0, hip * 0.7),
            ],
            Vec2::new(ox, 0.0),
        )
    };
    let waist_y = opts
        .profile
        .map_or(m.height * WAIST_OF_HEIGHT, |p| p.landmarks().waist);
    let top = waist_y + 18.0;
    let mut f = new_piece(project, "Skirt Front", panel(0.0));
    f.arr = Arrangement::slot("front", 70.0).at_height(top - len / 2.0);
    let wl = seg_len(&f.pts, "wR") * 2.0 + WAISTBAND_OVERLAP_MM;
    let f = project.add_piece(f);
    let mut bk = new_piece(project, "Skirt Back", panel(2.0 * flare + 120.0));
    bk.arr = Arrangement::slot("back", 70.0).at_height(top - len / 2.0);
    let bk = project.add_piece(bk);
    let bw = WAISTBAND_MM;
    let mut wb = new_piece(
        project,
        "Waistband",
        vec![
            corner("a", 0.0, -120.0),
            corner("b", wl, -120.0),
            corner("c", wl, bw - 120.0),
            corner("d", 0.0, bw - 120.0),
        ],
    );
    wb.pd = 12.0;
    wb.fabric = FabricId::new("fab_wb");
    wb.arr = Arrangement::slot("front", 8.0)
        .at_height(top + bw / 2.0 - 2.0)
        .angle(-65.0);
    let wb = project.add_piece(wb);
    if !project.fabrics.contains_key(&FabricId::new("fab_wb")) {
        let mut fab = make_fabric(project.id_gen(), FabricPreset::Wool);
        fab.id = FabricId::new("fab_wb");
        "Waistband — Wool".clone_into(&mut fab.name);
        "#3e3a35".clone_into(&mut fab.color);
        project.fabrics.insert(fab.id.clone(), fab);
    }
    let side = SeamSide::segment;
    project.add_seam(
        vec![side(&f, "wL", false), side(&f, "hipL", false)],
        vec![side(&bk, "hipR", true), side(&bk, "hemR", true)],
    );
    project.add_seam(
        vec![side(&f, "hipR", true), side(&f, "hemR", true)],
        vec![side(&bk, "wL", false), side(&bk, "hipL", false)],
    );
    project.add_seam(
        vec![side(&wb, "c", false)],
        vec![side(&f, "wR", false), side(&bk, "wR", false)],
    );
    project.add_seam(vec![side(&wb, "b", false)], vec![side(&wb, "d", true)]);
    let mut fab = make_fabric(project.id_gen(), FabricPreset::Wool);
    fab.id = FabricId::new(MAIN_FABRIC);
    "Wool Suiting".clone_into(&mut fab.name);
    "#7a6a58".clone_into(&mut fab.color);
    "twill".clone_into(&mut fab.weave);
    assign_fabric(project, fab);
    add_pom(project, "Waist (half)", &[(&f, "wR")]);
    add_pom(project, "Side seam", &[(&f, "wL"), (&f, "hipL")]);
    add_pom(project, "Hem (half)", &[(&f, "hemL")]);
    name_if_untitled(project, "A-line Skirt");
}
