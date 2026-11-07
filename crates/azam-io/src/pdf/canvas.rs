//! A page-description canvas in mm with y down: drawing calls are recorded as [`Op`]s with the
//! colour, font and line state they are drawn with, so a layout can be inspected (and tested)
//! before it is written as PDF.

use serde::Serialize;

use super::metrics::{self, FontStyle, LINE_HEIGHT_FACTOR, PT_PER_MM};

/// An RGB colour, 0–255 per channel.
pub type Rgb = [u8; 3];

/// Black.
pub const BLACK: Rgb = [0, 0, 0];
/// White.
pub const WHITE: Rgb = [255, 255, 255];
/// Default line width, in mm.
pub const DEFAULT_LINE_WIDTH_MM: f64 = 0.200_025;
/// Default font size, in pt.
pub const DEFAULT_FONT_SIZE_PT: f64 = 16.0;

/// Horizontal alignment of text at its anchor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    /// Text starts at the anchor.
    #[default]
    Left,
    /// Text is centred on the anchor.
    Center,
    /// Text ends at the anchor.
    Right,
}

/// Vertical placement of text at its anchor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Baseline {
    /// The anchor is on the baseline.
    #[default]
    Alphabetic,
    /// The anchor is at the middle of the em box.
    Middle,
    /// The anchor is at the top of the em box.
    Top,
}

/// Placement of a text call.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextOpts {
    /// Horizontal alignment.
    pub align: Align,
    /// Vertical placement.
    pub baseline: Baseline,
    /// Counter-clockwise rotation about the anchor, in degrees.
    pub angle: f64,
    /// Wrap the text to this width in mm (0: no wrapping).
    pub max_width: f64,
}

impl TextOpts {
    /// Left aligned on the baseline.
    #[must_use]
    pub fn left() -> Self {
        Self::default()
    }

    /// Centred on the baseline.
    #[must_use]
    pub fn center() -> Self {
        Self {
            align: Align::Center,
            ..Self::default()
        }
    }

    /// Right aligned on the baseline.
    #[must_use]
    pub fn right() -> Self {
        Self {
            align: Align::Right,
            ..Self::default()
        }
    }

    /// The same, aligned `align`.
    #[must_use]
    pub fn aligned(self, align: Align) -> Self {
        Self { align, ..self }
    }

    /// The same, anchored at the middle of the em box.
    #[must_use]
    pub fn middle(self) -> Self {
        Self {
            baseline: Baseline::Middle,
            ..self
        }
    }

    /// The same, anchored at the top of the em box.
    #[must_use]
    pub fn top(self) -> Self {
        Self {
            baseline: Baseline::Top,
            ..self
        }
    }

    /// The same, rotated counter-clockwise by `angle` degrees.
    #[must_use]
    pub fn rotated(self, angle: f64) -> Self {
        Self { angle, ..self }
    }

    /// The same, wrapped to `max_width` mm.
    #[must_use]
    pub fn wrapped(self, max_width: f64) -> Self {
        Self { max_width, ..self }
    }
}

/// Stroke settings.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stroke {
    /// Line width in mm.
    pub width: f64,
    /// Colour.
    pub color: Rgb,
    /// Dash pattern in mm (empty: solid).
    pub dash: Vec<f64>,
    /// Dash phase in mm.
    pub phase: f64,
}

/// How a shape is painted.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Paint {
    /// Outline only.
    Stroke(Stroke),
    /// Interior only.
    Fill(Rgb),
    /// Interior, then outline.
    FillStroke(Rgb, Stroke),
}

impl Paint {
    /// The paint code of the recorded reference data: `S`, `F` or `FD`.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Stroke(_) => "S",
            Self::Fill(_) => "F",
            Self::FillStroke(..) => "FD",
        }
    }
}

/// Which painting a shape call asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Stroke the outline.
    Stroke,
    /// Fill the interior.
    Fill,
    /// Fill, then stroke.
    FillStroke,
}

