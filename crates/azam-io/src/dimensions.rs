//! Pattern dimensions: net (sewing line) and cut (with allowance) sizes, and the CAD dimension
//! model of a piece — lettered corners with ordinates and angles, edges with true lengths and
//! chords, mirror symmetry, notch positions and the seams on each edge.
//!
//! All values are in mm (angles in degrees). Ordinates are measured from the datum at the top-left
//! of the net bounding box.

use std::fmt::Write as _;

use azam_doc::{EdgeCoverage, Piece, PointId, Project, SeamId, SizeName, edge_coverage};
use azam_geom::{Outline, Vec2, contains, distance_to_segment, hypot, signed_area};
use indexmap::IndexMap;

use crate::format::{letters, number};
pub use crate::model::sa_max;
use crate::model::{Bounds, DEG, cut_line, edge_allowance, graded_outline, hem};

/// Interior angles closer than this to 180° are smooth (tangent) corners, in degrees.
const SMOOTH_CORNER_DEG: f64 = 1.0;
/// Smallest tolerance of the mirror-symmetry test, in mm.
const SYMMETRY_TOLERANCE_MM: f64 = 0.6;
/// Mirror-symmetry tolerance as a fraction of the piece width.
const SYMMETRY_TOLERANCE_OF_WIDTH: f64 = 0.002;
/// Overlaps shorter than this do not put a seam on an edge, in mm.
const MIN_SEAM_OVERLAP_MM: f64 = 0.5;
/// Directions shorter than this are degenerate.
const TANGENT_EPSILON: f64 = 1e-6;
/// Label search grid: candidates on an `N × N` lattice inside the bounding box.
const LABEL_GRID: usize = 28;
/// The outline is sampled with at most about this many vertices for the label search.
const LABEL_SAMPLES: usize = 400;
/// Clearance counts up to this fraction of the smaller box side.
const LABEL_CLEARANCE_CAP: f64 = 0.3;
/// Penalty per mm of distance from the centre (vertical distance counts half).
const LABEL_CENTRE_PULL: f64 = 0.12;
/// Candidates kept for the label search, best first.
const LABEL_CANDIDATES: usize = 120;

/// Allowance widths of a piece in words: `SA 10 mm`, `SA 10–15 mm · hem 30 mm`.
#[must_use]
pub fn allowance_summary(piece: &Piece) -> String {
    let ol = piece.outline();
    let mut sa: Vec<f64> = Vec::new();
    let mut hems: Vec<f64> = Vec::new();
    let add = |set: &mut Vec<f64>, v: f64| {
        // Values are told apart as printed, like keys of a table.
        if !set.iter().any(|&x| number(x) == number(v)) {
            set.push(v);
        }
    };
    for sg in &ol.segments {
        let h = hem(piece, &sg.id);
        if h > 0.0 {
            add(&mut hems, h);
        } else {
            add(&mut sa, edge_allowance(piece, &sg.id));
        }
    }
    sa.sort_by(f64::total_cmp);
    hems.sort_by(f64::total_cmp);
    let mut out = String::new();
    if let (Some(first), Some(last)) = (sa.first(), sa.last()) {
        out = if sa.len() > 1 {
            format!("SA {}–{} mm", number(*first), number(*last))
        } else {
            format!("SA {} mm", number(*first))
        };
    }
    if !hems.is_empty() {
        if !sa.is_empty() {
            out.push_str(" · ");
        }
        let list: Vec<String> = hems.iter().map(|&h| number(h)).collect();
        let _ = write!(out, "hem {} mm", list.join("/"));
    }
    out
}

/// Geometry of a piece in one size.
#[derive(Clone, Debug)]
pub struct PieceGeom {
    /// Sewing line.
    pub outline: Outline,
    /// Cut line (the sewing line when the piece has no allowance).
    pub cut: Vec<Vec2>,
    /// Bounds of the sewing line.
    pub bb: Bounds,
    /// Bounds of the cut line.
    pub cb: Bounds,
    /// Whether any edge has an allowance.
    pub has_sa: bool,
    /// Area inside the sewing line, mm².
    pub area: f64,
    /// Area inside the cut line, mm².
    pub cut_area: f64,
    /// Length of the sewing line, mm.
    pub perim: f64,
}

