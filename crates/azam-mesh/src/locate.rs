//! Barycentric lookup of 2D points in a mesh.

use azam_geom::Vec2;

use crate::PieceMesh;

/// Triangles with a barycentric denominator below this (twice the area, in mm²) are skipped.
const DEGENERATE_DENOMINATOR: f64 = 1e-12;

/// Where a point falls in a mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    /// Index of the triangle (into the triples of [`PieceMesh::tris`]).
    pub triangle: usize,
    /// The triangle's vertices.
    pub vertices: [u32; 3],
    /// Barycentric weights of the point for `vertices`; they sum to 1 and are all non-negative
    /// when the point is inside the triangle.
    pub barycentric: [f64; 3],
}

impl Location {
    /// How far the point is outside its triangle: the sum of the negative barycentric weights,
    /// negated (0 inside).
    #[must_use]
    pub fn outside_amount(&self) -> f64 {
        self.barycentric.iter().map(|&l| (-l).max(0.0)).sum()
    }
}

impl PieceMesh {
    /// The first triangle that contains `q`, or the triangle it is least outside of (by the sum of
    /// negative barycentric weights). `None` if the mesh has no usable triangle.
    #[must_use]
    pub fn locate(&self, q: Vec2) -> Option<Location> {
        let mut best: Option<Location> = None;
        let mut best_outside = f64::INFINITY;
        for (triangle, vertices) in self.triangles().enumerate() {
            let Some(barycentric) = self.barycentric(vertices, q) else {
                continue;
            };
            let location = Location {
                triangle,
                vertices,
                barycentric,
            };
            let outside = location.outside_amount();
            if outside < best_outside {
                best_outside = outside;
                best = Some(location);
                if outside == 0.0 {
                    break;
                }
            }
        }
        best
    }

    /// Barycentric weights of `q` in a triangle, or `None` if the triangle is degenerate.
    fn barycentric(&self, [a, b, c]: [u32; 3], q: Vec2) -> Option<[f64; 3]> {
        let (pa, pb, pc) = (
            self.vertex(a as usize),
            self.vertex(b as usize),
            self.vertex(c as usize),
        );
        let d = (pb.y - pc.y) * (pa.x - pc.x) + (pc.x - pb.x) * (pa.y - pc.y);
        if d.abs() < DEGENERATE_DENOMINATOR {
            return None;
        }
        let l1 = ((pb.y - pc.y) * (q.x - pc.x) + (pc.x - pb.x) * (q.y - pc.y)) / d;
        let l2 = ((pc.y - pa.y) * (q.x - pc.x) + (pa.x - pc.x) * (q.y - pc.y)) / d;
        Some([l1, l2, 1.0 - l1 - l2])
    }
}

#[cfg(test)]
mod tests {
    use azam_geom::{Outline, OutlinePoint};

    use super::*;

    fn unit_square_mesh() -> PieceMesh {
        let pts = [
            OutlinePoint::corner("a", 0.0, 0.0),
            OutlinePoint::corner("b", 10.0, 0.0),
            OutlinePoint::corner("c", 10.0, 10.0),
        ];
        PieceMesh {
            xy: vec![0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0],
            tris: vec![0, 1, 2, 0, 2, 3],
            boundary: vec![],
            boundary_segment: vec![],
            boundary_t: vec![],
            internals: vec![],
            outline: Outline::new(&pts),
            spacing: 10.0,
        }
    }

    #[test]
    fn inside_point_has_non_negative_weights() {
        let hit = unit_square_mesh()
            .locate(Vec2::new(2.0, 6.0))
            .expect("mesh has triangles");
        assert_eq!(hit.triangle, 1);
        assert_eq!(hit.vertices, [0, 2, 3]);
        assert!(hit.outside_amount().abs() < f64::EPSILON);
        let pos: Vec2 = [Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)]
            .iter()
            .zip(hit.barycentric)
            .map(|(p, w)| *p * w)
            .sum();
        assert!((pos - Vec2::new(2.0, 6.0)).length() < 1e-12);
    }

    #[test]
    fn outside_point_falls_back_to_the_nearest_triangle() {
        let hit = unit_square_mesh()
            .locate(Vec2::new(12.0, 5.0))
            .expect("mesh has triangles");
        assert_eq!(hit.triangle, 0);
        assert!(hit.outside_amount() > 0.0);
    }
}