/// One recorded drawing operation. Coordinates are mm from the top-left of the page.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Op {
    /// One line of text.
    Text {
        /// The text.
        text: String,
        /// Anchor x.
        x: f64,
        /// Anchor y.
        y: f64,
        /// Horizontal alignment.
        align: Align,
        /// Vertical placement.
        baseline: Baseline,
        /// Counter-clockwise rotation, degrees.
        angle: f64,
        /// Face.
        style: FontStyle,
        /// Size in pt.
        size: f64,
        /// Colour.
        color: Rgb,
    },
    /// A straight line.
    Line {
        /// Start.
        a: [f64; 2],
        /// End.
        b: [f64; 2],
        /// Stroke.
        stroke: Stroke,
    },
    /// A polyline or polygon.
    Path {
        /// Vertices.
        points: Vec<[f64; 2]>,
        /// Whether the last vertex joins the first.
        closed: bool,
        /// Painting.
        paint: Paint,
    },
    /// An axis-aligned rectangle.
    Rect {
        /// Left.
        x: f64,
        /// Top.
        y: f64,
        /// Width.
        w: f64,
        /// Height.
        h: f64,
        /// Painting.
        paint: Paint,
    },
    /// A circle.
    Circle {
        /// Centre x.
        x: f64,
        /// Centre y.
        y: f64,
        /// Radius.
        r: f64,
        /// Painting.
        paint: Paint,
    },
    /// A triangle.
    Triangle {
        /// Corners.
        points: [[f64; 2]; 3],
        /// Painting.
        paint: Paint,
    },
    /// Saves the graphics state (including the clip).
    Save,
    /// Restores the last saved graphics state.
    Restore,
    /// Intersects the clip with a rectangle.
    Clip {
        /// Left.
        x: f64,
        /// Top.
        y: f64,
        /// Width.
        w: f64,
        /// Height.
        h: f64,
    },
}

/// One page of a layout.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Page {
    /// Width in mm.
    pub width: f64,
    /// Height in mm.
    pub height: f64,
    /// Drawing operations in order.
    pub ops: Vec<Op>,
}

#[derive(Clone, Debug)]
struct State {
    text: Rgb,
    draw: Rgb,
    fill: Rgb,
    line_width: f64,
    dash: Vec<f64>,
    phase: f64,
    style: FontStyle,
    size: f64,
}

