//! Drawing the parts placed on pieces: internal lines, notches, topstitches, graphics, buttons,
//! buttonholes and comment pins.

use azam_geom::Vec2;
use blenderui::Painter;
use blenderui::prelude::*;

use super::{PatternScene, SELECTED, closed, line};

/// Length of a notch mark, in logical pixels.
const NOTCH_PX: f32 = 6.0;
/// Smallest drawn button radius, in logical pixels.
const MIN_BUTTON_PX: f32 = 3.0;
/// Half length of the bar tacks at a buttonhole's ends, in logical pixels.
const BAR_TACK_PX: f32 = 3.0;
/// Radius of a comment pin's head, in logical pixels.
const PIN_HEAD_PX: f32 = 9.0;
/// Size of the corner handles of a selected graphic, in logical pixels.
const HANDLE_PX: f32 = 6.0;
/// Dash and gap of internal lines, in logical pixels.
const INTERNAL_DASH_PX: (f32, f32) = (4.0, 3.0);
/// Dash and gap of fold lines, in logical pixels.
const FOLD_DASH_PX: (f32, f32) = (6.0, 4.0);
/// Thinnest drawn thread, in logical pixels.
const MIN_THREAD_PX: f32 = 1.0;
/// Dash periods shorter than this draw as a solid line, in logical pixels.
const MIN_DASH_PERIOD_PX: f32 = 2.0;
/// Most dashes one line is cut into; longer lines (far zoomed in) draw solid.
const MAX_DASHES: f32 = 20_000.0;
/// What is left of a dash or gap below this counts as done, in logical pixels.
const DASH_EPSILON_PX: f32 = 1e-3;

/// An internal line as drawn.
#[derive(Clone, Debug, Default)]
pub struct InternalDraw {
    /// The line, flattened (closed lines repeat their first point at the end).
    pub line: Vec<Vec2>,
    /// Whether it is a fold line.
    pub fold: bool,
    /// Whether it is selected.
    pub selected: bool,
}

/// A notch as drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct NotchDraw {
    /// Where it sits on the sewing line.
    pub at: Vec2,
    /// Unit direction out of the piece.
    pub outward: Vec2,
    /// Whether it is selected.
    pub selected: bool,
}

/// A topstitch as drawn: one polyline per row, already offset into the piece.
#[derive(Clone, Debug, Default)]
pub struct StitchDraw {
    /// Rows, in mm.
    pub rows: Vec<Vec<Vec2>>,
    /// Stitch length, mm.
    pub length: f64,
    /// Gap between stitches, mm.
    pub gap: f64,
    /// Thread width, mm.
    pub width: f64,
    /// Thread colour, `0xRRGGBB` (a zipper's teeth).
    pub color: u32,
    /// Whether it is selected.
    pub selected: bool,
    /// A zipper half's tape (a polyline per segment, along the middle of the tape); empty for a
    /// topstitch.
    pub tape: Vec<Vec<Vec2>>,
}

/// A graphic as drawn.
#[derive(Clone, Debug, Default)]
pub struct GraphicDraw {
    /// Centre, mm.
    pub centre: Vec2,
    /// Width and height, mm.
    pub size: Vec2,
    /// Rotation, degrees.
    pub rotation: f64,
    /// Image file to show, when it exists.
    pub image: Option<String>,
    /// Whether it is selected.
    pub selected: bool,
}

/// A button as drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct ButtonDraw {
    /// Centre, mm.
    pub at: Vec2,
    /// Diameter, mm.
    pub diameter: f64,
    /// Colour, `0xRRGGBB`.
    pub color: u32,
    /// Whether it is selected (or picked to fasten).
    pub selected: bool,
    /// Centre of the buttonhole it is fastened to.
    pub fastened_to: Option<Vec2>,
}

/// A buttonhole as drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct HoleDraw {
    /// Centre, mm.
    pub at: Vec2,
    /// Length, mm.
    pub length: f64,
    /// Angle, degrees.
    pub angle: f64,
    /// Whether it is selected.
    pub selected: bool,
}

/// A comment pin as drawn.
#[derive(Clone, Debug, Default)]
pub struct CommentDraw {
    /// Where the pin points, mm.
    pub at: Vec2,
    /// Number on the pin head.
    pub number: String,
    /// Whether the comment is resolved.
    pub resolved: bool,
    /// Whether it is selected.
    pub selected: bool,
}

/// Strokes the polyline `pts` (logical pixels) as dashes `dash` long with `gap` between them.
pub(crate) fn dashed(
    p: &mut Painter,
    pts: &[(f32, f32)],
    dash: f32,
    gap: f32,
    color: impl Into<Rgba>,
    width: f32,
) {
    let color = color.into();
    for run in dash_runs(pts, dash, gap) {
        p.line(&run, color, width);
    }
}

