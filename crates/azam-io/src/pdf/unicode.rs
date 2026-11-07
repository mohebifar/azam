//! Text the base-14 fonts cannot draw: bundled Unicode TrueType fonts, bidirectional reordering
//! and shaping.
//!
//! Text whose characters all have a `WinAnsiEncoding` code is drawn and measured with base-14
//! Helvetica, exactly as before. Any other text is split into visual runs with the Unicode
//! bidirectional algorithm (`unicode-bidi`). Inside them, left-to-right characters `WinAnsi` covers
//! stay in Helvetica; everything else is shaped by `rustybuzz` with the first bundled font that
//! has the character, applying the font's GSUB and GPOS (Arabic letters take their joined
//! contextual forms and ligatures, marks are positioned, right-to-left runs come out in visual
//! order), so the PDF writer can address the shaped glyph ids directly.
//!
//! Bundled fonts (`fonts/`, SIL Open Font License 1.1, licence text next to each font):
//!
//! - Vazirmatn v33.003 Regular and Bold (Persian, Arabic and Latin), from
//!   <https://github.com/rastikerdar/vazirmatn/releases/tag/v33.003>.
//! - Noto Sans Hebrew v3.001 Regular and Bold (unhinted), from
//!   <https://github.com/notofonts/hebrew/releases/tag/NotoSansHebrew-v3.001>.
//! - Noto Sans v2.015 Regular and Bold (unhinted; Latin, Greek and Cyrillic), from
//!   <https://github.com/notofonts/latin-greek-cyrillic/releases/tag/NotoSans-v2.015>.
//!
//! - `NotoSansCJKsc-Regular-subset.otf` (2.3 MB, used for bold too): a subset of Noto Sans CJK SC
//!   Regular from the release Sans2.004
//!   (<https://github.com/notofonts/noto-cjk/releases/tag/Sans2.004>, `08_NotoSansCJKsc.zip`,
//!   `NotoSansCJKsc-Regular.otf`, SHA-256
//!   `2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b`; licence
//!   `NotoSansCJK-OFL.txt`). It keeps 10,447 characters in 10,395 glyphs (CFF outlines, embedded
//!   as `OpenType` font files):
//!   - GB 2312 level 1 and 2 hanzi (6,763): the ideographs EUC-CN rows 16–87 (lead bytes
//!     0xB0–0xF7, trail bytes 0xA1–0xFE) decode to;
//!   - the Jōyō kanji: every code point with a `kJoyoKanji` field in Unihan 16.0
//!     (<https://www.unicode.org/Public/16.0.0/ucd/Unihan.zip>, `Unihan_OtherMappings.txt`);
//!   - KS X 1001 Hangul (2,350): the syllables EUC-KR rows 16–40 (lead bytes 0xB0–0xC8, trail
//!     bytes 0xA1–0xFE) decode to, and the compatibility jamo U+3131–U+318E;
//!   - kana U+3041–U+309F, U+30A0–U+30FF, U+31F0–U+31FF;
//!   - CJK punctuation U+3000–U+303F and full- and half-width forms U+FF01–U+FFEF.
//!
//!   Regenerate it (the output is byte-for-byte reproducible) with
//!   `cargo run --release --manifest-path crates/azam-io/tools/cjk-subset/Cargo.toml --
//!   NotoSansCJKsc-Regular.otf Unihan_OtherMappings.txt
//!   crates/azam-io/fonts/NotoSansCJKsc-Regular-subset.otf`: the tool subsets the glyphs with
//!   `subsetter` and adds back a `cmap` for the kept characters and the `OS/2` table (layout tables
//!   are dropped, so CJK text is drawn without vertical or locale variants).
//!
//! Characters outside all of these (rare ideographs, mostly) are looked up in one system font
//! that covers CJK, when the machine has one ([`SYSTEM_FONT_PATHS`]: Arial Unicode, `PingFang` or
//! Hiragino on macOS, Noto Sans CJK or `WenQuanYi` on Linux, Microsoft `YaHei`, MS Gothic or
//! `SimSun` on Windows), and embedded from it the same way (collections use their first face that
//! has CJK); such text depends on the machine. Characters no font has are written
//! as `?` in Helvetica; text with no character any of the fonts draws keeps the plain base-14
//! path, so its output is the same as for `WinAnsi` text.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use rustybuzz::ttf_parser::{self, name_id};
use rustybuzz::{BufferFlags, Direction, Face, UnicodeBuffer};
use unicode_bidi::{BidiClass, BidiInfo, Level, bidi_class};

