//! CAD dimensioning of a piece on paper.
//!
//! Full (scale ≥ 1:14): every edge with an aligned (straight) or arc-length (curved) dimension,
//! notch chains, ordinates to every corner from the datum, overall size, corner letters, angles,
//! right-angle and centre-line marks. Basic (smaller scales): overall size and corner letters.

use std::cmp::Ordering;
use std::f64::consts::PI;

use azam_geom::{Vec2, contains, cumulative_lengths, flatten_open_line, hypot, point_at_length};

use super::plan::{DIM_FULL, PdfItem};
use super::render::{MM_PER_PT, Renderer, Transform, rot, spread};
use crate::dimensions::{DimModel, dim_model, ray_crossings};
use crate::format::{fixed, length_number, length_with_unit};
use crate::model::{Bounds, DEG, line_points, line_points_closed, sa_max};
use crate::pdf::canvas::TextOpts;
use crate::pdf::metrics::FontStyle;

/// Size of dimension values, in pt.
const DFS: f64 = 5.6;
/// Distance of the ordinate rails from the cut line, in mm.
const RAIL_OFFSET: f64 = 7.0;
/// Length of the jog from a rail to its value, in mm.
const RAIL_JOG: f64 = 2.0;
/// Gap between a rail and its values, in mm.
const RAIL_TEXT_GAP: f64 = 2.6;
/// Corner-letter colour.
const LETTER: [u8; 3] = [96, 96, 104];
/// Centre-line colour.
const CENTRE_LINE: [u8; 3] = [110, 110, 118];

/// Which side of the piece an ordinate rail runs along.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rail {
    Top,
    Bottom,
    Left,
    Right,
}

/// An ordinate: value from the datum, and the feature it measures.
#[derive(Clone, Copy)]
struct Ordinate {
    v: f64,
    f: Vec2,
}

impl Rail {
    /// How near a feature is to this rail (smaller is nearer).
    fn near(self, f: Vec2) -> f64 {
        match self {
            Self::Top => f.y,
            Self::Bottom => -f.y,
            Self::Left => f.x,
            Self::Right => -f.x,
        }
    }
}

