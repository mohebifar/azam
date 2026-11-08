//! Feedback the tools draw over the pattern: hovered edges, drafts being drawn, seams being
//! picked, the selection box, measurements, snapping and the rotation handle.

use azam_geom::Vec2;
use blenderui::Painter;
use blenderui::prelude::*;

use super::parts::dashed;
use super::{SELECTED, line};

/// Hover highlight colour.
const HOVER: u32 = 0xffffff;
/// Colour of side B while a seam is picked.
const SIDE_B: u32 = 0x3b82f6;
/// Colour of the snapping ring.
const SNAP: u32 = 0x10b981;
/// Measurement colour.
const MEASURE: u32 = 0x6fd3ff;
/// The Cut tool's line (Blender's knife red).
const CUT_LINE: u32 = 0xe0_4a_3f;
/// Distance of the rotation handle above the selection's box, in logical pixels.
pub const ROTATE_HANDLE_PX: f32 = 26.0;
/// Radius of the rotation handle, in logical pixels.
const ROTATE_HANDLE_R: f32 = 6.0;
/// Margin of the dashed box around rotated pieces, in logical pixels.
const BOX_MARGIN_PX: f32 = 4.0;

/// A polygon or internal line being drawn point by point.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    /// Points placed so far: position and whether it is a curve point.
    pub points: Vec<(Vec2, bool)>,
    /// Where the next point would go.
    pub cursor: Vec2,
    /// Dashed (an internal line) rather than solid (a piece).
    pub internal: bool,
}

/// A rectangle or ellipse being dragged out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapePreview {
    /// One corner of the box.
    pub a: Vec2,
    /// The opposite corner.
    pub b: Vec2,
    /// An ellipse in the box rather than the box itself.
    pub ellipse: bool,
}

/// Everything the tools draw over the scene, in pattern space.
#[derive(Clone, Debug, Default)]
pub struct Overlay {
    /// The units lengths are shown in.
    pub units: azam_doc::Units,
    /// The segment under the pointer.
    pub hover_edge: Option<Vec<Vec2>>,
    /// The nearest point on it.
    pub hover_dot: Option<Vec2>,
    /// The end a segment seam would start from.
    pub hover_start: Option<Vec2>,
    /// The outline of the piece under the pointer.
    pub hover_outline: Option<Vec<Vec2>>,
    /// Length chips: where (pattern space) and the text.
    pub labels: Vec<(Vec2, String)>,
    /// Seam sides picked so far: polyline and whether it belongs to side B.
    pub sew_sides: Vec<(Vec<Vec2>, bool)>,
    /// The stretch a free seam side would cover.
    pub sew_preview: Option<Vec<Vec2>>,
    /// The first end of a free seam side.
    pub pending_point: Option<Vec2>,
    /// A polygon or internal line being drawn.
    pub draft: Option<Draft>,
    /// A shape being dragged out.
    pub shape: Option<ShapePreview>,
    /// A selection box: corners.
    pub selection_box: Option<(Vec2, Vec2)>,
    /// A measurement: from and to.
    pub measure: Option<(Vec2, Vec2)>,
    /// A cut line being drawn: from and to.
    pub cut: Option<(Vec2, Vec2)>,
    /// Where the pointer snapped to.
    pub snap: Option<Vec2>,
    /// Box of the pieces the rotation handle turns: min and max corners.
    pub rotate_box: Option<(Vec2, Vec2)>,
    /// The button picked to fasten.
    pub fasten_from: Option<Vec2>,
}

/// A text chip centred at (x, y).
fn chip(p: &mut Painter, x: f32, y: f32, text: &str, fill: impl Into<Rgba>, color: impl Into<Rgba>) {
    let w = text.chars().count() as f32 * 6.2 + 10.0;
    p.quad(
        Bounds {
            origin: point(x - w / 2.0, y - 8.0),
            size: size(w, 15.0),
        },
        3.0,
        fill,
        rgba(0),
        0.0,
    );
    p.text(x - w / 2.0 + 5.0, y - 7.5, text, 10.5, color);
}