use super::metrics::FontStyle;

/// A bundled font file.
struct Source {
    /// PostScript name.
    name: &'static str,
    /// The TrueType file.
    data: &'static [u8],
}

/// The bundled fonts, indexed by [`FontId`].
const SOURCES: [Source; 7] = [
    Source {
        name: "Vazirmatn-Regular",
        data: include_bytes!("../../fonts/Vazirmatn-Regular.ttf"),
    },
    Source {
        name: "Vazirmatn-Bold",
        data: include_bytes!("../../fonts/Vazirmatn-Bold.ttf"),
    },
    Source {
        name: "NotoSansHebrew-Regular",
        data: include_bytes!("../../fonts/NotoSansHebrew-Regular.ttf"),
    },
    Source {
        name: "NotoSansHebrew-Bold",
        data: include_bytes!("../../fonts/NotoSansHebrew-Bold.ttf"),
    },
    Source {
        name: "NotoSans-Regular",
        data: include_bytes!("../../fonts/NotoSans-Regular.ttf"),
    },
    Source {
        name: "NotoSans-Bold",
        data: include_bytes!("../../fonts/NotoSans-Bold.ttf"),
    },
    Source {
        name: "NotoSansCJKsc-Regular",
        data: include_bytes!("../../fonts/NotoSansCJKsc-Regular-subset.otf"),
    },
];

/// System fonts that cover CJK, tried in order; the first that exists and has a face with
/// [`SYSTEM_FONT_PROBE`] is used.
pub(crate) const SYSTEM_FONT_PATHS: [&str; 16] = [
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/Library/Fonts/Arial Unicode.ttf",
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
    "C:\\Windows\\Fonts\\msyh.ttc",
    "C:\\Windows\\Fonts\\msgothic.ttc",
    "C:\\Windows\\Fonts\\simsun.ttc",
];
/// A character a system fallback font must have (中, "middle").
const SYSTEM_FONT_PROBE: char = '\u{4e2d}';

/// The system fallback font, after the bundled ones.
const SYSTEM: FontId = FontId(SOURCES.len() as u8);
/// Fonts tried in order for the regular (and oblique) face.
const REGULAR_CHAIN: [FontId; 5] = [FontId(0), FontId(2), FontId(4), FontId(6), SYSTEM];
/// Fonts tried in order for the bold face (the CJK and system fonts have one weight).
const BOLD_CHAIN: [FontId; 5] = [FontId(1), FontId(3), FontId(5), FontId(6), SYSTEM];

/// One of the bundled fonts, or the system fallback font.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FontId(u8);

impl FontId {
    /// Vazirmatn Regular.
    #[cfg(test)]
    pub(crate) const REGULAR: Self = Self(0);
    /// The bundled CJK subset.
    #[cfg(test)]
    pub(crate) const CJK: Self = Self(6);

    /// Position in the font list (bundled fonts, then the system font).
    pub(crate) fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// A parsed font.
pub(crate) struct Font {
    /// PostScript name.
    pub name: String,
    /// The font file (a collection for some system fonts).
    pub data: &'static [u8],
    /// Index of the face in a collection (0 for a single font).
    pub index: u32,
    /// Whether the outlines are CFF (else TrueType).
    pub cff: bool,
    /// The parsed face, for shaping and metrics.
    pub face: Face<'static>,
}

impl Font {
    /// Face `index` of `data`, if it parses and has outlines.
    fn parse(data: &'static [u8], index: u32, name: Option<&str>) -> Option<Self> {
        let face = Face::from_slice(data, index)?;
        let tables = face.tables();
        let cff = tables.glyf.is_none() && tables.cff.is_some();
        if tables.glyf.is_none() && !cff {
            return None;
        }
        let name = name.map_or_else(|| postscript_name(&face), str::to_owned);
        Some(Self {
            name,
            data,
            index,
            cff,
            face,
        })
    }

    /// Font units per em.
    pub(crate) fn units_per_em(&self) -> f64 {
        f64::from(self.face.units_per_em())
    }

    /// The nominal (unshaped) advance of glyph `gid`, in em.
    pub(crate) fn advance(&self, gid: u16) -> f64 {
        let units = self.face.glyph_hor_advance(ttf_parser::GlyphId(gid)).unwrap_or(0);
        f64::from(units) / self.units_per_em()
    }

