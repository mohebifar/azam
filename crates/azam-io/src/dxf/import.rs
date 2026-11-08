//! DXF import: pattern pieces from AAMA-style blocks or from loose closed polylines.

use std::fmt;

use azam_geom::{OutlinePoint, Vec2, distance_to_segment, signed_area};

use super::layer;
use crate::format::{parse_float_prefix, parse_int_prefix};

/// Polylines are simplified until no dropped vertex is further than this from the result, in mm.
const SIMPLIFY_TOLERANCE_MM: f64 = 0.4;
/// End points closer than this close a polyline, in mm.
const CLOSE_DISTANCE_MM: f64 = 0.5;
/// Text that names a piece starts with this (compared without regard to case).
const NAME_LABEL: &str = "piece name";
/// A rule number's text sits within this distance of its boundary vertex, in mm.
const RULE_TEXT_REACH_MM: f64 = 0.5;

/// An open or closed line inside an imported piece.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedLine {
    /// Points in mm, pattern frame (y down).
    pub points: Vec<Vec2>,
    /// Whether the line is closed.
    pub closed: bool,
}

/// A piece read from a DXF file.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedPiece {
    /// Name from the block's "Piece Name:" text, else the block name, else `Piece <n>`.
    pub name: String,
    /// Sewing line (or the largest outline when there is none): corner points with ids `p0`,
    /// `p1`…, in mm, pattern frame (y down).
    pub points: Vec<OutlinePoint>,
    /// Internal lines (layer 8).
    pub internals: Vec<ImportedLine>,
    /// Notch positions (layer 4): the end of each notch line nearest the outline, in mm.
    pub notches: Vec<Vec2>,
    /// Grain line angle (layer 7) in degrees from vertical, -90..90, if the block has one.
    pub grain: Option<f64>,
    /// ASTM grade rule number of each point (a `# n` text on it), or empty when the block
    /// carries no rule numbers. A graded block's points are its boundary's vertices as written.
    pub rules: Vec<Option<u32>>,
    /// Whether the outline is the cut line (an ASTM boundary), so its seam allowance is already
    /// in it.
    pub allowance_included: bool,
}

/// Why a DXF file could not be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DxfError {
    /// A coordinate value is not a number.
    InvalidNumber {
        /// 1-based line of the value.
        line: usize,
        /// The value as written.
        value: String,
    },
    /// The file holds no outline to make a piece from.
    NoPieces,
}

impl fmt::Display for DxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNumber { line, value } => write!(f, "line {line}: {value:?} is not a number"),
            Self::NoPieces => f.write_str("no pattern pieces found in the DXF file"),
        }
    }
}

impl std::error::Error for DxfError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Block,
    Polyline,
    LwPolyline,
    Line,
    Text,
}

/// One entity being read.
#[derive(Debug)]
struct Entity {
    kind: Kind,
    layer: String,
    pts: Vec<Vec2>,
    closed: bool,
    /// Whether a polyline has started its vertices (point codes then belong to the last vertex).
    in_vertex: bool,
    text: String,
}

impl Entity {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            layer: layer::DEFAULT.to_owned(),
            pts: if kind == Kind::Line {
                vec![Vec2::ZERO, Vec2::ZERO]
            } else {
                Vec::new()
            },
            closed: false,
            in_vertex: false,
            text: String::new(),
        }
    }

    /// Whether this is a polyline with enough points to be an outline or internal line.
    fn is_shape(&self) -> bool {
        matches!(self.kind, Kind::Polyline | Kind::LwPolyline) && self.pts.len() > 2
    }
}

#[derive(Default)]
struct Block {
    name: String,
    items: Vec<usize>,
}

/// Reader state. Entities live in an arena; blocks and the loose list refer to them by index, so
/// an entity filed when it starts still receives the codes that follow it.
#[derive(Default)]
struct Reader {
    arena: Vec<Entity>,
    blocks: Vec<(String, Vec<usize>)>,
    loose: Vec<usize>,
    block: Option<Block>,
    entity: Option<usize>,
}

impl Reader {
    fn file(&mut self, e: usize) {
        match &mut self.block {
            Some(b) => b.items.push(e),
            None => self.loose.push(e),
        }
    }

    fn current(&mut self) -> Option<&mut Entity> {
        self.entity.map(|i| &mut self.arena[i])
    }

    fn new_entity(&mut self, kind: Kind) -> usize {
        self.arena.push(Entity::new(kind));
        self.arena.len() - 1
    }

