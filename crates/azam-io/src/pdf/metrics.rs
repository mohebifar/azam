//! Text measurement with the base-14 Helvetica metrics (and the bundled Unicode fonts for text
//! `WinAnsi` cannot encode), and line wrapping.

use super::metrics_data::{BOLD, OBLIQUE, REGULAR};
use super::unicode;

/// Points per millimetre.
pub const PT_PER_MM: f64 = 72.0 / 25.4;
/// Line spacing as a multiple of the font size.
pub const LINE_HEIGHT_FACTOR: f64 = 1.15;

/// Style of the Helvetica face.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    /// Helvetica.
    #[default]
    Normal,
    /// Helvetica-Bold.
    Bold,
    /// Helvetica-Oblique.
    Italic,
}

impl FontStyle {
    /// Name of the style as used in the recorded reference data.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Bold => "bold",
            Self::Italic => "italic",
        }
    }

    /// PostScript name of the base-14 font.
    #[must_use]
    pub fn base_font(self) -> &'static str {
        match self {
            Self::Normal => "Helvetica",
            Self::Bold => "Helvetica-Bold",
            Self::Italic => "Helvetica-Oblique",
        }
    }

    pub(super) fn metrics(self) -> &'static FaceMetrics {
        match self {
            Self::Normal => &REGULAR,
            Self::Bold => &BOLD,
            Self::Italic => &OBLIQUE,
        }
    }
}

/// Advance widths and kerning of one face, indexed by UTF-16 code unit.
pub(super) struct FaceMetrics {
    /// Widths are stored in 1/`width_unit` of the font size.
    pub width_unit: f64,
    /// Kerning values are stored in 1/`kerning_unit` of the font size (negative: tighter).
    pub kerning_unit: f64,
    /// `(code unit, width)`, sorted by code unit. Code unit 0 holds the width of unknown glyphs.
    pub widths: &'static [(u16, u16)],
    /// `(code unit, previous code unit, adjustment)`, sorted.
    pub kerning: &'static [(u16, u16, i16)],
}

impl FaceMetrics {
    fn width(&self, c: u16) -> f64 {
        let w = |c: u16| {
            self.widths
                .binary_search_by_key(&c, |e| e.0)
                .ok()
                .map(|i| self.widths[i].1)
                .filter(|&w| w != 0)
        };
        // Without a default width an unknown glyph is one em wide.
        w(c).or_else(|| w(0))
            .map_or(1.0, |w| f64::from(w) / self.width_unit)
    }

    fn kern(&self, c: u16, prev: u16) -> f64 {
        self.kerning
            .binary_search_by_key(&(c, prev), |e| (e.0, e.1))
            .map_or(0.0, |i| f64::from(self.kerning[i].2) / self.kerning_unit)
    }

    /// Widths of each code unit of `units`, in font-size units.
    pub(super) fn unit_widths(&self, units: &[u16], kerning: bool) -> Vec<f64> {
        let mut prev = 0;
        units
            .iter()
            .map(|&c| {
                let k = if kerning { self.kern(c, prev) } else { 0.0 };
                prev = c;
                self.width(c) + k
            })
            .collect()
    }
}