    /// Whether the font has a glyph for `c`.
    fn covers(&self, c: char) -> bool {
        self.face.glyph_index(c).is_some_and(|g| g.0 != 0)
    }
}

/// The face's PostScript name, reduced to characters a PDF name may hold unescaped.
fn postscript_name(face: &Face<'_>) -> String {
    let name: String = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == name_id::POST_SCRIPT_NAME)
        .find_map(|n| n.to_string())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if name.is_empty() {
        "SystemFallback".to_owned()
    } else {
        name
    }
}

/// The bundled fonts, parsed on first use (`None` for one that fails to parse).
fn fonts() -> &'static [Option<Font>] {
    static FONTS: OnceLock<Vec<Option<Font>>> = OnceLock::new();
    FONTS.get_or_init(|| {
        SOURCES
            .iter()
            .map(|s| Font::parse(s.data, 0, Some(s.name)))
            .collect()
    })
}

/// The first of `paths` that reads and has a face (of a collection, the first such face) with
/// [`SYSTEM_FONT_PROBE`]: the file and the face index.
pub(crate) fn find_system_font(paths: &[&str]) -> Option<(Vec<u8>, u32)> {
    paths.iter().find_map(|path| {
        let data = std::fs::read(path).ok()?;
        let faces = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
        let index = (0..faces).find(|&i| {
            ttf_parser::Face::parse(&data, i).is_ok_and(|f| {
                let t = f.tables();
                (t.glyf.is_some() || t.cff.is_some())
                    && f.glyph_index(SYSTEM_FONT_PROBE).is_some_and(|g| g.0 != 0)
            })
        })?;
        Some((data, index))
    })
}

/// The system fallback font, read on first use.
fn system_font() -> Option<&'static Font> {
    static DATA: OnceLock<Option<(Vec<u8>, u32)>> = OnceLock::new();
    static FONT: OnceLock<Option<Font>> = OnceLock::new();
    FONT.get_or_init(|| {
        let (data, index) = DATA
            .get_or_init(|| find_system_font(&SYSTEM_FONT_PATHS))
            .as_ref()?;
        Font::parse(data, *index, None)
    })
    .as_ref()
}

/// The font `id`.
pub(crate) fn font(id: FontId) -> Option<&'static Font> {
    if id == SYSTEM {
        return system_font();
    }
    fonts().get(id.index()).and_then(Option::as_ref)
}

fn chain(style: FontStyle) -> &'static [FontId] {
    match style {
        FontStyle::Bold => &BOLD_CHAIN,
        FontStyle::Normal | FontStyle::Italic => &REGULAR_CHAIN,
    }
}

fn covers(id: FontId, c: char) -> bool {
    // Only visible characters are worth reading a system font for.
    if id == SYSTEM && (c.is_control() || c.is_whitespace()) {
        return false;
    }
    font(id).is_some_and(|f| f.covers(c))
}

/// The `WinAnsiEncoding` code of a character, if the encoding has one.
pub(crate) fn win_ansi(c: char) -> Option<u8> {
    let code = u32::from(c);
    if (0x20..=0x7e).contains(&code) || (0xa0..=0xff).contains(&code) {
        return Some(code as u8);
    }
    Some(match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8a,
        '‹' => 0x8b,
        'Œ' => 0x8c,
        'Ž' => 0x8e,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' | '●' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9a,
        '›' => 0x9b,
        'œ' => 0x9c,
        'ž' => 0x9e,
        'Ÿ' => 0x9f,
        _ => return None,
    })
}

/// Whether `text` has a character `WinAnsiEncoding` lacks, so it goes through [`layout`] rather
/// than straight to base-14 Helvetica.
#[must_use]
pub fn needs_unicode_font(text: &str) -> bool {
    text.chars().any(|c| win_ansi(c).is_none())
}

/// Characters with the Unicode `Default_Ignorable_Code_Point` property: invisible, and dropped
/// where no font draws them.
fn is_default_ignorable(c: char) -> bool {
    matches!(u32::from(c),
        0x00ad | 0x034f | 0x061c | 0x115f..=0x1160 | 0x17b4..=0x17b5 | 0x180b..=0x180f
        | 0x200b..=0x200f | 0x202a..=0x202e | 0x2060..=0x206f | 0x3164 | 0xfe00..=0xfe0f
        | 0xfeff | 0xffa0 | 0xfff0..=0xfff8 | 0x1bca0..=0x1bca3 | 0x1d173..=0x1d17a
        | 0xe0000..=0xe0fff)
}

