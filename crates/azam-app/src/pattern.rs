//! The 2D pattern editor: pieces drawn flat with their sewing lines, cut lines, seams and points.
//!
//! Pattern space is millimetres with y down. The view maps it to the canvas with a zoom and a
//! pan; the wheel zooms about the pointer and the middle button pans, as in Blender's 2D editors.

mod overlay;
mod parts;

use azam_geom::{Vec2, contains};
use blenderui::Painter;
use blenderui::prelude::*;

pub use overlay::{Draft, Overlay, ROTATE_HANDLE_PX, ShapePreview, rotate_handle_px};
pub use parts::{ButtonDraw, CommentDraw, GraphicDraw, HoleDraw, InternalDraw, NotchDraw, StitchDraw};

/// Element id of the pattern canvas.
pub const PATTERN_ID: &str = "pattern2d";
/// Zoom limits, in logical pixels per mm.
const ZOOM_RANGE: (f64, f64) = (0.02, 20.0);
/// Zoom factor per wheel notch.
const ZOOM_PER_NOTCH: f64 = 1.15;
/// Pick radius for points, in logical pixels.
const POINT_PICK_PX: f64 = 7.0;
/// Margin left around the pattern when framing, in logical pixels.
const FRAME_MARGIN_PX: f64 = 40.0;
/// Width of the tool shelf floating over the editor's left edge, in logical pixels: framing keeps
/// the pattern clear of it.
const TOOL_SHELF_PX: f64 = 42.0;
/// Blender's selected-object orange.
const SELECTED: u32 = 0xf15800;
/// Blender's active-object yellow-orange.
const ACTIVE: u32 = 0xffaa40;
/// Seam length differences above this are labelled on the seam, in mm.
const SEAM_DIFF_LABEL_MM: f64 = 2.0;
/// Length of the tick marking where a seam side starts, in logical pixels.
const SEAM_TICK_PX: f32 = 6.0;

/// One piece as the editor draws it.
#[derive(Clone, Debug, Default)]
pub struct PieceDraw {
    /// Display name.
    pub name: String,
    /// Sewing line, flattened.
    pub sewing: Vec<Vec2>,
    /// Cut line (with seam allowance), flattened.
    pub cut: Vec<Vec2>,
    /// Internal lines.
    pub internals: Vec<InternalDraw>,
    /// Notches.
    pub notches: Vec<NotchDraw>,
    /// Graphics.
    pub graphics: Vec<GraphicDraw>,
    /// Bézier control polygons of the curved segments.
    pub controls: Vec<Vec<Vec2>>,
    /// The selected segment, flattened.
    pub selected_segment: Option<Vec<Vec2>>,
    /// Outline points: position and whether it is a curve control point.
    pub points: Vec<(Vec2, bool)>,
    /// Fabric colour, `0xRRGGBB`.
    pub color: u32,
    /// Whether the piece is selected.
    pub selected: bool,
    /// Whether it is the active piece.
    pub active: bool,
    /// Selected point indices.
    pub selected_points: Vec<usize>,
    /// Grain line direction, in degrees from vertical.
    pub grain_deg: f64,
    /// Whether the piece is excluded from the simulation.
    pub disabled: bool,
    /// Sewing lines of every size with their colours (the nested grading view); empty otherwise.
    pub sizes: Vec<(Vec<Vec2>, u32)>,
    /// Colours of the collaborators who have the piece selected.
    pub collaborators: Vec<u32>,
}

/// A seam's two sides as polylines, with its colour and number.
#[derive(Clone, Debug, Default)]
pub struct SeamDraw {
    /// Polylines of side A.
    pub a: Vec<Vec<Vec2>>,
    /// Polylines of side B.
    pub b: Vec<Vec<Vec2>>,
    /// Colour, `0xRRGGBB`.
    pub color: u32,
    /// Number shown at the middle of each side.
    pub number: usize,
    /// Whether the seam is selected.
    pub selected: bool,
    /// Length of side A minus side B, in mm.
    pub diff_mm: f64,
}

