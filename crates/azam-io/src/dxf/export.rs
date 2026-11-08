//! DXF export in the AAMA style: one block per piece with the cut line, sewing line, internal
//! lines, grain line, notches and annotation text on the conventional layers.

use azam_doc::{Piece, Project};
use azam_geom::{Vec2, flatten_open_line, signed_area};

use super::layer;
use crate::format::{fixed, number, slug};
use crate::model::{
    Bounds, DEG, cut_line, graded_outline, line_points_closed, or_default, pieces_in_id_order, qty,
};

/// Decimals written for coordinates.
const COORD_DIGITS: usize = 3;
/// Half length of the grain line, as a fraction of the piece height.
const GRAIN_HALF_LENGTH: f64 = 0.35;
/// Notches reach this far past the seam allowance, in mm.
const NOTCH_OVERSHOOT_MM: f64 = 3.0;
/// Annotation text starts this far left of the piece centre, in mm.
const TEXT_INSET_MM: f64 = 40.0;
/// Text height of the piece name, in mm.
const NAME_TEXT_MM: f64 = 8.0;
/// Text height of the other annotations, in mm.
const TEXT_MM: f64 = 6.0;
/// Header value of `$INSUNITS` for millimetres.
const INSUNITS_MM: u32 = 4;
/// Header value of `$MEASUREMENT` for metric.
const MEASUREMENT_METRIC: u32 = 1;

/// Group-code/value pairs, one per line each.
struct Writer {
    out: Vec<String>,
}

impl Writer {
    fn pair(&mut self, code: u32, value: impl Into<String>) {
        self.out.push(code.to_string());
        self.out.push(value.into());
    }

    fn xy(&mut self, code_x: u32, p: Vec2) {
        // DXF y runs up; pattern y runs down.
        self.pair(code_x, fixed(p.x, COORD_DIGITS));
        self.pair(code_x + 10, fixed(-p.y, COORD_DIGITS));
        self.pair(code_x + 20, "0");
    }

    fn polyline(&mut self, pts: &[Vec2], layer: &str, closed: bool) {
        if pts.len() < 2 {
            return;
        }
        self.pair(0, "POLYLINE");
        self.pair(8, layer);
        self.pair(66, "1");
        self.pair(70, if closed { "1" } else { "0" });
        for &p in pts {
            self.pair(0, "VERTEX");
            self.pair(8, layer);
            self.xy(10, p);
        }
        self.pair(0, "SEQEND");
        self.pair(8, layer);
    }

    fn line(&mut self, a: Vec2, b: Vec2, layer: &str) {
        self.pair(0, "LINE");
        self.pair(8, layer);
        self.xy(10, a);
        self.xy(11, b);
    }

    fn text(&mut self, at: Vec2, text: &str, layer: &str, height: f64) {
        self.pair(0, "TEXT");
        self.pair(8, layer);
        self.xy(10, at);
        self.pair(40, number(height));
        self.pair(1, text);
    }

    fn section(&mut self, name: &str) {
        self.pair(0, "SECTION");
        self.pair(2, name);
    }

    fn end_section(&mut self) {
        self.pair(0, "ENDSEC");
    }
}

/// The pattern as DXF text, in mm, every piece in the active size.
///
/// Each piece is a block named after it (`FRONT_1`, `BACK_2`…) holding the cut line on layer 1,
/// the sewing line on layer 14, internal lines on layer 8, the grain line on layer 7, notches on
/// layer 4 and the piece name, size, material and quantity as text; the entities section inserts
/// every block at the origin.
#[must_use]
pub fn export_dxf(project: &Project) -> String {
    write_dxf(project, std::slice::from_ref(&project.active_size), false)
}

/// The graded nest as DXF text, in mm: every piece in every size of the project's size run, as
/// [`export_dxf`] draws one size, each size its own block (`FRONT_1_M`) and all inserted at the
/// origin, so the sizes lie over each other as graders and cutting rooms read them.
#[must_use]
pub fn export_dxf_graded(project: &Project) -> String {
    let sizes = if project.sizes.is_empty() {
        vec![project.active_size.clone()]
    } else {
        project.sizes.clone()
    };
    write_dxf(project, &sizes, true)
}