/// One shaped glyph of a bundled font.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Glyph {
    /// Glyph id in the full font.
    pub id: u16,
    /// Shaped advance, in em.
    pub advance: f64,
    /// Horizontal displacement from the pen position, in em.
    pub x_offset: f64,
    /// Vertical displacement from the baseline (up), in em.
    pub y_offset: f64,
    /// Byte offset of the glyph's cluster in the laid-out text.
    pub cluster: usize,
    /// The characters the glyph stands for (empty for the further glyphs of a cluster).
    pub text: String,
}

/// A run of text in one font, in visual order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Run {
    /// Base-14 Helvetica: `(byte offset, character as written)`, `?` for a character no font
    /// covers.
    Base(Vec<(usize, char)>),
    /// Shaped glyphs of a bundled font.
    Embedded(FontId, Vec<Glyph>),
}

impl Run {
    /// The characters of a base run as written.
    pub(crate) fn base_text(chars: &[(usize, char)]) -> String {
        chars.iter().map(|&(_, c)| c).collect()
    }

    /// Width at size 1 (Helvetica widths with or without kerning; shaped advances).
    pub(crate) fn width(&self, style: FontStyle, kerning: bool) -> f64 {
        match self {
            Self::Base(chars) => base_widths(chars, style, kerning).iter().sum(),
            Self::Embedded(_, glyphs) => glyphs.iter().map(|g| g.advance).sum(),
        }
    }
}

/// Helvetica widths of each character of a base run.
fn base_widths(chars: &[(usize, char)], style: FontStyle, kerning: bool) -> Vec<f64> {
    // Every written character has a WinAnsi code, so it is one UTF-16 unit.
    let units: Vec<u16> = chars
        .iter()
        .map(|&(_, c)| u16::try_from(u32::from(c)).unwrap_or(u16::from(b'?')))
        .collect();
    style.metrics().unit_widths(&units, kerning)
}

/// Splits `text[range]` (one bidi level run) into items: `None` for Helvetica, else the bundled
/// font that draws it. Left-to-right `WinAnsi` characters stay in Helvetica; other characters keep
/// the current font while it covers them, else take the first font of the chain that does.
fn itemize(
    text: &str,
    range: std::ops::Range<usize>,
    rtl: bool,
    chain: &[FontId],
) -> Vec<(Option<FontId>, std::ops::Range<usize>)> {
    let mut items: Vec<(Option<FontId>, std::ops::Range<usize>)> = Vec::new();
    for (i, c) in text[range.clone()].char_indices() {
        let at = range.start + i;
        let current = items.last().and_then(|it| it.0);
        let slot = if !rtl && win_ansi(c).is_some() {
            None
        } else if is_default_ignorable(c) {
            current
        } else if let Some(f) = current.filter(|&f| covers(f, c)) {
            Some(f)
        } else {
            chain.iter().copied().find(|&f| covers(f, c))
        };
        let end = at + c.len_utf8();
        match items.last_mut() {
            Some(last) if last.0 == slot => last.1.end = end,
            _ => items.push((slot, at..end)),
        }
    }
    items
}

/// Shapes `text[range]` with `font`; glyphs come out in visual order.
fn shape(font: &Font, text: &str, range: std::ops::Range<usize>, rtl: bool) -> Vec<Glyph> {
    let sub = &text[range.clone()];
    let mut buf = UnicodeBuffer::new();
    buf.push_str(sub);
    buf.set_direction(if rtl {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    });
    // Keep zero-width joiners and non-joiners as their own glyphs, so the text they stand for
    // survives in the ToUnicode map.
    buf.set_flags(BufferFlags::PRESERVE_DEFAULT_IGNORABLES);
    let out = rustybuzz::shape(&font.face, &[], buf);
    let upem = font.units_per_em();
    let infos = out.glyph_infos();
    let texts = cluster_texts(font, sub, infos, out.glyph_positions());
    infos
        .iter()
        .zip(out.glyph_positions())
        .zip(texts)
        .filter_map(|((info, pos), text)| {
            let start = info.cluster as usize;
            let id = u16::try_from(info.glyph_id).unwrap_or(0);
            if id == 0 && sub[start..].chars().next().is_some_and(is_default_ignorable) {
                return None;
            }
            Some(Glyph {
                id,
                advance: f64::from(pos.x_advance) / upem,
                x_offset: f64::from(pos.x_offset) / upem,
                y_offset: f64::from(pos.y_offset) / upem,
                cluster: range.start + start,
                text,
            })
        })
        .collect()
}