/// Everything the pattern editor draws.
#[derive(Clone, Debug, Default)]
pub struct PatternScene {
    /// Pieces in drawing order.
    pub pieces: Vec<PieceDraw>,
    /// Seams.
    pub seams: Vec<SeamDraw>,
    /// Topstitches.
    pub stitches: Vec<StitchDraw>,
    /// Buttons.
    pub buttons: Vec<ButtonDraw>,
    /// Buttonholes.
    pub holes: Vec<HoleDraw>,
    /// Comment pins.
    pub comments: Vec<CommentDraw>,
}

/// How to paint the scene beyond what it holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaintOptions {
    /// Show the points of every piece, not only the selected ones (point-editing tools).
    pub all_points: bool,
}

/// Pan and zoom of the pattern canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternView {
    /// Logical pixels per mm.
    pub zoom: f64,
    /// Canvas position of pattern origin, in logical pixels from the canvas's top-left.
    pub offset: Vec2,
}

impl Default for PatternView {
    fn default() -> Self {
        Self {
            zoom: 0.35,
            offset: Vec2::new(60.0, 60.0),
        }
    }
}

impl PatternView {
    /// Pattern point to canvas-local logical pixels.
    #[must_use]
    pub fn to_screen(&self, p: Vec2) -> Vec2 {
        p * self.zoom + self.offset
    }

    /// Canvas-local logical pixels to pattern space.
    #[must_use]
    pub fn to_pattern(&self, s: Vec2) -> Vec2 {
        (s - self.offset) / self.zoom
    }

    /// Zooms by `notches` wheel notches, keeping the pattern point under `anchor` fixed.
    pub fn zoom_at(&mut self, anchor: Vec2, notches: f64) {
        let before = self.to_pattern(anchor);
        self.zoom = (self.zoom * ZOOM_PER_NOTCH.powf(notches)).clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
        self.offset = anchor - before * self.zoom;
    }

    /// Fits `min`..`max` (pattern space) into a canvas of `size` logical pixels.
    pub fn frame(&mut self, min: Vec2, max: Vec2, size: Vec2) {
        let extent = (max - min).max(Vec2::splat(1.0));
        let shelf = Vec2::new(TOOL_SHELF_PX, 0.0);
        let room = (size - shelf - Vec2::splat(2.0 * FRAME_MARGIN_PX)).max(Vec2::splat(10.0));
        self.zoom = (room.x / extent.x)
            .min(room.y / extent.y)
            .clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
        self.offset = shelf + (size - shelf) / 2.0 - (min + max) / 2.0 * self.zoom;
    }

    /// Grid spacing in mm for the current zoom: lines at least ~12 px apart.
    #[must_use]
    pub fn grid_step(&self) -> f64 {
        [10.0, 50.0, 100.0, 500.0, 1000.0]
            .into_iter()
            .find(|s| s * self.zoom >= 12.0)
            .unwrap_or(1000.0)
    }
}

/// What a click in the pattern editor hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternHit {
    /// Outline point `point` of piece `piece` (indices into the scene).
    Point {
        /// Index of the piece in the scene.
        piece: usize,
        /// Index of the point in the piece.
        point: usize,
    },
    /// The inside of piece `piece`.
    Piece(usize),
}

impl PatternScene {
    /// The top-most point or piece under canvas-local `s`, if any.
    #[must_use]
    pub fn hit(&self, view: &PatternView, s: Vec2, points_first: bool) -> Option<PatternHit> {
        if points_first {
            for (pi, piece) in self.pieces.iter().enumerate().rev() {
                if !piece.selected {
                    continue;
                }
                for (k, (p, _)) in piece.points.iter().enumerate() {
                    if view.to_screen(*p).distance(s) <= POINT_PICK_PX {
                        return Some(PatternHit::Point { piece: pi, point: k });
                    }
                }
            }
        }
        let q = view.to_pattern(s);
        self.pieces
            .iter()
            .enumerate()
            .rev()
            .find(|(_, p)| contains(&p.sewing, q))
            .map(|(i, _)| PatternHit::Piece(i))
    }

