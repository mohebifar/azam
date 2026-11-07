//! Writes recorded pages as a PDF 1.4 file: vector paths and text, one compressed content stream
//! per page.
//!
//! Text `WinAnsiEncoding` covers is written in base-14 Helvetica. Other text is laid out by
//! [`super::unicode`]: its Helvetica runs stay base-14, and its shaped runs address glyphs of the
//! bundled Unicode fonts (or the system CJK fallback font), each embedded once per document as a
//! Type0 font (`Identity-H`, a CID font descendant with a `/W` widths array and a subset of the
//! glyphs used)
//! with a `ToUnicode` `CMap` mapping the glyphs back to the text, so it can be searched and copied.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use subsetter::GlyphRemapper;

use super::canvas::{Align, Baseline, Op, Page, Paint, Rgb, Stroke};
use super::metrics::{FontStyle, LINE_HEIGHT_FACTOR, PT_PER_MM, string_unit_width};
use super::unicode::{self, FontId, Glyph, Run, win_ansi};
use crate::format::fixed;

/// Name written as the producing application.
pub const PRODUCER: &str = "azam";
/// Control-point distance of a quarter circle drawn as a cubic Bézier, as a fraction of the radius.
const CIRCLE_KAPPA: f64 = 0.552_284_749_8;
/// Decimals of coordinates in content streams (1/1000 pt).
const COORD_DIGITS: usize = 3;
/// Decimals of colour components.
const COLOR_DIGITS: usize = 4;
/// Compression level of content streams (0–10).
const DEFLATE_LEVEL: u8 = 6;
/// Faces in resource order (`/F1`, `/F2`, `/F3`).
const FACES: [FontStyle; 3] = [FontStyle::Normal, FontStyle::Bold, FontStyle::Italic];
/// Byte written for characters the font encoding lacks.
const MISSING_GLYPH: u8 = b'?';
/// Objects written per embedded font: Type0 font, CID font, descriptor, font file, `ToUnicode`.
const OBJECTS_PER_FONT: usize = 5;
/// Glyph space units per em in PDF font dictionaries and `TJ` adjustments.
const GLYPH_UNITS: f64 = 1000.0;
/// Entries per `bfchar` block of a `CMap` (the format's limit).
const CMAP_BLOCK: usize = 100;

/// Document information written to the file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PdfInfo {
    /// Document title.
    pub title: String,
}

fn num(v: f64) -> String {
    let s = fixed(v, COORD_DIGITS);
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    };
    if s == "-0" { "0".to_owned() } else { s }
}

fn color(c: Rgb) -> String {
    c.map(|v| {
        let s = fixed(f64::from(v) / 255.0, COLOR_DIGITS);
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    })
    .map(|s| if s.is_empty() { "0".to_owned() } else { s })
    .join(" ")
}

/// `text` as a PDF string literal in `WinAnsiEncoding`.
fn string_literal(text: &str) -> String {
    let mut out = String::from("(");
    for c in text.chars() {
        let b = win_ansi(c).unwrap_or(MISSING_GLYPH);
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(char::from(b));
            }
            0x20..=0x7e => out.push(char::from(b)),
            _ => {
                let _ = write!(out, "\\{b:03o}");
            }
        }
    }
    out.push(')');
    out
}

/// `text` as a document-information string: a literal when it is printable ASCII, else UTF-16BE
/// with a byte-order mark (the information dictionary does not use the font encoding).
fn info_string(text: &str) -> String {
    if text.chars().all(|c| (' '..='~').contains(&c)) {
        return string_literal(text);
    }
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        let _ = write!(out, "{unit:04X}");
    }
    out.push('>');
    out
}

/// Graphics state already set in the content stream (`None`: unknown).
#[derive(Clone, Default)]
struct Emitted {
    stroke: Option<Rgb>,
    fill: Option<Rgb>,
    width: Option<f64>,
    dash: Option<(Vec<f64>, f64)>,
}

