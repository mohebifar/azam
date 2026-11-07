//! PDF output: a small vector page model ([`canvas`]), Helvetica text metrics, a PDF writer
//! (base-14 Helvetica, plus embedded subsets of bundled Unicode fonts for text `WinAnsi` cannot
//! encode, shaped and in bidirectional order), and the pattern and tech-pack documents built on
//! them.

pub mod canvas;
pub mod metrics;
mod metrics_data;
pub mod pattern;
pub mod techpack;
mod unicode;
pub mod writer;

pub use unicode::needs_unicode_font;

/// ISO A4 short and long side, mm.
pub const A4_MM: [f64; 2] = [210.0, 297.0];

/// Whether `text` has characters outside the Latin-1 and typographic set the base-14 fonts draw.
///
/// The PDF writer draws text outside `WinAnsiEncoding` (see [`needs_unicode_font`]) with the
/// bundled Unicode fonts and measures it with their shaped advances; characters none of them has
/// are written as `?`.
#[must_use]
pub fn needs_raster(text: &str) -> bool {
    text.chars().any(|c| {
        !matches!(c,
            '\n' | '\r' | ' '..='~' | '\u{a0}'..='\u{ff}'
            | '\u{2013}' | '\u{2014}' | '\u{2018}' | '\u{2019}' | '\u{201c}' | '\u{201d}' | '\u{2022}' | '\u{2026}')
    })
}