impl Renderer<'_> {
    /// Corner letters (both levels), angles and right-angle marks (full level).
    fn corner_marks(&mut self, dm: &DimModel, it: &PdfItem, t: &Transform, full: bool) {
        let k = t.k;
        let n = dm.edges.len();
        for c in &dm.corners {
            let p = t.map(c.pos.x, c.pos.y);
            let v1 = -c.dir_in;
            let v2 = c.dir_out;
            let ia = c.angle * DEG;
            let s = {
                let (a, b2) = (rot(v1, ia), rot(v1, -ia));
                if a.x * v2.x + a.y * v2.y >= b2.x * v2.x + b2.y * v2.y {
                    1.0
                } else {
                    -1.0
                }
            };
            let bis = rot(v1, s * ia / 2.0);
            self.font(FontStyle::Bold, if full { 5.6 } else { 4.2 }, LETTER);
            let rl = if full { 2.7 } else { 1.6 };
            let (lx, ly) = (p[0] + bis.x * rl, p[1] + bis.y * rl);
            self.text(&c.letter, lx, ly, TextOpts::center().middle());
            self.occ_add(lx - 1.3, ly - 1.2, lx + 1.3, ly + 1.2);
            if !full || c.smooth {
                continue;
            }
            let (e0, e1) = (&dm.edges[(c.k + n - 1) % n], &dm.edges[c.k]);
            if e0.len.min(e1.len) * k < 7.0 {
                continue;
            }
            self.dim_ink();
            self.canvas.set_line_width(0.12);
            if (c.angle - 90.0).abs() < 0.3 {
                // perpendicular constraint
                let r = 1.9;
                self.plines(
                    vec![
                        [p[0] + v1.x * r, p[1] + v1.y * r],
                        [p[0] + (v1.x + v2.x) * r, p[1] + (v1.y + v2.y) * r],
                        [p[0] + v2.x * r, p[1] + v2.y * r],
                    ],
                    false,
                );
                continue;
            }
            let big_r = 4.6;
            let steps = 6.0_f64.max((c.angle / 8.0).ceil());
            let q = (0..=steps as usize)
                .map(|i| {
                    let d = rot(v1, s * ia * i as f64 / steps);
                    [p[0] + d.x * big_r, p[1] + d.y * big_r]
                })
                .collect();
            self.plines(q, false);
            self.canvas.set_font_size(5.0);
            self.canvas.set_text_color(super::render::DIM);
            let txt = format!("{}°", fixed(c.angle, 1));
            let tw = self.canvas.text_width(&txt);
            let th = 5.0 * MM_PER_PT;
            let r0 = big_r + if c.angle < 60.0 { 4.2 } else { 2.9 };
            let mut best: Option<[f64; 2]> = None;
            let mut best_a = f64::INFINITY;
            for (turn, dist) in [
                (0.0, r0),
                (0.35, r0 + 0.6),
                (-0.35, r0 + 0.6),
                (0.0, r0 + 3.0),
                (0.6, r0 + 2.0),
                (-0.6, r0 + 2.0),
                (0.0, r0 + 6.0),
            ] {
                let d = rot(bis, turn * 1.0_f64.min(ia / PI));
                let (x, y) = (p[0] + d.x * dist, p[1] + d.y * dist);
                if !contains(&it.geom.outline.poly, t.unmap(x, y)) {
                    continue;
                }
                let a = self.occ_hit([
                    x - tw / 2.0 - 0.3,
                    y - th / 2.0 - 0.3,
                    x + tw / 2.0 + 0.3,
                    y + th / 2.0 + 0.3,
                ]);
                if a < best_a - 1e-6 {
                    best_a = a;
                    best = Some([x, y]);
                }
            }
            let bp = best.unwrap_or([p[0] + bis.x * r0, p[1] + bis.y * r0]);
            self.text(&txt, bp[0], bp[1], TextOpts::center().middle());
            self.occ_add(
                bp[0] - tw / 2.0,
                bp[1] - th / 2.0,
                bp[0] + tw / 2.0,
                bp[1] + th / 2.0,
            );
        }
    }

    /// Overall width and height only, for small scales.
    fn basic_dims(&mut self, dm: &DimModel, it: &PdfItem, t: &Transform, show_sa: bool, extremes: [Vec2; 4]) {
        let k = t.k;
        let bb = dm.bb;
        let units = self.units;
        let sa_k = (if show_sa { sa_max(it.piece) } else { 0.0 }) * k;
        let off = sa_k + 5.0;
        let fs2 = 5.0_f64.max(9.0_f64.min(7.0 * k.sqrt() + 3.0));
        let corner = t.map(bb.x0, bb.y0);
        let (top, left) = (corner[1] - off, corner[0] - off);
        let [pl, pr, pt, pb] = extremes.map(|p| t.map(p.x, p.y));
        self.dim_ink();
        self.canvas.set_line_width(0.15);
        let edge_t = corner[1] - sa_k - 1.0;
        let edge_l = corner[0] - sa_k - 1.0;
        self.canvas
            .line(pl[0], (pl[1] - 1.0).min(edge_t), pl[0], top - 1.5);
        self.canvas
            .line(pr[0], (pr[1] - 1.0).min(edge_t), pr[0], top - 1.5);
        self.canvas.line(pl[0], top, pr[0], top);
        self.arrow(pl[0], top, -1.0, 0.0, 1.8);
        self.arrow(pr[0], top, 1.0, 0.0, 1.8);
        self.canvas
            .line((pt[0] - 1.0).min(edge_l), pt[1], left - 1.5, pt[1]);
        self.canvas
            .line((pb[0] - 1.0).min(edge_l), pb[1], left - 1.5, pb[1]);
        self.canvas.line(left, pt[1], left, pb[1]);
        self.arrow(left, pt[1], 0.0, -1.0, 1.8);
        self.arrow(left, pb[1], 0.0, 1.0, 1.8);
        self.label(
            &length_with_unit(bb.w, units),
            pl[0].midpoint(pr[0]),
            top,
            fs2,
            false,
        );
        self.label(
            &length_with_unit(bb.h, units),
            left,
            pt[1].midpoint(pb[1]),
            fs2,
            true,
        );
        if k >= 0.03 {
            self.corner_marks(dm, it, t, false);
        }
    }

    /// An aligned or arc-length dimension along part of a segment, `dp` paper mm outside the
    /// sewing line.
    #[allow(clippy::too_many_arguments, reason = "one dimension")]
    fn dim_along(
        &mut self,
        it: &PdfItem,
        t: &Transform,
        sgn: f64,
        seg: usize,
        la: f64,
        lb: f64,
        dp: f64,
        curve: bool,
        txt: &str,
    ) {
        let sg = &it.geom.outline.segments[seg];
        let mut q0 = vec![point_at_length(&sg.poly, &sg.cum, la).pos];
        for (p, &c) in sg.poly.iter().zip(&sg.cum) {
            if c > la + 1e-6 && c < lb - 1e-6 {
                q0.push(*p);
            }
        }
        q0.push(point_at_length(&sg.poly, &sg.cum, lb).pos);
        let m = q0.len();
        let nr: Vec<Vec2> = (0..m)
            .map(|i| {
                let (a, b) = (q0[i.saturating_sub(1)], q0[(i + 1).min(m - 1)]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let l = hypot(dx, dy);
                let l = if l == 0.0 { 1.0 } else { l };
                let (dx, dy) = (dx / l, dy / l);
                Vec2::new(dy * sgn, -dx * sgn)
            })
            .collect();
        let base: Vec<[f64; 2]> = q0.iter().map(|p| t.map(p.x, p.y)).collect();
        let q: Vec<[f64; 2]> = base
            .iter()
            .zip(&nr)
            .map(|(b, n)| [b[0] + n.x * dp, b[1] + n.y * dp])
            .collect();
        let bad = (1..m).any(|i| {
            let (ox, oy) = (base[i][0] - base[i - 1][0], base[i][1] - base[i - 1][1]);
            let (nx, ny) = (q[i][0] - q[i - 1][0], q[i][1] - q[i - 1][1]);
            ox * nx + oy * ny < 0.0
        });
        let mut cum = vec![0.0];
        for i in 1..m {
            cum.push(cum[i - 1] + hypot(q[i][0] - q[i - 1][0], q[i][1] - q[i - 1][1]));
        }
        let lp = cum[m - 1];
        if lp < 2.2 {
            return;
        }
        self.dim_ink();
        self.canvas.set_line_width(0.12);
        for i in [0, m - 1] {
            self.canvas.line(
                base[i][0] + nr[i].x * 0.7,
                base[i][1] + nr[i].y * 0.7,
                base[i][0] + nr[i].x * (dp + 1.1),
                base[i][1] + nr[i].y * (dp + 1.1),
            );
        }
        self.canvas.set_font_size(DFS);
        let tw = self.canvas.text_width(txt);
        if !bad {
            self.plines(q.clone(), false);
            if lp > tw + 4.5 {
                self.arrow(q[0][0], q[0][1], q[0][0] - q[1][0], q[0][1] - q[1][1], 1.5);
                self.arrow(
                    q[m - 1][0],
                    q[m - 1][1],
                    q[m - 1][0] - q[m - 2][0],
                    q[m - 1][1] - q[m - 2][1],
                    1.5,
                );
            }
        }
        // value at the middle of the dimension line, just outside it
        let mut j = 1;
        while j < m - 1 && cum[j] < lp / 2.0 {
            j += 1;
        }
        let span = cum[j] - cum[j - 1];
        let u = (lp / 2.0 - cum[j - 1]) / if span == 0.0 { 1.0 } else { span };
        let mx = q[j - 1][0] + (q[j][0] - q[j - 1][0]) * u;
        let my = q[j - 1][1] + (q[j][1] - q[j - 1][1]) * u;
        let (ux, uy) = (q[j][0] - q[j - 1][0], q[j][1] - q[j - 1][1]);
        let ul = hypot(ux, uy);
        let ul = if ul == 0.0 { 1.0 } else { ul };
        let (ux, uy) = (ux / ul, uy / ul);
        let ni = nr[(m - 1).min(j)];
        let h = DFS * MM_PER_PT;
        let lift =
            0.75 + h * 0.36 + if curve { 0.9 } else { 0.0 } + if lp < tw + 1.5 { h * 1.05 } else { 0.0 };
        let (cx, cy) = (mx + ni.x * lift, my + ni.y * lift);
        let r = self.rtext(txt, cx, cy, ux, uy, DFS);
        if curve {
            self.arc_sym(cx, cy, r);
        }
    }

    /// Dimensions of one piece: full CAD dimensioning, or overall sizes at small scales.
    #[allow(clippy::too_many_lines, reason = "one drawing pass in drawing order")]
    pub(super) fn draw_dims(&mut self, it: &PdfItem, t: &Transform, show_sa: bool) {
        let pc = it.piece;
        let g = &it.geom;
        let k = t.k;
        let ol = &g.outline;
        let dm = dim_model(pc, ol);
        let sgn = dm.winding;
        let bb = dm.bb;
        let units = self.units;
        let full = k >= DIM_FULL;
        let cb = Bounds::of(if show_sa { &g.cut } else { &ol.poly }.iter().copied());
        let (x0, x1) = (t.map(cb.x0, 0.0)[0], t.map(cb.x1, 0.0)[0]);
        let (y0, y1) = (t.map(0.0, cb.y0)[1], t.map(0.0, cb.y1)[1]);
        let mut ext = [ol.poly[0]; 4];
        for &q in &ol.poly {
            if q.x < ext[0].x {
                ext[0] = q;
            }
            if q.x > ext[1].x {
                ext[1] = q;
            }
            if q.y < ext[2].y {
                ext[2] = q;
            }
            if q.y > ext[3].y {
                ext[3] = q;
            }
        }
        if !full {
            self.basic_dims(&dm, it, t, show_sa, ext);
            return;
        }
        // edges (with chained notch positions)
        for (kk, e) in dm.edges.iter().enumerate() {
            let sg = &ol.segments[kk];
            let d1 = (if show_sa { e.allow } else { 0.0 }) * k + 3.0;
            let mut nts: Vec<f64> = pc
                .notches
                .iter()
                .filter(|n| n.s == sg.id)
                .map(|n| sg.len * n.t.unwrap_or(0.0))
                .filter(|&l| l > 0.5 && l < sg.len - 0.5)
                .collect();
            nts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            if nts.is_empty() {
                self.dim_along(
                    it,
                    t,
                    sgn,
                    kk,
                    0.0,
                    sg.len,
                    d1,
                    e.curve,
                    &length_number(sg.len, units),
                );
            } else {
                let mut cuts = vec![0.0];
                cuts.extend(nts);
                cuts.push(sg.len);
                for w in cuts.windows(2) {
                    self.dim_along(
                        it,
                        t,
                        sgn,
                        kk,
                        w[0],
                        w[1],
                        d1,
                        e.curve,
                        &length_number(w[1] - w[0], units),
                    );
                }
                self.dim_along(
                    it,
                    t,
                    sgn,
                    kk,
                    0.0,
                    sg.len,
                    d1 + 4.2,
                    e.curve,
                    &length_number(sg.len, units),
                );
            }
        }
        // internal lines: length along the line
        for il in &pc.internals {
            let pl = flatten_open_line(&line_points_closed(il));
            if pl.len() < 2 {
                continue;
            }
            let cm = cumulative_lengths(&pl);
            let l = cm[cm.len() - 1];
            if l * k < 8.0 {
                continue;
            }
            let mp = point_at_length(&pl, &cm, l / 2.0);
            let p = t.map(mp.pos.x, mp.pos.y);
            self.dim_ink();
            let (cx, cy) = (p[0] - mp.tangent.y * 1.6, p[1] + mp.tangent.x * 1.6);
            let r = self.rtext(&length_number(l, units), cx, cy, mp.tangent.x, mp.tangent.y, DFS);
            if il.pts.iter().any(|q| q.curve) {
                self.arc_sym(cx, cy, r);
            }
        }
        self.ordinates(&dm, it, t, [x0, x1, y0, y1]);
        // overall size (outermost) is drawn by `ordinates`; then the datum and centre line
        let l0 = t.map(bb.x0, 0.0)[0];
        let t0 = t.map(0.0, bb.y0)[1];
        self.canvas.set_line_width(0.12);
        self.canvas.circle(l0, t0, 0.9, crate::pdf::canvas::Style::Stroke);
        self.canvas.line(l0 - 1.6, t0, l0 + 1.6, t0);
        self.canvas.line(l0, t0 - 1.6, l0, t0 + 1.6);
        if dm.symmetric {
            let cxp = t.map(dm.cx, 0.0)[0];
            self.canvas.set_draw_color(CENTRE_LINE);
            self.canvas.set_line_width(0.15);
            self.canvas.set_dash(&[6.0, 1.2, 1.0, 1.2], 0.0);
            self.canvas.line(cxp, y0 - 3.0, cxp, y1 + 3.0);
            self.canvas.set_dash(&[], 0.0);
            self.font(FontStyle::Bold, 5.4, CENTRE_LINE);
            self.canvas.set_font(FontStyle::Normal);
        }
        self.corner_marks(&dm, it, t, true);
        if dm.symmetric {
            let cxp = t.map(dm.cx, 0.0)[0];
            self.font(FontStyle::Bold, 5.4, CENTRE_LINE);
            let mut yb = None;
            let mut y = y0 + 9.0;
            while y < y1 - 6.0 && yb.is_none() {
                if contains(&ol.poly, Vec2::new(dm.cx, (y - t.oy) / k + t.by))
                    && self.occ_hit([cxp + 0.5, y - 1.3, cxp + 4.6, y + 1.3]) == 0.0
                {
                    yb = Some(y);
                }
                y += 2.5;
            }
            if let Some(yb) = yb {
                self.text("CL", cxp + 1.0, yb, TextOpts::left().middle());
                self.occ_add(cxp + 0.5, yb - 1.2, cxp + 4.4, yb + 1.2);
            }
            self.canvas.set_font(FontStyle::Normal);
        }
    }

    /// Ordinates from the datum (each feature to the nearest rail it can reach), then the
    /// overall width and height outside them.
    fn ordinates(&mut self, dm: &DimModel, it: &PdfItem, t: &Transform, cut: [f64; 4]) {
        let [x0, x1, y0, y1] = cut;
        let bb = dm.bb;
        let units = self.units;
        let rails_order = RAILS;
        let rails = ordinate_rails(dm, it);
        let h = DFS * MM_PER_PT;
        let gap = h + 0.55;
        let mut max_w = [0.0; 4];
        self.dim_ink();
        self.canvas.set_line_width(0.12);
        self.canvas.set_font_size(DFS);
        for (ri, key) in rails_order.iter().enumerate() {
            let arr = rails[ri].clone();
            if arr.is_empty() {
                continue;
            }
            let horiz = matches!(key, Rail::Top | Rail::Bottom);
            let rail = match key {
                Rail::Top => y0 - RAIL_OFFSET,
                Rail::Bottom => y1 + RAIL_OFFSET,
                Rail::Left => x0 - RAIL_OFFSET,
                Rail::Right => x1 + RAIL_OFFSET,
            };
            let dir = if matches!(key, Rail::Top | Rail::Left) {
                -1.0
            } else {
                1.0
            };
            let fp: Vec<[f64; 2]> = arr.iter().map(|e| t.map(e.f.x, e.f.y)).collect();
            let want: Vec<f64> = fp.iter().map(|p| if horiz { p[0] } else { p[1] }).collect();
            let at = spread(&want, gap);
            for (i, e) in arr.iter().enumerate() {
                let p = fp[i];
                let txt = length_number(e.v, units);
                let w = self.canvas.text_width(&txt);
                max_w[ri] = f64::max(max_w[ri], w);
                if horiz {
                    self.canvas.line(p[0], p[1] + dir * 0.7, p[0], rail);
                    self.canvas.line(p[0], rail, at[i], rail + dir * RAIL_JOG);
                    self.rtext(
                        &txt,
                        at[i],
                        rail + dir * (RAIL_TEXT_GAP + w / 2.0),
                        0.0,
                        -1.0,
                        DFS,
                    );
                } else {
                    self.canvas.line(p[0] + dir * 0.7, p[1], rail, p[1]);
                    self.canvas.line(rail, p[1], rail + dir * RAIL_JOG, at[i]);
                    self.rtext(&txt, rail + dir * (RAIL_TEXT_GAP + w / 2.0), at[i], 1.0, 0.0, DFS);
                }
            }
        }
        // overall size (outermost)
        let ov_t = if rails[0].is_empty() {
            y0 - 5.0
        } else {
            y0 - RAIL_OFFSET - RAIL_TEXT_GAP - max_w[0] - 3.4
        };
        let ov_l = if rails[2].is_empty() {
            x0 - 5.0
        } else {
            x0 - RAIL_OFFSET - RAIL_TEXT_GAP - max_w[2] - 3.4
        };
        self.overall_size(&bb, t, [x0, y0], [ov_t, ov_l]);
    }

    /// Overall width and height, outside the ordinates (`ov`: top and left dimension lines).
    fn overall_size(&mut self, bb: &Bounds, t: &Transform, cut_min: [f64; 2], ov: [f64; 2]) {
        let [x0, y0] = cut_min;
        let [ov_t, ov_l] = ov;
        let units = self.units;
        let h = DFS * MM_PER_PT;
        let (l0, r0) = (t.map(bb.x0, 0.0)[0], t.map(bb.x1, 0.0)[0]);
        let (t0, b0) = (t.map(0.0, bb.y0)[1], t.map(0.0, bb.y1)[1]);
        self.dim_ink();
        self.canvas.set_line_width(0.12);
        self.canvas.line(l0, y0 - 1.0, l0, ov_t - 1.2);
        self.canvas.line(r0, y0 - 1.0, r0, ov_t - 1.2);
        self.canvas.line(l0, ov_t, r0, ov_t);
        self.arrow(l0, ov_t, -1.0, 0.0, 1.6);
        self.arrow(r0, ov_t, 1.0, 0.0, 1.6);
        self.canvas.line(x0 - 1.0, t0, ov_l - 1.2, t0);
        self.canvas.line(x0 - 1.0, b0, ov_l - 1.2, b0);
        self.canvas.line(ov_l, t0, ov_l, b0);
        self.arrow(ov_l, t0, 0.0, -1.0, 1.6);
        self.arrow(ov_l, b0, 0.0, 1.0, 1.6);
        self.canvas.set_font(FontStyle::Bold);
        self.rtext(
            &length_number(bb.w, units),
            l0.midpoint(r0),
            ov_t - 0.8 - h * 0.36,
            1.0,
            0.0,
            DFS + 0.6,
        );
        self.rtext(
            &length_number(bb.h, units),
            ov_l - 0.8 - h * 0.36,
            t0.midpoint(b0),
            0.0,
            -1.0,
            DFS + 0.6,
        );
        self.canvas.set_font(FontStyle::Normal);
    }
}