/// The glyphs a document uses from one bundled font.
#[derive(Default)]
struct Subset {
    /// Glyph ids of the full font to the consecutive ids of the subset (the CIDs).
    remapper: GlyphRemapper,
    /// Subset glyph id to the full font's glyph id and the text it stands for.
    glyphs: BTreeMap<u16, (u16, String)>,
}

impl Subset {
    /// The CID of `glyph`, recording its use.
    fn cid(&mut self, glyph: &Glyph) -> u16 {
        let cid = self.remapper.remap(glyph.id);
        let entry = self
            .glyphs
            .entry(cid)
            .or_insert_with(|| (glyph.id, String::new()));
        if entry.1.is_empty() {
            entry.1.clone_from(&glyph.text);
        }
        cid
    }
}

/// Resource name of a bundled font: after the base-14 faces.
fn embedded_resource(id: FontId) -> String {
    format!("F{}", FACES.len() + 1 + id.index())
}

/// Builds one page's content stream.
struct Content<'a> {
    out: String,
    height: f64,
    state: Emitted,
    saved: Vec<Emitted>,
    /// Glyphs used from the bundled fonts, over the whole document.
    subsets: &'a mut BTreeMap<FontId, Subset>,
    /// Bundled fonts this page uses.
    used: BTreeSet<FontId>,
}

