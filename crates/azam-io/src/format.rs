//! Number and text formatting shared by every exporter.
//!
//! Exported files must match the recorded reference data character for character, so numbers are
//! printed the way the reference formats them: [`fixed`] rounds half away from zero on exact ties
//! and never prints `-0`, and [`number`] prints the shortest round-trip form with exponents only
//! below 1e-6 and from 1e21.

use azam_doc::Units;

/// Millimetres per inch.
pub const MM_PER_INCH: f64 = 25.4;
/// Millimetres per centimetre.
pub const MM_PER_CM: f64 = 10.0;
/// Square millimetres per square inch.
pub const MM2_PER_IN2: f64 = 645.16;
/// Square millimetres per square centimetre.
pub const MM2_PER_CM2: f64 = 100.0;
/// Placeholder printed for a missing value.
pub const MISSING: &str = "–";
/// Name used when a garment or file has no usable name.
const FALLBACK_NAME: &str = "garment";
/// Digits kept beyond the requested precision when checking for an exact rounding tie. A double
/// that ties exactly at digit `n + 1` has no further digits, so any non-zero digit here breaks it.
const TIE_DIGITS: usize = 80;

/// `v` with exactly `digits` decimals.
///
/// Exact ties round away from zero (2.5 → "3"), and zero never prints a sign.
#[must_use]
pub fn fixed(v: f64, digits: usize) -> String {
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    let v = if v == 0.0 { 0.0 } else { v };
    let long = format!("{:.*}", digits + TIE_DIGITS, v.abs());
    let dot = long.find('.').unwrap_or(long.len());
    let tail = &long[dot + 1 + digits..];
    let is_tie = tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0');
    let magnitude = if is_tie {
        // Round the tie up in magnitude: bump the digit before it.
        round_up(&long[..dot + 1 + digits], digits)
    } else {
        format!("{:.*}", digits, v.abs())
    };
    let sign = if v < 0.0 { "-" } else { "" };
    format!("{sign}{magnitude}")
}