/// Rails in drawing order.
const RAILS: [Rail; 4] = [Rail::Top, Rail::Bottom, Rail::Left, Rail::Right];

/// Ordinates of every corner and internal-line end, each on the nearest rail it can reach
/// without crossing the outline, one per value (from the feature nearest the rail).
fn ordinate_rails(dm: &DimModel, it: &PdfItem) -> [Vec<Ordinate>; 4] {
    let ol = &it.geom.outline;
    let bb = dm.bb;
    let mut feats: Vec<Vec2> = dm.corners.iter().map(|c| c.pos).collect();
    for il in &it.piece.internals {
        let q = flatten_open_line(&line_points(il));
        if let (Some(&a), Some(&b)) = (q.first(), q.last()) {
            feats.push(a);
            feats.push(b);
        }
    }
    let mut rails: [Vec<Ordinate>; 4] = Default::default();
    for &f in &feats {
        let score = |dx: f64, dy: f64, dist: f64| ray_crossings(&ol.poly, f, dx, dy, 0.8) as f64 * 1e6 + dist;
        let (st, sb) = (score(0.0, -1.0, f.y - bb.y0), score(0.0, 1.0, bb.y1 - f.y));
        let (sl, sr) = (score(-1.0, 0.0, f.x - bb.x0), score(1.0, 0.0, bb.x1 - f.x));
        rails[usize::from(st > sb)].push(Ordinate { v: f.x - bb.x0, f });
        rails[2 + usize::from(sl > sr)].push(Ordinate { v: f.y - bb.y0, f });
    }
    // one tick per value, from the feature nearest the rail
    for (key, list) in RAILS.iter().zip(rails.iter_mut()) {
        let near = |e: &Ordinate| key.near(e.f);
        list.sort_by(|a, b| {
            let d = a.v - b.v;
            let d = if d == 0.0 || d.is_nan() {
                near(a) - near(b)
            } else {
                d
            };
            d.partial_cmp(&0.0).unwrap_or(Ordering::Equal)
        });
        let mut out: Vec<Ordinate> = Vec::new();
        for e in list.iter() {
            match out.last_mut() {
                Some(l) if (l.v - e.v).abs() < 0.3 => {
                    if near(e) < near(l) {
                        *l = *e;
                    }
                }
                _ => out.push(*e),
            }
        }
        *list = out;
    }
    rails
}