impl Content<'_> {
    fn x(v: f64) -> String {
        num(v * PT_PER_MM)
    }

    fn y(&self, v: f64) -> String {
        num((self.height - v) * PT_PER_MM)
    }

    fn pt(&self, p: [f64; 2]) -> String {
        format!("{} {}", Self::x(p[0]), self.y(p[1]))
    }

    fn set_fill(&mut self, c: Rgb) {
        if self.state.fill != Some(c) {
            let _ = writeln!(self.out, "{} rg", color(c));
            self.state.fill = Some(c);
        }
    }

    fn set_stroke(&mut self, s: &Stroke) {
        if self.state.stroke != Some(s.color) {
            let _ = writeln!(self.out, "{} RG", color(s.color));
            self.state.stroke = Some(s.color);
        }
        #[allow(clippy::float_cmp, reason = "the same recorded value")]
        if self.state.width.is_none_or(|w| w != s.width) {
            let _ = writeln!(self.out, "{} w", num(s.width * PT_PER_MM));
            self.state.width = Some(s.width);
        }
        let dash = (s.dash.clone(), s.phase);
        if self.state.dash.as_ref() != Some(&dash) {
            let list: Vec<String> = s.dash.iter().map(|d| num(d * PT_PER_MM)).collect();
            let _ = writeln!(self.out, "[{}] {} d", list.join(" "), num(s.phase * PT_PER_MM));
            self.state.dash = Some(dash);
        }
    }

    /// Sets the paint state and returns the painting operator.
    fn paint(&mut self, paint: &Paint) -> &'static str {
        match paint {
            Paint::Stroke(s) => {
                self.set_stroke(s);
                "S"
            }
            Paint::Fill(c) => {
                self.set_fill(*c);
                "f"
            }
            Paint::FillStroke(c, s) => {
                self.set_fill(*c);
                self.set_stroke(s);
                "B"
            }
        }
    }

    fn op(&mut self, op: &Op) {
        match op {
            Op::Text {
                text,
                x,
                y,
                align,
                baseline,
                angle,
                style,
                size,
                color,
            } => self.text(text, *x, *y, *align, *baseline, *angle, *style, *size, *color),
            Op::Line { a, b, stroke } => {
                self.set_stroke(stroke);
                let _ = writeln!(self.out, "{} m {} l S", self.pt(*a), self.pt(*b));
            }
            Op::Path {
                points,
                closed,
                paint,
            } => {
                let Some(first) = points.first() else {
                    return;
                };
                let painter = self.paint(paint);
                let mut s = format!("{} m", self.pt(*first));
                for p in &points[1..] {
                    let _ = write!(s, "\n{} l", self.pt(*p));
                }
                if *closed {
                    s.push_str(" h");
                }
                let _ = writeln!(self.out, "{s} {painter}");
            }
            Op::Rect { x, y, w, h, paint } => {
                let painter = self.paint(paint);
                let _ = writeln!(
                    self.out,
                    "{} {} {} {} re {painter}",
                    Self::x(*x),
                    self.y(y + h),
                    Self::x(*w),
                    Self::x(*h)
                );
            }
            Op::Circle { x, y, r, paint } => {
                let painter = self.paint(paint);
                let k = r * CIRCLE_KAPPA;
                let p = |dx: f64, dy: f64| self.pt([x + dx, y + dy]);
                let _ = writeln!(
                    self.out,
                    "{} m\n{} {} {} c\n{} {} {} c\n{} {} {} c\n{} {} {} c {painter}",
                    p(*r, 0.0),
                    p(*r, k),
                    p(k, *r),
                    p(0.0, *r),
                    p(-k, *r),
                    p(-r, k),
                    p(-r, 0.0),
                    p(-r, -k),
                    p(-k, -r),
                    p(0.0, -r),
                    p(k, -r),
                    p(*r, -k),
                    p(*r, 0.0)
                );
            }
            Op::Triangle { points, paint } => {
                let painter = self.paint(paint);
                let _ = writeln!(
                    self.out,
                    "{} m {} l {} l h {painter}",
                    self.pt(points[0]),
                    self.pt(points[1]),
                    self.pt(points[2])
                );
            }
            Op::Save => {
                self.out.push_str("q\n");
                self.saved.push(self.state.clone());
            }
            Op::Restore => {
                self.out.push_str("Q\n");
                self.state = self.saved.pop().unwrap_or_default();
            }
            Op::Clip { x, y, w, h } => {
                let _ = writeln!(
                    self.out,
                    "{} {} {} {} re W n",
                    Self::x(*x),
                    self.y(y + h),
                    Self::x(*w),
                    Self::x(*h)
                );
            }
        }
    }

    /// Text placed like the recorded call: the anchor moves by the alignment (measured without
    /// kerning) and baseline before the rotation about it.
    #[allow(clippy::too_many_arguments, reason = "the fields of one text operation")]
    fn text(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        align: Align,
        baseline: Baseline,
        angle: f64,
        style: FontStyle,
        size: f64,
        color: Rgb,
    ) {
        let height = size / PT_PER_MM;
        let descent = height * (LINE_HEIGHT_FACTOR - 1.0);
        let y = match baseline {
            Baseline::Alphabetic => y,
            Baseline::Middle => y + height / 2.0 - descent,
            Baseline::Top => y + height - descent,
        };
        let runs = unicode::layout(text, style);
        let width = match &runs {
            None => string_unit_width(text, style, false),
            Some(runs) => runs.iter().map(|r| r.width(style, false)).sum(),
        } * size
            / PT_PER_MM;
        let x = match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        };
        let font = FACES.iter().position(|&f| f == style).unwrap_or(0) + 1;
        self.set_fill(color);
        let (s, c) = angle.to_radians().sin_cos();
        let Some(runs) = runs else {
            let _ = writeln!(
                self.out,
                "BT /F{font} {} Tf {} {} {} {} {} Tm {} Tj ET",
                num(size),
                num(c),
                num(s),
                num(-s),
                num(c),
                self.pt([x, y]),
                string_literal(text)
            );
            return;
        };
        let mut line = format!(
            "BT {} {} {} {} {} Tm",
            num(c),
            num(s),
            num(-s),
            num(c),
            self.pt([x, y])
        );
        for run in &runs {
            match run {
                Run::Base(chars) => {
                    let _ = write!(
                        line,
                        " /F{font} {} Tf {} Tj",
                        num(size),
                        string_literal(&Run::base_text(chars))
                    );
                }
                Run::Embedded(id, glyphs) => self.glyphs(&mut line, *id, glyphs, size),
            }
        }
        line.push_str(" ET");
        let _ = writeln!(self.out, "{line}");
    }

    /// Shaped glyphs of a bundled font as `TJ` arrays of CIDs: the arrays' numbers turn the
    /// fonts' nominal advances into the shaped ones and apply horizontal offsets; vertical offsets
    /// set the text rise.
    fn glyphs(&mut self, line: &mut String, id: FontId, glyphs: &[Glyph], size: f64) {
        let Some(font) = unicode::font(id) else {
            return;
        };
        self.used.insert(id);
        let subset = self.subsets.entry(id).or_default();
        let _ = write!(line, " /{} {} Tf", embedded_resource(id), num(size));
        let mut array = TjArray::default();
        let mut rise = 0.0;
        for g in glyphs {
            #[allow(clippy::float_cmp, reason = "the same computed value")]
            if g.y_offset != rise {
                array.flush(line);
                let _ = write!(line, " {} Ts", num(g.y_offset * size));
                rise = g.y_offset;
            }
            array.adjust(-g.x_offset * GLYPH_UNITS);
            array.glyph(subset.cid(g));
            array.adjust((font.advance(g.id) + g.x_offset - g.advance) * GLYPH_UNITS);
        }
        array.flush(line);
        if rise != 0.0 {
            line.push_str(" 0 Ts");
        }
    }
}

