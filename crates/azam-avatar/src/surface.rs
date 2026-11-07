//! Display mesh of the body extracted from its distance grid (surface nets).

use glam::DVec3;

use crate::grid::SdfGrid;
use crate::math::hypot3;

/// A triangle mesh with per-vertex normals, as flat buffers ready for upload.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AvatarMesh {
    /// Vertex positions, `x, y, z` per vertex, in millimetres.
    pub positions: Vec<f32>,
    /// Unit vertex normals, `x, y, z` per vertex.
    pub normals: Vec<f32>,
    /// Counter-clockwise triangles (seen from outside), three vertex indices each.
    pub indices: Vec<u32>,
}

impl AvatarMesh {
    /// Number of vertices.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Corner offsets of a grid cell, bit `c` of the inside mask belongs to corner `c`.
const CORNERS: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];
/// Cell edges as pairs of corner numbers.
const EDGES: [[usize; 2]; 12] = [
    [0, 1],
    [2, 3],
    [4, 5],
    [6, 7],
    [0, 2],
    [1, 3],
    [4, 6],
    [5, 7],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];

/// Mean of the points where the surface crosses the edges of a cell with corner values `v`, in
/// cell units.
fn crossing_centroid(v: &[f32; 8]) -> [f64; 3] {
    let (mut sx, mut sy, mut sz, mut count) = (0.0, 0.0, 0.0, 0.0);
    for e in &EDGES {
        let (a, b) = (f64::from(v[e[0]]), f64::from(v[e[1]]));
        if (a < 0.0) == (b < 0.0) {
            continue;
        }
        let t = a / (a - b);
        let (qa, qb) = (CORNERS[e[0]].map(|c| c as f64), CORNERS[e[1]].map(|c| c as f64));
        sx += qa[0] + (qb[0] - qa[0]) * t;
        sy += qa[1] + (qb[1] - qa[1]) * t;
        sz += qa[2] + (qb[2] - qa[2]) * t;
        count += 1.0;
    }
    [sx / count, sy / count, sz / count]
}

/// Extracts the zero level set of `grid` as a smooth closed mesh.
///
/// Every cell crossed by the surface gets one vertex at the mean of its edge crossings; every
/// grid edge crossed by the surface becomes a quad joining the four cells around it. Normals are
/// the normalised grid gradient.
#[must_use]
#[allow(clippy::cast_possible_wrap)] // vertex counts fit in i32
pub fn surface_mesh(grid: &SdfGrid) -> AvatarMesh {
    let [nx, ny, nz] = grid.dims;
    let data = &grid.data;
    let g = grid.cell;
    let at = |i: usize, j: usize, k: usize| data[i + nx * (j + ny * k)];
    let mut vertex_of = vec![-1_i32; nx * ny * nz];
    let mut mesh = AvatarMesh::default();
    let mut gradient = DVec3::ZERO;
    for k in 0..nz.saturating_sub(1) {
        for j in 0..ny.saturating_sub(1) {
            for i in 0..nx.saturating_sub(1) {
                let mut v = [0.0_f32; 8];
                let mut mask = 0_u32;
                for (c, q) in CORNERS.iter().enumerate() {
                    v[c] = at(i + q[0], j + q[1], k + q[2]);
                    if v[c] < 0.0 {
                        mask |= 1 << c;
                    }
                }
                if mask == 0 || mask == 255 {
                    continue;
                }
                let c = crossing_centroid(&v);
                let p = DVec3::new(
                    grid.origin.x + (i as f64 + c[0]) * g,
                    grid.origin.y + (j as f64 + c[1]) * g,
                    grid.origin.z + (k as f64 + c[2]) * g,
                );
                vertex_of[i + nx * (j + ny * k)] = (mesh.positions.len() / 3) as i32;
                mesh.positions.extend([p.x as f32, p.y as f32, p.z as f32]);
                if let Some(s) = grid.sample(p) {
                    gradient = s.gradient;
                }
                let len = hypot3(gradient.x, gradient.y, gradient.z);
                let len = if len > 0.0 { len } else { 1.0 };
                mesh.normals.extend([
                    (gradient.x / len) as f32,
                    (gradient.y / len) as f32,
                    (gradient.z / len) as f32,
                ]);
            }
        }
    }
    let cell = |i: usize, j: usize, k: usize| vertex_of[i + nx * (j + ny * k)];
    let quad = |indices: &mut Vec<u32>, q: [i32; 4], inside: bool| {
        let [a, b, c, d] = q;
        if a < 0 || b < 0 || c < 0 || d < 0 {
            return;
        }
        let [a, b, c, d] = [a, b, c, d].map(|x| x as u32);
        if inside {
            indices.extend([a, b, c, a, c, d]);
        } else {
            indices.extend([a, c, b, a, d, c]);
        }
    };
    for k in 1..nz.saturating_sub(1) {
        for j in 1..ny.saturating_sub(1) {
            for i in 1..nx.saturating_sub(1) {
                let d0 = at(i, j, k) < 0.0;
                if (at(i + 1, j, k) < 0.0) != d0 {
                    let q = [
                        cell(i, j - 1, k - 1),
                        cell(i, j, k - 1),
                        cell(i, j, k),
                        cell(i, j - 1, k),
                    ];
                    quad(&mut mesh.indices, q, d0);
                }
                if (at(i, j + 1, k) < 0.0) != d0 {
                    let q = [
                        cell(i - 1, j, k - 1),
                        cell(i - 1, j, k),
                        cell(i, j, k),
                        cell(i, j, k - 1),
                    ];
                    quad(&mut mesh.indices, q, d0);
                }
                if (at(i, j, k + 1) < 0.0) != d0 {
                    let q = [
                        cell(i - 1, j - 1, k),
                        cell(i, j - 1, k),
                        cell(i, j, k),
                        cell(i - 1, j, k),
                    ];
                    quad(&mut mesh.indices, q, d0);
                }
            }
        }
    }
    mesh
}