    fn end_block(&mut self) {
        if let Some(b) = self.block.take() {
            let key = if b.name.is_empty() {
                format!("B{}", self.blocks.len())
            } else {
                b.name
            };
            // A repeated name replaces the earlier block, keeping its place.
            match self.blocks.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = b.items,
                None => self.blocks.push((key, b.items)),
            }
        }
        self.entity = None;
    }

    /// Group code 0: a new entity or section marker.
    fn start(&mut self, value: &str) {
        if let Some(i) = self.entity.filter(|&i| self.arena[i].kind == Kind::LwPolyline) {
            self.file(i);
            self.entity = None;
        }
        match value {
            // Section markers leave the current entity as it is.
            "SECTION" => {}
            "BLOCK" => {
                self.block = Some(Block::default());
                self.entity = Some(self.new_entity(Kind::Block));
            }
            "ENDBLK" => self.end_block(),
            "POLYLINE" => self.entity = Some(self.new_entity(Kind::Polyline)),
            "VERTEX" => {
                if let Some(e) = self.current().filter(|e| e.kind == Kind::Polyline) {
                    e.in_vertex = true;
                    e.pts.push(Vec2::ZERO);
                }
            }
            "SEQEND" => {
                if let Some(i) = self
                    .entity
                    .take()
                    .filter(|&i| self.arena[i].kind == Kind::Polyline)
                {
                    self.file(i);
                }
            }
            "LWPOLYLINE" => self.entity = Some(self.new_entity(Kind::LwPolyline)),
            "LINE" | "TEXT" => {
                let i = self.new_entity(if value == "LINE" { Kind::Line } else { Kind::Text });
                self.file(i);
                self.entity = Some(i);
            }
            _ => {
                if self.current().is_some_and(|e| e.kind != Kind::Polyline) {
                    self.entity = None;
                }
            }
        }
    }

    /// Any other group code: a property of the current entity.
    fn property(&mut self, code: i64, value: &str, line: usize) -> Result<(), DxfError> {
        let Some(i) = self.entity else {
            return Ok(());
        };
        let kind = self.arena[i].kind;
        if kind == Kind::Block
            && code == 2
            && let Some(b) = &mut self.block
        {
            value.clone_into(&mut b.name);
        }
        let e = &mut self.arena[i];
        if code == 8 && !(kind == Kind::Polyline && e.in_vertex) {
            value.clone_into(&mut e.layer);
        }
        let num = || {
            parse_float_prefix(value).ok_or_else(|| DxfError::InvalidNumber {
                line,
                value: value.to_owned(),
            })
        };
        let flag_closed = || parse_int_prefix(value).is_some_and(|f| f & 1 == 1);
        match (kind, code) {
            (Kind::Polyline, 70) if !e.in_vertex => e.closed = flag_closed(),
            (Kind::Polyline, 10) if e.in_vertex => {
                if let Some(p) = e.pts.last_mut() {
                    p.x = num()?;
                }
            }
            (Kind::Polyline, 20) if e.in_vertex => {
                if let Some(p) = e.pts.last_mut() {
                    p.y = -num()?;
                }
            }
            (Kind::LwPolyline, 70) => e.closed = flag_closed(),
            (Kind::LwPolyline, 10) => e.pts.push(Vec2::new(num()?, 0.0)),
            (Kind::LwPolyline, 20) => {
                let y = -num()?;
                if let Some(p) = e.pts.last_mut() {
                    p.y = y;
                }
            }
            (Kind::Line, 10) => e.pts[0].x = num()?,
            (Kind::Line, 20) => e.pts[0].y = -num()?,
            (Kind::Line, 11) => e.pts[1].x = num()?,
            (Kind::Line, 21) => e.pts[1].y = -num()?,
            (Kind::Text, 1) => value.clone_into(&mut e.text),
            (Kind::Text, 10) => e.pts = vec![Vec2::new(num()?, e.pts.first().map_or(0.0, |p| p.y))],
            (Kind::Text, 20) => {
                let y = -num()?;
                match e.pts.first_mut() {
                    Some(p) => p.y = y,
                    None => e.pts.push(Vec2::new(0.0, y)),
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Reads pattern pieces from DXF text.
///
/// Each block with at least one outline becomes a piece: its sewing line is the polyline on layer
/// 14, else layer 1, else the polyline of largest area; polylines on layer 8 become internal lines
/// and a "Piece Name:" text names it. A file without such blocks makes one piece of every closed
/// loose polyline. Outlines are simplified to within 0.4 mm. Coordinates are flipped to the
/// pattern frame (y down).
///
/// # Errors
/// [`DxfError::InvalidNumber`] if a coordinate is not a number, [`DxfError::NoPieces`] if no
/// outline was found.
pub fn import_dxf(text: &str) -> Result<Vec<ImportedPiece>, DxfError> {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let pairs: Vec<(Option<i64>, &str, usize)> = (0..lines.len() / 2)
        .map(|k| {
            (
                parse_int_prefix(lines[2 * k].trim()),
                lines[2 * k + 1].trim(),
                2 * k + 2,
            )
        })
        .collect();
    let mut reader = Reader::default();
    for &(code, value, line) in &pairs {
        match code {
            Some(0) => reader.start(value),
            Some(c) => reader.property(c, value, line)?,
            None => {}
        }
    }
    let Reader {
        arena, blocks, loose, ..
    } = reader;

    let mut groups: Vec<(String, Vec<usize>)> = blocks
        .into_iter()
        .filter(|(_, items)| items.iter().any(|&i| arena[i].is_shape()))
        .collect();
    if groups.is_empty() {
        groups = loose
            .into_iter()
            .filter(|&i| {
                let e = &arena[i];
                e.is_shape() && (e.closed || ends_meet(&e.pts))
            })
            .enumerate()
            .map(|(n, i)| (format!("Piece {}", n + 1), vec![i]))
            .collect();
    }

    let pieces: Vec<ImportedPiece> = groups
        .iter()
        .filter_map(|(name, items)| make_piece(&arena, name, items))
        .collect();
    if pieces.is_empty() {
        return Err(DxfError::NoPieces);
    }
    Ok(pieces)
}

fn ends_meet(pts: &[Vec2]) -> bool {
    match (pts.first(), pts.last()) {
        (Some(a), Some(b)) => azam_geom::hypot(a.x - b.x, a.y - b.y) < CLOSE_DISTANCE_MM,
        _ => false,
    }
}

fn make_piece(arena: &[Entity], block_name: &str, items: &[usize]) -> Option<ImportedPiece> {
    let polys: Vec<&Entity> = items
        .iter()
        .map(|&i| &arena[i])
        .filter(|e| e.is_shape())
        .collect();
    // ASTM rule numbers: `# n` texts on the boundary (layer 1)
    let rule_marks: Vec<(Vec2, u32)> = items
        .iter()
        .map(|&i| &arena[i])
        .filter(|e| e.kind == Kind::Text && e.layer == layer::CUT && e.pts.len() == 1)
        .filter_map(|e| Some((e.pts[0], rule_number(&e.text)?)))
        .collect();
    let boundary = polys.iter().position(|e| e.layer == layer::CUT);
    let graded = !rule_marks.is_empty() && boundary.is_some();
    let net_index = if graded {
        boundary?
    } else {
        polys
            .iter()
            .position(|e| e.layer == layer::SEW)
            .or(boundary)
            .or_else(|| largest(&polys))?
    };
    let net = polys[net_index];

    let mut pts = net.pts.clone();
    if ends_meet(&pts) {
        pts.pop();
    }
    let rules = if graded {
        // every vertex as written: rules hang on them
        pts.iter()
            .map(|&p| {
                rule_marks
                    .iter()
                    .find(|(at, _)| (*at - p).length() <= RULE_TEXT_REACH_MM)
                    .map(|&(_, n)| n)
            })
            .collect()
    } else {
        pts.push(pts[0]);
        pts = simplify(&pts, SIMPLIFY_TOLERANCE_MM);
        pts.pop();
        Vec::new()
    };

    let name = items
        .iter()
        .map(|&i| &arena[i])
        .find(|e| e.kind == Kind::Text && contains_ignore_case(&e.text, NAME_LABEL))
        .map_or_else(|| block_name.to_owned(), |e| strip_name_label(&e.text));
    let internals = polys
        .iter()
        .enumerate()
        .filter(|&(i, e)| i != net_index && e.layer == layer::INTERNAL)
        .map(|(_, e)| ImportedLine {
            points: simplify(&e.pts, SIMPLIFY_TOLERANCE_MM),
            closed: e.closed,
        })
        .collect();
    let lines = || {
        items
            .iter()
            .map(|&i| &arena[i])
            .filter(|e| e.pts.len() >= 2 && !e.is_shape())
    };
    let notches = lines()
        .filter(|e| e.layer == layer::NOTCH)
        .map(|e| {
            let (a, b) = (e.pts[0], e.pts[e.pts.len() - 1]);
            let gap = |q: Vec2| distance_to_polygon(q, &pts);
            if gap(a) <= gap(b) { a } else { b }
        })
        .collect();
    let grain = lines().find(|e| e.layer == layer::GRAIN).and_then(|e| {
        let d = e.pts[e.pts.len() - 1] - e.pts[0];
        (d.length() > 0.0).then(|| {
            // export draws the line from top to bottom of the piece turned by the grain angle
            let deg = (-d.x).atan2(d.y).to_degrees();
            if deg > 90.0 {
                deg - 180.0
            } else if deg <= -90.0 {
                deg + 180.0
            } else {
                deg
            }
        })
    });
    Some(ImportedPiece {
        name,
        points: pts
            .iter()
            .enumerate()
            .map(|(i, p)| OutlinePoint::corner(format!("p{i}"), p.x, p.y))
            .collect(),
        internals,
        notches,
        grain,
        rules,
        allowance_included: graded,
    })
}

/// The rule number of an ASTM rule text (`# 12`, `#12`).
fn rule_number(text: &str) -> Option<u32> {
    text.trim().strip_prefix('#')?.trim().parse().ok()
}

/// Distance from `q` to the closed polygon `pts`.
fn distance_to_polygon(q: Vec2, pts: &[Vec2]) -> f64 {
    (0..pts.len())
        .map(|i| distance_to_segment(q, pts[i], pts[(i + 1) % pts.len()]).distance)
        .fold(f64::INFINITY, f64::min)
}

/// Index of the polyline with the largest area (the first of equals).
fn largest(polys: &[&Entity]) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, e) in polys.iter().enumerate() {
        let a = signed_area(&e.pts).abs();
        if best.is_none_or(|(_, b)| a > b) {
            best = Some((i, a));
        }
    }
    best.map(|(i, _)| i)
}

fn contains_ignore_case(text: &str, needle: &str) -> bool {
    text.to_lowercase().contains(needle)
}

/// The text with its first "piece name:" label (and the spaces after it) removed.
fn strip_name_label(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let label = format!("{NAME_LABEL}:");
    match lower.find(&label) {
        Some(at) => {
            let rest = &text[at + label.len()..];
            format!("{}{}", &text[..at], rest.trim_start())
        }
        None => text.to_owned(),
    }
}

/// Douglas–Peucker simplification keeping the end points.
fn simplify(pts: &[Vec2], tol: f64) -> Vec<Vec2> {
    if pts.len() < 4 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut max_d = 0.0;
        let mut max_i = 0;
        for i in a + 1..b {
            let d = distance_to_segment(pts[i], pts[a], pts[b]).distance;
            if d > max_d {
                max_d = d;
                max_i = i;
            }
        }
        if max_d > tol && max_i > 0 {
            keep[max_i] = true;
            stack.push((max_i, b));
            stack.push((a, max_i));
        }
    }
    pts.iter()
        .zip(&keep)
        .filter(|&(_, &k)| k)
        .map(|(&p, _)| p)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_label_is_stripped() {
        assert_eq!(strip_name_label("Piece Name: Front"), "Front");
        assert_eq!(strip_name_label("PIECE NAME:Yoke"), "Yoke");
    }

    #[test]
    fn simplify_drops_collinear_points() {
        let pts = [
            Vec2::new(0.0, 0.0),
            Vec2::new(5.0, 0.1),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
        ];
        assert_eq!(simplify(&pts, 0.4), vec![pts[0], pts[2], pts[3]]);
    }

    #[test]
    fn notches_and_grain_survive_a_round_trip() {
        let mut p = azam_doc::Project::new(None, azam_doc::Sex::Female, 0.0);
        let square =
            [(0.0, 0.0), (200.0, 0.0), (200.0, 300.0), (0.0, 300.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let id = azam_doc::create_piece(&mut p, &square).expect("piece");
        let top = p.pieces[&id].pts[0].id.clone();
        azam_doc::add_notch(&mut p, &id, &top, 0.25).expect("notch");
        p.pieces.get_mut(&id).expect("piece").grain = 30.0;
        let pieces = import_dxf(&crate::export_dxf(&p)).expect("imports");
        assert_eq!(pieces.len(), 1);
        let back = &pieces[0];
        assert_eq!(back.notches.len(), 1);
        assert!(
            back.notches[0].distance(Vec2::new(50.0, 0.0)) < 0.5,
            "notch at {:?}",
            back.notches[0]
        );
        let grain = back.grain.expect("grain line");
        assert!((grain - 30.0).abs() < 0.01, "grain {grain}°");
    }

    #[test]
    fn empty_file_has_no_pieces() {
        assert_eq!(import_dxf("0\nEOF\n"), Err(DxfError::NoPieces));
    }

    #[test]
    fn bad_coordinate_is_reported() {
        let text = "0\nLWPOLYLINE\n10\nabc\n";
        assert_eq!(
            import_dxf(text),
            Err(DxfError::InvalidNumber {
                line: 4,
                value: "abc".into()
            })
        );
    }
}