    /// Bounding box of every piece's cut line, in pattern space.
    #[must_use]
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        let mut it = self.pieces.iter().flat_map(|p| p.cut.iter().chain(&p.sewing));
        let first = *it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))))
    }

    /// Paints the scene into `b` (the canvas bounds) with `view`, then `overlay` over it.
    pub fn paint(
        &self,
        b: Bounds<f32>,
        p: &mut Painter,
        view: &PatternView,
        options: PaintOptions,
        overlay: &Overlay,
    ) {
        p.fill(b, rgb(0x2b2b2b));
        p.with_clip(b, |p| {
            let origin = Vec2::new(f64::from(b.origin.x), f64::from(b.origin.y));
            let to_px = |q: Vec2| {
                let s = view.to_screen(q) + origin;
                (s.x as f32, s.y as f32)
            };
            let zoom = view.zoom as f32;
            paint_grid(p, b, view, origin);
            for piece in &self.pieces {
                paint_piece(p, piece, &to_px, zoom, options);
            }
            for stitch in &self.stitches {
                parts::paint_stitch(p, stitch, &to_px, zoom);
            }
            for seam in &self.seams {
                paint_seam(p, seam, &to_px);
            }
            parts::paint_items(p, self, &to_px, zoom);
            overlay.paint(p, &to_px);
        });
    }
}

fn line(
    p: &mut Painter,
    pts: &[Vec2],
    to_px: &impl Fn(Vec2) -> (f32, f32),
    color: impl Into<Rgba>,
    width: f32,
) {
    if pts.len() >= 2 {
        let px: Vec<(f32, f32)> = pts.iter().map(|q| to_px(*q)).collect();
        p.line(&px, color, width);
    }
}

fn closed(pts: &[Vec2], to_px: &impl Fn(Vec2) -> (f32, f32)) -> Vec<(f32, f32)> {
    let mut px: Vec<(f32, f32)> = pts.iter().map(|q| to_px(*q)).collect();
    if let Some(first) = px.first().copied() {
        px.push(first);
    }
    px
}

fn paint_grid(p: &mut Painter, b: Bounds<f32>, view: &PatternView, origin: Vec2) {
    let step = view.grid_step();
    let size = Vec2::new(f64::from(b.size.width), f64::from(b.size.height));
    let lo = view.to_pattern(Vec2::ZERO);
    let hi = view.to_pattern(size);
    let mut x = (lo.x / step).floor() * step;
    while x <= hi.x {
        let sx = (view.to_screen(Vec2::new(x, 0.0)).x + origin.x) as f32;
        let color = if ((x / step).round() as i64).rem_euclid(10) == 0 {
            rgb(0x383838)
        } else {
            rgb(0x313131)
        };
        p.line(&[(sx, b.origin.y), (sx, b.origin.y + b.size.height)], color, 1.0);
        x += step;
    }
    let mut y = (lo.y / step).floor() * step;
    while y <= hi.y {
        let sy = (view.to_screen(Vec2::new(0.0, y)).y + origin.y) as f32;
        let color = if ((y / step).round() as i64).rem_euclid(10) == 0 {
            rgb(0x383838)
        } else {
            rgb(0x313131)
        };
        p.line(&[(b.origin.x, sy), (b.origin.x + b.size.width, sy)], color, 1.0);
        y += step;
    }
}

/// Approximate width of one character of a 12 px label, in logical pixels.
const LABEL_CHAR_PX: f32 = 6.6;
/// Smallest on-screen piece height that holds its label inside, in logical pixels.
const LABEL_MIN_HEIGHT_PX: f32 = 46.0;