/// A `TJ` array being built: CIDs as hex strings, with position adjustments between them.
#[derive(Default)]
struct TjArray {
    items: Vec<String>,
    hex: String,
    /// Adjustment not yet written, in thousandths of the font size (positive: left).
    pending: f64,
}

impl TjArray {
    fn adjust(&mut self, v: f64) {
        self.pending += v;
    }

    fn write_pending(&mut self) {
        if self.pending.abs() > 1e-6 {
            if !self.hex.is_empty() {
                self.items.push(format!("<{}>", self.hex));
                self.hex.clear();
            }
            self.items.push(num(self.pending));
        }
        self.pending = 0.0;
    }

    fn glyph(&mut self, cid: u16) {
        self.write_pending();
        let _ = write!(self.hex, "{cid:04X}");
    }

    /// Writes the array (with its trailing adjustment, which moves the following text) as a `TJ`.
    fn flush(&mut self, line: &mut String) {
        self.write_pending();
        if !self.hex.is_empty() {
            self.items.push(format!("<{}>", self.hex));
            self.hex.clear();
        }
        if !self.items.is_empty() {
            let _ = write!(line, " [{}] TJ", self.items.join(" "));
            self.items.clear();
        }
    }
}

/// Six capital letters naming a font subset, derived from its glyphs.
fn subset_tag(name: &str, subset: &Subset) -> String {
    // FNV-1a over the name and the glyph list.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |b: u8| {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    };
    name.bytes().for_each(&mut feed);
    for (cid, (gid, _)) in &subset.glyphs {
        cid.to_be_bytes()
            .into_iter()
            .chain(gid.to_be_bytes())
            .for_each(&mut feed);
    }
    (0..6)
        .map(|i| char::from(b'A' + ((h >> (i * 8)) % 26) as u8))
        .collect()
}

/// A compressed stream object with `extra` dictionary entries.
fn stream_object(data: &[u8], extra: &str) -> Vec<u8> {
    let data = miniz_oxide::deflate::compress_to_vec_zlib(data, DEFLATE_LEVEL);
    let mut stream = format!(
        "<< /Length {}{extra} /Filter /FlateDecode >>\nstream\n",
        data.len()
    )
    .into_bytes();
    stream.extend_from_slice(&data);
    stream.extend_from_slice(b"\nendstream");
    stream
}

/// The `/W` array of a subset: runs of consecutive CIDs with their widths.
fn widths_array(font: &unicode::Font, subset: &Subset) -> String {
    let mut out = String::from("[");
    let mut prev: Option<u16> = None;
    for (&cid, &(gid, _)) in &subset.glyphs {
        let w = num(font.advance(gid) * GLYPH_UNITS);
        if prev.is_some_and(|p| p + 1 == cid) {
            let _ = write!(out, " {w}");
        } else {
            if prev.is_some() {
                out.push_str("] ");
            }
            let _ = write!(out, "{cid} [{w}");
        }
        prev = Some(cid);
    }
    if prev.is_some() {
        out.push(']');
    }
    out.push(']');
    out
}

