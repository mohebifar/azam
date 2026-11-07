//! Page furniture of the pattern PDF: line and size legends, scale bars, table headers, the CAD
//! sheet frame and title block.

use azam_doc::Units;

use super::plan::PdfItem;
use super::render::{HEM_FILL, INK, MUTE, NEST_COLORS, Renderer, SA_FILL, SEAM_COLOR, hex};
use crate::dimensions::allowance_summary;
use crate::format::{area_with_unit, fixed, length_with_unit, number, unit_word};
use crate::model::{or_default, qty};
use crate::pdf::canvas::{Style, TextOpts, WHITE};
use crate::pdf::metrics::FontStyle;

/// Side margin of schedule and table pages, in mm.
pub(super) const M: f64 = 12.0;

/// A table column: header, width and alignment.
pub(super) struct Col {
    pub(super) h: &'static str,
    pub(super) w: f64,
    pub(super) right: bool,
    pub(super) center: bool,
}

pub(super) const fn col(h: &'static str, w: f64) -> Col {
    Col {
        h,
        w,
        right: false,
        center: false,
    }
}

pub(super) const fn rcol(h: &'static str, w: f64) -> Col {
    Col {
        h,
        w,
        right: true,
        center: false,
    }
}

impl Col {
    pub(super) fn anchor(&self, x: f64) -> (f64, TextOpts) {
        if self.right {
            (x + self.w - 1.0, TextOpts::right())
        } else if self.center {
            (x + self.w / 2.0, TextOpts::center())
        } else {
            (x, TextOpts::left())
        }
    }
}

/// Unit and count of the scale bar: 10 cm, or 4 in.
pub(super) fn bar_units(units: Units) -> (f64, usize, &'static str) {
    if units == Units::In {
        (25.4, 4, " in")
    } else {
        (10.0, 10, " cm")
    }
}

