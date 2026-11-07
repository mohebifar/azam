//! Drawing state and primitives shared by the pattern PDF sheets.

use azam_doc::{Project, SeamId, Units};
use azam_geom::Vec2;
use indexmap::IndexMap;

use super::PatternPdfOptions;
use crate::format::hex_rgb;
use crate::model::DEG;
use crate::pdf::canvas::{BLACK, Canvas, Rgb, Style, TextOpts, WHITE};
use crate::pdf::metrics::FontStyle;

/// Main ink.
pub const INK: Rgb = [34, 34, 38];
/// Secondary text.
pub const MUTE: Rgb = [128, 128, 136];
/// Dimension lines and values.
pub const DIM: Rgb = [70, 82, 104];
/// Rules under headers.
pub const RULE: Rgb = [220, 218, 214];
/// Table header rules.
pub const TABLE_RULE: Rgb = [200, 198, 194];
/// Table row rules.
pub const ROW_RULE: Rgb = [236, 234, 230];
/// Seam allowance band.
pub const SA_FILL: Rgb = [236, 233, 228];
/// Hem allowance band.
pub const HEM_FILL: Rgb = [222, 216, 206];
/// Colour of a seam without one.
pub const SEAM_COLOR: &str = "#E8654A";
/// Colours of nested sizes, in size order.
pub const NEST_COLORS: [&str; 8] = [
    "#3B82F6", "#10B981", "#111111", "#F59E0B", "#EF4444", "#8B5CF6", "#EC4899", "#14B8A6",
];
/// Millimetres per point, as the layout measures text height.
pub const MM_PER_PT: f64 = 0.3528;
/// Product name in page headers.
pub const PRODUCT: &str = "AZAM";

/// Model-to-paper transform: paper = `o + (model - b) * k`.
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    /// Scale.
    pub k: f64,
    /// Model x mapped to `ox`.
    pub bx: f64,
    /// Model y mapped to `oy`.
    pub by: f64,
    /// Paper x of `bx`.
    pub ox: f64,
    /// Paper y of `by`.
    pub oy: f64,
}

impl Transform {
    /// Paper position of model point `(x, y)`.
    pub fn map(&self, x: f64, y: f64) -> [f64; 2] {
        [self.ox + (x - self.bx) * self.k, self.oy + (y - self.by) * self.k]
    }

    /// Model position of paper point `(x, y)`.
    pub fn unmap(&self, x: f64, y: f64) -> Vec2 {
        Vec2::new((x - self.ox) / self.k + self.bx, (y - self.oy) / self.k + self.by)
    }
}

/// Measured rotated text, for decorations around it.
#[derive(Clone, Copy, Debug)]
pub struct RotatedText {
    /// Width, mm.
    pub w: f64,
    /// Height, mm.
    pub h: f64,
    /// Reading direction.
    pub ux: f64,
    /// Reading direction.
    pub uy: f64,
    /// Up direction.
    pub upx: f64,
    /// Up direction.
    pub upy: f64,
}

/// `#rrggbb` as RGB (black if malformed).
pub fn hex(color: &str) -> Rgb {
    hex_rgb(color).unwrap_or(BLACK)
}

/// −1, 0 or 1 by the sign of `v` (zero stays zero).
pub fn sign(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        v
    }
}

/// Rotates `v` by `a` radians.
pub fn rot(v: Vec2, a: f64) -> Vec2 {
    let (s, c) = (a.sin(), a.cos());
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Pattern PDF renderer: the canvas plus page numbering and the label-collision boxes of the
/// piece being drawn.
pub struct Renderer<'a> {
    pub(super) project: &'a Project,
    pub(super) o: &'a PatternPdfOptions,
    pub(super) units: Units,
    pub(super) canvas: Canvas,
    pub(super) page_no: usize,
    pub(super) total: usize,
    pub(super) nums: IndexMap<SeamId, usize>,
    /// Paper-space boxes already used on the current piece.
    pub(super) occ: Vec<[f64; 4]>,
}

impl<'a> Renderer<'a> {
    pub(super) fn new(project: &'a Project, o: &'a PatternPdfOptions, total: usize) -> Self {
        Self {
            project,
            o,
            units: o.units,
            canvas: Canvas::new(),
            page_no: 0,
            total,
            nums: azam_doc::seam_numbers(project),
            occ: Vec::new(),
        }
    }

    /// Starts a page; returns its size.
    pub(super) fn new_page(&mut self, w: f64, h: f64) -> [f64; 2] {
        self.canvas.add_page(w, h);
        self.page_no += 1;
        [w, h]
    }

