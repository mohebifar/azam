//! Meshing a piece: boundary samples, internal-line samples, grain lattice and triangulation.

use azam_geom::{Outline, OutlinePoint, Vec2, bbox, contains, flatten_open_line, point_at_length};

use crate::delaunay::triangulate;
use crate::numeric::{arc_lengths, hypot};
use crate::spatial::VertexGrid;

/// Vertex spacing used when none is given, in mm.
pub const DEFAULT_SPACING_MM: f64 = 20.0;
/// Smallest allowed vertex spacing, in mm.
pub const MIN_SPACING_MM: f64 = 3.0;
/// Largest allowed vertex spacing, in mm.
pub const MAX_SPACING_MM: f64 = 80.0;

/// Internal-line samples closer than this fraction of the spacing to an earlier boundary or
/// internal-line vertex are dropped.
const LINE_CLEARANCE: f64 = 0.35;
/// Lattice points closer than this fraction of the spacing to a boundary or internal-line vertex
/// are dropped, so no sliver triangles form along edges.
const LATTICE_CLEARANCE: f64 = 0.62;
/// Triangles whose doubled area is below this fraction of the squared spacing are dropped.
const SLIVER_AREA: f64 = 0.004;

/// How finely to mesh a piece.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshOptions {
    /// Target distance between vertices, in mm. Clamped to
    /// [`MIN_SPACING_MM`]..=[`MAX_SPACING_MM`]; 0 or NaN selects [`DEFAULT_SPACING_MM`].
    pub spacing_mm: f64,
    /// Grain direction in degrees: rows of the interior lattice run along it.
    pub grain_deg: f64,
}

impl Default for MeshOptions {
    fn default() -> Self {
        Self {
            spacing_mm: DEFAULT_SPACING_MM,
            grain_deg: 0.0,
        }
    }
}

impl MeshOptions {
    /// The spacing actually used, in mm.
    #[must_use]
    pub fn effective_spacing(&self) -> f64 {
        let spacing = if self.spacing_mm == 0.0 || self.spacing_mm.is_nan() {
            DEFAULT_SPACING_MM
        } else {
            self.spacing_mm
        };
        spacing.clamp(MIN_SPACING_MM, MAX_SPACING_MM)
    }
}

/// An internal line of a piece (a fold, dart or placement line) to sample into the mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InternalLine<'a> {
    /// Stable id of the line.
    pub id: &'a str,
    /// The line's points: corners and Bézier curve points, as for an outline but open.
    pub points: &'a [OutlinePoint],
    /// Fold angle in degrees (180 = flat), or `None` for a line that does not fold.
    pub fold: Option<f64>,
}

/// The mesh vertices placed along one internal line.
#[derive(Clone, Debug, PartialEq)]
pub struct InternalSamples {
    /// Index of the line in the list passed to [`mesh_piece`].
    pub line: usize,
    /// Id of the line.
    pub id: String,
    /// Fold angle of the line in degrees, as given.
    pub fold: Option<f64>,
    /// Mesh vertices on the line, in order along it. Samples outside the piece or too close to
    /// another vertex are left out, so the list may be empty.
    pub verts: Vec<u32>,
}

/// A triangle mesh of one piece in its 2D pattern frame.
///
/// The first [`boundary_count`](Self::boundary_count) vertices are the boundary samples, in
/// outline order; internal-line samples follow, then the interior lattice.
#[derive(Clone, Debug, PartialEq)]
pub struct PieceMesh {
    /// Vertex positions as `x, y` pairs, in mm.
    pub xy: Vec<f64>,
    /// Triangles as vertex index triples, all with positive signed area in the y-down frame
    /// (clockwise on screen).
    pub tris: Vec<u32>,
    /// Mesh vertex of each boundary sample (the identity: boundary samples come first).
    pub boundary: Vec<u32>,
    /// Outline segment index of each boundary sample.
    pub boundary_segment: Vec<u32>,
    /// Fraction along its segment of each boundary sample.
    pub boundary_t: Vec<f32>,
    /// Samples of each internal line that has at least two flattened points.
    pub internals: Vec<InternalSamples>,
    /// The piece outline the mesh was built from.
    pub outline: Outline,
    /// Vertex spacing used, in mm.
    pub spacing: f64,
}

impl PieceMesh {
    /// Number of vertices.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.xy.len() / 2
    }

    /// Number of boundary vertices (the first ones in the vertex list).
    #[must_use]
    pub fn boundary_count(&self) -> usize {
        self.boundary.len()
    }

    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.tris.len() / 3
    }

    /// Position of vertex `i`.
    ///
    /// # Panics
    /// Panics if `i` is not a vertex index.
    #[must_use]
    pub fn vertex(&self, i: usize) -> Vec2 {
        Vec2::new(self.xy[2 * i], self.xy[2 * i + 1])
    }

    /// The triangles as vertex index triples.
    #[must_use]
    pub fn triangles(&self) -> impl ExactSizeIterator<Item = [u32; 3]> + '_ {
        self.tris.chunks_exact(3).map(|t| [t[0], t[1], t[2]])
    }
}