/// The text each shaped glyph stands for. A cluster of one glyph (a letter, or a ligature) stands
/// for all its characters. In a cluster of several glyphs (a letter and its marks), a glyph that
/// is a character's own glyph in the font (a mark, usually) takes that character, and the first
/// glyph with an advance (the base letter, in whatever form) takes the rest.
fn cluster_texts(
    font: &Font,
    text: &str,
    infos: &[rustybuzz::GlyphInfo],
    positions: &[rustybuzz::GlyphPosition],
) -> Vec<String> {
    let starts: BTreeSet<usize> = infos.iter().map(|g| g.cluster as usize).collect();
    let mut out = vec![String::new(); infos.len()];
    for &start in &starts {
        let end = starts.range(start + 1..).next().copied().unwrap_or(text.len());
        let members: Vec<usize> = (0..infos.len())
            .filter(|&i| infos[i].cluster as usize == start)
            .collect();
        let [first, ..] = members[..] else {
            continue;
        };
        if members.len() == 1 {
            text[start..end].clone_into(&mut out[first]);
            continue;
        }
        let mut rest = String::new();
        for c in text[start..end].chars() {
            let own = font.face.glyph_index(c).map(|g| u32::from(g.0));
            match members
                .iter()
                .find(|&&i| out[i].is_empty() && own == Some(infos[i].glyph_id))
            {
                Some(&i) => out[i].push(c),
                None => rest.push(c),
            }
        }
        if !rest.is_empty() {
            let base = members
                .iter()
                .copied()
                .find(|&i| positions[i].x_advance != 0 && out[i].is_empty())
                .unwrap_or(first);
            out[base].insert_str(0, &rest);
        }
    }
    out
}

/// `text` (one line) as font runs in visual order, left to right; `None` when it needs no
/// bundled font (all of it is `WinAnsi`, or no bundled font has any of its other characters).
pub(crate) fn layout(text: &str, style: FontStyle) -> Option<Vec<Run>> {
    if !needs_unicode_font(text) {
        return None;
    }
    // Lines that mix in left-to-right letters are composed by the (left-to-right) layout, such
    // as a name followed by a size and a date, so they keep that reading order; other text takes
    // the direction of its first strong character.
    let ltr = text.chars().any(|c| bidi_class(c) == BidiClass::L);
    let info = BidiInfo::new(text, ltr.then(Level::ltr));
    let mut runs: Vec<Run> = Vec::new();
    for para in &info.paragraphs {
        let (levels, level_runs) = info.visual_runs(para, para.range.clone());
        for range in level_runs {
            let rtl = levels.get(range.start).is_some_and(Level::is_rtl);
            let mut shaped: Vec<Run> = itemize(text, range, rtl, chain(style))
                .into_iter()
                .map(|(slot, r)| {
                    if let Some((id, f)) = slot.and_then(|id| font(id).map(|f| (id, f))) {
                        return Run::Embedded(id, shape(f, text, r, rtl));
                    }
                    let mut chars: Vec<(usize, char)> = text[r.clone()]
                        .char_indices()
                        .filter(|&(_, c)| !is_default_ignorable(c))
                        .map(|(i, c)| (r.start + i, if win_ansi(c).is_some() { c } else { '?' }))
                        .collect();
                    if rtl {
                        chars.reverse();
                    }
                    Run::Base(chars)
                })
                .collect();
            if rtl {
                shaped.reverse();
            }
            for run in shaped {
                push_run(&mut runs, run);
            }
        }
    }
    runs.iter()
        .any(|r| matches!(r, Run::Embedded(..)))
        .then_some(runs)
}

/// Appends `run`, joining it to the last run when both use the same font.
fn push_run(runs: &mut Vec<Run>, run: Run) {
    match (runs.last_mut(), run) {
        (Some(Run::Base(a)), Run::Base(b)) => a.extend(b),
        (Some(Run::Embedded(fa, a)), Run::Embedded(fb, b)) if *fa == fb => a.extend(b),
        (_, Run::Base(b)) if b.is_empty() => {}
        (_, Run::Embedded(_, b)) if b.is_empty() => {}
        (_, run) => runs.push(run),
    }
}