/// Adds one unit in the last place to a non-negative decimal string with `digits` decimals.
fn round_up(truncated: &str, digits: usize) -> String {
    let text = if digits == 0 {
        truncated.trim_end_matches('.')
    } else {
        truncated
    };
    let mut bytes = text.as_bytes().to_vec();
    let mut i = bytes.len();
    loop {
        if i == 0 {
            bytes.insert(0, b'1');
            break;
        }
        i -= 1;
        match bytes[i] {
            b'.' => {}
            b'9' => bytes[i] = b'0',
            d => {
                bytes[i] = d + 1;
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap_or_default()
}

/// `v` in its shortest round-trip form: whole numbers without a fraction, exponents only for
/// magnitudes below 1e-6 or from 1e21 (`1e-7`, `1.5e+21`), and `0` for negative zero.
#[must_use]
pub fn number(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if v == 0.0 {
        return "0".to_owned();
    }
    let a = v.abs();
    if (1e-6..1e21).contains(&a) {
        return format!("{v}");
    }
    let e = format!("{v:e}");
    match e.split_once('e') {
        Some((m, x)) if !x.starts_with('-') => format!("{m}e+{x}"),
        _ => e,
    }
}

/// Parses the longest leading decimal number of `s` (after leading whitespace), ignoring the rest:
/// `"12.5 mm"` → 12.5. Returns `None` when there is no number.
#[must_use]
pub fn parse_float_prefix(s: &str) -> Option<f64> {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    if s[i..].starts_with("Infinity") {
        return Some(if b.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        let frac_start = i + 1;
        let mut j = frac_start;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        digits += j - frac_start;
        if digits > 0 {
            i = j;
        }
    }
    if digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    s[..i].parse().ok()
}

/// Parses the leading decimal integer of `s` (after leading whitespace): `"10.5"` → 10.
#[must_use]
pub fn parse_int_prefix(s: &str) -> Option<i64> {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut i = usize::from(!b.is_empty() && (b[0] == b'+' || b[0] == b'-'));
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return None;
    }
    s[..i].parse().ok()
}

/// Parses the leading hexadecimal digits of `s` (`"efeae1"` → 0xefeae1).
#[must_use]
pub fn parse_hex_prefix(s: &str) -> Option<u32> {
    let end = s.bytes().position(|b| !b.is_ascii_hexdigit()).unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    // Colours have at most 8 hex digits; longer runs saturate.
    u32::from_str_radix(&s[..end.min(8)], 16).ok()
}

/// A `#rrggbb` colour as red, green and blue bytes, or `None` if it does not start with hex digits
/// after the `#`.
#[must_use]
pub fn hex_rgb(color: &str) -> Option<[u8; 3]> {
    let n = parse_hex_prefix(color.get(1..)?)?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// A length in display units without the unit: cm and mm with one decimal, inches with two and
/// an inch mark. `digits` overrides the number of decimals.
#[must_use]
pub fn length(mm: Option<f64>, units: Units, digits: Option<usize>) -> String {
    let Some(mm) = mm.filter(|v| !v.is_nan()) else {
        return MISSING.to_owned();
    };
    match units {
        Units::In => format!("{}″", fixed(mm / MM_PER_INCH, digits.unwrap_or(2))),
        Units::Mm => fixed(mm, digits.unwrap_or(1)),
        Units::Cm => fixed(mm / MM_PER_CM, digits.unwrap_or(1)),
    }
}

/// Short label of a unit: `"cm"`, `"mm"` or `"in"`.
#[must_use]
pub fn unit_label(units: Units) -> &'static str {
    match units {
        Units::In => "in",
        Units::Mm => "mm",
        Units::Cm => "cm",
    }
}

/// The unit spelled out: `"centimetres"`, `"millimetres"` or `"inches"`.
#[must_use]
pub fn unit_word(units: Units) -> &'static str {
    match units {
        Units::In => "inches",
        Units::Mm => "millimetres",
        Units::Cm => "centimetres",
    }
}

/// A value typed in display units, in mm, or `None` if it does not start with a number.
#[must_use]
pub fn to_mm(text: &str, units: Units) -> Option<f64> {
    let n = parse_float_prefix(text)?;
    Some(match units {
        Units::In => n * MM_PER_INCH,
        Units::Mm => n,
        Units::Cm => n * MM_PER_CM,
    })
}

/// A length in mm converted to display units.
#[must_use]
pub fn from_mm(mm: f64, units: Units) -> f64 {
    match units {
        Units::In => mm / MM_PER_INCH,
        Units::Mm => mm,
        Units::Cm => mm / MM_PER_CM,
    }
}

/// A length with its unit, in plain ASCII: `"12.5 cm"`, `"125 mm"`, `"4.92 in"`.
#[must_use]
pub fn length_with_unit(mm: f64, units: Units) -> String {
    if mm.is_nan() {
        return MISSING.to_owned();
    }
    match units {
        Units::In => format!("{} in", fixed(mm / MM_PER_INCH, 2)),
        Units::Mm => format!("{} mm", number(round_half_up(mm))),
        Units::Cm => format!("{} cm", fixed(mm / MM_PER_CM, 1)),
    }
}

/// A length as a bare number in display units (the unit is stated elsewhere).
#[must_use]
pub fn length_number(mm: f64, units: Units) -> String {
    if mm.is_nan() {
        return MISSING.to_owned();
    }
    match units {
        Units::In => fixed(mm / MM_PER_INCH, 2),
        Units::Mm => number(round_half_up(mm)),
        Units::Cm => fixed(mm / MM_PER_CM, 1),
    }
}

/// An area with its unit: `"1234 cm²"` or `"191.3 in²"`.
#[must_use]
pub fn area_with_unit(mm2: f64, units: Units) -> String {
    match units {
        Units::In => format!("{} in²", fixed(mm2 / MM2_PER_IN2, 1)),
        _ => format!("{} cm²", fixed(mm2 / MM2_PER_CM2, 0)),
    }
}

/// Rounds to the nearest integer, halves towards positive infinity (−2.5 → −2, 2.5 → 3).
#[must_use]
pub fn round_half_up(v: f64) -> f64 {
    let f = v.floor();
    if v - f >= 0.5 { f + 1.0 } else { f }
}

/// A file-name and id friendly form of a name: ASCII letters and digits, runs of anything else
/// as one `-`, lower case; `"garment"` if nothing is left.
#[must_use]
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            if dash {
                out.push('-');
                dash = false;
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    // A leading run of other characters became a dash before the first letter; drop it, and the
    // trailing one is never written.
    let trimmed = out.strip_prefix('-').unwrap_or(&out);
    if trimmed.is_empty() {
        FALLBACK_NAME.to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// A name made safe for a file name: characters file systems reject become spaces, runs of white
/// space collapse to one space.
#[must_use]
pub fn file_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c <= '\u{1f}' {
                ' '
            } else {
                c
            }
        })
        .collect();
    let collapsed = replaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        FALLBACK_NAME.to_owned()
    } else {
        collapsed
    }
}