fn ring(p: &mut Painter, (x, y): (f32, f32), r: f32, color: impl Into<Rgba>, width: f32) {
    p.oval(
        Bounds {
            origin: point(x - r, y - r),
            size: size(2.0 * r, 2.0 * r),
        },
        rgba(0),
        color,
        width,
    );
}

fn dot(p: &mut Painter, (x, y): (f32, f32), r: f32, color: impl Into<Rgba>) {
    let color = color.into();
    p.oval(
        Bounds {
            origin: point(x - r, y - r),
            size: size(2.0 * r, 2.0 * r),
        },
        color,
        color,
        0.0,
    );
}

/// The rotation handle's position for the box `lo`..`hi`, in logical pixels.
#[must_use]
pub fn rotate_handle_px(lo: Vec2, hi: Vec2, to_px: &impl Fn(Vec2) -> (f32, f32)) -> (f32, f32) {
    let (x, y) = to_px(Vec2::new(f64::midpoint(lo.x, hi.x), lo.y));
    (x, y - BOX_MARGIN_PX - ROTATE_HANDLE_PX)
}

impl Overlay {
    /// Paints the overlay.
    pub(super) fn paint(&self, p: &mut Painter, to_px: &impl Fn(Vec2) -> (f32, f32)) {
        if let Some(outline) = &self.hover_outline {
            let mut px: Vec<(f32, f32)> = outline.iter().map(|q| to_px(*q)).collect();
            if let Some(first) = px.first().copied() {
                px.push(first);
            }
            p.line(&px, rgba((HOVER << 8) | 0x90), 2.0);
        }
        if let Some(edge) = &self.hover_edge {
            line(p, edge, to_px, rgba((HOVER << 8) | 0x99), 4.0);
        }
        if let Some(q) = self.hover_dot {
            dot(p, to_px(q), 3.5, rgb(SELECTED));
        }
        if let Some(q) = self.hover_start {
            ring(p, to_px(q), 6.0, rgb(SELECTED), 2.0);
        }
        for (q, text) in &self.labels {
            let (x, y) = to_px(*q);
            chip(p, x, y, text, rgba(0x1d1d1de0), rgb(0xe6e6e6));
        }
        for (side, b) in &self.sew_sides {
            let color = if *b { rgb(SIDE_B) } else { rgb(SELECTED) };
            line(p, side, to_px, color, 4.0);
        }
        if let Some(preview) = &self.sew_preview {
            line(p, preview, to_px, rgba((SELECTED << 8) | 0x8c), 4.0);
        }
        if let Some(q) = self.pending_point {
            dot(p, to_px(q), 5.0, rgb(SELECTED));
        }
        if let Some(q) = self.fasten_from {
            ring(p, to_px(q), 10.0, rgb(SELECTED), 2.0);
        }
        if let Some(draft) = &self.draft {
            paint_draft(p, draft, to_px, self.units);
        }
        if let Some(shape) = &self.shape {
            paint_shape(p, shape, to_px, self.units);
        }
        if let Some((a, b)) = self.selection_box {
            let (pa, pb) = (to_px(a), to_px(b));
            let (x0, y0, x1, y1) = (pa.0.min(pb.0), pa.1.min(pb.1), pa.0.max(pb.0), pa.1.max(pb.1));
            let r = Bounds {
                origin: point(x0, y0),
                size: size(x1 - x0, y1 - y0),
            };
            p.quad(r, 0.0, rgba((SELECTED << 8) | 0x1a), rgb(SELECTED), 1.0);
        }
        if let Some((a, c)) = self.measure {
            let (pa, pc) = (to_px(a), to_px(c));
            p.line(&[pa, pc], rgb(MEASURE), 1.5);
            let label = crate::units::text(self.units, a.distance(c)).to_string();
            chip(
                p,
                f32::midpoint(pa.0, pc.0),
                f32::midpoint(pa.1, pc.1) - 14.0,
                &label,
                rgb(MEASURE),
                rgb(0x111111),
            );
        }
        if let Some((a, c)) = self.cut {
            dashed(p, &[to_px(a), to_px(c)], 6.0, 4.0, rgb(CUT_LINE), 1.5);
        }
        if let Some(q) = self.snap {
            ring(p, to_px(q), 7.0, rgb(SNAP), 1.5);
        }
        if let Some((lo, hi)) = self.rotate_box {
            let (a, b) = (to_px(lo), to_px(hi));
            let m = BOX_MARGIN_PX;
            let frame = [
                (a.0 - m, a.1 - m),
                (b.0 + m, a.1 - m),
                (b.0 + m, b.1 + m),
                (a.0 - m, b.1 + m),
                (a.0 - m, a.1 - m),
            ];
            dashed(p, &frame, 3.0, 3.0, rgba((SELECTED << 8) | 0x80), 1.0);
            let h = rotate_handle_px(lo, hi, to_px);
            p.line(
                &[(h.0, h.1 + ROTATE_HANDLE_R), (h.0, a.1 - m)],
                rgb(SELECTED),
                1.0,
            );
            p.oval(
                Bounds {
                    origin: point(h.0 - ROTATE_HANDLE_R, h.1 - ROTATE_HANDLE_R),
                    size: size(2.0 * ROTATE_HANDLE_R, 2.0 * ROTATE_HANDLE_R),
                },
                rgb(0x303030),
                rgb(SELECTED),
                1.5,
            );
        }
    }
}