/// Geometry of `piece` in `size` (the active size when `None`).
#[must_use]
pub fn piece_geom(project: &Project, piece: &Piece, size: Option<&SizeName>) -> PieceGeom {
    let outline = graded_outline(project, piece, size);
    let has_sa = sa_max(piece) > 0.0;
    let cut = if has_sa {
        cut_line(piece, &outline)
    } else {
        outline.poly.clone()
    };
    PieceGeom {
        bb: Bounds::of(outline.poly.iter().copied()),
        cb: Bounds::of(cut.iter().copied()),
        has_sa,
        area: signed_area(&outline.poly).abs(),
        cut_area: signed_area(&cut).abs(),
        perim: outline.total,
        cut,
        outline,
    }
}

/// Overall sizes of a piece.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PieceDims {
    /// Net width, mm.
    pub w: f64,
    /// Net height, mm.
    pub h: f64,
    /// Cut width, mm.
    pub cw: f64,
    /// Cut height, mm.
    pub ch: f64,
    /// Net area, mm².
    pub area: f64,
    /// Cut area, mm².
    pub cut_area: f64,
    /// Net perimeter, mm.
    pub perim: f64,
}

/// Overall sizes of `piece` in `size` (the active size when `None`).
#[must_use]
pub fn piece_dims(project: &Project, piece: &Piece, size: Option<&SizeName>) -> PieceDims {
    let g = piece_geom(project, piece, size);
    PieceDims {
        w: g.bb.w,
        h: g.bb.h,
        cw: g.cb.w,
        ch: g.cb.h,
        area: g.area,
        cut_area: g.cut_area,
        perim: g.perim,
    }
}

/// A lettered corner of the dimension model.
#[derive(Clone, Debug, PartialEq)]
pub struct Corner {
    /// Index of the segment that starts here.
    pub k: usize,
    /// Id of that segment.
    pub id: PointId,
    /// Letter: A, B, C… in outline order.
    pub letter: String,
    /// Position.
    pub pos: Vec2,
    /// Ordinate from the datum (top-left of the net bounds).
    pub ordinate: Vec2,
    /// Interior angle in degrees.
    pub angle: f64,
    /// Unit direction arriving at the corner.
    pub dir_in: Vec2,
    /// Unit direction leaving the corner.
    pub dir_out: Vec2,
    /// Whether the outline runs smoothly through (angle within 1° of 180°).
    pub smooth: bool,
}

/// An edge of the dimension model.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    /// Index of the segment.
    pub k: usize,
    /// Id of the segment.
    pub id: PointId,
    /// Letter of its start corner.
    pub from: String,
    /// Letter of its end corner.
    pub to: String,
    /// Whether it is curved.
    pub curve: bool,
    /// True length (along the curve).
    pub len: f64,
    /// Straight distance between its corners.
    pub chord: f64,
    /// Allowance outside it.
    pub allow: f64,
    /// Whether it is hemmed.
    pub hem: bool,
}

/// The CAD dimension model of a piece.
#[derive(Clone, Debug, PartialEq)]
pub struct DimModel {
    /// Corners in outline order.
    pub corners: Vec<Corner>,
    /// Edges in outline order (edge `k` leaves corner `k`).
    pub edges: Vec<Edge>,
    /// Net bounds.
    pub bb: Bounds,
    /// +1 when the outline runs clockwise on screen, −1 otherwise.
    pub winding: f64,
    /// Whether the piece is mirror-symmetric about the vertical centre line of its bounds.
    pub symmetric: bool,
    /// x of that centre line.
    pub cx: f64,
}

fn unit(x: f64, y: f64) -> Vec2 {
    let l = hypot(x, y);
    let l = if l == 0.0 { 1.0 } else { l };
    Vec2::new(x / l, y / l)
}

/// Direction leaving a segment's start (first control point that is not on the start).
fn tangent_out(ctrl: &[Vec2]) -> Vec2 {
    for c in &ctrl[1..] {
        let (dx, dy) = (c.x - ctrl[0].x, c.y - ctrl[0].y);
        if hypot(dx, dy) > TANGENT_EPSILON {
            return unit(dx, dy);
        }
    }
    Vec2::X
}