/// The `ToUnicode` `CMap` of a subset: each CID to the UTF-16BE text its glyph stands for.
fn to_unicode_cmap(subset: &Subset) -> String {
    let entries: Vec<String> = subset
        .glyphs
        .iter()
        .filter(|(_, (_, text))| !text.is_empty())
        .map(|(cid, (_, text))| {
            let units = text.encode_utf16().fold(String::new(), |mut s, u| {
                let _ = write!(s, "{u:04X}");
                s
            });
            format!("<{cid:04X}> <{units}>")
        })
        .collect();
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for block in entries.chunks(CMAP_BLOCK) {
        let _ = writeln!(out, "{} beginbfchar", block.len());
        for e in block {
            let _ = writeln!(out, "{e}");
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}

/// The objects of one embedded font, numbered from `first`: Type0 font, CID font descendant,
/// descriptor, subset font file and `ToUnicode` `CMap`. TrueType outlines make a `CIDFontType2`
/// with a `FontFile2`; CFF outlines (some system CJK fonts) a `CIDFontType0` with an `OpenType`
/// `FontFile3`, whose subset is CID-keyed with CIDs equal to the subset's glyph ids.
fn font_objects(font: &unicode::Font, subset: &Subset, first: usize) -> Vec<Vec<u8>> {
    let face = &font.face;
    let name = format!("{}+{}", subset_tag(&font.name, subset), font.name);
    let scale = |v: f64| num(v * GLYPH_UNITS / font.units_per_em());
    let bbox = face.global_bounding_box();
    let ascent = f64::from(face.ascender());
    let descent = f64::from(face.descender());
    let cap = face.capital_height().map_or(ascent, f64::from);
    let weight = f64::from(face.weight().to_number());
    let stem_v = num((10.0 + 220.0 * (weight - 50.0) / 900.0).round());
    // A subset that fails to build leaves the font unembedded (viewers substitute one), the text
    // still maps through the ToUnicode CMap.
    let file = subsetter::subset(font.data, font.index, &subset.remapper).ok();
    let (subtype, file_key, file_extra, gid_map) = if font.cff {
        ("CIDFontType0", "FontFile3", " /Subtype /OpenType".to_owned(), "")
    } else {
        let len = file.as_ref().map_or(0, Vec::len);
        (
            "CIDFontType2",
            "FontFile2",
            format!(" /Length1 {len}"),
            " /CIDToGIDMap /Identity",
        )
    };
    let mut descriptor = format!(
        "<< /Type /FontDescriptor /FontName /{name} /Flags 4 /FontBBox [{} {} {} {}] /ItalicAngle {} /Ascent {} /Descent {} /CapHeight {} /StemV {stem_v}",
        scale(f64::from(bbox.x_min)),
        scale(f64::from(bbox.y_min)),
        scale(f64::from(bbox.x_max)),
        scale(f64::from(bbox.y_max)),
        num(f64::from(face.italic_angle())),
        scale(ascent),
        scale(descent),
        scale(cap),
    );
    if file.is_some() {
        let _ = write!(descriptor, " /{file_key} {} 0 R", first + 3);
    }
    descriptor.push_str(" >>");
    let file = file.unwrap_or_default();
    vec![
        format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{} 0 R] /ToUnicode {} 0 R >>",
            first + 1,
            first + 4
        )
        .into_bytes(),
        format!(
            "<< /Type /Font /Subtype /{subtype} /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {} 0 R /DW 0 /W {}{gid_map} >>",
            first + 2,
            widths_array(font, subset)
        )
        .into_bytes(),
        descriptor.into_bytes(),
        stream_object(&file, &file_extra),
        stream_object(to_unicode_cmap(subset).as_bytes(), ""),
    ]
}