fn paint_draft(p: &mut Painter, draft: &Draft, to_px: &impl Fn(Vec2) -> (f32, f32), units: azam_doc::Units) {
    let mut px: Vec<(f32, f32)> = draft.points.iter().map(|(q, _)| to_px(*q)).collect();
    px.push(to_px(draft.cursor));
    if draft.internal {
        dashed(p, &px, 4.0, 3.0, rgb(SELECTED), 1.5);
    } else {
        p.line(&px, rgb(SELECTED), 1.5);
    }
    for (q, curve) in &draft.points {
        let (x, y) = to_px(*q);
        if *curve {
            dot(p, (x, y), 3.5, rgb(SELECTED));
        } else {
            p.fill(
                Bounds {
                    origin: point(x - 3.0, y - 3.0),
                    size: size(6.0, 6.0),
                },
                rgb(SELECTED),
            );
        }
    }
    if let Some((last, _)) = draft.points.last() {
        let (x, y) = to_px(draft.cursor);
        let label = crate::units::text(units, last.distance(draft.cursor)).to_string();
        p.text(x + 10.0, y - 18.0, &label, 10.5, rgb(0xe6e6e6));
    }
}

fn paint_shape(
    p: &mut Painter,
    shape: &ShapePreview,
    to_px: &impl Fn(Vec2) -> (f32, f32),
    units: azam_doc::Units,
) {
    let (lo, hi) = (shape.a.min(shape.b), shape.a.max(shape.b));
    let pts: Vec<(f32, f32)> = if shape.ellipse {
        let (c, r) = ((lo + hi) / 2.0, (hi - lo) / 2.0);
        (0..=64)
            .map(|i| {
                let a = f64::from(i) / 64.0 * std::f64::consts::TAU;
                to_px(c + Vec2::new(r.x * a.cos(), r.y * a.sin()))
            })
            .collect()
    } else {
        [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y), lo]
            .iter()
            .map(|q| to_px(*q))
            .collect()
    };
    dashed(p, &pts, 4.0, 3.0, rgb(SELECTED), 1.5);
    let (x, y) = to_px(shape.b);
    let label = format!(
        "{} × {}",
        crate::units::number(units, hi.x - lo.x),
        crate::units::text(units, hi.y - lo.y)
    );
    p.text(x + 8.0, y + 6.0, &label, 10.5, rgb(0xe6e6e6));
}