/// Records drawing calls page by page.
#[derive(Clone, Debug)]
pub struct Canvas {
    pages: Vec<Page>,
    st: State,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    /// An empty canvas: black, 0.2 mm lines, Helvetica 16 pt.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            st: State {
                text: BLACK,
                draw: BLACK,
                fill: BLACK,
                line_width: DEFAULT_LINE_WIDTH_MM,
                dash: Vec::new(),
                phase: 0.0,
                style: FontStyle::Normal,
                size: DEFAULT_FONT_SIZE_PT,
            },
        }
    }

    /// The recorded pages.
    #[must_use]
    pub fn into_pages(self) -> Vec<Page> {
        self.pages
    }

    /// Number of pages so far.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Starts a page of `width` × `height` mm. Drawing state carries over.
    pub fn add_page(&mut self, width: f64, height: f64) {
        self.pages.push(Page {
            width,
            height,
            ops: Vec::new(),
        });
    }

    fn push(&mut self, op: Op) {
        if self.pages.is_empty() {
            self.add_page(super::A4_MM[0], super::A4_MM[1]);
        }
        if let Some(page) = self.pages.last_mut() {
            page.ops.push(op);
        }
    }

    /// Sets the text colour.
    pub fn set_text_color(&mut self, c: Rgb) {
        self.st.text = c;
    }

    /// Sets the stroke colour.
    pub fn set_draw_color(&mut self, c: Rgb) {
        self.st.draw = c;
    }

    /// Sets the fill colour.
    pub fn set_fill_color(&mut self, c: Rgb) {
        self.st.fill = c;
    }

    /// Sets the line width in mm.
    pub fn set_line_width(&mut self, w: f64) {
        self.st.line_width = w;
    }

    /// Sets the dash pattern (mm; empty for solid lines).
    pub fn set_dash(&mut self, dash: &[f64], phase: f64) {
        self.st.dash = dash.to_vec();
        self.st.phase = phase;
    }

    /// Sets the face.
    pub fn set_font(&mut self, style: FontStyle) {
        self.st.style = style;
    }

    /// Sets the font size in pt.
    pub fn set_font_size(&mut self, size: f64) {
        self.st.size = size;
    }

    /// The font size in pt.
    #[must_use]
    pub fn font_size(&self) -> f64 {
        self.st.size
    }

    /// The face.
    #[must_use]
    pub fn font(&self) -> FontStyle {
        self.st.style
    }

    /// Width of `text` in mm in the current face and size.
    #[must_use]
    pub fn text_width(&self, text: &str) -> f64 {
        metrics::text_width(text, self.st.style, self.st.size)
    }

    /// `text` split into lines no wider than `max_width` mm in the current face and size.
    #[must_use]
    pub fn split_to_width(&self, text: &str, max_width: f64) -> Vec<String> {
        metrics::split_to_width(text, self.st.style, self.st.size, max_width)
    }

    fn stroke(&self) -> Stroke {
        Stroke {
            width: self.st.line_width,
            color: self.st.draw,
            dash: self.st.dash.clone(),
            phase: self.st.phase,
        }
    }

    fn paint(&self, style: Style) -> Paint {
        match style {
            Style::Stroke => Paint::Stroke(self.stroke()),
            Style::Fill => Paint::Fill(self.st.fill),
            Style::FillStroke => Paint::FillStroke(self.st.fill, self.stroke()),
        }
    }

    /// Draws text at `(x, y)`. Text with line breaks (or wrapped to `opts.max_width`) is drawn one
    /// line per [`LINE_HEIGHT_FACTOR`] × size below the other.
    pub fn text(&mut self, text: &str, x: f64, y: f64, opts: TextOpts) {
        if text.is_empty() {
            return;
        }
        let lines: Vec<String> = if text.contains(['\r', '\n']) {
            text.replace("\r\n", "\n")
                .split(['\n', '\r'])
                .map(str::to_owned)
                .collect()
        } else {
            vec![text.to_owned()]
        };
        self.text_lines(&lines, x, y, opts);
    }

    /// Draws lines of text, the first at `(x, y)`.
    pub fn text_lines(&mut self, lines: &[String], x: f64, y: f64, opts: TextOpts) {
        if lines.is_empty() {
            return;
        }
        let lines: Vec<String> = if opts.max_width > 0.0 {
            lines
                .iter()
                .flat_map(|l| self.split_to_width(l, opts.max_width))
                .collect()
        } else {
            lines.to_vec()
        };
        let leading = self.st.size * LINE_HEIGHT_FACTOR / PT_PER_MM;
        for (i, line) in lines.into_iter().enumerate() {
            self.push(Op::Text {
                text: line,
                x,
                y: y + i as f64 * leading,
                align: opts.align,
                baseline: opts.baseline,
                angle: opts.angle,
                style: self.st.style,
                size: self.st.size,
                color: self.st.text,
            });
        }
    }

    /// A straight line.
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let stroke = self.stroke();
        self.push(Op::Line {
            a: [x1, y1],
            b: [x2, y2],
            stroke,
        });
    }

    /// A polyline through `points` (closed back to the first when `closed`).
    pub fn path(&mut self, points: Vec<[f64; 2]>, closed: bool, style: Style) {
        let paint = self.paint(style);
        self.push(Op::Path {
            points,
            closed,
            paint,
        });
    }

    /// A rectangle.
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, style: Style) {
        let paint = self.paint(style);
        self.push(Op::Rect { x, y, w, h, paint });
    }

    /// A circle.
    pub fn circle(&mut self, x: f64, y: f64, r: f64, style: Style) {
        let paint = self.paint(style);
        self.push(Op::Circle { x, y, r, paint });
    }

    /// A triangle.
    pub fn triangle(&mut self, points: [[f64; 2]; 3], style: Style) {
        let paint = self.paint(style);
        self.push(Op::Triangle { points, paint });
    }

    /// Saves the graphics state.
    pub fn save(&mut self) {
        self.push(Op::Save);
    }

    /// Restores the graphics state.
    pub fn restore(&mut self) {
        self.push(Op::Restore);
    }

    /// Clips further drawing to a rectangle (until [`Canvas::restore`]).
    pub fn clip_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.push(Op::Clip { x, y, w, h });
    }
}
