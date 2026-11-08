//! Marker making: every piece of the pattern, in the sizes asked for and as many copies as each
//! is cut, nested on a fabric of a given width with the grain along the selvedge, so the fabric
//! a lay needs (the marker's length) and how much of it the pieces use (its efficiency) are known
//! before cutting.
//!
//! Pieces are rasterised at a fixed resolution (their cut lines grown by the gap) and placed one
//! by one, largest first, at the leftmost spot where they fit, as a bottom-left fill does: each
//! may turn 180° (two-way fabric) and a pair's second copy is flipped, as pairs are cut face to
//! face. The marker runs along x (its length) with the fabric's width across y, in mm.

use azam_doc::{Piece, Project};
use azam_geom::{SizeName, Vec2, signed_area};

use crate::model::{DEG, cut_line, graded_outline, pieces_in_id_order, qty};

/// Settings for a marker.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerOptions {
    /// Usable fabric width, mm.
    pub width_mm: f64,
    /// Space kept between pieces (and from the fabric's edges), mm.
    pub gap_mm: f64,
    /// Whether pieces may turn 180° (a fabric without nap or one-way print).
    pub rotate_180: bool,
    /// The sizes laid in the marker, each with how many garments of it (a "size ratio").
    pub sizes: Vec<(SizeName, u32)>,
    /// Raster cell, mm: smaller nests tighter and takes longer.
    pub resolution_mm: f64,
}

impl MarkerOptions {
    /// One garment of `size` on 1.5 m fabric, 5 mm apart, turning allowed.
    #[must_use]
    pub fn single(size: &SizeName) -> Self {
        Self {
            width_mm: DEFAULT_WIDTH_MM,
            gap_mm: DEFAULT_GAP_MM,
            rotate_180: true,
            sizes: vec![(size.clone(), 1)],
            resolution_mm: DEFAULT_RESOLUTION_MM,
        }
    }
}

/// Usable width of the default fabric, mm.
pub const DEFAULT_WIDTH_MM: f64 = 1500.0;
/// Default space between pieces, mm.
pub const DEFAULT_GAP_MM: f64 = 5.0;
/// Default raster cell, mm.
pub const DEFAULT_RESOLUTION_MM: f64 = 5.0;
/// Most raster columns a marker may grow to (a 25 m lay at the default cell), so a piece wider
/// than the fabric cannot make it run on.
const MAX_COLUMNS: usize = 5000;

/// One piece copy laid in the marker.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// The piece's name.
    pub name: String,
    /// The size it is cut in.
    pub size: SizeName,
    /// Which copy of the piece it is (from 1).
    pub copy: u32,
    /// Whether it is turned 180° from its grain direction.
    pub turned: bool,
    /// Whether it is flipped (a pair's other side).
    pub flipped: bool,
    /// Its cut line, in marker mm.
    pub outline: Vec<Vec2>,
}

/// A nested marker.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    /// Fabric width, mm.
    pub width_mm: f64,
    /// Length of fabric the lay needs, mm.
    pub length_mm: f64,
    /// The laid pieces.
    pub placements: Vec<Placement>,
    /// Pieces that could not be laid (wider than the fabric), by name and size.
    pub left_out: Vec<(String, SizeName)>,
}

impl Marker {
    /// Area of the laid pieces over the fabric they take, 0..1.
    #[must_use]
    pub fn efficiency(&self) -> f64 {
        let used: f64 = self
            .placements
            .iter()
            .map(|p| signed_area(&p.outline).abs())
            .sum();
        let fabric = self.width_mm * self.length_mm;
        if fabric > 0.0 { used / fabric } else { 0.0 }
    }
}

/// A piece copy as raster rows: for each row (across the width), the occupied cell spans.
struct Shape {
    /// Columns (along the length) and rows (across the width) the shape spans.
    cols: usize,
    rows: usize,
    /// Per row, `[start, end)` column spans.
    spans: Vec<Vec<(usize, usize)>>,
    /// The cut line, relative to the shape's raster origin, mm.
    outline: Vec<Vec2>,
    turned: bool,
}

/// What is laid so far: per row, the occupied cells as a bitset.
struct Occupancy {
    words: usize,
    rows: Vec<Vec<u64>>,
}