/// A boundary sample: where it lies on the outline.
struct BoundarySample {
    pos: Vec2,
    segment: usize,
    t: f64,
}

/// Meshes a piece given its outline points and internal lines.
///
/// The outline is sampled every `spacing` mm (at least once per segment); internal lines are
/// sampled at the same spacing; the rest of the inside is filled with a triangular lattice whose
/// rows follow the grain. Triangles outside the outline, or with an edge that leaves it, are
/// dropped.
#[must_use]
pub fn mesh_piece(
    points: &[OutlinePoint],
    internals: &[InternalLine<'_>],
    options: MeshOptions,
) -> PieceMesh {
    let spacing = options.effective_spacing();
    let outline = Outline::new(points);
    let boundary = sample_boundary(&outline, spacing);

    let mut vertices = VertexGrid::new(spacing);
    for b in &boundary {
        vertices.push_indexed(b.pos);
    }
    let internals = internals
        .iter()
        .enumerate()
        .filter_map(|(line, il)| sample_internal_line(&mut vertices, &outline.poly, spacing, line, il))
        .collect();
    fill_lattice(&mut vertices, &outline.poly, spacing, options.grain_deg);

    let xy: Vec<f64> = vertices.points.iter().flat_map(|p| [p.x, p.y]).collect();
    let tris = keep_inside_triangles(&vertices.points, boundary.len(), &outline.poly, spacing);

    PieceMesh {
        xy,
        tris,
        boundary: (0..boundary.len()).map(|i| i as u32).collect(),
        boundary_segment: boundary.iter().map(|b| b.segment as u32).collect(),
        boundary_t: boundary.iter().map(|b| b.t as f32).collect(),
        internals,
        outline,
        spacing,
    }
}

/// Samples every segment at `round(len / spacing)` (at least one) even steps, starting at its
/// start corner. The end corner is the next segment's first sample.
fn sample_boundary(outline: &Outline, spacing: f64) -> Vec<BoundarySample> {
    let mut out = Vec::new();
    for (segment, seg) in outline.segments.iter().enumerate() {
        let cum = arc_lengths(&seg.poly);
        let len = cum[cum.len() - 1];
        let n = (len / spacing).round().max(1.0) as usize;
        for i in 0..n {
            let t = i as f64 / n as f64;
            out.push(BoundarySample {
                // `len * (i / n)`; internal lines use `(len * i) / n`. Both match the golden data.
                pos: point_at_length(&seg.poly, &cum, len * t).pos,
                segment,
                t,
            });
        }
    }
    out
}

/// Samples an internal line at even steps, keeping the samples inside the piece that are clear of
/// earlier vertices. Returns `None` for a line with fewer than two flattened points.
fn sample_internal_line(
    vertices: &mut VertexGrid,
    poly: &[Vec2],
    spacing: f64,
    line: usize,
    il: &InternalLine<'_>,
) -> Option<InternalSamples> {
    let flat = flatten_open_line(il.points);
    if flat.len() < 2 {
        return None;
    }
    let cum = arc_lengths(&flat);
    let len = cum[cum.len() - 1];
    let n = (len / spacing).round().max(1.0) as usize;
    let clearance = spacing * LINE_CLEARANCE;
    let mut verts = Vec::new();
    for i in 0..=n {
        let p = point_at_length(&flat, &cum, len * i as f64 / n as f64).pos;
        if !contains(poly, p) || vertices.has_indexed_within(p, clearance) {
            continue;
        }
        verts.push(vertices.len() as u32);
        vertices.push_indexed(p);
    }
    Some(InternalSamples {
        line,
        id: il.id.to_owned(),
        fold: il.fold,
        verts,
    })
}

/// Fills the inside of `poly` with a triangular lattice of side `spacing`, rows along the grain,
/// centred on the bounding box. Points too close to a boundary or internal-line vertex are skipped.
fn fill_lattice(vertices: &mut VertexGrid, poly: &[Vec2], spacing: f64, grain_deg: f64) {
    let Some(bounds) = bbox(poly.iter().copied()) else {
        return;
    };
    let (sin, cos) = grain_deg.to_radians().sin_cos();
    let center = bounds.center();
    let size = bounds.size();
    let reach = hypot(size.x, size.y) / 2.0 + spacing;
    let row_step = spacing * 3f64.sqrt() / 2.0;
    let clearance = spacing * LATTICE_CLEARANCE;

    // Accumulate the lattice coordinates by repeated addition, as the reference does, so the
    // points land on the same bits.
    let mut row = 0_usize;
    let mut v = -reach;
    while v <= reach {
        let mut u = -reach + if row % 2 == 1 { spacing / 2.0 } else { 0.0 };
        while u <= reach {
            let p = Vec2::new(center.x + u * cos - v * sin, center.y + u * sin + v * cos);
            let in_box =
                p.x >= bounds.min.x && p.x <= bounds.max.x && p.y >= bounds.min.y && p.y <= bounds.max.y;
            if in_box && contains(poly, p) && !vertices.has_indexed_within(p, clearance) {
                vertices.push(p);
            }
            u += spacing;
        }
        v += row_step;
        row += 1;
    }
}

/// Triangulates the vertices and keeps the triangles that lie inside `poly`, oriented with
/// positive signed area.
///
/// A triangle is dropped if it is a sliver, if its centroid is outside, or if the midpoint of any
/// edge is outside — except edges between neighbouring boundary samples, which lie on the outline.
fn keep_inside_triangles(points: &[Vec2], boundary_count: usize, poly: &[Vec2], spacing: f64) -> Vec<u32> {
    let is_outline_edge = |i: usize, j: usize| {
        i < boundary_count && j < boundary_count && {
            let d = i.abs_diff(j);
            d == 1 || d == boundary_count - 1
        }
    };
    let min_area = spacing * spacing * SLIVER_AREA;
    let mut tris = Vec::new();
    for t in triangulate(points).chunks_exact(3) {
        let (a, b, c) = (t[0], t[1], t[2]);
        let (pa, pb, pc) = (points[a], points[b], points[c]);
        let doubled_area = (pb.x - pa.x) * (pc.y - pa.y) - (pb.y - pa.y) * (pc.x - pa.x);
        if doubled_area.abs() < min_area {
            continue;
        }
        let centroid = Vec2::new((pa.x + pb.x + pc.x) / 3.0, (pa.y + pb.y + pc.y) / 3.0);
        if !contains(poly, centroid) {
            continue;
        }
        let edges_inside = [(a, b), (b, c), (c, a)].into_iter().all(|(i, j)| {
            is_outline_edge(i, j) || {
                let (p, q) = (points[i], points[j]);
                contains(poly, Vec2::new(p.x.midpoint(q.x), p.y.midpoint(q.y)))
            }
        });
        if !edges_inside {
            continue;
        }
        let tri = if doubled_area > 0.0 { [a, b, c] } else { [a, c, b] };
        tris.extend(tri.map(|i| i as u32));
    }
    tris
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(side: f64) -> Vec<OutlinePoint> {
        vec![
            OutlinePoint::corner("a", 0.0, 0.0),
            OutlinePoint::corner("b", side, 0.0),
            OutlinePoint::corner("c", side, side),
            OutlinePoint::corner("d", 0.0, side),
        ]
    }

    #[test]
    fn spacing_is_clamped_and_defaulted() {
        let spacing = |spacing_mm| {
            MeshOptions {
                spacing_mm,
                grain_deg: 0.0,
            }
            .effective_spacing()
        };
        assert!((spacing(0.0) - DEFAULT_SPACING_MM).abs() < f64::EPSILON);
        assert!((spacing(f64::NAN) - DEFAULT_SPACING_MM).abs() < f64::EPSILON);
        assert!((spacing(1.0) - MIN_SPACING_MM).abs() < f64::EPSILON);
        assert!((spacing(500.0) - MAX_SPACING_MM).abs() < f64::EPSILON);
        assert!((spacing(12.5) - 12.5).abs() < f64::EPSILON);
    }

    #[test]
    fn square_mesh_has_boundary_first_and_positive_triangles() {
        let mesh = mesh_piece(&square(100.0), &[], MeshOptions::default());
        assert_eq!(mesh.boundary_count(), 20);
        assert_eq!(mesh.boundary_segment[5], 1);
        assert!((mesh.boundary_t[6] - 0.2).abs() < 1e-6);
        assert!(mesh.vertex_count() > mesh.boundary_count());
        assert!(mesh.triangle_count() > 0);
        for [a, b, c] in mesh.triangles() {
            let (pa, pb, pc) = (
                mesh.vertex(a as usize),
                mesh.vertex(b as usize),
                mesh.vertex(c as usize),
            );
            assert!((pb - pa).perp_dot(pc - pa) > 0.0);
        }
    }

    #[test]
    fn internal_line_outside_the_piece_keeps_no_samples() {
        let pts = [
            OutlinePoint::corner("p", 200.0, 0.0),
            OutlinePoint::corner("q", 300.0, 0.0),
        ];
        let lines = [InternalLine {
            id: "fold",
            points: &pts,
            fold: Some(90.0),
        }];
        let mesh = mesh_piece(&square(100.0), &lines, MeshOptions::default());
        assert_eq!(mesh.internals.len(), 1);
        assert_eq!(mesh.internals[0].id, "fold");
        assert!(mesh.internals[0].verts.is_empty());
    }
}