    pub(super) fn nested(&self) -> bool {
        self.o.all_sizes && self.project.sizes.len() > 1
    }

    /// Width of `text` as the layout measures it.
    pub(super) fn uni_width(&self, text: &str) -> f64 {
        self.canvas.text_width(text)
    }

    pub(super) fn text(&mut self, text: &str, x: f64, y: f64, opts: TextOpts) {
        self.canvas.text(text, x, y, opts);
    }

    pub(super) fn font(&mut self, style: FontStyle, size: f64, color: Rgb) {
        self.canvas.set_font(style);
        self.canvas.set_font_size(size);
        self.canvas.set_text_color(color);
    }

    /// Page header: product, garment, size and date, page number, rule and optional title.
    pub(super) fn header(&mut self, pg: [f64; 2], title: Option<&str>) {
        const M: f64 = 12.0;
        self.canvas.set_text_color(INK);
        self.canvas.set_font(FontStyle::Bold);
        self.canvas.set_font_size(8.5);
        self.text(&format!("{PRODUCT} · PATTERN"), M, 10.0, TextOpts::left());
        self.canvas.set_font(FontStyle::Normal);
        self.canvas.set_text_color(MUTE);
        let name = self.project.meta.name.clone();
        self.text(&name, M + 38.0, 10.0, TextOpts::left());
        let line = format!(
            "·   size {}{}   ·   {}",
            self.project.active_size.0,
            if self.nested() { " (all sizes nested)" } else { "" },
            self.o.date
        );
        let x = M + 38.0 + self.uni_width(&name) + 3.0;
        self.text(&line, x, 10.0, TextOpts::left());
        let page = format!("{} / {}", self.page_no, self.total);
        self.text(&page, pg[0] - M, 10.0, TextOpts::right());
        self.canvas.set_draw_color(RULE);
        self.canvas.set_line_width(0.2);
        self.canvas.line(M, 13.0, pg[0] - M, 13.0);
        if let Some(title) = title {
            self.font(FontStyle::Bold, 14.0, INK);
            self.text(title, M, 22.0, TextOpts::left());
        }
    }

    /// Strokes model polyline `pts` through `t`.
    pub(super) fn poly(&mut self, pts: &[Vec2], t: &Transform, closed: bool) {
        if pts.len() < 2 {
            return;
        }
        let q = pts.iter().map(|p| t.map(p.x, p.y)).collect();
        self.canvas.path(q, closed, Style::Stroke);
    }

    /// Fills model polygon `pts` through `t` with `color`.
    pub(super) fn fill_poly(&mut self, pts: &[Vec2], t: &Transform, color: Rgb) {
        if pts.len() < 3 {
            return;
        }
        self.canvas.set_fill_color(color);
        let q = pts.iter().map(|p| t.map(p.x, p.y)).collect();
        self.canvas.path(q, true, Style::Fill);
    }

    /// Strokes a paper-space polyline.
    pub(super) fn plines(&mut self, q: Vec<[f64; 2]>, closed: bool) {
        if q.len() < 2 {
            return;
        }
        self.canvas.path(q, closed, Style::Stroke);
    }

    /// A filled arrow head with its tip at `(x, y)` pointing along `(dx, dy)`.
    pub(super) fn arrow(&mut self, x: f64, y: f64, dx: f64, dy: f64, len: f64) {
        let n = azam_geom::hypot(dx, dy);
        let n = if n == 0.0 { 1.0 } else { n };
        let (dx, dy) = (dx / n, dy / n);
        let w = len * 0.36;
        self.canvas.triangle(
            [
                [x, y],
                [x - dx * len - dy * w, y - dy * len + dx * w],
                [x - dx * len + dy * w, y - dy * len - dx * w],
            ],
            Style::Fill,
        );
    }

    /// Text centred on `(x, y)` over a white box (rotated 90° when `vertical`).
    pub(super) fn label(&mut self, txt: &str, x: f64, y: f64, size: f64, vertical: bool) {
        self.canvas.set_font_size(size);
        let w = self.canvas.text_width(txt);
        self.canvas.set_fill_color(WHITE);
        if vertical {
            self.canvas.rect(
                x - size * 0.22,
                y - w / 2.0 - 0.8,
                size * 0.44,
                w + 1.6,
                Style::Fill,
            );
        } else {
            self.canvas.rect(
                x - w / 2.0 - 0.8,
                y - size * 0.22,
                w + 1.6,
                size * 0.44,
                Style::Fill,
            );
        }
        let opts = TextOpts::center().middle();
        self.text(txt, x, y, if vertical { opts.rotated(90.0) } else { opts });
    }

