//! The schedule pages of the pattern PDF: piece schedule, sewing schedule and dimension tables.

use azam_doc::sew_lengths;

use super::plan::{DIM_TABLE_ROW, DimBlock, PdfItem, SEAMS_PER_PAGE, dim_table_pages};
use super::render::{INK, MUTE, ROW_RULE, Renderer, SEAM_COLOR, TABLE_RULE, hex};
use super::sheets::{Col, M, col, rcol};
use crate::format::{area_with_unit, fixed, length_number, length_with_unit, number, unit_word};
use crate::model::qty;
use crate::pdf::canvas::{Rgb, Style, TextOpts, WHITE};
use crate::pdf::metrics::FontStyle;
use crate::techpack::side_names;

/// Colour of a seam-length mismatch worth attention.
const MISMATCH: Rgb = [200, 70, 50];
/// Seam-length differences above this are highlighted, in mm.
const MISMATCH_MM: f64 = 3.0;

impl Renderer<'_> {
    /// Sewing schedule: every seam with its number, both sides and their lengths.
    pub(super) fn sew_schedule(&mut self, paper: [f64; 2]) {
        let project = self.project;
        let units = self.units;
        let lens = sew_lengths(project);
        let mut list: Vec<_> = project.sewing.values().collect();
        list.sort_by_key(|sw| self.nums.get(&sw.id).copied().unwrap_or(0));
        let cols = [
            col("#", 10.0),
            col("Side A", 104.0),
            col("Side B", 104.0),
            rcol("A", 20.0),
            rcol("B", 20.0),
            rcol("Δ", 15.0),
        ];
        for (chunk_i, chunk) in list.chunks(SEAMS_PER_PAGE).enumerate() {
            let pg = self.new_page(paper[1], paper[0]);
            let title = if list.len() > SEAMS_PER_PAGE {
                format!("Sewing schedule ({})", chunk_i + 1)
            } else {
                "Sewing schedule".to_owned()
            };
            self.header(pg, Some(&title));
            let mut y = 32.0;
            let cx = self.table_header(&cols, M, y, 7.5, |c| {
                if c.h == "Δ" {
                    "DIFF".to_owned()
                } else {
                    c.h.to_uppercase()
                }
            });
            self.canvas.set_draw_color(TABLE_RULE);
            self.canvas.set_line_width(0.2);
            self.canvas.line(M, y + 1.8, cx, y + 1.8);
            for sw in chunk {
                y += 7.0;
                let (la, lb) = lens.get(&sw.id).map_or((0.0, 0.0), |l| (l.a, l.b));
                let color = if sw.color.is_empty() {
                    SEAM_COLOR
                } else {
                    sw.color.as_str()
                };
                self.canvas.set_fill_color(hex(color));
                self.canvas.circle(M + 2.6, y - 1.1, 2.3, Style::Fill);
                self.font(FontStyle::Bold, 7.2, WHITE);
                let n = self
                    .nums
                    .get(&sw.id)
                    .map_or_else(|| "undefined".to_owned(), ToString::to_string);
                self.text(&n, M + 2.6, y - 1.05, TextOpts::center().middle());
                self.font(FontStyle::Normal, 8.5, INK);
                let mut x = M + cols[0].w;
                self.text(&side_names(project, &sw.a), x, y, TextOpts::left());
                x += cols[1].w;
                self.text(&side_names(project, &sw.b), x, y, TextOpts::left());
                x += cols[2].w;
                self.text(
                    &length_with_unit(la, units),
                    x + cols[3].w - 1.0,
                    y,
                    TextOpts::right(),
                );
                x += cols[3].w;
                self.text(
                    &length_with_unit(lb, units),
                    x + cols[4].w - 1.0,
                    y,
                    TextOpts::right(),
                );
                x += cols[4].w;
                let df = (la - lb).abs();
                self.canvas
                    .set_text_color(if df > MISMATCH_MM { MISMATCH } else { MUTE });
                let diff = if df < 0.5 {
                    "–".to_owned()
                } else {
                    format!("{} mm", number(crate::format::round_half_up(df)))
                };
                self.text(&diff, x + cols[5].w - 1.0, y, TextOpts::right());
                self.canvas.set_draw_color(ROW_RULE);
                self.canvas.line(M, y + 2.2, x + cols[5].w, y + 2.2);
            }
            self.canvas.set_font_size(7.0);
            self.canvas.set_text_color(MUTE);
            let note = format!(
                "Lengths along the stitching line, size {}. The coloured number is printed beside both edges of each seam on the pattern pieces; sew edges with the same number together.",
                project.active_size.0
            );
            self.text(&note, M, pg[1] - 10.0, TextOpts::left());
        }
    }

    /// Piece schedule: every piece with its net and cut sizes.
    pub(super) fn schedule(&mut self, items: &[PdfItem], paper: [f64; 2]) {
        let project = self.project;
        let units = self.units;
        let pg = self.new_page(paper[1], paper[0]);
        self.header(pg, Some("Piece schedule"));
        let cols = [
            col("#", 8.0),
            col("Piece", 54.0),
            col("Fabric", 46.0),
            rcol("Cut", 12.0),
            rcol("Net W", 22.0),
            rcol("Net L", 22.0),
            rcol("Cut W", 22.0),
            rcol("Cut L", 22.0),
            rcol("Perimeter", 24.0),
            rcol("Area (cut)", 24.0),
        ];
        let mut y = 32.0;
        let cx = self.table_header(&cols, M, y, 7.5, |c| c.h.to_uppercase());
        self.canvas.set_draw_color(TABLE_RULE);
        self.canvas.set_line_width(0.2);
        self.canvas.line(M, y + 1.8, cx, y + 1.8);
        self.canvas.set_font(FontStyle::Normal);
        self.canvas.set_font_size(8.5);
        for (i, it) in items.iter().enumerate() {
            y += 7.0;
            if y > pg[1] - 16.0 {
                continue;
            }
            let g = &it.geom;
            let pc = it.piece;
            let fab = project.fabrics.get(&pc.fabric).map_or("—", |f| f.name.as_str());
            let row = [
                (i + 1).to_string(),
                pc.name.clone(),
                fab.to_owned(),
                format!(
                    "{}{}",
                    number(qty(pc)),
                    if pc.mirror.is_some() { " M" } else { "" }
                ),
                length_with_unit(g.bb.w, units),
                length_with_unit(g.bb.h, units),
                length_with_unit(g.cb.w, units),
                length_with_unit(g.cb.h, units),
                length_with_unit(g.perim, units),
                area_with_unit(g.cut_area, units),
            ];
            let mut x = M;
            self.canvas.set_text_color(INK);
            for (c, v) in cols.iter().zip(&row) {
                let (ax, opts) = c.anchor(x);
                self.text(v, ax, y, opts);
                x += c.w;
            }
            self.canvas.set_draw_color(ROW_RULE);
            self.canvas.line(M, y + 2.0, x, y + 2.0);
        }
        self.canvas.set_font_size(7.0);
        self.canvas.set_text_color(MUTE);
        self.text(
            "Net = sewing line. Cut = including seam allowance. W = width, L = length (2D pattern axes). \"M\" = mirrored pair.",
            M,
            pg[1] - 10.0,
            TextOpts::left(),
        );
    }

    /// Dimension tables: every corner (ordinate, angle) and the edge that leaves it.
    pub(super) fn dim_tables(&mut self, items: &[PdfItem], paper: [f64; 2], paper_kind: super::Paper) {
        let project = self.project;
        let units = self.units;
        let pages = dim_table_pages(project, items, paper_kind);
        let cols = [
            col("PT", 8.0),
            rcol("X", 14.0),
            rcol("Y", 14.0),
            rcol("ANGLE", 14.0),
            Col {
                h: "EDGE",
                w: 13.0,
                right: false,
                center: true,
            },
            col("TYPE", 11.0),
            rcol("LENGTH", 15.0),
            rcol("CHORD", 15.0),
            rcol("SEAM", 10.0),
            rcol("FINISH", 19.0),
        ];
        let n_pages = pages.len();
        for (pi, page) in pages.iter().enumerate() {
            let pg = self.new_page(paper[1], paper[0]);
            let title = if n_pages > 1 {
                format!("Dimension tables ({} / {n_pages})", pi + 1)
            } else {
                "Dimension tables".to_owned()
            };
            self.header(pg, Some(&title));
            let col_w = (pg[0] - 24.0 - 8.0) / 2.0;
            for (ci, column) in page.iter().enumerate() {
                let cx0 = 12.0 + ci as f64 * (col_w + 8.0);
                for blk in column {
                    self.dim_table_block(blk, &items[blk.item], cx0, col_w, &cols);
                }
            }
            self.canvas.set_font_size(7.0);
            self.canvas.set_text_color(MUTE);
            self.canvas.set_font(FontStyle::Normal);
            let note = format!(
                "Values in {}, size {}, on the sewing line. X, Y from datum 0 at the top-left of each piece's net bounding box (Y downward). Angle = interior angle between edge tangents. Length runs along the edge (along the curve for curves); chord = straight distance.",
                unit_word(units),
                project.active_size.0
            );
            self.text(&note, 12.0, pg[1] - 10.0, TextOpts::left().wrapped(pg[0] - 24.0));
        }
    }

    /// One piece's block of the dimension tables, at the top-left `cx0` of its column.
    fn dim_table_block(&mut self, blk: &DimBlock, it: &PdfItem, cx0: f64, col_w: f64, cols: &[Col]) {
        let units = self.units;
        let rh = DIM_TABLE_ROW;
        let t = &blk.table;
        let mut y = 32.0 + blk.y;
        self.font(FontStyle::Bold, 8.5, INK);
        self.text(&it.piece.name, cx0, y, TextOpts::left());
        self.font(FontStyle::Normal, 6.5, MUTE);
        let summary = format!(
            "net {} × {}  ·  perimeter {}{}",
            length_with_unit(it.geom.bb.w, units),
            length_with_unit(it.geom.bb.h, units),
            length_with_unit(it.geom.perim, units),
            if t.model.symmetric {
                "  ·  symmetric about CL"
            } else {
                ""
            }
        );
        self.text(&summary, cx0 + col_w, y, TextOpts::right());
        y += rh;
        self.table_header(cols, cx0, y, 5.8, |c| c.h.to_owned());
        self.canvas.set_draw_color(TABLE_RULE);
        self.canvas.set_line_width(0.2);
        self.canvas.line(cx0, y + 1.3, cx0 + col_w, y + 1.3);
        self.canvas.set_font(FontStyle::Normal);
        self.canvas.set_font_size(7.0);
        for (c, (e, seams)) in t.model.corners.iter().zip(t.model.edges.iter().zip(&t.seams)) {
            y += rh;
            let seam_list: Vec<String> = seams.iter().map(ToString::to_string).collect();
            let vals = [
                c.letter.clone(),
                length_number(c.ordinate.x, units),
                length_number(c.ordinate.y, units),
                if c.smooth {
                    "tangent".to_owned()
                } else {
                    format!("{}°", fixed(c.angle, 1))
                },
                format!("{}–{}", e.from, e.to),
                if e.curve { "curve" } else { "line" }.to_owned(),
                length_number(e.len, units),
                if e.curve {
                    length_number(e.chord, units)
                } else {
                    String::new()
                },
                if seam_list.is_empty() {
                    "free".to_owned()
                } else {
                    seam_list.join(", ")
                },
                format!(
                    "{}{}",
                    if e.hem { "hem " } else { "SA " },
                    length_number(e.allow, units)
                ),
            ];
            let mut xx = cx0;
            for (j, (cc, v)) in cols.iter().zip(&vals).enumerate() {
                self.canvas.set_font(if j == 0 {
                    FontStyle::Bold
                } else {
                    FontStyle::Normal
                });
                self.canvas
                    .set_text_color(if j == 8 && seams.is_empty() { MUTE } else { INK });
                let (ax, opts) = cc.anchor(xx);
                self.text(v, ax, y, opts);
                xx += cc.w;
            }
            self.canvas.set_draw_color(ROW_RULE);
            self.canvas.line(cx0, y + 1.3, cx0 + col_w, y + 1.3);
        }
        if !t.notches.is_empty() {
            y += rh;
            self.font(FontStyle::Bold, 5.8, MUTE);
            self.text("NOTCHES", cx0, y, TextOpts::left());
            self.font(FontStyle::Normal, 7.0, INK);
            for nt in &t.notches {
                y += rh;
                let line = format!(
                    "on {}–{}:  {} from {}  ·  {} from {}",
                    nt.from,
                    nt.to,
                    length_number(nt.a, units),
                    nt.from,
                    length_number(nt.b, units),
                    nt.to
                );
                self.text(&line, cx0 + 8.0, y, TextOpts::left());
            }
        }
    }
}