/// Spreadsheet-style letters for an index: 0 → A, 25 → Z, 26 → AA.
#[must_use]
pub fn letters(index: usize) -> String {
    let mut s = Vec::new();
    let mut n = index;
    loop {
        s.push(b'A' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// Number of UTF-16 code units in `s` (the length text measurement and truncation count in).
#[must_use]
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The first `n` UTF-16 code units of `s` (never splitting a character).
#[must_use]
pub fn utf16_prefix(s: &str, n: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    for ch in s.chars() {
        count += ch.len_utf16();
        if count > n {
            break;
        }
        out.push(ch);
    }
    out
}

/// The UTC date (`MM-DD-YYYY`) and time (`HH:MM:SS`) `secs` seconds after the Unix epoch, as file
/// headers write them.
#[must_use]
pub fn utc_date_time(secs: u64) -> (String, String) {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // days to a civil date (proleptic Gregorian), after Howard Hinnant's algorithm
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        format!("{month:02}-{day:02}-{year:04}"),
        format!("{:02}:{:02}:{:02}", rem / 3_600, rem % 3_600 / 60, rem % 60),
    )
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "exact parse results")]
mod tests {
    use super::*;

    #[test]
    fn dates_from_the_epoch() {
        assert_eq!(utc_date_time(0), ("01-01-1970".into(), "00:00:00".into()));
        // 2026-11-07 13:05:09 UTC
        assert_eq!(
            utc_date_time(1_794_056_709),
            ("11-07-2026".into(), "13:05:09".into())
        );
        // a leap day
        assert_eq!(utc_date_time(951_782_400).0, "02-29-2000");
    }

    #[test]
    fn fixed_rounds_ties_away_from_zero() {
        assert_eq!(fixed(2.5, 0), "3");
        assert_eq!(fixed(0.125, 2), "0.13");
        assert_eq!(fixed(-2.5, 0), "-3");
        assert_eq!(fixed(1.005, 2), "1.00"); // 1.005 is just below the tie in binary
        assert_eq!(fixed(-0.0, 3), "0.000");
        assert_eq!(fixed(-0.0001, 2), "-0.00");
        assert_eq!(fixed(9.995, 2), "9.99");
        assert_eq!(fixed(99.5, 0), "100");
        assert_eq!(fixed(1234.5678, 3), "1234.568");
    }

    #[test]
    fn number_matches_reference_printing() {
        assert_eq!(number(1450.0), "1450");
        assert_eq!(number(-0.0), "0");
        assert_eq!(number(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(number(1e-7), "1e-7");
        assert_eq!(number(1.5e21), "1.5e+21");
        assert_eq!(number(123_456.0), "123456");
    }

    #[test]
    fn lenient_number_parsing() {
        assert_eq!(parse_float_prefix(" 12.5mm"), Some(12.5));
        assert_eq!(parse_float_prefix("-.5"), Some(-0.5));
        assert_eq!(parse_float_prefix("1e3x"), Some(1000.0));
        assert_eq!(parse_float_prefix("1e"), Some(1.0));
        assert_eq!(parse_float_prefix("abc"), None);
        assert_eq!(parse_float_prefix("."), None);
        assert_eq!(parse_int_prefix(" 10.5"), Some(10));
        assert_eq!(parse_int_prefix("x"), None);
        assert_eq!(hex_rgb("#efeae1"), Some([0xef, 0xea, 0xe1]));
    }

    #[test]
    fn units() {
        assert_eq!(length(Some(123.0), Units::Cm, None), "12.3");
        assert_eq!(length(Some(25.4), Units::In, None), "1.00″");
        assert_eq!(length(None, Units::Mm, None), MISSING);
        assert_eq!(to_mm("2", Units::In), Some(50.8));
        assert_eq!(length_with_unit(12.5, Units::Mm), "13 mm");
        assert_eq!(length_with_unit(-12.5, Units::Mm), "-12 mm");
        assert_eq!(area_with_unit(12_345.0, Units::Cm), "123 cm²");
    }

    #[test]
    fn names() {
        assert_eq!(slug("  Sleeve L (copy) "), "sleeve-l-copy");
        assert_eq!(slug("--Front--"), "front");
        assert_eq!(slug("جلو"), "garment");
        assert_eq!(file_name("a/b:c  d\t"), "a b c d");
        assert_eq!(letters(0), "A");
        assert_eq!(letters(27), "AB");
        assert_eq!(utf16_prefix("ab😀c", 3), "ab");
    }
}