/// The DXF of `project` with each piece in each of `sizes`; `suffix` names the blocks by size.
fn write_dxf(project: &Project, sizes: &[azam_geom::SizeName], suffix: bool) -> String {
    let mut w = Writer { out: Vec::new() };
    w.section("HEADER");
    w.pair(9, "$INSUNITS");
    w.pair(70, INSUNITS_MM.to_string());
    w.pair(9, "$MEASUREMENT");
    w.pair(70, MEASUREMENT_METRIC.to_string());
    w.end_section();

    w.section("BLOCKS");
    let mut names = Vec::new();
    for (i, pc) in pieces_in_id_order(project).into_iter().enumerate() {
        for size in sizes {
            let mut name = format!("{}_{}", slug(&pc.name).to_uppercase(), i + 1);
            if suffix {
                name = format!("{name}_{}", slug(&size.0).to_uppercase());
            }
            piece_block(&mut w, project, pc, &name, size);
            names.push(name);
        }
    }
    w.end_section();

    w.section("ENTITIES");
    for name in &names {
        w.pair(0, "INSERT");
        w.pair(8, "0");
        w.pair(2, name.as_str());
        w.pair(10, "0");
        w.pair(20, "0");
        w.pair(30, "0");
    }
    w.end_section();
    w.pair(0, "EOF");
    w.out.join("\n")
}

fn piece_block(w: &mut Writer, project: &Project, pc: &Piece, name: &str, size: &azam_geom::SizeName) {
    let ol = graded_outline(project, pc, Some(size));
    w.pair(0, "BLOCK");
    w.pair(8, "0");
    w.pair(2, name);
    w.pair(70, "0");
    w.pair(10, "0");
    w.pair(20, "0");
    w.pair(30, "0");
    w.pair(3, name);

    w.polyline(&cut_line(pc, &ol), layer::CUT, true);
    w.polyline(&ol.poly, layer::SEW, true);
    for il in &pc.internals {
        w.polyline(
            &flatten_open_line(&line_points_closed(il)),
            layer::INTERNAL,
            false,
        );
    }

    let bb = Bounds::of(ol.poly.iter().copied());
    let g = or_default(pc.grain, 0.0) * DEG;
    let half = bb.h * GRAIN_HALF_LENGTH;
    w.line(
        Vec2::new(bb.cx - g.sin() * -half, bb.cy - g.cos() * half),
        Vec2::new(bb.cx + g.sin() * -half, bb.cy + g.cos() * half),
        layer::GRAIN,
    );

    // Notches run from the sewing line straight out through the allowance.
    let outward = if signed_area(&ol.poly) > 0.0 { -1.0 } else { 1.0 };
    let reach = or_default(pc.sa, 0.0) + NOTCH_OVERSHOOT_MM;
    for n in &pc.notches {
        let Some(k) = ol.segment_index(&n.s) else {
            continue;
        };
        let p = ol.segments[k].point_at(n.t.unwrap_or(0.0));
        let (nx, ny) = (-p.tangent.y, p.tangent.x);
        w.line(
            p.pos,
            Vec2::new(p.pos.x - nx * reach * outward, p.pos.y - ny * reach * outward),
            layer::NOTCH,
        );
    }

    let material = project.fabrics.get(&pc.fabric).map_or("", |f| f.name.as_str());
    let at = |dy: f64| Vec2::new(bb.cx - TEXT_INSET_MM, bb.cy + dy);
    w.text(
        at(0.0),
        &format!("Piece Name: {}", pc.name),
        layer::CUT,
        NAME_TEXT_MM,
    );
    w.text(at(12.0), &format!("Size: {}", size.0), layer::CUT, TEXT_MM);
    w.text(at(22.0), &format!("Material: {material}"), layer::CUT, TEXT_MM);
    w.text(
        at(32.0),
        &format!("Quantity: {}", number(qty(pc))),
        layer::CUT,
        TEXT_MM,
    );
    w.pair(0, "ENDBLK");
    w.pair(8, "0");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_graded_nest_holds_every_size_of_every_piece() {
        let mut p = azam_doc::from_template(azam_doc::Template::Tee, azam_doc::Sex::Female, None, None, 0.0);
        p.ensure_ids_seeded();
        let pieces = p.pieces.len();
        let sizes = p.sizes.len();
        let dxf = export_dxf_graded(&p);
        let blocks = dxf.lines().filter(|l| *l == "BLOCK").count();
        assert_eq!(blocks, pieces * sizes);
        assert!(dxf.contains("Size: XS") && dxf.contains("Size: XL"));
        // one size alone is what export_dxf writes
        assert_eq!(export_dxf(&p).lines().filter(|l| *l == "BLOCK").count(), pieces);
    }
}
