//! SVG import: pattern pieces from the closed shapes of a drawing.
//!
//! Every closed shape (path, polygon, rect, circle, ellipse) becomes a piece; open shapes in the
//! same group become its internal lines. In a group that has a dashed closed shape (the sewing line,
//! as azam's own export draws it next to the cut line), that shape is the piece and the group's
//! other closed shapes are taken for its cut line. Path curves stay curves (cubic and quadratic
//! Béziers become control points); arcs are flattened into short segments. Group and shape
//! `transform`s (matrix, translate, scale, rotate, skew) are applied, and sizes come from the
//! drawing's `width`/`height` and `viewBox` (mm, cm, in, pt or px).

use std::fmt;

use azam_geom::{OutlinePoint, Vec2, signed_area};

use crate::dxf::{ImportedLine, ImportedPiece};

/// Millimetres per CSS pixel.
const MM_PER_PX: f64 = 25.4 / 96.0;
/// Segments a circle or ellipse is drawn with (cubic Béziers).
const ELLIPSE_KAPPA: f64 = 0.552_284_749_8;

/// Why an SVG file could not be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SvgError {
    /// The text holds no `<svg>` element.
    NotSvg,
    /// No closed shape was found.
    NoPieces,
}

impl fmt::Display for SvgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotSvg => "not an SVG drawing",
            Self::NoPieces => "the drawing has no closed shapes to make pieces from",
        })
    }
}

impl std::error::Error for SvgError {}

/// A shape read from the drawing.
struct Shape {
    /// Index of the top-level group it sits in, if any.
    group: Option<usize>,
    /// Points in user units: position and whether it is a curve control point.
    points: Vec<(Vec2, bool)>,
    closed: bool,
    dashed: bool,
    id: Option<String>,
}

/// A top-level group: its id and the text inside it.
#[derive(Default)]
struct Group {
    id: Option<String>,
    text: Option<String>,
}

/// The pieces drawn in `text` (an SVG document), in mm, pattern frame (y down).
///
/// # Errors
/// [`SvgError::NotSvg`] without an `<svg>` element, [`SvgError::NoPieces`] when nothing is closed.
pub fn import_svg(text: &str) -> Result<Vec<ImportedPiece>, SvgError> {
    let mut scale = None;
    let mut shapes = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut depth = 0usize;
    let mut top_group: Option<usize> = None;
    // transforms of the open groups, the innermost last (the root's viewBox origin first)
    let mut transforms: Vec<Affine> = vec![IDENTITY];
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else { break };
        let tag = &after[..end];
        let body_after = &after[end + 1..];
        rest = body_after;
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            if name.trim() == "g" {
                if transforms.len() > 1 {
                    transforms.pop();
                }
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    top_group = None;
                }
            }
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let (name, attrs) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
        let attr = |key: &str| attribute(attrs, key);
        match name {
            "svg" => {
                scale = Some(user_unit_mm(attr("width"), attr("height"), attr("viewBox")));
                if let Some(v) = attr("viewBox").map(numbers).filter(|v| v.len() == 4) {
                    transforms[0] = [1.0, 0.0, 0.0, 1.0, -v[0], -v[1]];
                }
            }
            "g" if !self_closing => {
                let outer = transforms.last().copied().unwrap_or(IDENTITY);
                transforms.push(mul(outer, attr("transform").map_or(IDENTITY, parse_transform)));
                if depth == 0 {
                    groups.push(Group {
                        id: attr("id").map(str::to_owned),
                        text: None,
                    });
                    top_group = Some(groups.len() - 1);
                }
                depth += 1;
            }
            // the text up to the next tag names the group
            "text" if !self_closing => {
                let content = body_after[..body_after.find('<').unwrap_or(0)].trim();
                if let Some(g) = top_group
                    && !content.is_empty()
                    && groups[g].text.is_none()
                {
                    groups[g].text = Some(content.to_owned());
                }
            }
            "path" | "polygon" | "polyline" | "rect" | "circle" | "ellipse" | "line" => {
                let num = |k: &str| attr(k).and_then(parse_len).unwrap_or(0.0);
                let (points, closed) = match name {
                    "path" => path_points(attr("d").unwrap_or("")),
                    "polygon" => (corners(&numbers(attr("points").unwrap_or(""))), true),
                    "polyline" => (corners(&numbers(attr("points").unwrap_or(""))), false),
                    "line" => (
                        vec![
                            (Vec2::new(num("x1"), num("y1")), false),
                            (Vec2::new(num("x2"), num("y2")), false),
                        ],
                        false,
                    ),
                    "rect" => {
                        let (x, y, w, h) = (num("x"), num("y"), num("width"), num("height"));
                        (corners(&[x, y, x + w, y, x + w, y + h, x, y + h]), true)
                    }
                    "circle" => {
                        let r = num("r");
                        (ellipse(num("cx"), num("cy"), r, r), true)
                    }
                    _ => (ellipse(num("cx"), num("cy"), num("rx"), num("ry")), true),
                };
                let m = mul(
                    transforms.last().copied().unwrap_or(IDENTITY),
                    attr("transform").map_or(IDENTITY, parse_transform),
                );
                let points: Vec<(Vec2, bool)> = points.into_iter().map(|(q, c)| (apply(m, q), c)).collect();
                if points.len() >= 2 {
                    shapes.push(Shape {
                        group: top_group,
                        points,
                        closed,
                        dashed: attr("stroke-dasharray").is_some_and(|d| d.trim() != "none"),
                        id: attr("id").map(str::to_owned),
                    });
                }
            }
            _ => {}
        }
    }
    let scale = scale.ok_or(SvgError::NotSvg)?;
    let pieces = assemble(&shapes, &groups, scale);
    if pieces.is_empty() {
        return Err(SvgError::NoPieces);
    }
    Ok(pieces)
}

