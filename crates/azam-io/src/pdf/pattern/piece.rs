//! One pattern piece on paper: allowance bands, cut and sewing lines, internal lines, notches,
//! edge labels, seam numbers, grain line, piece label and segment lengths.

use azam_doc::edge_coverage;
use azam_geom::{Vec2, contains, flatten_open_line, signed_area};
use serde_json::Value;

use super::Layout;
use super::plan::{DIM_FULL, PdfItem};
use super::render::{
    DIM, HEM_FILL, INK, MM_PER_PT, MUTE, NEST_COLORS, Renderer, SA_FILL, SEAM_COLOR, Transform, hex, sign,
};
use crate::dimensions::label_pole;
use crate::format::length_with_unit;
use crate::model::{DEG, edge_allowance, hem, line_points_closed, or_default, qty};
use crate::pdf::canvas::{Align, Baseline, Rgb, Style, TextOpts, WHITE};
use crate::pdf::metrics::FontStyle;

/// A label placed after everything else, where it collides least.
struct Deferred {
    p0: f64,
    p1: f64,
    d: f64,
    txt: String,
    font: FontStyle,
    color: Rgb,
    size: f64,
}

/// Fold angle of a line that lies flat (not a fold), in degrees.
const FLAT_FOLD_DEG: f64 = 180.0;

/// A run of consecutive edges with the same finish and allowance width.
struct Run {
    hem: bool,
    v: f64,
    p0: f64,
    p1: f64,
}

/// Whether an internal line is a fold line at an angle other than flat.
#[allow(clippy::float_cmp, reason = "compares with an exact stored angle")]
fn is_angled_fold(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) if s.is_empty() => false,
        Some(Value::String(s)) => !s.trim().parse::<f64>().is_ok_and(|f| f == FLAT_FOLD_DEG),
        Some(Value::Number(n)) => n.as_f64().is_none_or(|f| f != FLAT_FOLD_DEG),
        Some(_) => true,
    }
}