impl Occupancy {
    fn new(rows: usize, cols: usize) -> Self {
        let words = cols.div_ceil(64);
        Self {
            words,
            rows: vec![vec![0; words]; rows],
        }
    }

    fn get(&self, row: usize, col: usize) -> bool {
        col / 64 < self.words && self.rows[row][col / 64] >> (col % 64) & 1 == 1
    }

    fn set(&mut self, row: usize, col: usize) {
        if col / 64 < self.words {
            self.rows[row][col / 64] |= 1 << (col % 64);
        }
    }

    /// The first occupied column in `row` within `[a, b)`, if any.
    fn first_taken(&self, row: usize, a: usize, b: usize) -> Option<usize> {
        let mut c = a;
        while c < b {
            let (w, bit) = (c / 64, c % 64);
            if w >= self.words {
                return None;
            }
            let word = self.rows[row][w] >> bit;
            if word != 0 {
                let at = c + word.trailing_zeros() as usize;
                return (at < b).then_some(at);
            }
            c = (w + 1) * 64;
        }
        None
    }

    /// The first free column at or after `c` in `row`.
    fn next_free(&self, row: usize, mut c: usize) -> usize {
        while self.get(row, c) {
            c += 1;
        }
        c
    }
}

/// `pts` rasterised at `cell` mm: the spans of each row whose cell centre lies inside, grown by
/// `grow` cells every way (the gap).
fn rasterise(pts: &[Vec2], cell: f64, grow: usize) -> (usize, usize, Vec<Vec<(usize, usize)>>) {
    let cols = ((pts.iter().map(|p| p.x).fold(0.0, f64::max) / cell).ceil() as usize).max(1) + 2 * grow;
    let rows = ((pts.iter().map(|p| p.y).fold(0.0, f64::max) / cell).ceil() as usize).max(1) + 2 * grow;
    let mut filled = vec![vec![false; cols]; rows];
    let n = pts.len();
    for (r, row) in filled.iter_mut().enumerate().take(rows) {
        let y = (r as f64 - grow as f64 + 0.5) * cell;
        let mut xs: Vec<f64> = (0..n)
            .filter_map(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                ((a.y <= y) != (b.y <= y)).then(|| a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x))
            })
            .collect();
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks_exact(2) {
            let c0 = ((pair[0] / cell - 0.5).ceil().max(0.0) as usize) + grow;
            let c1 = ((pair[1] / cell - 0.5).floor().max(-1.0) + 1.0) as usize + grow;
            for cell_in in row.iter_mut().take(c1.min(cols)).skip(c0) {
                *cell_in = true;
            }
        }
    }
    // also mark the cells the outline passes through, so thin parts are never lost
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let steps = ((b - a).length() / (cell * 0.5)).ceil().max(1.0) as usize;
        for s in 0..=steps {
            let p = a + (b - a) * (s as f64 / steps as f64);
            let (c, r) = ((p.x / cell) as usize + grow, (p.y / cell) as usize + grow);
            if r < rows && c < cols {
                filled[r][c] = true;
            }
        }
    }
    // grow by the gap: a square neighbourhood of `grow` cells
    let grown: Vec<Vec<bool>> = (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    let (r0, r1) = (r.saturating_sub(grow), (r + grow).min(rows - 1));
                    let (c0, c1) = (c.saturating_sub(grow), (c + grow).min(cols - 1));
                    (r0..=r1).any(|rr| filled[rr][c0..=c1].iter().any(|&f| f))
                })
                .collect()
        })
        .collect();
    let spans = grown
        .iter()
        .map(|row| {
            let mut out = Vec::new();
            let mut c = 0;
            while c < cols {
                if row[c] {
                    let start = c;
                    while c < cols && row[c] {
                        c += 1;
                    }
                    out.push((start, c));
                } else {
                    c += 1;
                }
            }
            out
        })
        .collect();
    (cols, rows, spans)
}

