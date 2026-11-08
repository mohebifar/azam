//! The Tech Pack workspace: the tables a factory needs — the pattern list, the seam check, the
//! materials and the points of measure — read straight from the project.

use azam_doc::{PieceState, Project};
use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::scene::parse_hex;

/// Seams whose sides differ by more than this are flagged, in mm.
const SEAM_TOLERANCE_MM: f64 = 3.0;

impl AzamApp {
    /// The Tech Pack editor's body.
    pub(crate) fn techpack_body(&self, area: crate::areas::AreaId) -> Div {
        let project = &self.session.project;
        div()
            .class("techpack")
            .id(format!("techpack-{area}"))
            .child(section("Pattern Pieces", pieces_table(project)))
            .child(section("Seam Check", seams_table(project)))
            .child(section("Materials", materials_table(project)))
            .child(section("Points of Measure (cm)", poms_table(project)))
    }
}

fn section(title: &'static str, body: Div) -> Div {
    div()
        .class("tp-section")
        .child(div().class("tp-title").child(title))
        .child(body)
}

fn header(cells: &[&str]) -> Div {
    thead().child(tr().children(cells.iter().map(|c| th().child((*c).to_owned()))))
}

fn pieces_table(project: &Project) -> Div {
    let mut pieces: Vec<_> = project.pieces.values().collect();
    pieces.sort_by_key(|p| p.order);
    let rows = pieces.iter().map(|piece| {
        let fabric = project
            .fabrics
            .get(&piece.fabric)
            .map_or("—", |f| f.name.as_str())
            .to_owned();
        let net = azam_doc::piece_area(project, piece, None, false) / 100.0;
        let cut = azam_doc::piece_area(project, piece, None, true) / 100.0;
        let perimeter = piece.outline().total / 10.0;
        let state = match piece.state {
            PieceState::Active => "Active",
            PieceState::Frozen => "Frozen",
            PieceState::Off => "Off",
        };
        tr().child(td().child(piece.name.clone()))
            .child(td().child(fabric))
            .child(td().child(format!("{net:.0}")))
            .child(td().child(format!("{cut:.0}")))
            .child(td().child(format!("{perimeter:.1}")))
            .child(td().child(format!("{:.0}", piece.sa)))
            .child(td().child(state))
    });
    div().class("tp-table").child(
        table()
            .child(header(&[
                "Piece",
                "Fabric",
                "Net cm²",
                "Cut cm²",
                "Perimeter cm",
                "SA mm",
                "Sim",
            ]))
            .child(tbody().children(rows)),
    )
}

fn seams_table(project: &Project) -> Div {
    let lengths = azam_doc::sew_lengths(project);
    let numbers = azam_doc::seam_numbers(project);
    let rows = project.sewing.values().map(|seam| {
        let name = |sides: &[azam_doc::SeamSide]| {
            sides
                .iter()
                .filter_map(|s| project.pieces.get(&s.piece).map(|p| p.name.clone()))
                .collect::<Vec<_>>()
                .join(" + ")
        };
        let (a, b) = lengths.get(&seam.id).map_or((0.0, 0.0), |l| (l.a, l.b));
        let diff = (a - b).abs();
        let ok = diff <= SEAM_TOLERANCE_MM;
        let number = numbers.get(&seam.id).copied().unwrap_or(0);
        let color = parse_hex(&seam.color).unwrap_or(0xffcc33);
        tr().child(td().child(div().class("tp-dot").bg(rgb(color)).child(number.to_string())))
            .child(td().child(name(&seam.a)))
            .child(td().child(name(&seam.b)))
            .child(td().child(format!("{:.1}", a / 10.0)))
            .child(td().child(format!("{:.1}", b / 10.0)))
            .child(
                td().class(if ok { "tp-ok" } else { "tp-warn" })
                    .child(format!("{:+.1} mm", a - b)),
            )
    });
    div().class("tp-table").child(
        table()
            .child(header(&["#", "Side A", "Side B", "A cm", "B cm", "Difference"]))
            .child(tbody().children(rows)),
    )
}

fn materials_table(project: &Project) -> Div {
    let rows = project.fabrics.iter().map(|(fid, fabric)| {
        let area: f64 = project
            .pieces
            .values()
            .filter(|p| p.fabric == *fid)
            .map(|p| azam_doc::piece_area(project, p, None, true))
            .sum::<f64>()
            / 1.0e6;
        let weight = area * fabric.phys.density;
        let color = parse_hex(&fabric.color).unwrap_or(0x888888);
        tr().child(td().child(div().class("swatch").bg(rgb(color))))
            .child(td().child(fabric.name.clone()))
            .child(td().child(fabric.preset.clone()))
            .child(td().child(format!("{:.0}", fabric.phys.density)))
            .child(td().child(format!("{area:.2}")))
            .child(td().child(format!("{weight:.0}")))
    });
    div().class("tp-table").child(
        table()
            .child(header(&[
                "",
                "Fabric",
                "Preset",
                "g/m²",
                "Cut area m²",
                "Weight g",
            ]))
            .child(tbody().children(rows)),
    )
}

fn poms_table(project: &Project) -> Div {
    if project.poms.is_empty() {
        return div()
            .class("pnote")
            .child("No points of measure in this project.");
    }
    let mut cols: Vec<&str> = vec!["Measure"];
    cols.extend(project.sizes.iter().map(|s| s.0.as_str()));
    let rows = project.poms.values().map(|pom| {
        let mut row = tr().child(td().child(pom.name.clone()));
        for size in &project.sizes {
            row = row.child(td().child(format!("{:.1}", azam_doc::pom_value(project, pom, size) / 10.0)));
        }
        row
    });
    div()
        .class("tp-table")
        .child(table().child(header(&cols)).child(tbody().children(rows)))
}