/// Pieces from the shapes: per group, its dashed outline (or else each closed shape), with the
/// group's open shapes as internal lines; loose closed shapes on their own.
fn assemble(shapes: &[Shape], groups: &[Group], scale: f64) -> Vec<ImportedPiece> {
    let mut pieces = Vec::new();
    let mut emit = |outline: &Shape, internals: Vec<&Shape>, name: Option<String>| {
        let mut pts: Vec<(Vec2, bool)> = outline.points.iter().map(|&(p, c)| (p * scale, c)).collect();
        // a closing point that repeats the first is dropped; a curve point may not open the list
        if pts.len() > 2 && pts[0].0.distance(pts[pts.len() - 1].0) < 1e-6 && !pts[pts.len() - 1].1 {
            pts.pop();
        }
        if let Some(k) = pts.iter().position(|&(_, c)| !c) {
            pts.rotate_left(k);
        }
        let corners = pts.iter().filter(|(_, c)| !c).count();
        if corners < 2 || signed_area(&pts.iter().map(|(p, _)| *p).collect::<Vec<_>>()).abs() < 1.0 {
            return;
        }
        let n = pieces.len() + 1;
        pieces.push(ImportedPiece {
            name: name.unwrap_or_else(|| format!("Piece {n}")),
            points: pts
                .iter()
                .enumerate()
                .map(|(i, &(p, c))| {
                    if c {
                        OutlinePoint::curve(format!("p{i}"), p.x, p.y)
                    } else {
                        OutlinePoint::corner(format!("p{i}"), p.x, p.y)
                    }
                })
                .collect(),
            internals: internals
                .into_iter()
                .map(|s| ImportedLine {
                    points: s.points.iter().map(|(p, _)| *p * scale).collect(),
                    closed: false,
                })
                .collect(),
            notches: Vec::new(),
            grain: None,
        });
    };
    let area = |s: &Shape| signed_area(&s.points.iter().map(|(p, _)| *p).collect::<Vec<_>>()).abs();
    for (gi, group) in groups.iter().enumerate() {
        let members: Vec<&Shape> = shapes.iter().filter(|s| s.group == Some(gi)).collect();
        let open: Vec<&Shape> = members.iter().copied().filter(|s| !s.closed).collect();
        let name = group.text.clone().or_else(|| group.id.clone());
        let dashed = members
            .iter()
            .copied()
            .filter(|s| s.closed && s.dashed)
            .max_by(|a, b| area(a).total_cmp(&area(b)));
        match dashed {
            Some(outline) => emit(outline, open, name),
            None => {
                let closed: Vec<&Shape> = members.iter().copied().filter(|s| s.closed).collect();
                let single = closed.len() == 1;
                for s in closed {
                    let name = if single {
                        name.clone().or_else(|| s.id.clone())
                    } else {
                        s.id.clone().or_else(|| name.clone())
                    };
                    emit(s, if single { open.clone() } else { Vec::new() }, name);
                }
            }
        }
    }
    for s in shapes.iter().filter(|s| s.group.is_none() && s.closed) {
        emit(s, Vec::new(), s.id.clone());
    }
    pieces
}