impl Renderer<'_> {
    /// One-row legend of line styles.
    pub(super) fn line_legend(&mut self, x: f64, y: f64, max_x: f64) {
        const ITEMS: [(&str, &str); 6] = [
            ("cut", "cut line"),
            ("sew", "stitching line (sewn)"),
            ("free", "free edge (finished)"),
            ("hem", "hem fold"),
            ("band", "seam / hem allowance"),
            ("num", "seam number"),
        ];
        let mut x = x;
        self.canvas.set_font(FontStyle::Normal);
        self.canvas.set_font_size(6.3);
        for (kind, text) in ITEMS {
            let w = 7.0 + self.canvas.text_width(text) + 5.0;
            if x + w > max_x {
                continue;
            }
            self.canvas.set_draw_color(INK);
            self.canvas.set_dash(&[], 0.0);
            match kind {
                "cut" => {
                    self.canvas.set_line_width(0.4);
                    self.canvas.line(x, y, x + 5.5, y);
                }
                "sew" => {
                    self.canvas.set_line_width(0.22);
                    self.canvas.set_dash(&[1.6, 1.1], 0.0);
                    self.canvas.line(x, y, x + 5.5, y);
                }
                "free" => {
                    self.canvas.set_line_width(0.3);
                    self.canvas.line(x, y, x + 5.5, y);
                }
                "hem" => {
                    self.canvas.set_line_width(0.3);
                    self.canvas.set_dash(&[3.2, 1.0, 0.5, 1.0], 0.0);
                    self.canvas.line(x, y, x + 5.5, y);
                }
                "band" => {
                    self.canvas.set_fill_color(SA_FILL);
                    self.canvas.rect(x, y - 1.3, 2.7, 2.6, Style::Fill);
                    self.canvas.set_fill_color(HEM_FILL);
                    self.canvas.rect(x + 2.8, y - 1.3, 2.7, 2.6, Style::Fill);
                }
                _ => {
                    self.canvas.set_fill_color(hex(SEAM_COLOR));
                    self.canvas.circle(x + 2.5, y, 1.8, Style::Fill);
                    self.font(FontStyle::Bold, 5.5, WHITE);
                    self.text("3", x + 2.5, y + 0.05, TextOpts::center().middle());
                    self.canvas.set_font(FontStyle::Normal);
                    self.canvas.set_font_size(6.3);
                }
            }
            self.canvas.set_dash(&[], 0.0);
            self.canvas.set_text_color(MUTE);
            self.text(text, x + 7.0, y + 0.9, TextOpts::left());
            x += w;
        }
    }

    /// Table header row with a rule under it; returns the x after the last column.
    pub(super) fn table_header(
        &mut self,
        cols: &[Col],
        x: f64,
        y: f64,
        size: f64,
        label: impl Fn(&Col) -> String,
    ) -> f64 {
        self.font(FontStyle::Bold, size, MUTE);
        let mut cx = x;
        for c in cols {
            let (ax, opts) = c.anchor(cx);
            self.text(&label(c), ax, y, opts);
            cx += c.w;
        }
        cx
    }

    /// Scale bar of 10 cm (4 in) at scale `k`, with the scale written beside it.
    pub(super) fn scale_bar(&mut self, x: f64, y: f64, k: f64) {
        let (unit_mm, n, label) = bar_units(self.units);
        let len = unit_mm * n as f64 * k;
        self.canvas.set_draw_color(INK);
        self.canvas.set_line_width(0.2);
        for i in (0..n).step_by(2) {
            self.canvas.set_fill_color(INK);
            self.canvas
                .rect(x + i as f64 * unit_mm * k, y, unit_mm * k, 1.6, Style::Fill);
        }
        self.canvas.rect(x, y, len, 1.6, Style::Stroke);
        self.font(FontStyle::Normal, 6.5, MUTE);
        self.text("0", x, y + 4.6, TextOpts::center());
        self.text(&format!("{n}{label}"), x + len, y + 4.6, TextOpts::center());
        let scale = if k >= 0.999 {
            "Scale 1:1".to_owned()
        } else {
            format!("Scale 1:{}", fixed(1.0 / k, 1))
        };
        self.text(&scale, x + len + 6.0, y + 1.5, TextOpts::left());
    }

    /// Legend of nested size colours (when sizes are nested).
    pub(super) fn size_legend(&mut self, x: f64, y: f64) {
        if !self.nested() {
            return;
        }
        let mut x = x;
        self.canvas.set_font_size(7.0);
        self.canvas.set_font(FontStyle::Normal);
        let sizes = self.project.sizes.clone();
        for (i, sz) in sizes.iter().enumerate() {
            let active = *sz == self.project.active_size;
            let c = if active {
                INK
            } else {
                hex(NEST_COLORS[i % NEST_COLORS.len()])
            };
            self.canvas.set_draw_color(c);
            self.canvas.set_line_width(if active { 0.4 } else { 0.25 });
            self.canvas.line(x, y, x + 6.0, y);
            self.canvas.set_text_color(c);
            self.text(&sz.0, x + 7.5, y + 1.0, TextOpts::left());
            x += 16.0;
        }
    }

    /// CAD sheet frame.
    pub(super) fn frame(&mut self, pg: [f64; 2]) {
        self.canvas.set_draw_color(INK);
        self.canvas.set_dash(&[], 0.0);
        self.canvas.set_line_width(0.35);
        self.canvas
            .rect(6.0, 6.0, pg[0] - 12.0, pg[1] - 12.0, Style::Stroke);
    }

    /// Title block of a piece sheet.
    #[allow(clippy::too_many_lines, reason = "one block of cells")]
    pub(super) fn title_block(&mut self, pg: [f64; 2], it: &PdfItem, k: f64) {
        struct Cell {
            label: &'static str,
            value: String,
            frac: f64,
            bold: bool,
            kind: Kind,
        }
        #[derive(PartialEq)]
        enum Kind {
            Plain,
            Bar,
            Notes,
        }
        let cell = |label, value: String, frac, bold, kind| Cell {
            label,
            value,
            frac,
            bold,
            kind,
        };
        let project = self.project;
        let units = self.units;
        let g = &it.geom;
        let pc = it.piece;
        let fab = project.fabrics.get(&pc.fabric).map_or("—", |f| f.name.as_str());
        let (x0, x1) = (6.0, pg[0] - 6.0);
        let width = x1 - x0;
        let y0 = pg[1] - 38.0;
        let scale = if k >= 0.999 {
            "1 : 1".to_owned()
        } else {
            format!("1 : {}", fixed(1.0 / k, 1))
        };
        let rows: [(f64, Vec<Cell>); 3] = [
            (
                13.0,
                vec![
                    cell("PIECE", pc.name.clone(), 0.3, true, Kind::Plain),
                    cell("FABRIC", fab.to_owned(), 0.2, false, Kind::Plain),
                    cell(
                        "CUT",
                        format!(
                            "{}{}",
                            number(qty(pc)),
                            if pc.mirror.is_some() { " · mirrored" } else { "" }
                        ),
                        0.12,
                        false,
                        Kind::Plain,
                    ),
                    cell("SIZE", project.active_size.0.clone(), 0.08, false, Kind::Plain),
                    cell("SCALE", scale, 0.18, false, Kind::Bar),
                    cell(
                        "SHEET",
                        format!("{} / {}", self.page_no, self.total),
                        0.12,
                        false,
                        Kind::Plain,
                    ),
                ],
            ),
            (
                11.0,
                vec![
                    cell(
                        "NET W × L",
                        format!(
                            "{} × {}",
                            length_with_unit(g.bb.w, units),
                            length_with_unit(g.bb.h, units)
                        ),
                        0.2,
                        false,
                        Kind::Plain,
                    ),
                    cell(
                        "CUT W × L",
                        format!(
                            "{} × {}",
                            length_with_unit(g.cb.w, units),
                            length_with_unit(g.cb.h, units)
                        ),
                        0.2,
                        false,
                        Kind::Plain,
                    ),
                    cell(
                        "PERIMETER (NET)",
                        length_with_unit(g.perim, units),
                        0.15,
                        false,
                        Kind::Plain,
                    ),
                    cell(
                        "AREA (CUT)",
                        area_with_unit(g.cut_area, units),
                        0.13,
                        false,
                        Kind::Plain,
                    ),
                    cell("ALLOWANCES", allowance_summary(pc), 0.22, false, Kind::Plain),
                    cell(
                        "GRAIN",
                        format!("{}°", number(or_default(pc.grain, 0.0))),
                        0.1,
                        false,
                        Kind::Plain,
                    ),
                ],
            ),
            (8.0, vec![cell("NOTES", String::new(), 1.0, false, Kind::Notes)]),
        ];
        self.canvas.set_dash(&[], 0.0);
        let mut y = y0;
        for (h, cells) in rows {
            let mut x = x0;
            for c in cells {
                let w = width * c.frac;
                self.canvas.set_draw_color(INK);
                self.canvas.set_line_width(0.2);
                self.canvas.rect(x, y, w, h, Style::Stroke);
                self.font(FontStyle::Normal, 5.2, MUTE);
                self.text(c.label, x + 1.6, y + 2.9, TextOpts::left());
                if c.kind == Kind::Notes {
                    self.canvas.set_font_size(6.3);
                    self.canvas.set_text_color(INK);
                    let t1 = format!(
                        "Dimensions in {}, on the sewing line (net). Ordinates from datum 0 at the top-left of the net bounding box. ",
                        unit_word(units)
                    );
                    self.text(&t1, x + 13.0, y + 5.2, TextOpts::left());
                    let tx = x + 13.0 + self.canvas.text_width(&t1) + 1.4;
                    self.dim_ink();
                    self.arc_sym(
                        tx + 1.5,
                        y + 5.2 - 0.8,
                        super::render::RotatedText {
                            w: 3.0,
                            h: 1.6,
                            ux: 1.0,
                            uy: 0.0,
                            upx: 0.0,
                            upy: -1.0,
                        },
                    );
                    self.canvas.set_font_size(6.3);
                    self.canvas.set_text_color(INK);
                    self.text(
                        "= length along a curve.  Letters key the dimension table.",
                        tx + 3.6,
                        y + 5.2,
                        TextOpts::left(),
                    );
                } else {
                    let style = if c.bold {
                        FontStyle::Bold
                    } else {
                        FontStyle::Normal
                    };
                    self.font(style, if c.bold { 11.0 } else { 8.4 }, INK);
                    self.text(
                        &c.value,
                        x + 1.6,
                        y + h - if c.bold { 2.6 } else { 2.4 },
                        TextOpts::left(),
                    );
                    if c.kind == Kind::Bar {
                        // 10 cm (4 in) bar at drawing scale, if it fits
                        let (um, n, label) = bar_units(units);
                        let bw = um * n as f64 * k;
                        let (bx, by) = (x + w - bw - 2.4, y + h - 4.4);
                        self.canvas.set_font(FontStyle::Normal);
                        if bw < w - 22.0 && bw > 6.0 {
                            for i in (0..n).step_by(2) {
                                self.canvas.set_fill_color(INK);
                                self.canvas
                                    .rect(bx + i as f64 * um * k, by, um * k, 1.3, Style::Fill);
                            }
                            self.canvas.set_draw_color(INK);
                            self.canvas.set_line_width(0.15);
                            self.canvas.rect(bx, by, bw, 1.3, Style::Stroke);
                            self.canvas.set_font_size(4.8);
                            self.canvas.set_text_color(MUTE);
                            self.text("0", bx, by + 3.4, TextOpts::center());
                            self.text(&format!("{n}{label}"), bx + bw, by + 3.4, TextOpts::center());
                        }
                    }
                }
                x += w;
            }
            y += h;
        }
    }
}