/// Direction arriving at a segment's end.
fn tangent_in(ctrl: &[Vec2]) -> Vec2 {
    let m = ctrl.len() - 1;
    for c in ctrl[..m].iter().rev() {
        let (dx, dy) = (ctrl[m].x - c.x, ctrl[m].y - c.y);
        if hypot(dx, dy) > TANGENT_EPSILON {
            return unit(dx, dy);
        }
    }
    Vec2::X
}

/// The dimension model of `piece` with sewing line `ol`.
#[must_use]
pub fn dim_model(piece: &Piece, ol: &Outline) -> DimModel {
    let segs = &ol.segments;
    let n = segs.len();
    let winding = if signed_area(&ol.poly) > 0.0 { 1.0 } else { -1.0 };
    let bb = Bounds::of(ol.poly.iter().copied());
    let corners: Vec<Corner> = segs
        .iter()
        .enumerate()
        .map(|(k, sg)| {
            let prev = &segs[(k + n - 1) % n];
            let din = tangent_in(&prev.ctrl);
            let dout = tangent_out(&sg.ctrl);
            let turn = (din.x * dout.y - din.y * dout.x).atan2(din.x * dout.x + din.y * dout.y);
            let angle = 180.0 - turn * winding / DEG;
            let p = sg.ctrl[0];
            Corner {
                k,
                id: sg.id.clone(),
                letter: letters(k),
                pos: p,
                ordinate: Vec2::new(p.x - bb.x0, p.y - bb.y0),
                angle,
                dir_in: din,
                dir_out: dout,
                smooth: (angle - 180.0).abs() < SMOOTH_CORNER_DEG,
            }
        })
        .collect();
    let edges = segs
        .iter()
        .enumerate()
        .map(|(k, sg)| {
            let (a, b) = (sg.ctrl[0], sg.ctrl[sg.ctrl.len() - 1]);
            Edge {
                k,
                id: sg.id.clone(),
                from: corners[k].letter.clone(),
                to: corners[(k + 1) % n].letter.clone(),
                curve: sg.ctrl.len() > 2,
                len: sg.len,
                chord: hypot(b.x - a.x, b.y - a.y),
                allow: edge_allowance(piece, &sg.id),
                hem: hem(piece, &sg.id) > 0.0,
            }
        })
        .collect();
    let cx = bb.cx;
    let tol = SYMMETRY_TOLERANCE_MM.max(bb.w * SYMMETRY_TOLERANCE_OF_WIDTH);
    let symmetric = n >= 4
        && corners.iter().all(|c| {
            corners
                .iter()
                .any(|d| (d.pos.x - (2.0 * cx - c.pos.x)).abs() < tol && (d.pos.y - c.pos.y).abs() < tol)
        });
    DimModel {
        corners,
        edges,
        bb,
        winding,
        symmetric,
        cx,
    }
}

/// How many polygon edges a ray from `p` along `(dx, dy)` crosses, ignoring its first `skip` mm.
#[must_use]
pub fn ray_crossings(poly: &[Vec2], p: Vec2, dx: f64, dy: f64, skip: f64) -> usize {
    let (ox, oy) = (p.x + dx * skip, p.y + dy * skip);
    let m = poly.len();
    (0..m)
        .filter(|&i| {
            let (a, b) = (poly[i], poly[(i + 1) % m]);
            let (ex, ey) = (b.x - a.x, b.y - a.y);
            let den = dx * ey - dy * ex;
            if den.abs() < 1e-12 {
                return false;
            }
            let t = ((a.x - ox) * ey - (a.y - oy) * ex) / den;
            let u = ((a.x - ox) * dy - (a.y - oy) * dx) / den;
            t > 0.0 && (0.0..1.0).contains(&u)
        })
        .count()
}

/// A notch located along its edge.
#[derive(Clone, Debug, PartialEq)]
pub struct NotchDim {
    /// Index of the edge.
    pub k: usize,
    /// Letter of the edge's start corner.
    pub from: String,
    /// Letter of the edge's end corner.
    pub to: String,
    /// Distance from the start corner along the edge.
    pub a: f64,
    /// Distance from the end corner along the edge.
    pub b: f64,
}