/// The piece's name: centred inside when it fits, otherwise under the piece in a dimmer colour so
/// it never covers seam numbers and points on small pieces such as a neck rib.
fn paint_label(p: &mut Painter, piece: &PieceDraw, to_px: &impl Fn(Vec2) -> (f32, f32), lo: Vec2, hi: Vec2) {
    let (top_left, bottom_right) = (to_px(lo), to_px(hi));
    let (w, h) = (bottom_right.0 - top_left.0, bottom_right.1 - top_left.1);
    let text_w = piece.name.chars().count() as f32 * LABEL_CHAR_PX;
    let centre = to_px((lo + hi) / 2.0);
    if text_w <= w * 0.8 && h >= LABEL_MIN_HEIGHT_PX {
        p.text(
            centre.0 - text_w / 2.0,
            centre.1 - 7.0,
            &piece.name,
            12.0,
            rgb(0xf0f0f0),
        );
    } else {
        p.text(
            centre.0 - text_w / 2.0,
            bottom_right.1 + 8.0,
            &piece.name,
            11.0,
            rgb(0xb8b8b8),
        );
    }
}

fn paint_piece(
    p: &mut Painter,
    piece: &PieceDraw,
    to_px: &impl Fn(Vec2) -> (f32, f32),
    zoom: f32,
    options: PaintOptions,
) {
    if piece.sewing.len() < 3 {
        return;
    }
    let alpha = if piece.disabled { 0x30 } else { 0x70 };
    p.polygon(&closed(&piece.sewing, to_px), rgba((piece.color << 8) | alpha));
    for graphic in &piece.graphics {
        parts::paint_graphic(p, graphic, to_px, zoom);
    }
    p.line(&closed(&piece.cut, to_px), rgba(0xd0d0d090), 1.0);
    for (outline, color) in &piece.sizes {
        p.line(&closed(outline, to_px), rgb(*color), 1.0);
    }
    // Collaborators' selections: wide coloured bands under the outline, one per collaborator.
    for (i, color) in piece.collaborators.iter().enumerate().rev() {
        p.line(&closed(&piece.sewing, to_px), rgb(*color), 4.0 + 3.0 * i as f32);
    }
    let outline = if piece.active {
        rgb(ACTIVE)
    } else if piece.selected {
        rgb(SELECTED)
    } else {
        rgb(0xe6e6e6)
    };
    p.line(
        &closed(&piece.sewing, to_px),
        outline,
        if piece.selected { 2.0 } else { 1.3 },
    );
    for il in &piece.internals {
        parts::paint_internal(p, il, to_px);
    }
    for notch in &piece.notches {
        parts::paint_notch(p, notch, to_px, zoom);
    }
    // Grain line through the centre of the bounding box.
    let (lo, hi) = piece
        .sewing
        .iter()
        .fold((piece.sewing[0], piece.sewing[0]), |(l, h), q| {
            (l.min(*q), h.max(*q))
        });
    let centre = (lo + hi) / 2.0;
    let half = (hi.y - lo.y) * 0.3;
    let (s, c) = piece.grain_deg.to_radians().sin_cos();
    let dir = Vec2::new(s, c);
    let (ga, gb) = (to_px(centre - dir * half), to_px(centre + dir * half));
    p.line(&[ga, gb], rgba(0xe6e6e680), 1.0);
    paint_label(p, piece, to_px, lo, hi);
    if let Some(seg) = &piece.selected_segment {
        line(p, seg, to_px, rgb(SELECTED), 3.0);
    }
    if piece.selected || options.all_points {
        for control in &piece.controls {
            parts::dashed(
                p,
                &control.iter().map(|q| to_px(*q)).collect::<Vec<_>>(),
                2.0,
                2.0,
                rgba(0x9aa7c7a0),
                0.75,
            );
        }
        for (k, (q, curve)) in piece.points.iter().enumerate() {
            let (x, y) = to_px(*q);
            let on = piece.selected_points.contains(&k);
            let fill = if on {
                rgb(0xffffff)
            } else if *curve {
                rgb(0x9aa7c7)
            } else {
                rgb(0x1d1d1d)
            };
            let border = if on { rgb(SELECTED) } else { rgb(0xe6e6e6) };
            let r = if *curve { 3.0 } else { 3.5 };
            let rect = Bounds {
                origin: point(x - r, y - r),
                size: size(2.0 * r, 2.0 * r),
            };
            if *curve {
                p.oval(rect, fill, border, 1.0);
            } else {
                p.quad(rect, 0.0, fill, border, 1.0);
            }
        }
    }
}

