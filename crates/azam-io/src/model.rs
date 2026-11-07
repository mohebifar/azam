//! Read-only helpers over the project model that the exporters share.

use std::cmp::Ordering;

use azam_doc::{Fabric, FabricPreset, InternalLine, Piece, Project};
use azam_geom::{Outline, OutlinePoint, PointId, SizeName, Vec2, cut_outline};
use indexmap::IndexMap;
use serde_json::{Map, Value};

/// Pieces cut when a piece does not say how many.
pub const DEFAULT_QTY: f64 = 1.0;
/// Particle distance reported for a piece without one, in mm.
pub const DEFAULT_PARTICLE_DISTANCE_MM: f64 = 20.0;
/// Fabric roll width assumed when a fabric has none, in mm.
pub const DEFAULT_FABRIC_WIDTH_MM: f64 = 1450.0;
/// Cutting waste assumed when a fabric has none, as a fraction of the cut area.
pub const DEFAULT_FABRIC_WASTE: f64 = 0.25;
/// Degrees to radians.
pub const DEG: f64 = std::f64::consts::PI / 180.0;

/// A number stored under `key`, when it is a number.
#[must_use]
pub fn extra_f64(extra: &Map<String, Value>, key: &str) -> Option<f64> {
    extra.get(key).and_then(Value::as_f64)
}

/// A string stored under `key`, when it is a non-empty string.
#[must_use]
pub fn extra_str<'a>(extra: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    extra.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Whether the value stored under `key` is set and not false, zero or empty.
#[must_use]
pub fn extra_truthy(extra: &Map<String, Value>, key: &str) -> bool {
    match extra.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0 && !v.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `v` unless it is zero or NaN, else `fallback` (how optional settings default).
#[must_use]
pub fn or_default(v: f64, fallback: f64) -> f64 {
    if v == 0.0 || v.is_nan() { fallback } else { v }
}

/// Values of a keyed collection in key order (keys compared by UTF-16 code units).
pub fn sorted_by_key<K: std::fmt::Display, V>(map: &IndexMap<K, V>) -> Vec<&V> {
    let mut entries: Vec<(String, &V)> = map.iter().map(|(k, v)| (k.to_string(), v)).collect();
    entries.sort_by(|a, b| utf16_cmp(&a.0, &b.0));
    entries.into_iter().map(|(_, v)| v).collect()
}

fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Every piece in id order: the order exports list pieces in.
#[must_use]
pub fn pieces_in_id_order(project: &Project) -> Vec<&Piece> {
    sorted_by_key(&project.pieces)
}

/// How many of the piece are cut.
#[must_use]
pub fn qty(piece: &Piece) -> f64 {
    extra_f64(&piece.extra, "qty").map_or(DEFAULT_QTY, |q| or_default(q, DEFAULT_QTY))
}

/// Allowance outside the sewing line of segment `segment`: its hem, else its seam allowance.
#[must_use]
pub fn edge_allowance(piece: &Piece, segment: &PointId) -> f64 {
    piece.allowances().finish(segment).allowance
}

/// Hem allowance of segment `segment`, or 0 if it is not hemmed.
#[must_use]
pub fn hem(piece: &Piece, segment: &PointId) -> f64 {
    piece
        .hem
        .as_ref()
        .and_then(|h| h.get(segment))
        .copied()
        .filter(|&h| h > 0.0)
        .unwrap_or(0.0)
}

/// The widest allowance of a piece: piece default, per-edge overrides and hems.
#[must_use]
pub fn sa_max(piece: &Piece) -> f64 {
    let mut m = or_default(piece.sa, 0.0);
    for map in [&piece.sa_seg, &piece.hem].into_iter().flatten() {
        for &v in map.values() {
            if v > m {
                m = v;
            }
        }
    }
    m
}

/// The outline of a piece in `size` (the project's active size when `None`).
#[must_use]
pub fn graded_outline(project: &Project, piece: &Piece, size: Option<&SizeName>) -> Outline {
    piece.graded_outline(size.unwrap_or(&project.active_size), &project.base_size)
}

/// The cut line of a piece around outline `ol`.
#[must_use]
pub fn cut_line(piece: &Piece, ol: &Outline) -> Vec<Vec2> {
    let allowances = piece.allowances();
    cut_outline(ol, |id| allowances.finish(id))
}

/// Points of an internal line as outline points (internal-line points need no ids).
#[must_use]
pub fn line_points(line: &InternalLine) -> Vec<OutlinePoint> {
    line.pts
        .iter()
        .map(|p| OutlinePoint {
            id: p.id.clone().unwrap_or_else(|| PointId::new("")),
            pos: p.pos,
            curve: p.curve,
        })
        .collect()
}

/// Whether an internal line is closed (its last point joins its first).
#[must_use]
pub fn is_closed(line: &InternalLine) -> bool {
    extra_truthy(&line.extra, "closed")
}

/// Points of an internal line, with the first point repeated at the end when it is closed.
#[must_use]
pub fn line_points_closed(line: &InternalLine) -> Vec<OutlinePoint> {
    let mut pts = line_points(line);
    if is_closed(line)
        && let Some(first) = pts.first().cloned()
    {
        pts.push(first);
    }
    pts
}

/// Fabric content text: the stored content, else the name of the preset it was made from.
#[must_use]
pub fn fabric_content(fabric: &Fabric) -> String {
    if let Some(c) = extra_str(&fabric.extra, "content") {
        return c.to_owned();
    }
    FabricPreset::from_key(&fabric.preset).map_or_else(String::new, |p| p.spec().name.to_owned())
}

/// Axis-aligned bounds with the derived sizes the layouts use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// Smallest x.
    pub x0: f64,
    /// Smallest y.
    pub y0: f64,
    /// Largest x.
    pub x1: f64,
    /// Largest y.
    pub y1: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
    /// Centre x.
    pub cx: f64,
    /// Centre y.
    pub cy: f64,
}

impl Bounds {
    /// Bounds of `points`. With no points the bounds are inverted infinities (zero area, every
    /// comparison against them fails).
    pub fn of(points: impl IntoIterator<Item = Vec2>) -> Self {
        let (min, max) = azam_geom::bbox(points).map_or(
            (Vec2::splat(f64::INFINITY), Vec2::splat(f64::NEG_INFINITY)),
            |b| (b.min, b.max),
        );
        Self {
            x0: min.x,
            y0: min.y,
            x1: max.x,
            y1: max.y,
            w: max.x - min.x,
            h: max.y - min.y,
            cx: min.x.midpoint(max.x),
            cy: min.y.midpoint(max.y),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_order_is_string_order() {
        let mut m: IndexMap<String, u32> = IndexMap::new();
        for (k, v) in [("pc_2", 2), ("pc_10", 10), ("pc_1", 1)] {
            m.insert(k.to_owned(), v);
        }
        let vals: Vec<u32> = sorted_by_key(&m).into_iter().copied().collect();
        assert_eq!(vals, vec![1, 10, 2]);
    }

    #[test]
    fn truthiness() {
        let mut m = Map::new();
        m.insert("a".into(), Value::from(0));
        m.insert("b".into(), Value::from("x"));
        assert!(!extra_truthy(&m, "a"));
        assert!(extra_truthy(&m, "b"));
        assert!(!extra_truthy(&m, "c"));
    }
}
