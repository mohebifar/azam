//! Unique mesh edges with the vertices opposite them, for bending constraints.

use std::collections::HashMap;

use crate::PieceMesh;

/// An undirected mesh edge and the vertices opposite it in its triangles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// Smaller vertex index.
    pub a: u32,
    /// Larger vertex index.
    pub b: u32,
    /// Vertex opposite the edge in the first triangle that has it.
    pub opposite: u32,
    /// Vertex opposite the edge in the last other triangle that has it; `None` on the mesh
    /// boundary.
    pub other_opposite: Option<u32>,
}

impl Edge {
    /// Whether only one triangle has this edge.
    #[must_use]
    pub fn is_boundary(&self) -> bool {
        self.other_opposite.is_none()
    }
}

/// The edges of a mesh in first-seen order, walking the triangles and their edges in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Topology {
    /// Unique edges.
    pub edges: Vec<Edge>,
    by_vertices: HashMap<(u32, u32), usize>,
}

impl Topology {
    /// Collects the edges of `tris` (vertex index triples).
    #[must_use]
    pub fn new(tris: &[u32]) -> Self {
        let mut topology = Self::default();
        for t in tris.chunks_exact(3) {
            for e in 0..3 {
                let (i, j, o) = (t[e], t[(e + 1) % 3], t[(e + 2) % 3]);
                let key = (i.min(j), i.max(j));
                if let Some(&index) = topology.by_vertices.get(&key) {
                    topology.edges[index].other_opposite = Some(o);
                } else {
                    topology.by_vertices.insert(key, topology.edges.len());
                    topology.edges.push(Edge {
                        a: key.0,
                        b: key.1,
                        opposite: o,
                        other_opposite: None,
                    });
                }
            }
        }
        topology
    }

    /// Index in [`edges`](Self::edges) of the edge between vertices `i` and `j`, in either order.
    #[must_use]
    pub fn edge_index(&self, i: u32, j: u32) -> Option<usize> {
        self.by_vertices.get(&(i.min(j), i.max(j))).copied()
    }

    /// The edge between vertices `i` and `j`, in either order.
    #[must_use]
    pub fn edge(&self, i: u32, j: u32) -> Option<&Edge> {
        self.edge_index(i, j).map(|index| &self.edges[index])
    }
}

impl PieceMesh {
    /// The unique edges of the mesh with their opposite vertices.
    #[must_use]
    pub fn topology(&self) -> Topology {
        Topology::new(&self.tris)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_triangles_share_one_edge() {
        let topo = Topology::new(&[0, 1, 2, 2, 1, 3]);
        assert_eq!(topo.edges.len(), 5);
        let shared = topo.edge(2, 1).expect("edge exists");
        assert_eq!((shared.a, shared.b), (1, 2));
        assert_eq!((shared.opposite, shared.other_opposite), (0, Some(3)));
        assert!(topo.edge(0, 1).expect("edge exists").is_boundary());
        assert_eq!(topo.edge_index(0, 3), None);
    }
}