/// The dimension table of a piece: one row per corner (with the edge leaving it and the numbers
/// of the seams on that edge), and its notches.
#[derive(Clone, Debug, PartialEq)]
pub struct DimTable {
    /// The dimension model.
    pub model: DimModel,
    /// Seam numbers on each edge, ascending.
    pub seams: Vec<Vec<usize>>,
    /// Notches.
    pub notches: Vec<NotchDim>,
}

/// The dimension table of `piece` with sewing line `ol`, using seam numbers `numbers`.
#[must_use]
pub fn dim_table(
    project: &Project,
    piece: &Piece,
    ol: &Outline,
    numbers: &IndexMap<SeamId, usize>,
) -> DimTable {
    let model = dim_model(piece, ol);
    let cov: EdgeCoverage = edge_coverage(project, piece, ol, Some(numbers));
    let total = ol.total;
    let seams = model
        .edges
        .iter()
        .map(|e| {
            let (p0, p1) = (ol.starts[e.k], ol.starts[e.k] + e.len);
            let mut list: Vec<usize> = cov
                .sides
                .iter()
                .filter(|sd| {
                    let (a, b) = (sd.start.max(p0), sd.end.min(p1));
                    let (a2, b2) = ((sd.start - total).max(p0), (sd.end - total).min(p1));
                    b - a > MIN_SEAM_OVERLAP_MM || b2 - a2 > MIN_SEAM_OVERLAP_MM
                })
                .map(|sd| sd.number.unwrap_or(0))
                .collect();
            list.sort_unstable();
            list.dedup();
            list
        })
        .collect();
    let n = model.corners.len();
    let notches = piece
        .notches
        .iter()
        .filter_map(|nt| {
            let k = ol.segment_index(&nt.s)?;
            let len = ol.segments[k].len;
            let t = nt.t.unwrap_or(0.0);
            Some(NotchDim {
                k,
                from: model.corners[k].letter.clone(),
                to: model.corners[(k + 1) % n].letter.clone(),
                a: len * t,
                b: len * (1.0 - t),
            })
        })
        .collect();
    DimTable {
        model,
        seams,
        notches,
    }
}

/// A roomy point inside a piece for its label.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelPole {
    /// The best point.
    pub pos: Vec2,
    /// Its score (clearance, capped, minus the pull to the centre).
    pub score: f64,
    /// Its distance from the outline.
    pub clearance: f64,
    /// The best candidates with their scores, best first.
    pub candidates: Vec<(Vec2, f64)>,
}

/// A point inside the outline far from every edge, leaning to the centre.
#[must_use]
pub fn label_pole(ol: &Outline) -> LabelPole {
    let bb = Bounds::of(ol.poly.iter().copied());
    let poly = &ol.poly;
    let step = (poly.len() / LABEL_SAMPLES).max(1);
    let q: Vec<Vec2> = poly.iter().step_by(step).copied().collect();
    let mut best = LabelPole {
        pos: Vec2::new(bb.cx, bb.cy),
        score: f64::NEG_INFINITY,
        clearance: 0.0,
        candidates: Vec::new(),
    };
    let mut cands: Vec<(Vec2, f64)> = Vec::new();
    let n = LABEL_GRID as f64;
    for i in 1..LABEL_GRID {
        for j in 1..LABEL_GRID {
            let x = bb.x0 + bb.w * i as f64 / n;
            let y = bb.y0 + bb.h * j as f64 / n;
            let p = Vec2::new(x, y);
            if !contains(poly, p) {
                continue;
            }
            let mut d = f64::INFINITY;
            for m in 0..q.len() {
                let r = distance_to_segment(p, q[m], q[(m + 1) % q.len()]).distance;
                if r < d {
                    d = r;
                }
            }
            let s = d.min(bb.w.min(bb.h) * LABEL_CLEARANCE_CAP)
                - LABEL_CENTRE_PULL * hypot(x - bb.cx, (y - bb.cy) * 0.5);
            cands.push((p, s));
            if s > best.score {
                best.pos = p;
                best.score = s;
                best.clearance = d;
            }
        }
    }
    cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    cands.truncate(LABEL_CANDIDATES);
    best.candidates = cands;
    best
}