/// The pages as a PDF file.
#[must_use]
pub fn write_pdf(pages: &[Page], info: &PdfInfo) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = Vec::new();
    // 1 catalog, 2 page tree, 3 info, 4..6 fonts, then a page and its content per page, then the
    // objects of each embedded font.
    let font_base = 4;
    let page_base = font_base + FACES.len();
    let kids: Vec<String> = (0..pages.len())
        .map(|i| format!("{} 0 R", page_base + 2 * i))
        .collect();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        )
        .into_bytes(),
    );
    objects.push(
        format!(
            "<< /Producer {} /Creator {} /Title {} >>",
            info_string(PRODUCER),
            info_string(PRODUCER),
            info_string(&info.title)
        )
        .into_bytes(),
    );
    for face in FACES {
        objects.push(
            format!(
                "<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>",
                face.base_font()
            )
            .into_bytes(),
        );
    }
    let base_fonts: Vec<String> = (0..FACES.len())
        .map(|i| format!("/F{} {} 0 R", i + 1, font_base + i))
        .collect();
    let mut subsets: BTreeMap<FontId, Subset> = BTreeMap::new();
    let contents = page_contents(pages, &mut subsets);
    // Embedded fonts follow the pages, in font order.
    let embedded_base = page_base + 2 * pages.len();
    let embedded_first: BTreeMap<FontId, usize> = subsets
        .keys()
        .enumerate()
        .map(|(k, &id)| (id, embedded_base + OBJECTS_PER_FONT * k))
        .collect();
    for (i, (page, (out, used))) in pages.iter().zip(&contents).enumerate() {
        let mut fonts = base_fonts.clone();
        fonts.extend(used.iter().filter_map(|id| {
            embedded_first
                .get(id)
                .map(|n| format!("/{} {n} 0 R", embedded_resource(*id)))
        }));
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << {} >> >> /Contents {} 0 R >>",
                num(page.width * PT_PER_MM),
                num(page.height * PT_PER_MM),
                fonts.join(" "),
                page_base + 2 * i + 1
            )
            .into_bytes(),
        );
        objects.push(stream_object(out.as_bytes(), ""));
    }
    for (id, subset) in &subsets {
        if let (Some(font), Some(&first)) = (unicode::font(*id), embedded_first.get(id)) {
            objects.extend(font_objects(font, subset, first));
        }
    }

    // CFF font files (`FontFile3` of subtype `OpenType`) need PDF 1.6.
    let cff = subsets.keys().any(|&id| unicode::font(id).is_some_and(|f| f.cff));
    serialize(&objects, if cff { "1.6" } else { "1.4" })
}

/// Each page's content stream and the bundled fonts it uses; `subsets` collects the glyphs.
fn page_contents(pages: &[Page], subsets: &mut BTreeMap<FontId, Subset>) -> Vec<(String, BTreeSet<FontId>)> {
    pages
        .iter()
        .map(|page| {
            let mut content = Content {
                out: String::new(),
                height: page.height,
                state: Emitted::default(),
                saved: Vec::new(),
                subsets,
                used: BTreeSet::new(),
            };
            for op in &page.ops {
                content.op(op);
            }
            (content.out, content.used)
        })
        .collect()
}