/// Width of each UTF-16 code unit of `text` at size 1, in logical order: a glyph's advance
/// counts on the first unit of its cluster. `None` when [`layout`] is.
pub(crate) fn unit_widths(text: &str, style: FontStyle, kerning: bool) -> Option<Vec<f64>> {
    let runs = layout(text, style)?;
    let mut unit_at = vec![0; text.len() + 1];
    let mut units = 0;
    for (i, c) in text.char_indices() {
        unit_at[i] = units;
        units += c.len_utf16();
    }
    let mut widths = vec![0.0; units];
    for run in &runs {
        match run {
            Run::Base(chars) => {
                for (&(at, _), w) in chars.iter().zip(base_widths(chars, style, kerning)) {
                    widths[unit_at[at]] += w;
                }
            }
            Run::Embedded(_, glyphs) => {
                for g in glyphs {
                    widths[unit_at[g.cluster]] += g.advance;
                }
            }
        }
    }
    Some(widths)
}

/// Width of `text` at size 1 (see [`unit_widths`]).
pub(crate) fn unit_width(text: &str, style: FontStyle, kerning: bool) -> Option<f64> {
    Some(layout(text, style)?.iter().map(|r| r.width(style, kerning)).sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fonts_parse_and_cover_their_scripts() {
        for id in 0..SOURCES.len() {
            assert!(font(FontId(id as u8)).is_some(), "font {id}");
        }
        for c in "آستین پشت جلو یقه چپ راست گژ\u{200c}"
            .chars()
            .filter(|c| *c != ' ')
        {
            assert!(covers(FontId(0), c) && covers(FontId(1), c), "{c:?}");
        }
        for c in "שלום".chars() {
            assert!(covers(FontId(2), c) && covers(FontId(3), c), "{c:?}");
        }
        for c in "Δλ Ωж".chars().filter(|c| *c != ' ') {
            assert!(covers(FontId(4), c) && covers(FontId(5), c), "{c:?}");
        }
    }

    #[test]
    fn latin_text_needs_no_unicode_font() {
        assert!(!needs_unicode_font("Front · 12–14 cm “x” €"));
        assert!(needs_unicode_font("آستین"));
        assert!(needs_unicode_font("Ł"));
    }

    #[test]
    fn persian_is_shaped_right_to_left() {
        let runs = layout("آستین", FontStyle::Normal).expect("embedded");
        let [Run::Embedded(id, glyphs)] = runs.as_slice() else {
            panic!("one embedded run: {runs:?}");
        };
        assert_eq!(*id, FontId(0));
        // Visual order: the last letter (ن, final form) comes first.
        let text: Vec<&str> = glyphs.iter().map(|g| g.text.as_str()).collect();
        assert_eq!(text.concat().chars().rev().collect::<String>(), "آستین");
        // Joined forms differ from the isolated ones the cmap gives.
        let face = &font(FontId(0)).map(|f| &f.face).expect("font");
        let isolated = face.glyph_index('ن').map(|g| g.0);
        assert_ne!(Some(glyphs[0].id), isolated, "final nun");
        assert!(glyphs.iter().all(|g| g.advance > 0.0));
    }

    #[test]
    fn mixed_text_keeps_latin_in_helvetica() {
        let runs = layout("Sleeve آستین cm", FontStyle::Bold).expect("embedded");
        assert_eq!(runs.len(), 3, "{runs:?}");
        assert!(matches!(&runs[0], Run::Base(c) if Run::base_text(c) == "Sleeve "));
        assert!(matches!(&runs[1], Run::Embedded(FontId(1), _)));
        assert!(matches!(&runs[2], Run::Base(c) if Run::base_text(c) == " cm"));
        let total = unit_width("Sleeve آستین cm", FontStyle::Bold, true).expect("embedded");
        let parts: f64 = unit_widths("Sleeve آستین cm", FontStyle::Bold, true)
            .expect("embedded")
            .iter()
            .sum();
        assert!((total - parts).abs() < 1e-12);
    }

    #[test]
    fn mixed_lines_keep_left_to_right_order() {
        // The name comes first, at the left, as the line was composed.
        let runs = layout("لباده  ·  base size M", FontStyle::Normal).expect("embedded");
        assert!(matches!(&runs[0], Run::Embedded(FontId(0), _)), "{runs:?}");
        assert!(matches!(runs.last(), Some(Run::Base(c)) if Run::base_text(c).ends_with("base size M")));
    }

    #[test]
    fn right_to_left_paragraph_orders_runs_visually() {
        // An RTL paragraph: the number reads left to right at the visual left end.
        let runs = layout("جلو 2", FontStyle::Normal).expect("embedded");
        assert!(matches!(&runs[0], Run::Base(c) if Run::base_text(c) == "2"));
        let hebrew = layout("שלום", FontStyle::Normal).expect("embedded");
        assert!(matches!(&hebrew[..], [Run::Embedded(FontId(2), _)]));
    }

    #[test]
    fn uncovered_characters_fall_back_to_question_marks() {
        // Nothing for a bundled font: the plain base-14 path.
        // (Linear B, which none of the fonts has.)
        assert_eq!(layout("\u{10000}\u{10001}", FontStyle::Normal), None);
        let runs = layout("\u{10000} جلو", FontStyle::Normal).expect("embedded");
        assert!(matches!(&runs[0], Run::Base(c) if Run::base_text(c) == "? "));
    }

    #[test]
    fn marks_keep_their_own_text() {
        // Beh with kasra: the kasra glyph maps to the kasra, the (initial) beh to the beh.
        let runs = layout("بِسْمِ", FontStyle::Normal).expect("embedded");
        let [Run::Embedded(_, glyphs)] = runs.as_slice() else {
            panic!("one embedded run");
        };
        let logical: String = glyphs.iter().rev().map(|g| g.text.as_str()).collect();
        assert_eq!(logical, "بِسْمِ");
        assert!(glyphs.iter().all(|g| g.text.chars().count() == 1), "{glyphs:?}");
        // The marks sit on their letters: no advance, shifted.
        assert!(glyphs.iter().any(|g| g.advance == 0.0 && g.y_offset != 0.0));
    }

    #[test]
    fn greek_uses_noto_sans_and_keeps_latin_in_helvetica() {
        let runs = layout("Δ mm", FontStyle::Bold).expect("embedded");
        assert!(matches!(&runs[0], Run::Embedded(FontId(5), g) if g.len() == 1 && g[0].text == "Δ"));
        assert!(matches!(&runs[1], Run::Base(c) if Run::base_text(c) == " mm"));
    }

    #[test]
    fn cjk_uses_the_bundled_subset() {
        let cjk = font(FontId(6)).expect("bundled CJK font");
        assert!(cjk.cff);
        // Simplified and traditional hanzi, Jōyō kanji, kana, Hangul, punctuation.
        for c in "汉字漢袖子剝ひらがなカタカナ한국어가힘、。「」（）！".chars() {
            assert!(cjk.covers(c), "{c:?}");
        }
        let runs = layout("漢字 sleeve", FontStyle::Bold).expect("embedded");
        let Run::Embedded(id, glyphs) = &runs[0] else {
            panic!("{runs:?}");
        };
        assert_eq!(*id, FontId(6));
        let text: String = glyphs.iter().map(|g| g.text.as_str()).collect();
        assert_eq!(text, "漢字");
        assert!(
            glyphs.iter().all(|g| (g.advance - 1.0).abs() < 1e-9),
            "full-width"
        );
    }

    #[test]
    fn rare_ideographs_use_a_system_font_when_there_is_one() {
        // 丂 (U+4E02) is in GBK but not GB 2312 nor the Jōyō list.
        assert!(!covers(FontId(6), '丂'));
        let runs = layout("丂", FontStyle::Normal);
        if font(SYSTEM).is_some_and(|f| f.covers('丂')) {
            assert!(
                matches!(runs.as_deref(), Some([Run::Embedded(SYSTEM, _)])),
                "{runs:?}"
            );
        } else {
            eprintln!("no system font with U+4E02; skipping");
            assert_eq!(runs, None);
        }
    }

    #[test]
    fn missing_system_fonts_are_skipped() {
        assert!(find_system_font(&["/nonexistent/font.ttc"]).is_none());
    }

    #[test]
    fn zero_width_non_joiner_keeps_its_text() {
        let runs = layout("سرمه\u{200c}ای", FontStyle::Normal).expect("embedded");
        let [Run::Embedded(_, glyphs)] = runs.as_slice() else {
            panic!("one embedded run");
        };
        let logical: String = glyphs.iter().rev().map(|g| g.text.as_str()).collect();
        assert_eq!(logical, "سرمه\u{200c}ای");
    }
}