impl Renderer<'_> {
    /// A point `d` model mm outside the sewing line (inside when negative) at perimeter `p`.
    fn out_at(it: &PdfItem, sgn: f64, p: f64, d: f64) -> (Vec2, Vec2) {
        let q = it.geom.outline.point_at_perimeter(p);
        (
            Vec2::new(q.pos.x + q.tangent.y * sgn * d, q.pos.y - q.tangent.x * sgn * d),
            q.tangent,
        )
    }

    /// Text beside the sewing line at perimeter `p`, `d` model mm out (in when negative),
    /// aligned away from the line. Returns its box; draws it unless `measure_only`.
    #[allow(clippy::too_many_arguments, reason = "placement of one label")]
    fn edge_text(
        &mut self,
        it: &PdfItem,
        t: &Transform,
        sgn: f64,
        p: f64,
        d: f64,
        txt: &str,
        measure_only: bool,
    ) -> [f64; 4] {
        let (q, tan) = Self::out_at(it, sgn, p, d);
        let pq = t.map(q.x, q.y);
        let sx = sign(d) * tan.y * sgn;
        let sy = -sign(d) * tan.x * sgn;
        let (align, baseline) = if sx.abs() > 0.45 {
            (
                if sx > 0.0 { Align::Left } else { Align::Right },
                Baseline::Middle,
            )
        } else {
            (
                Align::Center,
                if sy > 0.0 { Baseline::Top } else { Baseline::Middle },
            )
        };
        let ty = pq[1]
            + if baseline == Baseline::Middle && sx.abs() <= 0.45 {
                -0.8
            } else {
                0.0
            };
        let tw = self.uni_width(txt);
        let th = self.canvas.font_size() * MM_PER_PT;
        let x0 = match align {
            Align::Left => pq[0],
            Align::Right => pq[0] - tw,
            Align::Center => pq[0] - tw / 2.0,
        };
        let y0 = if baseline == Baseline::Top {
            ty
        } else {
            ty - th * 0.5
        };
        let bx = [x0 - 0.3, y0 - 0.3, x0 + tw + 0.3, y0 + th + 0.3];
        if measure_only {
            return bx;
        }
        let opts = TextOpts {
            align,
            baseline,
            ..TextOpts::default()
        };
        self.text(txt, pq[0], ty, opts);
        self.occ_add(x0, y0, x0 + tw, y0 + th);
        bx
    }

    /// Draws one piece through transform `t`.
    #[allow(clippy::too_many_lines, reason = "one drawing pass in drawing order")]
    pub(super) fn draw_piece(&mut self, it: &PdfItem, t: &Transform) {
        let project = self.project;
        let pc = it.piece;
        let g = &it.geom;
        let ol = &g.outline;
        let k = t.k;
        let fsz = 7.5_f64.max(12.0_f64.min(9.0 * k.sqrt() + 3.0));
        let sgn = if signed_area(&ol.poly) > 0.0 { 1.0 } else { -1.0 };
        let cov = edge_coverage(project, pc, ol, Some(&self.nums));
        let show_sa = self.o.sa && g.has_sa;
        let full_dims = self.o.dims && k >= DIM_FULL;
        let units = self.units;
        self.occ.clear();
        let mut deferred: Vec<Deferred> = Vec::new();

        // allowance bands: seam allowance tint, hem allowance darker
        if show_sa {
            self.fill_poly(&g.cut, t, SA_FILL);
            for sg in &ol.segments {
                let h = hem(pc, &sg.id);
                if h <= 0.0 {
                    continue;
                }
                let p = &sg.poly;
                let n = p.len();
                let outer: Vec<Vec2> = (0..n)
                    .map(|i| {
                        let (q, r) = (p[(i + 1).min(n - 1)], p[i.saturating_sub(1)]);
                        let (dx, dy) = (q.x - r.x, q.y - r.y);
                        let l = azam_geom::hypot(dx, dy);
                        let l = if l == 0.0 { 1.0 } else { l };
                        Vec2::new(p[i].x + dy / l * sgn * h, p[i].y - dx / l * sgn * h)
                    })
                    .collect();
                let band: Vec<Vec2> = p.iter().copied().chain(outer.into_iter().rev()).collect();
                self.fill_poly(&band, t, HEM_FILL);
            }
            self.fill_poly(&ol.poly, t, WHITE);
        }
        // nested sizes
        if let Some(nest) = &it.nest {
            for n in nest {
                if n.size == project.active_size {
                    continue;
                }
                self.canvas
                    .set_draw_color(hex(NEST_COLORS[n.index % NEST_COLORS.len()]));
                self.canvas.set_line_width(0.18);
                self.canvas.set_dash(&[], 0.0);
                self.poly(&n.poly, t, true);
            }
        }
        // cut line
        self.canvas.set_draw_color(INK);
        self.canvas.set_dash(&[], 0.0);
        self.canvas.set_line_width(0.4);
        self.poly(if show_sa { &g.cut } else { &ol.poly }, t, true);
        // sewing line: sewn parts dashed; free parts solid (finished edge) or dash-dot (hem fold)
        if show_sa {
            self.canvas.set_line_width(0.22);
            self.canvas.set_dash(&[1.6, 1.1], 0.0);
            for m in &cov.sewn {
                self.poly(&ol.path_between(m[0], m[1]), t, false);
            }
        }
        for f in &cov.free_parts {
            let is_hem = hem(pc, &f.segment_id) > 0.0;
            self.canvas
                .set_dash(if is_hem { &[3.2, 1.0, 0.5, 1.0] } else { &[] }, 0.0);
            self.canvas
                .set_line_width(if is_hem || show_sa { 0.3 } else { 0.5 });
            self.canvas.set_draw_color(INK);
            if show_sa || is_hem {
                self.poly(&ol.path_between(f.start, f.end), t, false);
            }
        }
        self.canvas.set_dash(&[], 0.0);
        // internal lines
        for il in &pc.internals {
            let pl = flatten_open_line(&line_points_closed(il));
            self.canvas.set_draw_color([90, 90, 98]);
            self.canvas.set_line_width(0.2);
            if is_angled_fold(il.extra.get("fold")) {
                self.canvas.set_dash(&[2.4, 1.0, 0.4, 1.0], 0.0);
            } else {
                self.canvas.set_dash(&[1.2, 0.9], 0.0);
            }
            self.poly(&pl, t, false);
            self.canvas.set_dash(&[], 0.0);
        }
        // notches: from the sewing line out through the allowance
        for n in &pc.notches {
            let Some(kk) = ol.segment_index(&n.s) else {
                continue;
            };
            let pt = ol.segments[kk].point_at(n.t.unwrap_or(0.0));
            let (ox, oy) = (pt.tangent.y * sgn, -pt.tangent.x * sgn);
            let sa = if show_sa { edge_allowance(pc, &n.s) } else { 0.0 };
            let ln = if sa > 0.0 {
                sa
            } else {
                -(4.0_f64.max(1.5 / k.max(0.05)))
            };
            let a = t.map(pt.pos.x, pt.pos.y);
            let b = t.map(pt.pos.x + ox * ln, pt.pos.y + oy * ln);
            self.canvas.set_draw_color(INK);
            self.canvas.set_line_width(0.3);
            self.canvas.line(a[0], a[1], b[0], b[1]);
        }
        let small = 5.2_f64.max(7.0_f64.min(fsz * 0.6));
        // "free edge" labels (free parts without a hem)
        for f in &cov.free_parts {
            if hem(pc, &f.segment_id) > 0.0 || (f.end - f.start) * k < 16.0 {
                continue;
            }
            self.font(FontStyle::Italic, small, MUTE);
            self.edge_text(it, t, sgn, f.start.midpoint(f.end), -1.6 / k, "free edge", false);
        }
        // allowance labels: one per run of consecutive edges with the same finish and width
        let mut runs: Vec<Run> = Vec::new();
        for (kk, sg) in ol.segments.iter().enumerate() {
            let is_h = hem(pc, &sg.id) > 0.0;
            let v = edge_allowance(pc, &sg.id);
            #[allow(clippy::float_cmp, reason = "runs group identical stored widths")]
            match runs.last_mut() {
                Some(last) if last.hem == is_h && last.v == v => last.p1 = ol.starts[kk] + sg.len,
                _ => runs.push(Run {
                    hem: is_h,
                    v,
                    p0: ol.starts[kk],
                    p1: ol.starts[kk] + sg.len,
                }),
            }
        }
        #[allow(clippy::float_cmp, reason = "runs group identical stored widths")]
        if runs.len() > 1
            && runs[0].hem == runs[runs.len() - 1].hem
            && runs[0].v == runs[runs.len() - 1].v
            && let Some(l) = runs.pop()
        {
            runs[0].p0 = l.p0 - ol.total;
        }
        if !ol.segments.is_empty() {
            for rn in &runs {
                let len_pg = (rn.p1 - rn.p0) * k;
                let mid = rn.p0.midpoint(rn.p1);
                if rn.hem {
                    if len_pg < 10.0 {
                        continue;
                    }
                    let h = rn.v;
                    let q0 = ol.point_at_perimeter(mid);
                    let vertical = q0.tangent.x.abs() < 0.6;
                    let in_band = show_sa && h * k >= 3.2;
                    self.font(FontStyle::Bold, small, INK);
                    let label = format!("HEM {}", length_with_unit(h, units));
                    if in_band {
                        let (q, _) = Self::out_at(it, sgn, mid, h / 2.0);
                        let pq = t.map(q.x, q.y);
                        let opts = TextOpts::center().middle();
                        self.text(
                            &label,
                            pq[0],
                            pq[1],
                            if vertical { opts.rotated(90.0) } else { opts },
                        );
                    } else if full_dims {
                        deferred.push(Deferred {
                            p0: rn.p0,
                            p1: rn.p1,
                            d: -1.4 / k,
                            txt: format!("{label} (fold)"),
                            font: FontStyle::Bold,
                            color: INK,
                            size: small,
                        });
                        continue;
                    } else {
                        self.edge_text(
                            it,
                            t,
                            sgn,
                            mid,
                            (if show_sa { h } else { 0.0 }) + 1.6 / k,
                            &label,
                            false,
                        );
                    }
                    self.font(FontStyle::Italic, small * 0.9, MUTE);
                    self.edge_text(it, t, sgn, mid, -1.4 / k, "hem fold", false);
                } else if show_sa && k >= 0.09 && rn.v > 0.0 && len_pg >= 12.0 {
                    self.font(FontStyle::Normal, small, DIM);
                    let label = format!("SA {}", length_with_unit(rn.v, units));
                    if full_dims {
                        deferred.push(Deferred {
                            p0: rn.p0,
                            p1: rn.p1,
                            d: -1.5 / k,
                            txt: label,
                            font: FontStyle::Italic,
                            color: MUTE,
                            size: small,
                        });
                    } else {
                        self.edge_text(
                            it,
                            t,
                            sgn,
                            rn.p0 + (rn.p1 - rn.p0) * 0.62,
                            rn.v + 1.6 / k,
                            &label,
                            false,
                        );
                    }
                }
            }
        }
        // seam numbers on every sewn side (same number on both pieces)
        let rb = if k >= 0.25 { 2.4 } else { 1.9 };
        for sd in &cov.sides {
            let mid = sd.start.midpoint(sd.end);
            let (q, _) = Self::out_at(it, sgn, mid, -(rb + 1.6) / k);
            let pq = t.map(q.x, q.y);
            let color = project
                .sewing
                .get(&sd.seam)
                .map(|sw| sw.color.as_str())
                .filter(|c| !c.is_empty())
                .unwrap_or(SEAM_COLOR);
            self.canvas.set_fill_color(hex(color));
            self.canvas.circle(pq[0], pq[1], rb, Style::Fill);
            self.occ_add(
                pq[0] - rb - 0.3,
                pq[1] - rb - 0.3,
                pq[0] + rb + 0.3,
                pq[1] + rb + 0.3,
            );
            self.font(FontStyle::Bold, if rb >= 2.4 { 7.5 } else { 6.2 }, WHITE);
            let n = sd.number.unwrap_or(0).to_string();
            self.text(&n, pq[0], pq[1] + 0.05, TextOpts::center().middle());
            if self.o.layout == Layout::Pieces && (sd.end - sd.start) * k > 26.0 {
                let mut partners = Vec::new();
                for pid in &sd.partners {
                    if !partners.contains(&pid) {
                        partners.push(pid);
                    }
                }
                let names: Vec<&str> = partners
                    .iter()
                    .map(|&pid| {
                        if *pid == pc.id {
                            "same piece"
                        } else {
                            project
                                .pieces
                                .get(pid)
                                .map(|p| p.name.as_str())
                                .filter(|n| !n.is_empty())
                                .unwrap_or("?")
                        }
                    })
                    .collect();
                self.font(FontStyle::Normal, 5.6, MUTE);
                self.edge_text(it, t, sgn, mid, -(2.0 * rb + 2.6) / k, &names.join(" + "), false);
            }
        }
        self.grain_and_label(it, t, fsz);
        // segment lengths (superseded by the edge dimensions when the piece is fully dimensioned)
        if self.o.lengths && !(self.o.dims && k >= DIM_FULL) {
            self.canvas.set_text_color(DIM);
            for sg in &ol.segments {
                if sg.len < 8.0 / k.max(0.05) {
                    continue;
                }
                let m = sg.point_at(0.5);
                let off = (if show_sa { edge_allowance(pc, &sg.id) } else { 0.0 }) + 7.0 / k;
                let q = t.map(
                    m.pos.x + m.tangent.y * sgn * off,
                    m.pos.y - m.tangent.x * sgn * off,
                );
                self.label(
                    &length_with_unit(sg.len, units),
                    q[0],
                    q[1],
                    5.0_f64.max(fsz * 0.62),
                    false,
                );
            }
        }
        if self.o.dims {
            self.draw_dims(it, t, show_sa);
        }
        for dfr in deferred {
            self.font(dfr.font, dfr.size, dfr.color);
            let mut best = dfr.p0;
            let mut best_d = dfr.d;
            let mut best_a = f64::INFINITY;
            for dm in [1.0, 2.6, 4.2] {
                for f in [0.5, 0.3, 0.7, 0.18, 0.82, 0.4, 0.6] {
                    if best_a == 0.0 {
                        continue;
                    }
                    let p = dfr.p0 + (dfr.p1 - dfr.p0) * f;
                    let bx = self.edge_text(it, t, sgn, p, dfr.d * dm, &dfr.txt, true);
                    let a = self.occ_hit(bx) + (dm - 1.0) * 0.01;
                    if a < best_a - 1e-6 {
                        best_a = if a < 0.02 { 0.0 } else { a };
                        best = p;
                        best_d = dfr.d * dm;
                    }
                }
            }
            self.edge_text(it, t, sgn, best, best_d, &dfr.txt, false);
        }
    }

    /// The grain line (kept inside the piece) and the piece label at the roomiest free spot.
    fn grain_and_label(&mut self, it: &PdfItem, t: &Transform, fsz: f64) {
        let project = self.project;
        let pc = it.piece;
        let ol = &it.geom.outline;
        let bb = it.geom.bb;
        let k = t.k;
        let gr = or_default(pc.grain, 0.0) * DEG;
        let pole = label_pole(ol);
        let fab = project.fabrics.get(&pc.fabric);
        self.font(FontStyle::Bold, fsz, INK);
        let lab_w = self.uni_width(&pc.name).max(18.0) + 1.0;
        let lab_box = |cx: f64, cy: f64| {
            [
                cx + 2.5,
                cy - fsz * 0.25 - fsz * MM_PER_PT * 0.8,
                cx + 3.0 + lab_w,
                cy + fsz * 0.25 + 3.0 * fsz * 0.34,
            ]
        };
        let mut c = t.map(pole.pos.x, pole.pos.y);
        if self.occ_hit(lab_box(c[0], c[1])) > 0.0 {
            // look for a roomier spot that stays inside the piece
            let mut best_a = self.occ_hit(lab_box(c[0], c[1]));
            for (cd, _) in &pole.candidates {
                let q = t.map(cd.x, cd.y);
                let b = lab_box(q[0], q[1]);
                let inv = |x: f64, y: f64| contains(&ol.poly, t.unmap(x, y));
                if !(inv(b[0], b[1]) && inv(b[2], b[1]) && inv(b[0], b[3]) && inv(b[2], b[3])) {
                    continue;
                }
                let a = self.occ_hit(b);
                if a < best_a - 1e-6 {
                    best_a = a;
                    c = q;
                }
            }
        }
        let mut gl = bb.h * 0.32;
        let gcx = (c[0] - t.ox) / k + t.bx;
        let mut gcy = bb.cy;
        for tries in 0..12 {
            let ends_inside = contains(&ol.poly, Vec2::new(gcx + gr.sin() * gl, gcy - gr.cos() * gl))
                && contains(&ol.poly, Vec2::new(gcx - gr.sin() * gl, gcy + gr.cos() * gl));
            if ends_inside {
                break;
            }
            if tries == 5 {
                gcy = pole.pos.y;
            }
            gl *= 0.85;
        }
        let ga = t.map(gcx + gr.sin() * gl, gcy - gr.cos() * gl);
        let gb = t.map(gcx - gr.sin() * gl, gcy + gr.cos() * gl);
        self.occ_add(
            ga[0].min(gb[0]) - 1.2,
            ga[1].min(gb[1]),
            ga[0].max(gb[0]) + 1.2,
            ga[1].max(gb[1]),
        );
        self.canvas.set_draw_color(INK);
        self.canvas.set_fill_color(INK);
        self.canvas.set_line_width(0.25);
        self.canvas.line(ga[0], ga[1], gb[0], gb[1]);
        self.arrow(ga[0], ga[1], ga[0] - gb[0], ga[1] - gb[1], 2.4);
        self.arrow(gb[0], gb[1], gb[0] - ga[0], gb[1] - ga[1], 2.4);
        let lx = c[0] + 3.0;
        let b = lab_box(c[0], c[1]);
        self.occ_add(b[0], b[1], b[2], b[3]);
        self.text(&pc.name, lx, c[1] - fsz * 0.25, TextOpts::left());
        self.font(FontStyle::Normal, fsz * 0.72, MUTE);
        let lines = [
            format!("Size {}", project.active_size.0),
            format!(
                "Cut {}{}",
                crate::format::number(qty(pc)),
                if pc.mirror.is_some() { " (mirrored)" } else { "" }
            ),
            fab.map(|f| f.name.clone()).unwrap_or_default(),
        ];
        for (i, line) in lines.iter().filter(|l| !l.is_empty()).enumerate() {
            self.text(
                line,
                lx,
                c[1] + fsz * 0.25 + i as f64 * fsz * 0.34,
                TextOpts::left(),
            );
        }
    }
}
