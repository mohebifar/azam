//! Measurements of a project: seam lengths, points of measure and piece areas.

use std::collections::HashMap;

use azam_geom::{Outline, SizeName, cut_outline, signed_area};
use indexmap::IndexMap;

use crate::ids::{PieceId, SeamId};
use crate::piece::Piece;
use crate::project::{Pom, Project, SeamSide};

/// Lengths of the two groups of sides of a seam, in mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeamLengths {
    /// Total length of the sides in group `a`.
    pub a: f64,
    /// Total length of the sides in group `b`.
    pub b: f64,
}

/// Length of every seam's two groups of sides in the active size, for checking that sewn edges
/// match. Sides on missing pieces or segments count as zero.
#[must_use]
pub fn sew_lengths(project: &Project) -> IndexMap<SeamId, SeamLengths> {
    let mut outlines: HashMap<PieceId, Outline> = HashMap::new();
    let mut total = |sides: &[SeamSide]| -> f64 {
        sides
            .iter()
            .map(|sd| {
                let Some(pc) = project.pieces.get(&sd.piece) else {
                    return 0.0;
                };
                let ol = outlines
                    .entry(sd.piece.clone())
                    .or_insert_with(|| pc.graded_outline(&project.active_size, &project.base_size));
                sd.side.range(ol).map_or(0.0, |r| r.length)
            })
            .sum()
    };
    project
        .sewing
        .iter()
        .map(|(id, sw)| {
            let a = total(&sw.a);
            let b = total(&sw.b);
            (id.clone(), SeamLengths { a, b })
        })
        .collect()
}

/// Value of a point of measure in `size`: the summed length of its segments, in mm.
#[must_use]
pub fn pom_value(project: &Project, pom: &Pom, size: &SizeName) -> f64 {
    pom.refs
        .iter()
        .filter_map(|r| {
            let pc = project.pieces.get(&r.p)?;
            let ol = pc.graded_outline(size, &project.base_size);
            let k = ol.segment_index(&r.s)?;
            Some(ol.segments[k].len)
        })
        .sum()
}

/// Area of a piece in `size` (the active size when `None`), in mm², inside the sewing line or,
/// with `with_allowance`, inside the cut line.
#[must_use]
pub fn piece_area(project: &Project, piece: &Piece, size: Option<&SizeName>, with_allowance: bool) -> f64 {
    let size = size.unwrap_or(&project.active_size);
    let ol = piece.graded_outline(size, &project.base_size);
    if !with_allowance {
        return signed_area(&ol.poly).abs();
    }
    let allowances = piece.allowances();
    signed_area(&cut_outline(&ol, |id| allowances.finish(id))).abs()
}