/// The dashes of the polyline `pts`: `dash` long with `gap` between them. Dashes too short to see
/// or too many to draw give the whole line as one run.
fn dash_runs(pts: &[(f32, f32)], dash: f32, gap: f32) -> Vec<Vec<(f32, f32)>> {
    let period = dash + gap;
    let total: f32 = pts
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .sum();
    if pts.len() < 2 {
        return Vec::new();
    }
    if period < MIN_DASH_PERIOD_PX || dash <= 0.0 || gap <= 0.0 || total / period > MAX_DASHES {
        return vec![pts.to_vec()];
    }
    let mut runs = Vec::new();
    // What is left of the current dash or gap, and which of the two it is.
    let (mut left, mut on) = (dash, true);
    let mut run: Vec<(f32, f32)> = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if len <= f32::EPSILON {
            continue;
        }
        let at = |d: f32| (a.0 + (b.0 - a.0) * d / len, a.1 + (b.1 - a.1) * d / len);
        let mut done = 0.0_f32;
        while done < len {
            let step = left.min(len - done);
            if on {
                if run.is_empty() {
                    run.push(at(done));
                }
                run.push(at(done + step));
            }
            done += step;
            left -= step;
            if left <= DASH_EPSILON_PX {
                if on && run.len() >= 2 {
                    runs.push(std::mem::take(&mut run));
                }
                run.clear();
                on = !on;
                left = if on { dash } else { gap };
            }
        }
    }
    if on && run.len() >= 2 {
        runs.push(run);
    }
    runs
}

pub(super) fn paint_internal(p: &mut Painter, il: &InternalDraw, to_px: &impl Fn(Vec2) -> (f32, f32)) {
    let px: Vec<(f32, f32)> = il.line.iter().map(|q| to_px(*q)).collect();
    let (dash, gap) = if il.fold { FOLD_DASH_PX } else { INTERNAL_DASH_PX };
    let (color, width) = if il.selected {
        (rgb(SELECTED), 2.0)
    } else {
        (rgba(0xe6e6e6c0), 1.0)
    };
    dashed(p, &px, dash, gap, color, width);
}

pub(super) fn paint_notch(
    p: &mut Painter,
    notch: &NotchDraw,
    to_px: &impl Fn(Vec2) -> (f32, f32),
    zoom: f32,
) {
    let a = to_px(notch.at);
    let b = to_px(notch.at + notch.outward * f64::from(NOTCH_PX / zoom));
    let color = if notch.selected {
        rgb(SELECTED)
    } else {
        rgb(0xe6e6e6)
    };
    p.line(&[a, b], color, if notch.selected { 2.5 } else { 1.5 });
}

pub(super) fn paint_stitch(p: &mut Painter, st: &StitchDraw, to_px: &impl Fn(Vec2) -> (f32, f32), zoom: f32) {
    let width = (st.width as f32 * zoom).max(MIN_THREAD_PX);
    let tape = crate::scene::parse_hex(azam_doc::ZIPPER_TAPE_COLOR).unwrap_or(0x2a2a2c);
    for line in &st.tape {
        let px: Vec<(f32, f32)> = line.iter().map(|q| to_px(*q)).collect();
        p.line(
            &px,
            rgb(tape),
            (azam_doc::ZIPPER_TAPE_MM as f32 * zoom).max(width + 2.0),
        );
    }
    for row in &st.rows {
        let px: Vec<(f32, f32)> = row.iter().map(|q| to_px(*q)).collect();
        if st.selected {
            p.line(&px, rgba((SELECTED << 8) | 0x60), width + 3.0);
        }
        dashed(
            p,
            &px,
            (st.length as f32 * zoom).max(1.5),
            (st.gap as f32 * zoom).max(1.0),
            rgb(st.color),
            width,
        );
    }
}

/// The corners of a graphic's frame, in pattern space.
fn graphic_corners(g: &GraphicDraw) -> [Vec2; 4] {
    let (s, c) = g.rotation.to_radians().sin_cos();
    let h = g.size / 2.0;
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y)| {
        let (dx, dy) = (x * h.x, y * h.y);
        g.centre + Vec2::new(dx * c - dy * s, dx * s + dy * c)
    })
}