/// A 2D affine map `[a, b, c, d, e, f]`: `x' = a x + c y + e`, `y' = b x + d y + f` (SVG's matrix).
type Affine = [f64; 6];
const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `outer` after `inner` (apply `inner` first).
fn mul(outer: Affine, inner: Affine) -> Affine {
    let [a, b, c, d, e, f] = outer;
    let [a2, b2, c2, d2, e2, f2] = inner;
    [
        a * a2 + c * b2,
        b * a2 + d * b2,
        a * c2 + c * d2,
        b * c2 + d * d2,
        a * e2 + c * f2 + e,
        b * e2 + d * f2 + f,
    ]
}

fn apply(m: Affine, q: Vec2) -> Vec2 {
    Vec2::new(m[0] * q.x + m[2] * q.y + m[4], m[1] * q.x + m[3] * q.y + m[5])
}

/// A `transform` attribute's list (`translate(…) rotate(…) …`, applied right to left) as one map.
fn parse_transform(s: &str) -> Affine {
    let mut m = IDENTITY;
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let v = numbers(&rest[open + 1..open + close]);
        let get = |i: usize, or: f64| v.get(i).copied().unwrap_or(or);
        let t = match name {
            "matrix" if v.len() == 6 => [v[0], v[1], v[2], v[3], v[4], v[5]],
            "translate" => [1.0, 0.0, 0.0, 1.0, get(0, 0.0), get(1, 0.0)],
            "scale" => {
                let sx = get(0, 1.0);
                [sx, 0.0, 0.0, get(1, sx), 0.0, 0.0]
            }
            "rotate" => {
                let (sn, cs) = get(0, 0.0).to_radians().sin_cos();
                let (cx, cy) = (get(1, 0.0), get(2, 0.0));
                let r = [cs, sn, -sn, cs, 0.0, 0.0];
                mul(
                    mul([1.0, 0.0, 0.0, 1.0, cx, cy], r),
                    [1.0, 0.0, 0.0, 1.0, -cx, -cy],
                )
            }
            "skewX" => [1.0, 0.0, get(0, 0.0).to_radians().tan(), 1.0, 0.0, 0.0],
            "skewY" => [1.0, get(0, 0.0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0],
            _ => IDENTITY,
        };
        m = mul(m, t);
        rest = &rest[open + close + 1..];
    }
    m
}

