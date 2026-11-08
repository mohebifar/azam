//! A nested marker as DXF: the fabric's outline and every laid piece's cut line with its name
//! and size, in mm, as cutting rooms and plotters take a marker.

use azam_geom::Vec2;

use super::export::{INSUNITS_MM, MEASUREMENT_METRIC, TEXT_MM, Writer};
use super::layer;
use crate::marker::Marker;

/// Layer of the fabric's outline.
const FABRIC_LAYER: &str = "MARKER";

/// `marker` as DXF text (x along the length, y across the width, in mm).
#[must_use]
pub fn marker_dxf(marker: &Marker) -> String {
    let mut w = Writer { out: Vec::new() };
    w.section("HEADER");
    w.pair(9, "$INSUNITS");
    w.pair(70, INSUNITS_MM.to_string());
    w.pair(9, "$MEASUREMENT");
    w.pair(70, MEASUREMENT_METRIC.to_string());
    w.end_section();
    w.section("ENTITIES");
    let (l, h) = (marker.length_mm, marker.width_mm);
    w.polyline(
        &[Vec2::ZERO, Vec2::new(l, 0.0), Vec2::new(l, h), Vec2::new(0.0, h)],
        FABRIC_LAYER,
        true,
    );
    for p in &marker.placements {
        w.polyline(&p.outline, layer::CUT, true);
        let n = p.outline.len().max(1) as f64;
        let centre = p.outline.iter().fold(Vec2::ZERO, |a, &q| a + q) * (1.0 / n);
        w.text(centre, &format!("{} {}", p.name, p.size.0), layer::CUT, TEXT_MM);
    }
    w.end_section();
    w.pair(0, "EOF");
    w.out.join("\n")
}