/// The objects (numbered from 1) as a PDF file of `version` with its cross-reference table and
/// trailer.
fn serialize(objects: &[Vec<u8>], version: &str) -> Vec<u8> {
    let mut out: Vec<u8> = format!("%PDF-{version}\n").into_bytes();
    out.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for off in offsets {
        let _ = writeln!(table, "{off:010} 00000 n ");
    }
    let _ = write!(
        table,
        "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.extend_from_slice(table.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_compact() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(2.5), "2.5");
        assert_eq!(color([255, 0, 128]), "1 0 0.502");
    }

    #[test]
    fn strings_are_escaped_and_encoded() {
        assert_eq!(string_literal("a(b)\\"), "(a\\(b\\)\\\\)");
        assert_eq!(string_literal("–·"), "(\\226\\267)");
        assert_eq!(string_literal("ج"), "(?)");
        assert_eq!(info_string("a—b"), "<FEFF006120140062>");
        assert_eq!(info_string("plain (x)"), "(plain \\(x\\))");
    }

    fn text_page(text: &str) -> Page {
        Page {
            width: 100.0,
            height: 100.0,
            ops: vec![Op::Text {
                text: text.into(),
                x: 10.0,
                y: 10.0,
                align: Align::Left,
                baseline: Baseline::Alphabetic,
                angle: 0.0,
                style: FontStyle::Normal,
                size: 10.0,
                color: [0, 0, 0],
            }],
        }
    }

    #[test]
    fn latin_text_keeps_the_base_font_path() {
        let mut subsets = BTreeMap::new();
        let contents = page_contents(&[text_page("Front · €5 ●")], &mut subsets);
        assert!(subsets.is_empty());
        assert_eq!(
            contents[0].0,
            "0 0 0 rg\nBT /F1 10 Tf 1 0 0 1 28.346 255.118 Tm (Front \\267 \\2005 \\225) Tj ET\n"
        );
    }

    #[test]
    fn marks_are_positioned_with_rise_and_adjustments() {
        let mut subsets = BTreeMap::new();
        let contents = page_contents(&[text_page("بِسْمِ")], &mut subsets);
        let line = &contents[0].0;
        assert!(line.contains(" Ts ["), "{line}");
        // The rise returns to the baseline for the letters.
        assert!(line.contains(" 0 Ts [<"), "{line}");
        assert!(!line.contains("[]"), "{line}");
        let cmap = to_unicode_cmap(&subsets[&contents[0].1.iter().copied().next().expect("font")]);
        assert!(cmap.contains("<0650>") && cmap.contains("<0628>"), "{cmap}");
    }

    #[test]
    fn cff_fonts_embed_as_open_type_font_files() {
        // The bundled CJK subset has CFF outlines.
        let font = unicode::font(FontId::CJK).expect("bundled CJK font");
        assert!(font.cff);
        let mut subset = Subset::default();
        for c in ['漢', '字'] {
            let id = font.face.glyph_index(c).map_or(0, |g| g.0);
            subset.cid(&Glyph {
                id,
                advance: font.advance(id),
                x_offset: 0.0,
                y_offset: 0.0,
                cluster: 0,
                text: c.to_string(),
            });
        }
        let objects = font_objects(font, &subset, 10);
        let text = |i: usize| String::from_utf8_lossy(&objects[i]).into_owned();
        assert!(text(1).contains("/Subtype /CIDFontType0 ") && !text(1).contains("CIDToGIDMap"));
        assert!(text(2).contains("/FontFile3 13 0 R"));
        assert!(text(3).contains("/Subtype /OpenType"));
        let start = objects[3]
            .windows(7)
            .position(|w| w == b"stream\n")
            .expect("stream")
            + 7;
        let end = objects[3].len() - b"\nendstream".len();
        let file = miniz_oxide::inflate::decompress_to_vec_zlib(&objects[3][start..end]).expect("inflates");
        assert!(file.starts_with(b"OTTO"), "an OpenType CFF font");
        let face = rustybuzz::ttf_parser::Face::parse(&file, 0).expect("subset parses");
        assert_eq!(face.number_of_glyphs(), 3);
    }

    #[test]
    fn mixed_lines_switch_fonts_inside_one_text_object() {
        let mut subsets = BTreeMap::new();
        let contents = page_contents(&[text_page("Sleeve آستین")], &mut subsets);
        let line = &contents[0].0;
        assert!(
            line.starts_with("0 0 0 rg\nBT 1 0 0 1 28.346 255.118 Tm /F1 10 Tf (Sleeve ) Tj /F4 10 Tf [<"),
            "{line}"
        );
        assert!(line.ends_with("] TJ ET\n"), "{line}");
        let widths = widths_array(
            unicode::font(FontId::REGULAR).expect("font"),
            &subsets[&FontId::REGULAR],
        );
        assert!(widths.starts_with("[1 ["), "{widths}");
    }
}
