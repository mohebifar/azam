//! The tech-pack PDF: overview, bill of materials, points of measure, pattern pieces and sewing,
//! colorways and open review notes, on five landscape A4 pages.

use azam_doc::Project;
use serde_json::Value;

use super::A4_MM;
use super::canvas::{Canvas, Page, Rgb, Style, TextOpts};
use super::metrics::FontStyle;
use super::writer::{PdfInfo, write_pdf};
use crate::format::{fixed, hex_rgb, number, utf16_len, utf16_prefix};
use crate::model::{extra_str, extra_truthy, pieces_in_id_order, sorted_by_key};
use crate::techpack::{TechData, tech_data};

/// Page margin, mm.
const M: f64 = 14.0;
/// Table row height, mm.
const ROW_H: f64 = 6.2;
/// Rows stop this far above the page bottom, mm.
const BOTTOM: f64 = 12.0;
/// Average character width used to truncate table cells, mm.
const CHAR_W: f64 = 1.65;
/// Text colours.
const INK: Rgb = [28, 29, 25];
const MUTE: Rgb = [120, 120, 125];
/// Page background.
const PAPER: Rgb = [250, 249, 247];
/// Column of the overview facts, mm from the left.
const FACTS_X: f64 = 238.0;
/// Millimetres per centimetre.
const MM_PER_CM: f64 = 10.0;

/// Text fields printed on the tech pack.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TechPackInfo {
    /// Season, such as `SS27`.
    pub season: String,
    /// Designer.
    pub designer: String,
    /// Currency printed after the estimated cost.
    pub currency: String,
    /// Free-form notes for the overview page.
    pub notes: String,
    /// Date printed in page headers.
    pub date: String,
}

/// A table column.
struct Col {
    h: String,
    w: f64,
    right: bool,
    swatch: bool,
}

fn col(h: &str, w: f64) -> Col {
    Col {
        h: h.to_owned(),
        w,
        right: false,
        swatch: false,
    }
}

fn rcol(h: &str, w: f64) -> Col {
    Col {
        right: true,
        ..col(h, w)
    }
}

fn swatch(h: &str, w: f64) -> Col {
    Col {
        swatch: true,
        ..col(h, w)
    }
}

/// A stored value as cell text (numbers printed plainly, missing values empty).
fn cell_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.as_f64().map(number).unwrap_or_default(),
        Some(other) => other.to_string(),
    }
}

struct TechPdf<'a> {
    project: &'a Project,
    info: &'a TechPackInfo,
    canvas: Canvas,
    w: f64,
    h: f64,
}

