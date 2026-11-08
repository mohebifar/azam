//! Shared loading and comparison helpers for the golden tests.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::sync::OnceLock;

use azam_doc::{PieceId, Project, Units};
use azam_io::{Layout, Paper, PatternPdfOptions};
use serde_json::Value;

const GOLDEN: &str = include_str!("../golden/io.json");

/// The recorded reference data.
pub fn golden() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(GOLDEN).expect("golden data parses"))
}

/// Every recorded project case.
pub fn projects() -> &'static [Value] {
    golden()["projects"].as_array().expect("projects")
}

/// The project of a case, read like a stored file.
pub fn load(case: &Value) -> Project {
    Project::load(case["project"].clone()).expect("recorded project loads")
}

/// The project of the case named `name`.
pub fn project_named(name: &str) -> Project {
    let case = projects()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no project {name}"));
    load(case)
}

/// The header date of the recorded PDFs.
pub fn date() -> String {
    golden()["date"].as_str().expect("date").to_owned()
}

/// Pattern PDF options as recorded.
pub fn pdf_options(o: &Value) -> PatternPdfOptions {
    PatternPdfOptions {
        layout: match o["layout"].as_str() {
            Some("pieces") => Layout::Pieces,
            Some("overview") => Layout::Overview,
            Some("plotter") => Layout::Plotter,
            _ => Layout::Tiled,
        },
        paper: match o["paper"].as_str() {
            Some("a3") => Paper::A3,
            Some("letter") => Paper::Letter,
            Some("tabloid") => Paper::Tabloid,
            _ => Paper::A4,
        },
        units: match o["units"].as_str() {
            Some("mm") => Units::Mm,
            Some("in") => Units::In,
            _ => Units::Cm,
        },
        sa: o["sa"].as_bool().unwrap_or(false),
        lengths: o["lengths"].as_bool().unwrap_or(false),
        dims: o["dims"].as_bool().unwrap_or(false),
        all_sizes: o["sizes"] == "all",
        plotter_width_mm: o["plotter"].as_f64().unwrap_or(914.0),
        piece_ids: o["ids"]
            .as_array()
            .map(|ids| ids.iter().filter_map(Value::as_str).map(PieceId::from).collect())
            .unwrap_or_default(),
        date: date(),
    }
}

fn close(e: f64, a: f64, tol: f64) -> bool {
    (e - a).abs() <= tol + 1e-9 * e.abs().max(a.abs())
}

/// Compares `actual` with `expected`: numbers within `tol` (plus 1e-9 relative), everything else
/// exactly. Object keys missing from `expected` are ignored. Returns the first difference.
pub fn diff(expected: &Value, actual: &Value, tol: f64, path: &str) -> Option<String> {
    match (expected, actual) {
        (Value::Number(e), Value::Number(a)) => {
            let (e, a) = (e.as_f64().unwrap_or(f64::NAN), a.as_f64().unwrap_or(f64::NAN));
            (!close(e, a, tol)).then(|| format!("{path}: expected {e}, got {a}"))
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                return Some(format!("{path}: expected {} items, got {}", e.len(), a.len()));
            }
            e.iter()
                .zip(a)
                .enumerate()
                .find_map(|(i, (e, a))| diff(e, a, tol, &format!("{path}[{i}]")))
        }
        (Value::Object(e), Value::Object(a)) => e.iter().find_map(|(k, ev)| match a.get(k) {
            Some(av) => diff(ev, av, tol, &format!("{path}.{k}")),
            None => Some(format!("{path}: missing key {k}")),
        }),
        _ => (expected != actual).then(|| format!("{path}: expected {expected}, got {actual}")),
    }
}

/// Asserts that `actual` matches `expected` (see [`diff`]).
pub fn assert_close(expected: &Value, actual: &Value, tol: f64, what: &str) {
    if let Some(d) = diff(expected, actual, tol, what) {
        panic!("{d}");
    }
}

/// Splits text into runs of number characters and other characters.
fn tokens(s: &str) -> Vec<(bool, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut numeric = None;
    for (i, c) in s.char_indices() {
        let n = c.is_ascii_digit() || c == '.' || c == '-';
        if numeric != Some(n) {
            if let Some(kind) = numeric {
                out.push((kind, &s[start..i]));
            }
            start = i;
            numeric = Some(n);
        }
    }
    if let Some(kind) = numeric {
        out.push((kind, &s[start..]));
    }
    out
}

/// Asserts that two texts are equal, or differ only in numbers that agree within `tol` (for
/// last-digit rounding of values computed with different trigonometry libraries).
pub fn assert_text_close(expected: &str, actual: &str, tol: f64, what: &str) {
    if expected == actual {
        return;
    }
    let (e, a) = (tokens(expected), tokens(actual));
    assert_eq!(e.len(), a.len(), "{what}: texts differ in structure");
    for (i, ((en, et), (an, at))) in e.iter().zip(&a).enumerate() {
        if et == at {
            continue;
        }
        let ok = *en
            && *an
            && match (et.parse::<f64>(), at.parse::<f64>()) {
                (Ok(x), Ok(y)) => close(x, y, tol),
                _ => false,
            };
        assert!(ok, "{what}: token {i}: expected {et:?}, got {at:?}");
    }
}