/// Width of `text` at size 1 (multiply by the font size in pt), with kerning.
///
/// Text with characters `WinAnsiEncoding` lacks that a bundled Unicode font draws is measured as
/// it is drawn: Helvetica for the left-to-right `WinAnsi` runs and the fonts' shaped advances for
/// the rest.
#[must_use]
pub fn string_unit_width(text: &str, style: FontStyle, kerning: bool) -> f64 {
    if let Some(width) = unicode::unit_width(text, style, kerning) {
        return width;
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    style.metrics().unit_widths(&units, kerning).iter().sum()
}

/// Width of `text` in mm at `size` pt, with kerning.
#[must_use]
pub fn text_width(text: &str, style: FontStyle, size: f64) -> f64 {
    string_unit_width(text, style, true) * size / PT_PER_MM
}

/// Splits `text` into lines no wider than `max_width` mm at `size` pt: paragraphs at line breaks,
/// words at spaces, and words wider than a line at characters.
#[must_use]
pub fn split_to_width(text: &str, style: FontStyle, size: f64, max_width: f64) -> Vec<String> {
    let max_len = PT_PER_MM * max_width / size;
    let metrics = style.metrics();
    let widths = |units: &[u16]| {
        unicode::unit_widths(&String::from_utf16_lossy(units), style, true)
            .unwrap_or_else(|| metrics.unit_widths(units, true))
    };
    let space = metrics.unit_widths(&[u16::from(b' ')], true)[0];
    text.split('\n')
        .map(|p| p.strip_suffix('\r').unwrap_or(p))
        .flat_map(|p| split_paragraph(p, &widths, space, max_len))
        .collect()
}

/// Splits one paragraph; `unit_widths` gives the width of each code unit of a word.
fn split_paragraph(
    text: &str,
    unit_widths: &dyn Fn(&[u16]) -> Vec<f64>,
    space: f64,
    max_len: f64,
) -> Vec<String> {
    let mut lines: Vec<Vec<Vec<u16>>> = vec![Vec::new()];
    let mut line_len = 0.0;
    let mut separator = 0.0;
    for word in text.split(' ') {
        let units: Vec<u16> = word.encode_utf16().collect();
        let widths = unit_widths(&units);
        let mut word_len: f64 = widths.iter().sum();
        if line_len + separator + word_len > max_len {
            if word_len > max_len {
                let mut parts = split_long_word(&units, &widths, max_len - (line_len + separator), max_len);
                let first = parts.remove(0);
                if let Some(line) = lines.last_mut() {
                    line.push(first);
                }
                let last = parts.pop().unwrap_or_default();
                for part in parts {
                    lines.push(vec![part]);
                }
                word_len = widths[units.len() - last.len()..].iter().sum();
                lines.push(vec![last]);
            } else {
                lines.push(vec![units]);
            }
            line_len = word_len;
            separator = space;
        } else {
            if let Some(line) = lines.last_mut() {
                line.push(units);
            }
            line_len += separator + word_len;
            separator = space;
        }
    }
    lines
        .into_iter()
        .map(|words| {
            let joined: Vec<u16> = words.join(&u16::from(b' '));
            String::from_utf16_lossy(&joined)
        })
        .collect()
}

fn split_long_word(word: &[u16], widths: &[f64], first_max: f64, max: f64) -> Vec<Vec<u16>> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut len = 0.0;
    while i != word.len() && len + widths[i] < first_max {
        len += widths[i];
        i += 1;
    }
    out.push(word[..i].to_vec());
    let mut start = i;
    len = 0.0;
    while i != word.len() {
        if len + widths[i] > max {
            out.push(word[start..i].to_vec());
            len = 0.0;
            start = i;
        }
        len += widths[i];
        i += 1;
    }
    if start != i {
        out.push(word[start..i].to_vec());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_include_kerning() {
        // "VA" kerns tighter than V + A.
        let plain =
            string_unit_width("V", FontStyle::Normal, true) + string_unit_width("A", FontStyle::Normal, true);
        assert!((string_unit_width("VA", FontStyle::Normal, true) - 1.24).abs() < 1e-12);
        assert!((string_unit_width("VA", FontStyle::Normal, false) - plain).abs() < 1e-12);
        // Unknown glyphs take the default width.
        assert!((string_unit_width("●", FontStyle::Normal, true) - 0.53).abs() < 1e-12);
    }

    #[test]
    fn wraps_at_spaces() {
        let lines = split_to_width("one two three four", FontStyle::Normal, 10.0, 12.0);
        assert!(lines.len() > 1);
        assert_eq!(lines.join(" "), "one two three four");
    }
}