pub(super) fn paint_graphic(
    p: &mut Painter,
    g: &GraphicDraw,
    to_px: &impl Fn(Vec2) -> (f32, f32),
    zoom: f32,
) {
    let corners = graphic_corners(g);
    let frame = closed(&corners, to_px);
    match &g.image {
        Some(path) if g.rotation.abs() < f64::EPSILON => {
            let (x0, y0) = to_px(g.centre - g.size / 2.0);
            let bounds = Bounds {
                origin: point(x0, y0),
                size: size(g.size.x as f32 * zoom, g.size.y as f32 * zoom),
            };
            p.image_file(bounds, path);
        }
        _ => {
            // No image to show: a placeholder frame with its diagonals.
            p.polygon(&frame, rgba(0xffffff14));
            line(p, &[corners[0], corners[2]], to_px, rgba(0xc8c8c860), 1.0);
            line(p, &[corners[1], corners[3]], to_px, rgba(0xc8c8c860), 1.0);
        }
    }
    let color = if g.selected {
        rgb(SELECTED)
    } else {
        rgba(0xc8c8c8a0)
    };
    p.line(&frame, color, if g.selected { 1.5 } else { 1.0 });
    if g.selected {
        for c in corners {
            let (x, y) = to_px(c);
            p.fill(
                Bounds {
                    origin: point(x - HANDLE_PX / 2.0, y - HANDLE_PX / 2.0),
                    size: size(HANDLE_PX, HANDLE_PX),
                },
                rgb(SELECTED),
            );
        }
    }
}

/// Buttons, buttonholes and comment pins, over everything on the pieces.
pub(super) fn paint_items(
    p: &mut Painter,
    scene: &PatternScene,
    to_px: &impl Fn(Vec2) -> (f32, f32),
    zoom: f32,
) {
    for b in &scene.buttons {
        let (x, y) = to_px(b.at);
        if let Some(hole) = b.fastened_to {
            dashed(p, &[(x, y), to_px(hole)], 3.0, 3.0, rgba(0xb0b0b0c0), 1.0);
        }
        let r = (b.diameter as f32 / 2.0 * zoom).max(MIN_BUTTON_PX);
        let border = if b.selected {
            rgb(SELECTED)
        } else {
            rgba(0xffffffb0)
        };
        p.oval(
            Bounds {
                origin: point(x - r, y - r),
                size: size(2.0 * r, 2.0 * r),
            },
            rgb(b.color),
            border,
            1.5,
        );
    }
    for h in &scene.holes {
        let (s, c) = h.angle.to_radians().sin_cos();
        let dir = Vec2::new(c, s);
        let half = dir * h.length / 2.0;
        let (a, b) = (to_px(h.at - half), to_px(h.at + half));
        let color = if h.selected { rgb(SELECTED) } else { rgb(0xe6e6e6) };
        p.line(&[a, b], color, 2.0);
        let (nx, ny) = (-s as f32 * BAR_TACK_PX, c as f32 * BAR_TACK_PX);
        for end in [a, b] {
            p.line(&[(end.0 - nx, end.1 - ny), (end.0 + nx, end.1 + ny)], color, 2.0);
        }
    }
    for c in &scene.comments {
        let (x, y) = to_px(c.at);
        let fill = if c.resolved { rgb(0x8a8a8a) } else { rgb(SELECTED) };
        let head_y = y - PIN_HEAD_PX - 1.0;
        p.polygon(&[(x - 4.0, y - 5.0), (x, y), (x + 4.0, y - 5.0)], fill);
        p.oval(
            Bounds {
                origin: point(x - PIN_HEAD_PX, head_y - PIN_HEAD_PX),
                size: size(2.0 * PIN_HEAD_PX, 2.0 * PIN_HEAD_PX),
            },
            fill,
            if c.selected { rgb(0xffffff) } else { rgba(0) },
            1.5,
        );
        p.text(
            x - c.number.len() as f32 * 3.0,
            head_y - 6.5,
            &c.number,
            10.0,
            rgb(0xffffff),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_run_along_corners() {
        let runs = dash_runs(&[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)], 3.0, 1.0);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0], vec![(0.0, 0.0), (3.0, 0.0)]);
        assert_eq!(
            runs[1],
            vec![(4.0, 0.0), (5.0, 0.0), (5.0, 2.0)],
            "a dash turns the corner"
        );
        assert_eq!(runs[2], vec![(5.0, 3.0), (5.0, 5.0)]);
        assert_eq!(
            dash_runs(&[(0.0, 0.0), (9.0, 0.0)], 0.5, 0.5).len(),
            1,
            "too fine: solid"
        );
    }

    #[test]
    fn graphic_corners_turn_with_the_graphic() {
        let g = GraphicDraw {
            centre: Vec2::new(10.0, 10.0),
            size: Vec2::new(20.0, 10.0),
            rotation: 90.0,
            ..Default::default()
        };
        let c = graphic_corners(&g);
        assert!(c[0].distance(Vec2::new(15.0, 0.0)) < 1e-9, "{:?}", c[0]);
    }
}