/// Points along the elliptical arc from `from` to `to` (radii `r`, x-axis turned `phi` degrees,
/// large-arc and sweep flags as SVG gives them), excluding `from`: about one point per 22.5°.
fn arc_points(from: Vec2, r: (f64, f64), phi: f64, large: bool, sweep: bool, to: Vec2) -> Vec<Vec2> {
    let (mut rx, mut ry) = (r.0.abs(), r.1.abs());
    if rx < 1e-9 || ry < 1e-9 || from.distance(to) < 1e-9 {
        return vec![to];
    }
    let (sp, cp) = phi.to_radians().sin_cos();
    // endpoint to centre parameterisation (SVG implementation notes, F.6.5)
    let d = (from - to) * 0.5;
    let x1 = cp * d.x + sp * d.y;
    let y1 = -sp * d.x + cp * d.y;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let k = if den > 0.0 { (num / den).sqrt() } else { 0.0 } * if large == sweep { -1.0 } else { 1.0 };
    let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let mid = (from + to) * 0.5;
    let centre = Vec2::new(cp * cx1 - sp * cy1 + mid.x, sp * cx1 + cp * cy1 + mid.y);
    let angle = |ux: f64, uy: f64| uy.atan2(ux);
    let t1 = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dt = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry) - t1;
    let tau = std::f64::consts::TAU;
    if sweep && dt < 0.0 {
        dt += tau;
    } else if !sweep && dt > 0.0 {
        dt -= tau;
    }
    let steps = ((dt.abs() / (tau / 16.0)).ceil() as usize).max(1);
    (1..=steps)
        .map(|i| {
            if i == steps {
                return to;
            }
            let t = t1 + dt * i as f64 / steps as f64;
            let (st, ct) = t.sin_cos();
            Vec2::new(
                cp * rx * ct - sp * ry * st + centre.x,
                sp * rx * ct + cp * ry * st + centre.y,
            )
        })
        .collect()
}

/// The value of attribute `key` in `attrs` (a tag's text after its name).
fn attribute<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = attrs;
    while let Some(at) = rest.find(key) {
        let before = rest[..at].chars().next_back();
        let after = &rest[at + key.len()..];
        let boundary = before.is_none_or(char::is_whitespace);
        if boundary && let Some(v) = after.trim_start().strip_prefix('=') {
            let v = v.trim_start();
            let quote = v.chars().next()?;
            if quote == '"' || quote == '\'' {
                let inner = &v[1..];
                return inner.find(quote).map(|e| &inner[..e]);
            }
        }
        rest = after;
    }
    None
}

/// A length in user units: a number, ignoring a trailing unit.
fn parse_len(s: &str) -> Option<f64> {
    let s = s.trim();
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .unwrap_or(s.len());
    s[..end].parse().ok()
}

/// Millimetres per user unit, from the root's `width`, `height` and `viewBox`.
fn user_unit_mm(width: Option<&str>, height: Option<&str>, view_box: Option<&str>) -> f64 {
    let mm = |s: &str| -> Option<f64> {
        let v = parse_len(s)?;
        let unit = s
            .trim()
            .trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'));
        Some(
            v * match unit.trim() {
                "mm" => 1.0,
                "cm" => 10.0,
                "in" => 25.4,
                "pt" => 25.4 / 72.0,
                "pc" => 25.4 / 6.0,
                _ => MM_PER_PX,
            },
        )
    };
    let vb = view_box
        .map(numbers)
        .filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0);
    match (vb, width.and_then(mm), height.and_then(mm)) {
        (Some(v), Some(w), _) => w / v[2],
        (Some(v), None, Some(h)) => h / v[3],
        (Some(_), None, None) => MM_PER_PX,
        (None, ..) => {
            // without a viewBox a user unit is a CSS pixel
            MM_PER_PX
        }
    }
}

/// The numbers in `s` (separated by spaces, commas or signs).
fn numbers(s: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<f64>| {
        if let Ok(v) = cur.parse() {
            out.push(v);
        }
        cur.clear();
    };
    let mut prev = ' ';
    for c in s.chars() {
        let starts_new = (c == '-' || c == '+') && !matches!(prev, 'e' | 'E') && !cur.is_empty();
        let dot_again = c == '.' && cur.contains('.') && !cur.contains(['e', 'E']);
        if c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E') {
            if starts_new || dot_again {
                flush(&mut cur, &mut out);
            }
            cur.push(c);
        } else {
            flush(&mut cur, &mut out);
        }
        prev = c;
    }
    flush(&mut cur, &mut out);
    out
}

/// Corner points from flat `x, y` pairs.
fn corners(v: &[f64]) -> Vec<(Vec2, bool)> {
    v.chunks_exact(2)
        .map(|c| (Vec2::new(c[0], c[1]), false))
        .collect()
}