/// `outline` laid along the marker: its grain line (at `grain` degrees from the pattern's
/// vertical) along x, turned 180° if `turned`, mirrored across its grain if `flipped`, and moved
/// so its bounding box starts at the origin.
fn orient(outline: &[Vec2], grain: f64, turned: bool, flipped: bool) -> Vec<Vec2> {
    // the grain direction in the pattern, as an angle from +x (pattern y runs down)
    let g = (90.0 - grain) * DEG;
    let mut a = -g + if turned { std::f64::consts::PI } else { 0.0 };
    if !a.is_finite() {
        a = 0.0;
    }
    let (s, c) = a.sin_cos();
    let mut pts: Vec<Vec2> = outline
        .iter()
        .map(|p| {
            let q = Vec2::new(p.x * c - p.y * s, p.x * s + p.y * c);
            if flipped { Vec2::new(q.x, -q.y) } else { q }
        })
        .collect();
    let (x0, y0) = pts.iter().fold((f64::INFINITY, f64::INFINITY), |(x, y), p| {
        (x.min(p.x), y.min(p.y))
    });
    for p in &mut pts {
        *p = Vec2::new(p.x - x0, p.y - y0);
    }
    pts
}

/// One piece copy to lay.
struct ToLay<'a> {
    piece: &'a Piece,
    size: SizeName,
    copy: u32,
    flipped: bool,
    outline: Vec<Vec2>,
    area: f64,
}

/// Nests `project`'s pieces as `options` asks.
#[must_use]
pub fn make_marker(project: &Project, options: &MarkerOptions) -> Marker {
    let cell = options.resolution_mm.max(1.0);
    let grow = (options.gap_mm.max(0.0) / 2.0 / cell).round() as usize;
    let rows = ((options.width_mm / cell).floor() as usize).max(1);
    let mut copies = Vec::new();
    for (size, garments) in &options.sizes {
        for pc in pieces_in_id_order(project) {
            let outline = cut_line(pc, &graded_outline(project, pc, Some(size)));
            if outline.len() < 3 {
                continue;
            }
            let area = signed_area(&outline).abs();
            let per_garment = qty(pc).round().max(1.0) as u32;
            for k in 0..per_garment * garments {
                copies.push(ToLay {
                    piece: pc,
                    size: size.clone(),
                    copy: k + 1,
                    // a pair's second copy is cut face to face
                    flipped: per_garment > 1 && k % 2 == 1,
                    outline: outline.clone(),
                    area,
                });
            }
        }
    }
    copies.sort_by(|a, b| b.area.total_cmp(&a.area));

    let mut occ = Occupancy::new(rows, MAX_COLUMNS);
    let mut placements = Vec::new();
    let mut left_out = Vec::new();
    for c in &copies {
        let turns: &[bool] = if options.rotate_180 {
            &[false, true]
        } else {
            &[false]
        };
        let shapes: Vec<Shape> = turns
            .iter()
            .map(|&turned| {
                let outline = orient(&c.outline, c.piece.grain, turned, c.flipped);
                let (cols, rows, spans) = rasterise(&outline, cell, grow);
                Shape {
                    cols,
                    rows,
                    spans,
                    outline,
                    turned,
                }
            })
            .collect();
        let best = shapes
            .iter()
            .enumerate()
            .filter_map(|(i, s)| fit(&occ, s, rows).map(|(x, y)| (x + s.cols, x, y, i)))
            .min();
        let Some((_, x, y, i)) = best else {
            left_out.push((c.piece.name.clone(), c.size.clone()));
            continue;
        };
        let s = &shapes[i];
        for (r, spans) in s.spans.iter().enumerate() {
            for &(a, b) in spans {
                for col in a..b {
                    occ.set(y + r, x + col);
                }
            }
        }
        let origin = Vec2::new((x + grow) as f64 * cell, (y + grow) as f64 * cell);
        placements.push(Placement {
            name: c.piece.name.clone(),
            size: c.size.clone(),
            copy: c.copy,
            turned: s.turned,
            flipped: c.flipped,
            outline: s.outline.iter().map(|&p| p + origin).collect(),
        });
    }
    let length_mm = placements
        .iter()
        .flat_map(|p| p.outline.iter().map(|q| q.x))
        .fold(0.0, f64::max)
        + options.gap_mm.max(0.0) / 2.0;
    Marker {
        width_mm: options.width_mm,
        length_mm: if placements.is_empty() { 0.0 } else { length_mm },
        placements,
        left_out,
    }
}