fn paint_seam(p: &mut Painter, seam: &SeamDraw, to_px: &impl Fn(Vec2) -> (f32, f32)) {
    let width = if seam.selected { 4.0 } else { 2.5 };
    let mut mids = Vec::new();
    for side in seam.a.iter().chain(&seam.b) {
        line(p, side, to_px, rgb(seam.color), width);
        if let [first, second, ..] = side.as_slice() {
            let (a, b) = (to_px(*first), to_px(*second));
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dx.hypot(dy).max(1e-3);
            let (nx, ny) = (-dy / len * SEAM_TICK_PX, dx / len * SEAM_TICK_PX);
            p.line(
                &[(a.0 - nx, a.1 - ny), (a.0 + nx, a.1 + ny)],
                rgb(seam.color),
                2.0,
            );
        }
        if let Some(mid) = side.get(side.len() / 2) {
            mids.push(to_px(*mid));
        }
    }
    if seam.selected && mids.len() >= 2 {
        parts::dashed(p, &mids, 3.0, 3.0, rgb(seam.color), 1.0);
    }
    if seam.diff_mm.abs() > SEAM_DIFF_LABEL_MM
        && let Some(&(x, y)) = mids.first()
    {
        let label = format!("Δ {:.1} cm", seam.diff_mm.abs() / 10.0);
        let w = label.chars().count() as f32 * 6.0 + 8.0;
        p.quad(
            Bounds {
                origin: point(x - w / 2.0, y - 24.0),
                size: size(w, 14.0),
            },
            3.0,
            rgb(seam.color),
            rgba(0),
            0.0,
        );
        p.text(x - w / 2.0 + 4.0, y - 24.0, &label, 10.0, rgb(0xffffff));
    }
    for &(x, y) in &mids {
        let disc = Bounds {
            origin: point(x - 7.0, y - 7.0),
            size: size(14.0, 14.0),
        };
        p.oval(disc, rgb(seam.color), rgb(0x1d1d1d), 1.0);
        let label = seam.number.to_string();
        p.text(x - label.len() as f32 * 3.3, y - 7.0, &label, 10.5, rgb(0x111111));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut view = PatternView::default();
        let anchor = Vec2::new(200.0, 150.0);
        let before = view.to_pattern(anchor);
        view.zoom_at(anchor, 3.0);
        assert!(view.to_pattern(anchor).distance(before) < 1e-9);
    }

    #[test]
    fn frame_centres_the_box_beside_the_tool_shelf() {
        let mut view = PatternView::default();
        view.frame(
            Vec2::new(0.0, 0.0),
            Vec2::new(1000.0, 500.0),
            Vec2::new(800.0, 600.0),
        );
        let centre = view.to_screen(Vec2::new(500.0, 250.0));
        assert!(centre.distance(Vec2::new(TOOL_SHELF_PX + (800.0 - TOOL_SHELF_PX) / 2.0, 300.0)) < 1e-9);
        let left = view.to_screen(Vec2::new(0.0, 0.0));
        assert!(left.x >= TOOL_SHELF_PX + FRAME_MARGIN_PX - 1e-9);
    }

    #[test]
    fn hit_finds_the_piece_under_the_pointer() {
        let square = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(100.0, 100.0),
            Vec2::new(0.0, 100.0),
        ];
        let scene = PatternScene {
            pieces: vec![PieceDraw {
                sewing: square.clone(),
                cut: square,
                ..Default::default()
            }],
            ..Default::default()
        };
        let view = PatternView {
            zoom: 1.0,
            offset: Vec2::ZERO,
        };
        assert_eq!(
            scene.hit(&view, Vec2::new(50.0, 50.0), true),
            Some(PatternHit::Piece(0))
        );
        assert_eq!(scene.hit(&view, Vec2::new(150.0, 50.0), true), None);
    }
}
