//! Mesh vertices with a uniform-grid index for clearance queries.

use std::collections::HashMap;

use azam_geom::Vec2;

use crate::numeric::hypot;

/// Integer coordinates of a grid cell.
type Cell = (i64, i64);

/// The vertex list being built, with a grid over the vertices that later samples must keep clear
/// of (boundary and internal-line samples).
#[derive(Debug)]
pub(crate) struct VertexGrid {
    /// Edge length of a grid cell, in mm.
    cell: f64,
    /// Indices into `points`, by grid cell.
    buckets: HashMap<Cell, Vec<usize>>,
    /// All vertices, in mesh order.
    pub(crate) points: Vec<Vec2>,
}

impl VertexGrid {
    /// An empty vertex list whose grid cells are `cell` mm wide.
    pub(crate) fn new(cell: f64) -> Self {
        Self {
            cell,
            buckets: HashMap::new(),
            points: Vec::new(),
        }
    }

    /// Index the next pushed vertex will get.
    pub(crate) fn len(&self) -> usize {
        self.points.len()
    }

    /// Adds a vertex and indexes it in the grid.
    pub(crate) fn push_indexed(&mut self, p: Vec2) {
        let index = self.points.len();
        self.points.push(p);
        self.buckets.entry(self.cell_of(p)).or_default().push(index);
    }

    /// Adds a vertex without indexing it: later queries do not see it.
    pub(crate) fn push(&mut self, p: Vec2) {
        self.points.push(p);
    }

    /// Whether an indexed vertex lies closer than `radius` to `q`.
    pub(crate) fn has_indexed_within(&self, q: Vec2, radius: f64) -> bool {
        self.nearest_indexed_distance(q, radius) < radius
    }

    /// Distance from `q` to the nearest indexed vertex among the cells within `radius` of the
    /// cell of `q`, or infinity if there is none.
    fn nearest_indexed_distance(&self, q: Vec2, radius: f64) -> f64 {
        let (cx, cy) = self.cell_of(q);
        let reach = (radius / self.cell).ceil() as i64;
        let mut best = f64::INFINITY;
        for i in -reach..=reach {
            for j in -reach..=reach {
                let Some(bucket) = self.buckets.get(&(cx + i, cy + j)) else {
                    continue;
                };
                for &index in bucket {
                    let p = self.points[index];
                    let d = hypot(p.x - q.x, p.y - q.y);
                    if d < best {
                        best = d;
                    }
                }
            }
        }
        best
    }

    fn cell_of(&self, p: Vec2) -> Cell {
        ((p.x / self.cell).floor() as i64, (p.y / self.cell).floor() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_indexed_vertices_are_seen() {
        let mut grid = VertexGrid::new(10.0);
        grid.push_indexed(Vec2::new(5.0, 5.0));
        grid.push(Vec2::new(30.0, 30.0));
        assert_eq!(grid.len(), 2);
        assert!(grid.has_indexed_within(Vec2::new(8.0, 5.0), 3.5));
        assert!(!grid.has_indexed_within(Vec2::new(9.0, 5.0), 3.5));
        assert!(!grid.has_indexed_within(Vec2::new(30.0, 31.0), 3.5));
    }

    #[test]
    fn search_spans_neighbouring_cells() {
        let mut grid = VertexGrid::new(10.0);
        grid.push_indexed(Vec2::new(-1.0, 0.0));
        assert!(grid.has_indexed_within(Vec2::new(1.0, 0.0), 2.5));
    }
}
