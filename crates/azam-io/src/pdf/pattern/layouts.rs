//! The four page layouts of the pattern PDF: tiles with a cover, one sheet per piece, an
//! overview sheet, and plotter sheets.

use azam_doc::Units;

use super::plan::{
    DIM_FULL, DIM_ROOM, PLOTTER_CHROME, PdfItem, PdfPlan, PlanSheets, SHEET_MARGIN, pack, piece_fit,
};
use super::render::{INK, MUTE, Renderer, TABLE_RULE, Transform};
use crate::format::letters;
use crate::model::Bounds;
use crate::pdf::canvas::{Style, TextOpts};
use crate::pdf::metrics::FontStyle;

impl Renderer<'_> {
    /// Tile label: row letters and column number (`A1`, `B3`).
    fn tile_id(r: usize, c: usize) -> String {
        format!("{}{}", letters(r), c + 1)
    }

    /// Draws every page of the plan.
    #[allow(clippy::too_many_lines, reason = "the four layouts in turn")]
    pub(super) fn render(&mut self, plan: &PdfPlan) {
        let paper_kind = self.o.paper;
        let paper = paper_kind.size();
        let items = &plan.items;
        let dims = self.o.dims;
        match &plan.sheets {
            PlanSheets::Empty => {}
            PlanSheets::Pieces => {
                for it in items {
                    let f = it.fit.unwrap_or_else(|| piece_fit(&it.bounds, paper_kind, dims));
                    let (pw, ph, k) = (f.page_w, f.page_h, f.k);
                    let pg = self.new_page(pw, ph);
                    self.frame(pg);
                    self.header(pg, None);
                    self.font(FontStyle::Bold, 14.0, INK);
                    self.text(&it.piece.name, 12.0, 22.0, TextOpts::left());
                    let t = Transform {
                        k,
                        bx: it.bounds.x0,
                        by: it.bounds.y0,
                        ox: 12.0 + f.room[0] + (f.area_w - it.bounds.w * k) / 2.0,
                        oy: 27.0 + f.room[1] + (f.area_h - it.bounds.h * k) / 2.0,
                    };
                    self.draw_piece(it, &t);
                    self.line_legend(12.0, ph - 41.5, pw - 12.0);
                    self.title_block(pg, it, k);
                    self.size_legend(pw - 12.0 - self.project.sizes.len() as f64 * 16.0, 21.0);
                }
                self.schedule(items, paper);
                if dims {
                    self.dim_tables(items, paper, paper_kind);
                }
                self.sew_schedule(paper);
            }
            PlanSheets::Overview => {
                self.overview(items, paper);
                self.schedule(items, paper);
                if dims {
                    self.dim_tables(items, paper, paper_kind);
                }
                self.sew_schedule(paper);
            }
            PlanSheets::Tiled(tp) => {
                let m = SHEET_MARGIN;
                let (tw, th) = (tp.tile_w, tp.tile_h);
                let units = self.units;
                // cover
                let cv = self.new_page(paper[0], paper[1]);
                self.header(cv, Some("Full-scale pattern"));
                self.font(FontStyle::Normal, 9.0, INK);
                let lines = [
                    "Print every page at 100 % (\"Actual size\"), not \"Fit to page\".".to_owned(),
                    "Check the test square, then trim each tile on its border and butt the borders together,"
                        .to_owned(),
                    "matching the tile letters on each edge. Rows are letters, columns are numbers."
                        .to_owned(),
                ];
                self.canvas.text_lines(&lines, 12.0, 32.0, TextOpts::left());
                let sq = if units == Units::In { 50.8 } else { 50.0 };
                self.canvas.set_draw_color(INK);
                self.canvas.set_line_width(0.3);
                self.canvas.rect(12.0, 50.0, sq, sq, Style::Stroke);
                self.canvas.set_font_size(8.0);
                let sq_label = if units == Units::In {
                    "2 in test square"
                } else {
                    "5 cm test square"
                };
                self.text(
                    sq_label,
                    12.0 + sq / 2.0,
                    50.0 + sq / 2.0,
                    TextOpts::center().middle(),
                );
                // assembly map
                let (mx, my) = (12.0 + sq + 14.0, 50.0);
                let (mw, mh) = (cv[0] - mx - 12.0, cv[1] - my - 30.0);
                let mk = (mw / (tp.cols as f64 * tw)).min(mh / (tp.rows as f64 * th));
                self.canvas.set_font_size(6.5);
                for r in 0..tp.rows {
                    for c in 0..tp.cols {
                        let used = tp.used.contains(&(r, c));
                        let (x, y) = (mx + c as f64 * tw * mk, my + r as f64 * th * mk);
                        self.canvas.set_draw_color(TABLE_RULE);
                        self.canvas.set_line_width(0.15);
                        if used {
                            self.canvas.set_fill_color([244, 242, 238]);
                            self.canvas.rect(x, y, tw * mk, th * mk, Style::FillStroke);
                        } else {
                            self.canvas.rect(x, y, tw * mk, th * mk, Style::Stroke);
                        }
                        self.canvas.set_text_color(if used { INK } else { MUTE });
                        self.text(&Self::tile_id(r, c), x + 1.2, y + 3.0, TextOpts::left());
                    }
                }
                for (it, p) in items.iter().zip(&tp.pack.pos) {
                    self.canvas.set_draw_color(INK);
                    self.canvas.set_line_width(0.2);
                    let t = Transform {
                        k: mk,
                        bx: it.bounds.x0,
                        by: it.bounds.y0,
                        ox: mx + p.x * mk,
                        oy: my + p.y * mk,
                    };
                    self.poly(&it.geom.cut, &t, true);
                }
                self.line_legend(12.0, cv[1] - 14.0, cv[0] - 12.0);
                // tiles
                let room = tp.pack.room;
                for &(r, c) in &tp.used {
                    self.new_page(paper[0], paper[1]);
                    let (x0, y0) = (c as f64 * tw, r as f64 * th);
                    self.canvas.save();
                    self.canvas.clip_rect(m, m, tw, th);
                    for (it, p) in items.iter().zip(&tp.pack.pos) {
                        let off = p.x - room[0] - 2.0 > x0 + tw
                            || p.x + it.bounds.w + room[2] + 2.0 < x0
                            || p.y - room[1] - 2.0 > y0 + th
                            || p.y + it.bounds.h + room[3] + 2.0 < y0;
                        if off {
                            continue;
                        }
                        let t = Transform {
                            k: 1.0,
                            bx: it.bounds.x0,
                            by: it.bounds.y0,
                            ox: m + p.x - x0,
                            oy: m + p.y - y0,
                        };
                        self.draw_piece(it, &t);
                    }
                    self.canvas.restore();
                    self.canvas.set_draw_color([150, 150, 156]);
                    self.canvas.set_line_width(0.15);
                    self.canvas.set_dash(&[2.0, 1.5], 0.0);
                    self.canvas.rect(m, m, tw, th, Style::Stroke);
                    self.canvas.set_dash(&[], 0.0);
                    self.font(FontStyle::Bold, 9.0, INK);
                    self.text(&Self::tile_id(r, c), m + 1.5, m + 4.5, TextOpts::left());
                    self.font(FontStyle::Normal, 6.5, MUTE);
                    if r > 0 {
                        self.text(
                            &Self::tile_id(r - 1, c),
                            m + tw / 2.0,
                            m - 2.5,
                            TextOpts::center(),
                        );
                    }
                    if r + 1 < tp.rows {
                        self.text(
                            &Self::tile_id(r + 1, c),
                            m + tw / 2.0,
                            m + th + 4.5,
                            TextOpts::center(),
                        );
                    }
                    if c > 0 {
                        self.text(
                            &Self::tile_id(r, c - 1),
                            m - 2.5,
                            m + th / 2.0,
                            TextOpts::center().rotated(90.0),
                        );
                    }
                    if c + 1 < tp.cols {
                        self.text(
                            &Self::tile_id(r, c + 1),
                            m + tw + 4.5,
                            m + th / 2.0,
                            TextOpts::center().rotated(90.0),
                        );
                    }
                    let ft = format!(
                        "size {}  ·  {} / {}",
                        self.project.active_size.0, self.page_no, self.total
                    );
                    self.text(&ft, paper[0] - m, paper[1] - 4.0, TextOpts::right());
                    let name = self.project.meta.name.clone();
                    let x = paper[0] - m - self.uni_width(&ft) - 3.0;
                    self.text(&name, x, paper[1] - 4.0, TextOpts::right());
                }
                if dims {
                    self.dim_tables(items, paper, paper_kind);
                }
                self.sew_schedule(paper);
            }
            PlanSheets::Plotter(pp) => {
                let m = SHEET_MARGIN;
                let pw = pp.roll;
                let n_sizes = self.project.sizes.len() as f64;
                for sh in &pp.sheets {
                    let ph = sh.h + 2.0 * m + PLOTTER_CHROME;
                    let pg = self.new_page(pw, ph);
                    self.header(pg, None);
                    for &i in &sh.idx {
                        let (it, p) = (&items[i], pp.pack.pos[i]);
                        let t = Transform {
                            k: 1.0,
                            bx: it.bounds.x0,
                            by: it.bounds.y0,
                            ox: m + p.x,
                            oy: 22.0 + p.y - sh.y0,
                        };
                        self.draw_piece(it, &t);
                    }
                    self.scale_bar(12.0, ph - 9.0, 1.0);
                    self.line_legend(140.0, ph - 7.6, pw - 12.0 - n_sizes * 16.0 - 6.0);
                    self.size_legend(pw - 12.0 - n_sizes * 16.0, ph - 7.5);
                }
            }
        }
    }

    /// All pieces on one landscape sheet at a common scale.
    fn overview(&mut self, items: &[PdfItem], paper: [f64; 2]) {
        let (pw, ph) = (paper[1], paper[0]);
        let pg = self.new_page(pw, ph);
        self.header(pg, Some("Pattern overview"));
        let (aw, ah) = (pw - 24.0, ph - 30.0 - 20.0);
        let boxes: Vec<Bounds> = items.iter().map(|it| it.bounds).collect();
        let max_w = boxes.iter().map(|b| b.w + 14.0).fold(f64::NEG_INFINITY, f64::max);
        let sum_w: f64 = boxes.iter().map(|b| b.w + 54.0).sum();
        let mut pk = None;
        let mut k = 0.0;
        let mut kg = 0.08;
        for _ in 0..4 {
            // room for labels and dimension lines is fixed on the page, so it depends on the scale
            let room = if self.o.dims {
                if kg >= DIM_FULL {
                    DIM_ROOM.map(|v| v / kg)
                } else {
                    [13.0 / kg, 11.0 / kg, 2.0 / kg, 2.0 / kg]
                }
            } else {
                [2.0 / kg; 4]
            };
            let gap = 7.0 / kg;
            let mut best = None;
            let mut bk = 0.0;
            for i in 0..=24 {
                let w = max_w + (sum_w - max_w) * f64::from(i) / 24.0;
                let c = pack(&boxes, w + room[0], gap, room);
                let kk = (aw / c.w).min(ah / c.h).min(1.0);
                if kk > bk {
                    bk = kk;
                    best = Some(c);
                }
            }
            pk = best;
            k = bk;
            if ((kg - k) / k).abs() < 0.02 {
                break;
            }
            kg = k;
        }
        let Some(pk) = pk else {
            return;
        };
        let ox = 12.0 + (aw - pk.w * k) / 2.0;
        let oy = 28.0 + (ah - pk.h * k) / 2.0;
        for (it, p) in items.iter().zip(&pk.pos) {
            let t = Transform {
                k,
                bx: it.bounds.x0,
                by: it.bounds.y0,
                ox: ox + p.x * k,
                oy: oy + p.y * k,
            };
            self.draw_piece(it, &t);
        }
        self.scale_bar(12.0, ph - 9.0, k);
        self.line_legend(80.0, ph - 7.6, pw - 12.0);
        self.size_legend(pw - 12.0 - self.project.sizes.len() as f64 * 16.0, 21.0);
    }
}
