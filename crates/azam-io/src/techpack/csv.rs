//! Tech pack as CSV: bill of materials, points of measure, pattern pieces and sewing.

use azam_doc::Project;

use super::data::{TechData, tech_data};
use crate::format::{fixed, number};

/// Millimetres per centimetre.
const MM_PER_CM: f64 = 10.0;

/// The tech pack of a project as CSV text (every cell quoted, rows separated by `\n`).
#[must_use]
pub fn techpack_csv(project: &Project) -> String {
    csv_from(project, &tech_data(project))
}

/// The CSV for already computed tables.
#[must_use]
pub fn csv_from(project: &Project, d: &TechData) -> String {
    let mut rows: Vec<Vec<String>> = vec![vec!["azam tech pack".into(), project.meta.name.clone()]];
    bill_of_materials(&mut rows, d);
    points_of_measure(&mut rows, project, d);
    pattern_pieces(&mut rows, d);
    sewing(&mut rows, d);
    rows.iter()
        .map(|r| r.iter().map(|v| quote(v)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\n")
}

fn cm(mm: f64) -> String {
    fixed(mm / MM_PER_CM, 1)
}

fn header(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|&c| c.to_owned()).collect()
}

/// A blank row, then the section title.
fn section(rows: &mut Vec<Vec<String>>, title: &str) {
    rows.push(vec![]);
    rows.push(vec![title.to_owned()]);
}

fn bill_of_materials(rows: &mut Vec<Vec<String>>, d: &TechData) {
    section(rows, "BILL OF MATERIALS");
    rows.push(header(&[
        "Type",
        "Item",
        "Colour",
        "Placement",
        "Width (mm)",
        "Consumption",
        "Unit",
        "Price/unit",
        "Cost",
    ]));
    for f in &d.fabrics {
        rows.push(vec![
            "Fabric".into(),
            f.name.clone(),
            f.color.clone(),
            f.placement.clone(),
            number(f.width),
            fixed(f.linear, 2),
            "m".into(),
            number(f.price),
            fixed(f.cost, 2),
        ]);
    }
    for t in &d.trims {
        rows.push(vec![
            "Trim".into(),
            t.name.clone(),
            t.color.clone(),
            String::new(),
            String::new(),
            number(t.qty),
            t.unit.into(),
            String::new(),
            String::new(),
        ]);
    }
}

fn points_of_measure(rows: &mut Vec<Vec<String>>, project: &Project, d: &TechData) {
    section(rows, "POINTS OF MEASURE (cm)");
    let mut head = header(&["POM", "Tol ±"]);
    head.extend(project.sizes.iter().map(|s| s.0.clone()));
    rows.push(head);
    for p in &d.poms {
        let mut row = vec![p.name.clone(), cm(p.tol)];
        row.extend(
            project
                .sizes
                .iter()
                .map(|s| p.vals.get(s).map_or_else(|| "NaN".to_owned(), |&v| cm(v))),
        );
        rows.push(row);
    }
}

fn pattern_pieces(rows: &mut Vec<Vec<String>>, d: &TechData) {
    section(rows, "PATTERN PIECES");
    rows.push(header(&[
        "Piece",
        "Fabric",
        "Qty",
        "Width (cm)",
        "Length (cm)",
        "Net area (cm²)",
        "Cut area (cm²)",
        "Perimeter (cm)",
        "SA (mm)",
        "Grain (°)",
    ]));
    for p in &d.patterns {
        rows.push(vec![
            p.name.clone(),
            p.fabric.clone(),
            number(p.qty),
            cm(p.w),
            cm(p.h),
            fixed(p.net, 0),
            fixed(p.gross, 0),
            cm(p.perim),
            number(p.sa),
            number(p.grain),
        ]);
    }
}

fn sewing(rows: &mut Vec<Vec<String>>, d: &TechData) {
    section(rows, "SEWING");
    rows.push(header(&[
        "#",
        "Side A",
        "Side B",
        "Length A (cm)",
        "Length B (cm)",
        "Difference (mm)",
    ]));
    for s in &d.sewing {
        rows.push(vec![
            s.n.to_string(),
            s.a.clone(),
            s.b.clone(),
            cm(s.la),
            cm(s.lb),
            fixed(s.diff, 1),
        ]);
    }
}

fn quote(v: &str) -> String {
    format!("\"{}\"", v.replace('"', "\"\""))
}