impl TechPdf<'_> {
    fn header(&mut self, title: &str, page: usize) {
        let c = &mut self.canvas;
        c.set_fill_color(PAPER);
        c.rect(0.0, 0.0, self.w, self.h, Style::Fill);
        c.set_text_color(INK);
        c.set_font(FontStyle::Bold);
        c.set_font_size(9.0);
        c.text("AZAM · TECH PACK", M, 10.0, TextOpts::left());
        c.set_font(FontStyle::Normal);
        c.set_text_color(MUTE);
        let line = format!(
            "{}  ·  {}  ·  base size {}  ·  {}",
            self.project.meta.name, self.info.season, self.project.base_size.0, self.info.date
        );
        c.text(&line, M + 48.0, 10.0, TextOpts::left());
        c.text(&format!("p. {page}"), self.w - M, 10.0, TextOpts::right());
        c.set_draw_color([220, 218, 214]);
        c.line(M, 13.0, self.w - M, 13.0);
        c.set_text_color(INK);
        c.set_font(FontStyle::Bold);
        c.set_font_size(15.0);
        c.text(title, M, 23.0, TextOpts::left());
    }

    /// Draws a table; returns the y below its last row.
    fn table(&mut self, x: f64, y: f64, cols: &[Col], rows: &[Vec<String>]) -> f64 {
        let h = self.h;
        let c = &mut self.canvas;
        c.set_font_size(7.5);
        c.set_font(FontStyle::Bold);
        c.set_text_color(MUTE);
        let mut cx = x;
        for col in cols {
            let (ax, opts) = if col.right {
                (cx + col.w - 1.0, TextOpts::right())
            } else {
                (cx, TextOpts::left())
            };
            c.text(&col.h.to_uppercase(), ax, y, opts);
            cx += col.w;
        }
        let total: f64 = cols.iter().map(|col| col.w).sum();
        c.set_draw_color([200, 198, 194]);
        c.line(x, y + 1.6, x + total, y + 1.6);
        c.set_font(FontStyle::Normal);
        c.set_text_color(INK);
        c.set_font_size(8.5);
        let mut yy = y + ROW_H;
        for row in rows {
            if yy > h - BOTTOM {
                continue;
            }
            cx = x;
            for (col, v) in cols.iter().zip(row) {
                if col.swatch && !v.is_empty() {
                    if let Some(rgb) = hex_rgb(v) {
                        c.set_fill_color(rgb);
                        c.rect(cx, yy - 3.4, 4.0, 4.0, Style::Fill);
                    }
                } else {
                    let max_c = (col.w / CHAR_W).floor() as usize;
                    let t = if utf16_len(v) > max_c {
                        format!("{}…", utf16_prefix(v, max_c.saturating_sub(1)))
                    } else {
                        v.clone()
                    };
                    let (ax, opts) = if col.right {
                        (cx + col.w - 1.0, TextOpts::right())
                    } else {
                        (cx, TextOpts::left())
                    };
                    c.text(&t, ax, yy, opts);
                }
                cx += col.w;
            }
            c.set_draw_color([236, 234, 230]);
            c.line(x, yy + 1.8, cx, yy + 1.8);
            yy += ROW_H;
        }
        yy
    }

    fn overview(&mut self, d: &TechData) {
        let project = self.project;
        let info = self.info;
        self.header(&project.meta.name, 1);
        self.canvas.set_text_color(INK);
        self.canvas.set_font_size(9.0);
        let or_dash = |s: &str| {
            if s.is_empty() {
                "—".to_owned()
            } else {
                s.to_owned()
            }
        };
        let sizes: Vec<&str> = project.sizes.iter().map(|s| s.0.as_str()).collect();
        let colorway = project
            .current_colorway()
            .map(|c| c.name.as_str())
            .filter(|n| !n.is_empty())
            .unwrap_or("—");
        let cost = if d.total_cost == 0.0 || d.total_cost.is_nan() {
            "—".to_owned()
        } else {
            format!("{} {}", fixed(d.total_cost, 2), info.currency)
        };
        let facts = [
            ("Style", project.meta.name.clone()),
            ("Season", or_dash(&info.season)),
            ("Designer", or_dash(&info.designer)),
            ("Size range", sizes.join(" / ")),
            ("Base size", project.base_size.0.clone()),
            ("Colorway", colorway.to_owned()),
            ("Pieces", pieces_in_id_order(project).len().to_string()),
            ("Est. fabric cost", cost),
        ];
        let mut fy = 34.0;
        for (label, value) in facts {
            self.canvas.set_text_color(MUTE);
            self.canvas.set_font_size(7.5);
            self.canvas
                .text(&label.to_uppercase(), FACTS_X, fy, TextOpts::left());
            self.canvas.set_text_color(INK);
            self.canvas.set_font_size(10.0);
            self.canvas.text(&value, FACTS_X, fy + 5.0, TextOpts::left());
            fy += 13.0;
        }
        if !info.notes.is_empty() {
            self.canvas.set_font_size(8.5);
            self.canvas.set_text_color(INK);
            let lines = self.canvas.split_to_width(&info.notes, self.w - 2.0 * M);
            self.canvas.text_lines(&lines, M, 166.0, TextOpts::left());
        }
    }

    fn bill_of_materials(&mut self, d: &TechData) {
        self.canvas.add_page(self.w, self.h);
        self.header("Bill of materials", 2);
        let blank_zero = |v: f64| {
            if v == 0.0 || v.is_nan() {
                String::new()
            } else {
                number(v)
            }
        };
        let fabrics: Vec<Vec<String>> = d
            .fabrics
            .iter()
            .map(|f| {
                vec![
                    f.color.clone(),
                    f.name.clone(),
                    f.placement.clone(),
                    blank_zero(f.weight),
                    number(f.width),
                    fixed(f.area_gross, 3),
                    fixed(f.linear, 2),
                    blank_zero(f.price),
                    if f.cost == 0.0 || f.cost.is_nan() {
                        String::new()
                    } else {
                        fixed(f.cost, 2)
                    },
                ]
            })
            .collect();
        let y = self.table(
            M,
            32.0,
            &[
                swatch("", 7.0),
                col("Material", 55.0),
                col("Placement", 80.0),
                rcol("Weight g/m²", 22.0),
                rcol("Width mm", 20.0),
                rcol("Cut area m²", 22.0),
                rcol("Consumption m", 26.0),
                rcol("Price/m", 18.0),
                rcol("Cost", 18.0),
            ],
            &fabrics,
        );
        let trims: Vec<Vec<String>> = d
            .trims
            .iter()
            .map(|t| vec![t.color.clone(), t.name.clone(), number(t.qty), t.unit.to_owned()])
            .collect();
        self.table(
            M,
            y + 8.0,
            &[
                swatch("", 7.0),
                col("Trim", 135.0),
                rcol("Qty", 20.0),
                col("Unit", 20.0),
            ],
            &trims,
        );
    }

    fn points_of_measure(&mut self, d: &TechData) {
        let project = self.project;
        self.canvas.add_page(self.w, self.h);
        self.header("Points of measure (cm)", 3);
        let n = project.sizes.len().max(1) as f64;
        let mut cols = vec![col("POM", 70.0), rcol("Tol ±", 16.0)];
        for s in &project.sizes {
            let mark = if *s == project.base_size { " ●" } else { "" };
            cols.push(rcol(
                &format!("{}{mark}", s.0),
                26.0_f64.min((self.w - 2.0 * M - 86.0) / n),
            ));
        }
        let rows: Vec<Vec<String>> = d
            .poms
            .iter()
            .map(|p| {
                let mut row = vec![p.name.clone(), fixed(p.tol / MM_PER_CM, 1)];
                row.extend(project.sizes.iter().map(|s| {
                    p.vals
                        .get(s)
                        .map_or_else(|| "NaN".to_owned(), |v| fixed(v / MM_PER_CM, 1))
                }));
                row
            })
            .collect();
        self.table(M, 32.0, &cols, &rows);
    }

    fn patterns_and_sewing(&mut self, d: &TechData) {
        self.canvas.add_page(self.w, self.h);
        self.header("Pattern pieces & sewing", 4);
        let cm = |mm: f64| fixed(mm / MM_PER_CM, 1);
        let rows: Vec<Vec<String>> = d
            .patterns
            .iter()
            .map(|p| {
                vec![
                    p.name.clone(),
                    p.fabric.clone(),
                    number(p.qty),
                    format!("{} × {}", cm(p.w), cm(p.h)),
                    fixed(p.net, 0),
                    fixed(p.gross, 0),
                    cm(p.perim),
                    number(p.sa),
                    number(p.grain),
                ]
            })
            .collect();
        let y = self.table(
            M,
            32.0,
            &[
                col("Piece", 44.0),
                col("Fabric", 40.0),
                rcol("Qty", 10.0),
                rcol("W × L cm", 26.0),
                rcol("Net cm²", 20.0),
                rcol("Cut cm²", 22.0),
                rcol("Perim cm", 22.0),
                rcol("SA mm", 16.0),
                rcol("Grain°", 16.0),
            ],
            &rows,
        );
        let sewing: Vec<Vec<String>> = d
            .sewing
            .iter()
            .map(|s| {
                vec![
                    s.color.clone(),
                    s.n.to_string(),
                    s.a.clone(),
                    s.b.clone(),
                    cm(s.la),
                    cm(s.lb),
                    fixed(s.diff, 1),
                ]
            })
            .collect();
        self.table(
            M,
            y + 8.0,
            &[
                swatch("", 7.0),
                col("#", 8.0),
                col("Side A", 80.0),
                col("Side B", 80.0),
                rcol("A cm", 20.0),
                rcol("B cm", 20.0),
                rcol("Δ mm", 18.0),
            ],
            &sewing,
        );
    }

    fn colorways_and_notes(&mut self) {
        let project = self.project;
        self.canvas.add_page(self.w, self.h);
        self.header("Colorways & review notes", 5);
        let mut cws = sorted_by_key(&project.colorways);
        cws.sort_by_key(|c| c.order);
        let mut cols = vec![col("Material", 60.0)];
        cols.extend(cws.iter().map(|c| swatch(&c.name, 30.0)));
        let rows: Vec<Vec<String>> = project
            .fabrics
            .iter()
            .map(|(fid, fab)| {
                let mut row = vec![fab.name.clone()];
                row.extend(cws.iter().map(|c| {
                    c.over
                        .get(fid)
                        .and_then(|o| o.get("color"))
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map_or_else(|| fab.color.clone(), str::to_owned)
                }));
                row
            })
            .collect();
        self.table(M, 32.0, &cols, &rows);
        let comments: Vec<Vec<String>> = sorted_by_key(&project.comments)
            .into_iter()
            .filter(|c| !extra_truthy(&c.extra, "resolved"))
            .map(|c| {
                let n = if extra_truthy(&c.extra, "n") {
                    cell_text(c.extra.get("n"))
                } else {
                    String::new()
                };
                vec![
                    n,
                    c.p.as_ref()
                        .and_then(|p| project.pieces.get(p))
                        .map(|pc| pc.name.clone())
                        .unwrap_or_default(),
                    cell_text(c.extra.get("text")),
                    extra_str(&c.extra, "byName").unwrap_or("").to_owned(),
                ]
            })
            .collect();
        let y = 32.0 + 8.0 + (project.fabrics.len() + 1) as f64 * ROW_H + 8.0;
        self.table(
            M,
            y,
            &[
                col("#", 10.0),
                col("Pattern", 40.0),
                col("Note", 170.0),
                col("By", 40.0),
            ],
            &comments,
        );
    }
}

/// The pages of the tech-pack PDF as drawing operations.
#[must_use]
pub fn techpack_pdf_pages(project: &Project, info: &TechPackInfo) -> Vec<Page> {
    let [short, long] = A4_MM;
    let mut doc = TechPdf {
        project,
        info,
        canvas: Canvas::new(),
        w: long,
        h: short,
    };
    let d = tech_data(project);
    doc.canvas.add_page(long, short);
    doc.overview(&d);
    doc.bill_of_materials(&d);
    doc.points_of_measure(&d);
    doc.patterns_and_sewing(&d);
    doc.colorways_and_notes();
    doc.canvas.into_pages()
}

/// The tech-pack PDF file.
#[must_use]
pub fn techpack_pdf(project: &Project, info: &TechPackInfo) -> Vec<u8> {
    write_pdf(
        &techpack_pdf_pages(project, info),
        &PdfInfo {
            title: format!("{} — tech pack", project.meta.name),
        },
    )
}