/// The leftmost spot (column, row) where `shape` fits among what `occ` holds, within `rows`
/// rows; ties go to the lowest row.
fn fit(occ: &Occupancy, shape: &Shape, rows: usize) -> Option<(usize, usize)> {
    if shape.rows > rows {
        return None;
    }
    let mut best: Option<(usize, usize)> = None;
    for y in 0..=rows - shape.rows {
        let limit = best.map_or(MAX_COLUMNS, |(bx, _)| bx);
        let mut x = 0;
        'search: while x + shape.cols <= MAX_COLUMNS && x < limit {
            for (r, spans) in shape.spans.iter().enumerate() {
                for &(a, b) in spans {
                    if let Some(hit) = occ.first_taken(y + r, x + a, x + b) {
                        // move on until this span clears the occupied run
                        let free = occ.next_free(y + r, hit);
                        x = free.saturating_sub(a).max(x + 1);
                        continue 'search;
                    }
                }
            }
            if best.is_none_or(|(bx, _)| x < bx) {
                best = Some((x, y));
            }
            break;
        }
    }
    best
}

/// `marker` as an SVG drawing at 1 mm per unit: the fabric, each laid piece filled in a colour
/// of its size with its name, and the length and efficiency underneath.
#[must_use]
pub fn marker_svg(marker: &Marker) -> String {
    use std::fmt::Write as _;
    const FILLS: [&str; 6] = ["#e8a87c", "#85c1e9", "#a9dfbf", "#f7dc6f", "#d7bde2", "#f5b7b1"];
    let (l, h) = (marker.length_mm.max(1.0), marker.width_mm.max(1.0));
    let mut sizes: Vec<&SizeName> = Vec::new();
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-10 -10 {} {}" width="{}mm" height="{}mm">"#,
        l + 20.0,
        h + 60.0,
        l + 20.0,
        h + 60.0
    );
    let _ = write!(
        out,
        r##"<rect x="0" y="0" width="{l:.1}" height="{h:.1}" fill="#fbfaf7" stroke="#555" stroke-width="1"/>"##
    );
    for p in &marker.placements {
        let k = sizes.iter().position(|s| **s == p.size).unwrap_or_else(|| {
            sizes.push(&p.size);
            sizes.len() - 1
        });
        let pts: Vec<String> = p
            .outline
            .iter()
            .map(|q| format!("{:.1},{:.1}", q.x, q.y))
            .collect();
        let _ = write!(
            out,
            r##"<polygon points="{}" fill="{}" stroke="#333" stroke-width="0.8"/>"##,
            pts.join(" "),
            FILLS[k % FILLS.len()]
        );
        let n = p.outline.len().max(1) as f64;
        let c = p.outline.iter().fold(Vec2::ZERO, |a, &q| a + q) * (1.0 / n);
        let _ = write!(
            out,
            r##"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="14" text-anchor="middle" fill="#222">{} {}</text>"##,
            c.x,
            c.y,
            xml_escape(&p.name),
            xml_escape(&p.size.0)
        );
    }
    let _ = write!(
        out,
        r##"<text x="0" y="{:.1}" font-family="sans-serif" font-size="18" fill="#222">Length {:.0} mm · width {:.0} mm · efficiency {:.1}%</text></svg>"##,
        h + 35.0,
        marker.length_mm,
        marker.width_mm,
        marker.efficiency() * 100.0
    );
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tee() -> Project {
        let mut p = azam_doc::from_template(azam_doc::Template::Tee, azam_doc::Sex::Female, None, None, 0.0);
        p.ensure_ids_seeded();
        p
    }

    /// Whether two closed polygons' areas overlap, tested on a fine grid over their common box.
    fn overlap(a: &[Vec2], b: &[Vec2]) -> bool {
        let inside = |pts: &[Vec2], q: Vec2| {
            let n = pts.len();
            let mut odd = false;
            for i in 0..n {
                let (p0, p1) = (pts[i], pts[(i + 1) % n]);
                if (p0.y > q.y) != (p1.y > q.y) && q.x < p0.x + (q.y - p0.y) / (p1.y - p0.y) * (p1.x - p0.x) {
                    odd = !odd;
                }
            }
            odd
        };
        let bb = |pts: &[Vec2]| {
            pts.iter()
                .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(x0, y0, x1, y1), p| {
                    (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y))
                })
        };
        let (a0, b0) = (bb(a), bb(b));
        let (x0, y0, x1, y1) = (a0.0.max(b0.0), a0.1.max(b0.1), a0.2.min(b0.2), a0.3.min(b0.3));
        if x0 >= x1 || y0 >= y1 {
            return false;
        }
        let step = 2.0;
        let mut y = y0 + 1.0;
        while y < y1 {
            let mut x = x0 + 1.0;
            while x < x1 {
                let q = Vec2::new(x, y);
                if inside(a, q) && inside(b, q) {
                    return true;
                }
                x += step;
            }
            y += step;
        }
        false
    }

    #[test]
    fn every_piece_is_laid_inside_the_fabric_without_overlaps() {
        let p = tee();
        let options = MarkerOptions {
            sizes: vec![(p.base_size.clone(), 2)],
            ..MarkerOptions::single(&p.base_size)
        };
        let m = make_marker(&p, &options);
        let expected: u32 = p
            .pieces
            .values()
            .map(|pc| qty(pc).round().max(1.0) as u32 * 2)
            .sum();
        assert_eq!(m.placements.len() as u32, expected, "every copy laid");
        assert!(m.left_out.is_empty());
        for pl in &m.placements {
            for q in &pl.outline {
                assert!(
                    q.y >= -1e-6 && q.y <= m.width_mm + 1e-6,
                    "{} inside the width",
                    pl.name
                );
                assert!(
                    q.x >= -1e-6 && q.x <= m.length_mm + 1e-6,
                    "{} inside the length",
                    pl.name
                );
            }
        }
        for (i, a) in m.placements.iter().enumerate() {
            for b in &m.placements[i + 1..] {
                assert!(
                    !overlap(&a.outline, &b.outline),
                    "{} and {} overlap",
                    a.name,
                    b.name
                );
            }
        }
        let e = m.efficiency();
        assert!(e > 0.55 && e < 1.0, "efficiency {e:.3}");
    }

    #[test]
    fn a_narrower_fabric_needs_a_longer_lay_and_turning_helps() {
        let p = tee();
        let wide = make_marker(&p, &MarkerOptions::single(&p.base_size));
        let narrow = make_marker(
            &p,
            &MarkerOptions {
                width_mm: 1000.0,
                ..MarkerOptions::single(&p.base_size)
            },
        );
        assert!(
            narrow.length_mm > wide.length_mm,
            "{} vs {}",
            narrow.length_mm,
            wide.length_mm
        );
        let fixed = make_marker(
            &p,
            &MarkerOptions {
                rotate_180: false,
                ..MarkerOptions::single(&p.base_size)
            },
        );
        assert!(
            fixed.length_mm >= wide.length_mm - 1.0,
            "turning never makes it longer"
        );
    }

    #[test]
    fn a_piece_wider_than_the_fabric_is_left_out() {
        let p = tee();
        let m = make_marker(
            &p,
            &MarkerOptions {
                width_mm: 150.0,
                ..MarkerOptions::single(&p.base_size)
            },
        );
        assert!(!m.left_out.is_empty());
    }

    #[test]
    fn grain_runs_along_the_length() {
        // a 100 × 400 strip with its grain along its length lies 400 long, 100 across
        let pts = [
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(100.0, 400.0),
            Vec2::new(0.0, 400.0),
        ];
        let laid = orient(&pts, 0.0, false, false);
        let (w, h) = laid
            .iter()
            .fold((0.0f64, 0.0f64), |(w, h), p| (w.max(p.x), h.max(p.y)));
        assert!((w - 400.0).abs() < 1e-6 && (h - 100.0).abs() < 1e-6, "{w} × {h}");
    }

    #[test]
    fn the_marker_draws_and_exports() {
        let p = tee();
        let m = make_marker(&p, &MarkerOptions::single(&p.base_size));
        let svg = marker_svg(&m);
        assert_eq!(svg.matches("<polygon").count(), m.placements.len());
        assert!(svg.contains("efficiency"));
        let dxf = crate::marker_dxf(&m);
        assert_eq!(
            dxf.matches("POLYLINE").count(),
            m.placements.len() + 1,
            "pieces and the fabric"
        );
        if let Ok(dir) = std::env::var("MARKER_OUT") {
            let _ = std::fs::write(format!("{dir}/marker.svg"), &svg);
            eprintln!("length {:.0} efficiency {:.3}", m.length_mm, m.efficiency());
        }
    }
}
