//! Triangle meshes on the GPU.

use glam::Vec3;
use wgpu::util::DeviceExt;

/// Identifies a mesh uploaded to a [`Renderer`](crate::Renderer).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshId(pub u32);

/// Vertex and index data of a triangle mesh, in mm.
#[derive(Clone, Copy, Debug)]
pub struct MeshData<'a> {
    /// Vertex positions.
    pub positions: &'a [[f32; 3]],
    /// Vertex normals, one per position.
    pub normals: &'a [[f32; 3]],
    /// Optional linear RGBA vertex colours (fit maps), one per position.
    pub colors: Option<&'a [[f32; 4]]>,
    /// Optional texture coordinates in mm (a garment piece's pattern coordinates), one per
    /// position; they drive the procedural fabric weaves.
    pub uvs: Option<&'a [[f32; 2]]>,
    /// Triangle list, three indices per triangle.
    pub indices: &'a [u32],
}

/// A mesh's GPU buffers.
pub(crate) struct GpuMesh {
    pub positions: wgpu::Buffer,
    pub normals: wgpu::Buffer,
    pub colors: wgpu::Buffer,
    pub uvs: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    /// Each unique edge of the triangles once, as a line list (the wireframe overlay).
    pub edges: wgpu::Buffer,
    pub vertex_count: usize,
    pub index_count: u32,
    pub edge_count: u32,
    /// Bounding box of the positions, for fitting the shadow map.
    pub bounds: (Vec3, Vec3),
}

const WHITE: [f32; 4] = [1.0; 4];

/// The bounding box of `positions` (an empty box for no positions).
pub(crate) fn bounds_of(positions: &[[f32; 3]]) -> (Vec3, Vec3) {
    positions
        .iter()
        .fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| {
            let v = Vec3::from(*p);
            (lo.min(v), hi.max(v))
        })
}

/// The unique edges of triangle list `indices`, two indices per edge.
pub(crate) fn edge_list(indices: &[u32]) -> Vec<u32> {
    let mut edges: Vec<(u32, u32)> = indices
        .chunks_exact(3)
        .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges.into_iter().flat_map(|(a, b)| [a, b]).collect()
}

impl GpuMesh {
    pub fn new(device: &wgpu::Device, data: &MeshData) -> Self {
        let vertex = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST;
        let colors: Vec<[f32; 4]> = match data.colors {
            Some(c) => c.to_vec(),
            None => vec![WHITE; data.positions.len()],
        };
        let uvs: Vec<[f32; 2]> = match data.uvs {
            Some(u) => u.to_vec(),
            None => vec![[0.0; 2]; data.positions.len()],
        };
        let make = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        let edges = edge_list(data.indices);
        Self {
            positions: make("positions", bytemuck::cast_slice(data.positions), vertex),
            normals: make("normals", bytemuck::cast_slice(data.normals), vertex),
            colors: make("colors", bytemuck::cast_slice(&colors), vertex),
            uvs: make("uvs", bytemuck::cast_slice(&uvs), vertex),
            indices: make(
                "indices",
                bytemuck::cast_slice(data.indices),
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            ),
            edges: make(
                "edges",
                bytemuck::cast_slice(&edges),
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            ),
            vertex_count: data.positions.len(),
            index_count: data.indices.len() as u32,
            edge_count: edges.len() as u32,
            bounds: bounds_of(data.positions),
        }
    }

    /// `true` if `data` fits in these buffers as they are.
    pub fn fits(&self, data: &MeshData) -> bool {
        self.vertex_count == data.positions.len()
            && self.index_count as usize == data.indices.len()
            && self.edge_count as usize == edge_list(data.indices).len()
    }
}