    pub(super) fn occ_add(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.occ.push([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]);
    }

    pub(super) fn occ_hit(&self, b: [f64; 4]) -> f64 {
        self.occ
            .iter()
            .map(|r| {
                let w = r[2].min(b[2]) - r[0].max(b[0]);
                let h = r[3].min(b[3]) - r[1].max(b[1]);
                if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
            })
            .sum()
    }

    /// Text centred on `(cx, cy)` reading along `(ux, uy)`, flipped so it never reads upside
    /// down; its box is marked used.
    pub(super) fn rtext(&mut self, txt: &str, cx: f64, cy: f64, ux: f64, uy: f64, size: f64) -> RotatedText {
        let (ux, uy) = if ux < -1e-6 || (ux.abs() <= 1e-6 && uy > 0.0) {
            (-ux, -uy)
        } else {
            (ux, uy)
        };
        self.canvas.set_font_size(size);
        let w = self.canvas.text_width(txt);
        let h = size * MM_PER_PT;
        let (upx, upy) = (uy, -ux);
        let bx = cx - ux * w / 2.0 - upx * h * 0.36;
        let by = cy - uy * w / 2.0 - upy * h * 0.36;
        let ang = (-uy).atan2(ux) / DEG;
        if ang.abs() < 0.05 {
            self.text(txt, bx, by, TextOpts::left());
        } else {
            self.text(txt, bx, by, TextOpts::left().rotated(ang));
        }
        let hx = ux.abs() * w / 2.0 + upx.abs() * h * 0.45;
        let hy = uy.abs() * w / 2.0 + upy.abs() * h * 0.45;
        self.occ_add(cx - hx, cy - hy, cx + hx, cy + hy);
        RotatedText {
            w,
            h,
            ux,
            uy,
            upx,
            upy,
        }
    }

    /// The arc-length mark over a dimension value.
    pub(super) fn arc_sym(&mut self, cx: f64, cy: f64, r: RotatedText) {
        let hw = (r.w / 2.0).min(1.5);
        let b0x = cx + r.upx * (r.h * 0.36 + 0.45);
        let b0y = cy + r.upy * (r.h * 0.36 + 0.45);
        let q = (0..=8)
            .map(|i| {
                let s = -1.0 + f64::from(i) / 4.0;
                [
                    b0x + r.ux * s * hw + r.upx * (1.0 - s * s) * 0.65,
                    b0y + r.uy * s * hw + r.upy * (1.0 - s * s) * 0.65,
                ]
            })
            .collect();
        self.canvas.set_line_width(0.12);
        self.plines(q, false);
    }

    /// Dimension colour for lines, fills and text; solid lines; regular face.
    pub(super) fn dim_ink(&mut self) {
        self.canvas.set_draw_color(DIM);
        self.canvas.set_fill_color(DIM);
        self.canvas.set_text_color(DIM);
        self.canvas.set_dash(&[], 0.0);
        self.canvas.set_font(FontStyle::Normal);
    }
}

/// Non-overlapping label positions `gap` apart, clusters centred on their targets.
pub fn spread(want: &[f64], gap: f64) -> Vec<f64> {
    struct Cluster {
        s: f64,
        idx: Vec<usize>,
    }
    let mut cl: Vec<Cluster> = want
        .iter()
        .enumerate()
        .map(|(i, &v)| Cluster { s: v, idx: vec![i] })
        .collect();
    let mut moved = true;
    let mut guard = 0;
    while moved && guard < 200 {
        guard += 1;
        moved = false;
        for i in 0..cl.len().saturating_sub(1) {
            let (a, b) = (&cl[i], &cl[i + 1]);
            if a.s + a.idx.len() as f64 * gap > b.s + 1e-6 {
                let idx: Vec<usize> = a.idx.iter().chain(&b.idx).copied().collect();
                let n = idx.len() as f64;
                let mean = idx
                    .iter()
                    .enumerate()
                    .fold(0.0, |t, (jj, &j)| t + want[j] - jj as f64 * gap)
                    / n;
                cl.splice(i..i + 2, [Cluster { s: mean, idx }]);
                moved = true;
                break;
            }
        }
    }
    let mut out = vec![0.0; want.len()];
    for c in &cl {
        for (jj, &j) in c.idx.iter().enumerate() {
            out[j] = c.s + jj as f64 * gap;
        }
    }
    out
}
