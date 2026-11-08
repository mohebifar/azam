//! SVG export of the pattern at 1:1 (1 user unit = 1 mm).

use std::fmt::Write as _;

use azam_doc::Project;
use azam_geom::{Vec2, flatten_open_line};

use crate::format::{fixed, slug};
use crate::model::{Bounds, cut_line, graded_outline, line_points, pieces_in_id_order};

/// Margin around the pattern, in mm.
const PAD_MM: f64 = 20.0;
/// Size of the drawing when the pattern is empty, in mm.
const EMPTY_SIZE_MM: f64 = 100.0;

/// The pattern as SVG, every piece in the active size: cut line solid, sewing line dashed,
/// internal lines dotted and the piece name at the centre, one group per piece.
#[must_use]
pub fn export_svg(project: &Project) -> String {
    let items: Vec<_> = pieces_in_id_order(project)
        .into_iter()
        .map(|pc| {
            let ol = graded_outline(project, pc, None);
            let cut = cut_line(pc, &ol);
            (pc, ol, cut)
        })
        .collect();
    let all: Vec<Vec2> = items.iter().flat_map(|(_, _, cut)| cut.iter().copied()).collect();
    let bb = if all.is_empty() {
        Bounds::of([Vec2::ZERO, Vec2::splat(EMPTY_SIZE_MM)])
    } else {
        Bounds::of(all)
    };
    let points = |pts: &[Vec2]| -> String {
        pts.iter()
            .map(|p| {
                format!(
                    "{},{}",
                    fixed(p.x - bb.x0 + PAD_MM, 2),
                    fixed(p.y - bb.y0 + PAD_MM, 2)
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (w, h) = (fixed(bb.w + 2.0 * PAD_MM, 1), fixed(bb.h + 2.0 * PAD_MM, 1));
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n"
    );
    for (pc, ol, cut) in &items {
        let _ = write!(s, "<g id=\"{}\">", slug(&pc.name));
        let _ = write!(
            s,
            "<polygon points=\"{}\" fill=\"none\" stroke=\"#000\" stroke-width=\"0.5\"/>",
            points(cut)
        );
        let _ = write!(
            s,
            "<polygon points=\"{}\" fill=\"none\" stroke=\"#000\" stroke-width=\"0.3\" stroke-dasharray=\"3 2\"/>",
            points(&ol.poly)
        );
        for il in &pc.internals {
            let _ = write!(
                s,
                "<polyline points=\"{}\" fill=\"none\" stroke=\"#555\" stroke-width=\"0.3\" stroke-dasharray=\"1 1\"/>",
                points(&flatten_open_line(&line_points(il)))
            );
        }
        let b = Bounds::of(ol.poly.iter().copied());
        let name: String = pc.name.chars().filter(|&c| c != '<' && c != '&').collect();
        let _ = write!(
            s,
            "<text x=\"{}\" y=\"{}\" font-size=\"8\" text-anchor=\"middle\" font-family=\"Helvetica\">{name}</text>",
            fixed(b.cx - bb.x0 + PAD_MM, 1),
            fixed(b.cy - bb.y0 + PAD_MM, 1)
        );
        s.push_str("</g>\n");
    }
    s.push_str("</svg>");
    s
}