/// An ellipse as four cubic Béziers: corner, two control points, corner…
fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Vec<(Vec2, bool)> {
    let (kx, ky) = (rx * ELLIPSE_KAPPA, ry * ELLIPSE_KAPPA);
    let c = |x: f64, y: f64, curve: bool| (Vec2::new(cx + x, cy + y), curve);
    vec![
        c(rx, 0.0, false),
        c(rx, ky, true),
        c(kx, ry, true),
        c(0.0, ry, false),
        c(-kx, ry, true),
        c(-rx, ky, true),
        c(-rx, 0.0, false),
        c(-rx, -ky, true),
        c(-kx, -ry, true),
        c(0.0, -ry, false),
        c(kx, -ry, true),
        c(rx, -ky, true),
    ]
}

/// The points of path data `d` (the first subpath) and whether it closes.
fn path_points(d: &str) -> (Vec<(Vec2, bool)>, bool) {
    let mut pts: Vec<(Vec2, bool)> = Vec::new();
    let mut cur = Vec2::ZERO;
    let mut last_ctrl: Option<Vec2> = None;
    let mut closed = false;
    let mut chars = d.char_indices().peekable();
    let mut cmd = 'M';
    let mut moves = 0;
    while let Some(&(i, c)) = chars.peek() {
        if c.is_ascii_alphabetic() {
            cmd = c;
            chars.next();
            if matches!(cmd, 'Z' | 'z') {
                closed = true;
                break;
            }
            continue;
        }
        // the arguments of this command: up to the next letter
        let end = d[i..]
            .find(|ch: char| ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E')
            .map_or(d.len(), |e| i + e);
        let args = numbers(&d[i..end]);
        while chars.peek().is_some_and(|&(j, _)| j < end) {
            chars.next();
        }
        let rel = cmd.is_ascii_lowercase();
        let at = |x: f64, y: f64, cur: Vec2| {
            if rel {
                cur + Vec2::new(x, y)
            } else {
                Vec2::new(x, y)
            }
        };
        let arity = match cmd.to_ascii_uppercase() {
            'M' | 'L' | 'T' => 2,
            'H' | 'V' => 1,
            'C' => 6,
            'S' | 'Q' => 4,
            'A' => 7,
            _ => 2,
        };
        for (k, a) in args.chunks_exact(arity).enumerate() {
            let upper = cmd.to_ascii_uppercase();
            match upper {
                'M' if k == 0 => {
                    moves += 1;
                    if moves > 1 {
                        return (pts, closed);
                    }
                    cur = at(a[0], a[1], cur);
                    pts.push((cur, false));
                    last_ctrl = None;
                }
                'M' | 'L' => {
                    cur = at(a[0], a[1], cur);
                    pts.push((cur, false));
                    last_ctrl = None;
                }
                'H' => {
                    cur = Vec2::new(if rel { cur.x + a[0] } else { a[0] }, cur.y);
                    pts.push((cur, false));
                    last_ctrl = None;
                }
                'V' => {
                    cur = Vec2::new(cur.x, if rel { cur.y + a[0] } else { a[0] });
                    pts.push((cur, false));
                    last_ctrl = None;
                }
                'C' => {
                    let (c1, c2, p) = (at(a[0], a[1], cur), at(a[2], a[3], cur), at(a[4], a[5], cur));
                    pts.extend([(c1, true), (c2, true), (p, false)]);
                    last_ctrl = Some(c2);
                    cur = p;
                }
                'S' => {
                    let c1 = last_ctrl.map_or(cur, |c| cur * 2.0 - c);
                    let (c2, p) = (at(a[0], a[1], cur), at(a[2], a[3], cur));
                    pts.extend([(c1, true), (c2, true), (p, false)]);
                    last_ctrl = Some(c2);
                    cur = p;
                }
                'Q' => {
                    let (c1, p) = (at(a[0], a[1], cur), at(a[2], a[3], cur));
                    pts.extend([(c1, true), (p, false)]);
                    last_ctrl = Some(c1);
                    cur = p;
                }
                'T' => {
                    let c1 = last_ctrl.map_or(cur, |c| cur * 2.0 - c);
                    let p = at(a[0], a[1], cur);
                    pts.extend([(c1, true), (p, false)]);
                    last_ctrl = Some(c1);
                    cur = p;
                }
                _ => {
                    let end = at(a[5], a[6], cur);
                    pts.extend(
                        arc_points(cur, (a[0], a[1]), a[2], a[3] != 0.0, a[4] != 0.0, end)
                            .into_iter()
                            .map(|q| (q, false)),
                    );
                    cur = end;
                    last_ctrl = None;
                }
            }
        }
        // after the first pair, further M pairs are line-tos
        if cmd == 'M' {
            cmd = 'L';
        } else if cmd == 'm' {
            cmd = 'l';
        }
    }
    (pts, closed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn azam_svg_comes_back_as_its_pieces() {
        let mut p = azam_doc::Project::new(None, azam_doc::Sex::Female, 0.0);
        let square =
            [(0.0, 0.0), (200.0, 0.0), (200.0, 300.0), (0.0, 300.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let id = azam_doc::create_piece(&mut p, &square).expect("piece");
        p.pieces.get_mut(&id).expect("piece").name = "Front".into();
        let pieces = import_svg(&crate::export_svg(&p)).expect("imports");
        assert_eq!(pieces.len(), 1, "the cut line is not a second piece");
        assert_eq!(pieces[0].name, "Front");
        let pts: Vec<Vec2> = pieces[0].points.iter().map(|q| q.pos).collect();
        let area = signed_area(&pts).abs();
        assert!(
            (area - 60_000.0).abs() < 50.0,
            "the sewing line, 200 × 300 mm: {area}"
        );
    }

    #[test]
    fn paths_keep_their_curves_and_units() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20cm" height="10cm" viewBox="0 0 200 100">
            <path id="Yoke" d="M10,10 L110,10 C130,10 130,60 110,60 L10,60 z"/>
            <rect x="150" y="20" width="40" height="20"/>
            <polyline points="0,0 5,5"/>
        </svg>"#;
        let pieces = import_svg(svg).expect("imports");
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].name, "Yoke");
        assert_eq!(
            pieces[0].points.iter().filter(|p| p.curve).count(),
            2,
            "the cubic keeps its controls"
        );
        assert!(
            (pieces[0].points[1].pos.x - 110.0).abs() < 1e-9,
            "1 user unit = 1 mm here"
        );
        assert_eq!(pieces[1].points.len(), 4, "a rect is four corners");
    }

    #[test]
    fn transforms_and_arcs_are_followed() {
        let svg = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100">
            <g transform="translate(50,0)"><rect id="Moved" x="0" y="0" width="10" height="10" transform="scale(2)"/></g>
            <path id="Round" d="M0,50 A10,10 0 0 1 20,50 L20,70 L0,70 z"/>
        </svg>"#;
        let pieces = import_svg(svg).expect("imports");
        let moved = pieces.iter().find(|p| p.name == "Moved").expect("rect");
        let xs: Vec<f64> = moved.points.iter().map(|q| q.pos.x).collect();
        assert!(
            xs.iter().any(|&x| (x - 50.0).abs() < 1e-9) && xs.iter().any(|&x| (x - 70.0).abs() < 1e-9),
            "translate after scale: {xs:?}"
        );
        let round = pieces.iter().find(|p| p.name == "Round").expect("path");
        let top = round.points.iter().map(|q| q.pos.y).fold(f64::INFINITY, f64::min);
        assert!(
            (top - 40.0).abs() < 0.5,
            "the half circle bulges up to y = 40: {top}"
        );
        assert!(round.points.len() > 6, "the arc is several segments");
    }

    #[test]
    fn a_drawing_without_closed_shapes_has_no_pieces() {
        assert_eq!(
            import_svg("<svg><polyline points=\"0,0 1,1\"/></svg>"),
            Err(SvgError::NoPieces)
        );
        assert_eq!(import_svg("hello"), Err(SvgError::NotSvg));
    }
}
