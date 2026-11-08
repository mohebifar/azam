//! Sleeveless A-line shift dress.

use std::f64::consts::PI;

use azam_geom::{PointId, SideDirection, Vec2};

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
/// Width of each side bust dart (what it takes out of the side seam), in mm.
const BUST_DART_WIDTH_MM: f64 = 22.0;
/// The bust darts' centres sit this far below the underarm, in mm.
const BUST_DART_DROP_MM: f64 = 65.0;
/// Half the distance between the bust points, as a share of the bust girth.
const BUST_POINT_SHARE: f64 = 0.105;
/// The darts stop this far short of the bust point, in mm.
const BUST_DART_SHORT_MM: f64 = 25.0;
/// Shallowest bust dart, in mm.
const MIN_BUST_DART_MM: f64 = 60.0;

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

/// Gives the dress front a side bust dart on each side (azam's refinement of the reference
/// draft, which has none and so drapes flat over the bust).
///
/// The front's underarm is raised by the dart's width first, so after the dart is sewn the front
/// side seam is as long as the back's; then the side seams are taken off, the darts cut (each
/// sewn shut by its own seam) and the side seams sewn again around them.
pub(crate) fn add_bust_darts(project: &mut Project) {
    let Some(front) = project
        .pieces
        .values()
        .find(|pc| pc.name == "Front")
        .map(|pc| pc.id.clone())
    else {
        return;
    };
    let width = BUST_DART_WIDTH_MM;
    // lengthen the side seams by what the darts will take out
    if let Some(pc) = project.pieces.get_mut(&front) {
        for p in &mut pc.pts {
            if matches!(p.id.0.as_str(), "uaL" | "ahL2" | "uaR" | "ahR2") {
                p.pos.y -= width;
            }
        }
    }
    let m = project.avatar.body;
    let w = (m.bust + EASE_MM) / 4.0;
    let depth = (w - m.bust * BUST_POINT_SHARE - BUST_DART_SHORT_MM).max(MIN_BUST_DART_MM);
    let saved = std::mem::take(&mut project.sewing);
    // (segment, whether it runs from the underarm down)
    let mut splits = Vec::new();
    for (seg, from_underarm) in [("uaL", true), ("hipR", false)] {
        let Some(len) = project.pieces.get(&front).and_then(|pc| {
            let ol = pc.outline();
            ol.segment_index(&PointId::new(seg)).map(|k| ol.segments[k].len)
        }) else {
            continue;
        };
        let along = (BUST_DART_DROP_MM + width / 2.0) / len;
        let t = if from_underarm { along } else { 1.0 - along };
        if crate::add_dart(project, &front, &PointId::new(seg), t, width, depth).is_err() {
            continue;
        }
        // after the dart, the segment ends at its first leg; the edge goes on from the second
        let rest = project.pieces.get(&front).and_then(|pc| {
            let ol = pc.outline();
            let k = ol.segment_index(&PointId::new(seg))?;
            Some(ol.segments.get(k + 3)?.id.clone())
        });
        if let Some(rest) = rest {
            splits.push((PointId::new(seg), rest));
        }
    }
    let darts = std::mem::replace(&mut project.sewing, saved);
    for seam in project.sewing.values_mut() {
        for group in [&mut seam.a, &mut seam.b] {
            let mut out = Vec::with_capacity(group.len() + 1);
            for side in group.drain(..) {
                let whole = side.side.a.segment == side.side.b.segment;
                let split = splits
                    .iter()
                    .find(|(seg, _)| side.piece == front && whole && side.side.a.segment == *seg);
                match split {
                    Some((seg, rest)) => {
                        let backward = side.side.direction == SideDirection::Backward;
                        let (first, second) = (
                            SeamSide::segment(&front, &seg.0, backward),
                            SeamSide::segment(&front, &rest.0, backward),
                        );
                        if backward {
                            out.extend([second, first]);
                        } else {
                            out.extend([first, second]);
                        }
                    }
                    None => out.push(side),
                }
            }
            *group = out;
        }
    }
    project.sewing.extend(darts);
}
