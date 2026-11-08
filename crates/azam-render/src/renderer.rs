//! The wgpu device, pipelines and offscreen targets.

use std::collections::BTreeMap;
use std::fmt;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use crate::mesh::{GpuMesh, bounds_of, edge_list};
use crate::{MeshData, MeshId, OrbitCamera};

/// Colour format of the offscreen target; read-back bytes are sRGB-encoded RGBA.
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Multisample count for anti-aliasing.
const SAMPLES: u32 = 4;
/// Blender's default 3D viewport background, `#3d3d3d`.
const BACKGROUND_SRGB: [f32; 3] = [0.239, 0.239, 0.239];
/// Bytes per pixel of the read-back buffer.
const BYTES_PER_PIXEL: u32 = 4;
/// Size of one per-draw uniform block, before alignment.
const DRAW_UNIFORM_SIZE: u64 = std::mem::size_of::<DrawUniform>() as u64;
/// Side of the square shadow map, in texels (keep in sync with the shaders).
const SHADOW_SIZE: u32 = 2048;
/// Direction towards the key light (keep in sync with the look of the studio light).
const KEY_LIGHT: Vec3 = Vec3::new(0.42, 0.78, 0.46);
/// Margin around the shadow casters, in mm.
const SHADOW_MARGIN_MM: f32 = 150.0;

/// Why the renderer could not start.
#[derive(Debug)]
pub enum RenderError {
    /// No GPU adapter is available.
    NoAdapter,
    /// The adapter refused to create a device.
    Device(wgpu::RequestDeviceError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter => f.write_str("no GPU adapter is available"),
            Self::Device(e) => write!(f, "could not create a GPU device: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// A procedural fabric weave, drawn from a mesh's texture coordinates (in mm).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weave {
    /// No pattern (the avatar, leather, scuba).
    #[default]
    None,
    /// Plain weave: a fine grid of threads.
    Plain,
    /// Twill: diagonal ribs.
    Twill,
    /// Denim: a strong twill with a heathered look.
    Denim,
    /// Satin: nearly smooth.
    Satin,
    /// Jersey knit: rows of loops.
    Knit,
    /// Rib knit: vertical ribs.
    Rib,
    /// Herringbone: zigzag twill.
    Herringbone,
    /// Corduroy: raised wales.
    Corduroy,
    /// Gingham check.
    Check,
    /// Stripes.
    Stripe,
    /// Plaid / tartan.
    Plaid,
    /// Fleece: a soft mottle.
    Fleece,
}

impl Weave {
    /// The weave named `name` as stored on fabrics (`"plain"`, `"twill"`, `"jersey"` …).
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "plain" => Self::Plain,
            "twill" => Self::Twill,
            "denim" => Self::Denim,
            "satin" => Self::Satin,
            "jersey" | "knit" => Self::Knit,
            "rib" => Self::Rib,
            "herringbone" => Self::Herringbone,
            "corduroy" => Self::Corduroy,
            "check" => Self::Check,
            "stripe" => Self::Stripe,
            "plaid" => Self::Plaid,
            "fleece" | "quilt" => Self::Fleece,
            _ => Self::None,
        }
    }

    /// The shader's code for the weave.
    const fn code(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Plain => 1.0,
            Self::Twill => 2.0,
            Self::Denim => 3.0,
            Self::Satin => 4.0,
            Self::Knit => 5.0,
            Self::Rib => 6.0,
            Self::Herringbone => 7.0,
            Self::Corduroy => 8.0,
            Self::Check => 9.0,
            Self::Stripe => 10.0,
            Self::Plaid => 11.0,
            Self::Fleece => 12.0,
        }
    }
}

/// One mesh drawn with a material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Draw {
    /// The mesh to draw.
    pub mesh: MeshId,
    /// Base colour, sRGB 0..1, and alpha.
    pub color: [f32; 4],
    /// 0..1: how strongly the mesh is tinted as selected.
    pub highlight: f32,
    /// 0..1: how much the vertex colours replace the base colour (fit maps).
    pub vertex_colors: f32,
    /// Brightness of back faces (the inside of cloth), 0..1.
    pub inside: f32,
    /// Specular strength (sheen), 0..1.
    pub specular: f32,
    /// Roughness, 0 (glossy) .. 1 (matte).
    pub roughness: f32,
    /// Procedural weave.
    pub weave: Weave,
    /// Repeat of the weave, in mm (about two threads).
    pub weave_mm: f32,
    /// Whether the mesh casts a shadow.
    pub casts_shadow: bool,
    /// An image printed on the mesh (graphics on a piece), and where it lies in the mesh's UV
    /// space (pattern mm): `[min u, min v, 1 / width, 1 / height]`.
    pub decal: Option<(DecalId, [f32; 4])>,
}

/// Identifies a decal image uploaded to a [`Renderer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DecalId(pub u32);

impl Draw {
    /// A plain opaque draw of `mesh` in `color` (sRGB).
    #[must_use]
    pub const fn new(mesh: MeshId, color: [f32; 3]) -> Self {
        Self {
            mesh,
            color: [color[0], color[1], color[2], 1.0],
            highlight: 0.0,
            vertex_colors: 0.0,
            inside: 0.7,
            specular: 0.25,
            roughness: 0.6,
            weave: Weave::None,
            weave_mm: 1.2,
            casts_shadow: true,
            decal: None,
        }
    }
}

/// Viewport overlays and effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overlays {
    /// Draw the floor grid and axes.
    pub grid: bool,
    /// Shadows from the key light (on the floor and between meshes).
    pub shadows: bool,
    /// Draw every mesh's edges over its shading (Blender's Wireframe overlay).
    pub wireframe: bool,
}

impl Default for Overlays {
    fn default() -> Self {
        Self {
            grid: true,
            shadows: true,
            wireframe: false,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    light: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawUniform {
    color: [f32; 4],
    params: [f32; 4],
    material: [f32; 4],
    decal: [f32; 4],
}

/// `rgba` (`width` × `height`, sRGB, straight alpha) and its mip levels down to 1 × 1, each half
/// the size of the one before (a 2 × 2 box, averaged in linear light and weighted by alpha, so
/// thin marks fade instead of vanishing or turning dark at their edges).
fn mip_chain(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32, std::borrow::Cow<'_, [u8]>)> {
    let mut levels = vec![(width, height, std::borrow::Cow::Borrowed(rgba))];
    while let Some((w, h, data)) = levels.last()
        && (*w > 1 || *h > 1)
    {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let mut sum = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                    let at = ((sy * w + sx) * 4) as usize;
                    let a = f32::from(data[at + 3]) / 255.0;
                    for k in 0..3 {
                        sum[k] += srgb_to_linear(f32::from(data[at + k]) / 255.0) * a;
                    }
                    sum[3] += a;
                }
                let at = ((y * nw + x) * 4) as usize;
                if sum[3] > 0.0 {
                    for k in 0..3 {
                        next[at + k] = (linear_to_srgb(sum[k] / sum[3]) * 255.0).round() as u8;
                    }
                }
                next[at + 3] = (sum[3] / 4.0 * 255.0).round() as u8;
            }
        }
        levels.push((nw, nh, std::borrow::Cow::Owned(next)));
    }
    levels
}

/// Converts one linear channel to sRGB.
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Converts one sRGB channel to linear.
#[must_use]
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Offscreen targets of one size.
struct Targets {
    width: u32,
    height: u32,
    msaa: wgpu::TextureView,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    depth: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_row: u32,
}

impl Targets {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = |label, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let attach = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let msaa = texture("msaa", COLOR_FORMAT, SAMPLES, attach);
        let resolve = texture("resolve", COLOR_FORMAT, 1, attach | wgpu::TextureUsages::COPY_SRC);
        let depth = texture("depth", DEPTH_FORMAT, SAMPLES, attach);
        let padded_row = (width * BYTES_PER_PIXEL).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            width,
            height,
            msaa: msaa.create_view(&wgpu::TextureViewDescriptor::default()),
            resolve_view: resolve.create_view(&wgpu::TextureViewDescriptor::default()),
            resolve,
            depth: depth.create_view(&wgpu::TextureViewDescriptor::default()),
            readback,
            padded_row,
        }
    }
}

/// The pipelines and bind group layouts.
struct Pipelines {
    mesh: wgpu::RenderPipeline,
    grid: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    wire: wgpu::RenderPipeline,
    frame_layout: wgpu::BindGroupLayout,
    light_layout: wgpu::BindGroupLayout,
    draw_layout: wgpu::BindGroupLayout,
    decal_layout: wgpu::BindGroupLayout,
}

/// Draws scenes into RGBA pixels.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: Pipelines,
    frame_buffer: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    light_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    draw_buffer: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    draw_capacity: u64,
    draw_stride: u64,
    meshes: BTreeMap<MeshId, GpuMesh>,
    decals: BTreeMap<DecalId, wgpu::BindGroup>,
    no_decal: wgpu::BindGroup,
    decal_sampler: wgpu::Sampler,
    targets: Option<Targets>,
    pixels: Vec<u8>,
}

impl Renderer {
    /// Starts the renderer on the default GPU.
    ///
    /// # Errors
    /// Fails if no GPU adapter is available or it refuses to create a device.
    pub fn new() -> Result<Self, RenderError> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .ok_or(RenderError::NoAdapter)?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("azam"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(RenderError::Device)?;
        Ok(Self::with_device(device, queue))
    }

    /// A renderer on an existing device (shared with the cloth solver).
    #[must_use]
    pub fn with_device(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let pipelines = Pipelines::new(&device);
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d {
                width: SHADOW_SIZE,
                height: SHADOW_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow.create_view(&wgpu::TextureViewDescriptor::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &pipelines.frame_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });
        let light_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light"),
            layout: &pipelines.light_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });
        let draw_stride = DRAW_UNIFORM_SIZE
            .next_multiple_of(u64::from(device.limits().min_uniform_buffer_offset_alignment));
        let draw_capacity = 16;
        let (draw_buffer, draw_group) =
            Self::draw_slots(&device, &pipelines.draw_layout, draw_stride, draw_capacity);
        let decal_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("decal"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let no_decal = Self::decal_group(
            &device,
            &queue,
            &pipelines.decal_layout,
            &decal_sampler,
            1,
            1,
            &[0; 4],
        );
        Self {
            device,
            queue,
            pipelines,
            frame_buffer,
            frame_group,
            light_group,
            shadow_view,
            draw_buffer,
            draw_group,
            draw_capacity,
            draw_stride,
            meshes: BTreeMap::new(),
            decals: BTreeMap::new(),
            no_decal,
            decal_sampler,
            targets: None,
            pixels: Vec::new(),
        }
    }

    /// The GPU device (to share with the cloth solver).
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The GPU queue (to share with the cloth solver).
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    fn draw_slots(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        stride: u64,
        capacity: u64,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draws"),
            size: stride * capacity,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draws"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(DRAW_UNIFORM_SIZE),
                }),
            }],
        });
        (buffer, group)
    }

    /// A bind group holding a `width` × `height` sRGB RGBA image.
    fn decal_group(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> wgpu::BindGroup {
        let levels = mip_chain(width, height, rgba);
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("decal"),
            size,
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, (w, h, data)) in levels.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(*h),
                },
                wgpu::Extent3d {
                    width: *w,
                    height: *h,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("decal"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Uploads or replaces decal `id`: a `width` × `height` image of sRGB RGBA bytes (row by
    /// row, top first). An image whose byte count does not match is ignored.
    pub fn set_decal(&mut self, id: DecalId, width: u32, height: u32, rgba: &[u8]) {
        if width == 0 || height == 0 || rgba.len() != (width * height * 4) as usize {
            return;
        }
        let group = Self::decal_group(
            &self.device,
            &self.queue,
            &self.pipelines.decal_layout,
            &self.decal_sampler,
            width,
            height,
            rgba,
        );
        self.decals.insert(id, group);
    }

    /// Forgets decal `id`.
    pub fn remove_decal(&mut self, id: DecalId) {
        self.decals.remove(&id);
    }

    /// Uploads or replaces mesh `id`.
    pub fn set_mesh(&mut self, id: MeshId, data: &MeshData) {
        match self.meshes.get_mut(&id) {
            Some(mesh) if mesh.fits(data) => {
                self.queue
                    .write_buffer(&mesh.positions, 0, bytemuck::cast_slice(data.positions));
                self.queue
                    .write_buffer(&mesh.normals, 0, bytemuck::cast_slice(data.normals));
                if let Some(colors) = data.colors {
                    self.queue
                        .write_buffer(&mesh.colors, 0, bytemuck::cast_slice(colors));
                }
                if let Some(uvs) = data.uvs {
                    self.queue.write_buffer(&mesh.uvs, 0, bytemuck::cast_slice(uvs));
                }
                self.queue
                    .write_buffer(&mesh.indices, 0, bytemuck::cast_slice(data.indices));
                self.queue
                    .write_buffer(&mesh.edges, 0, bytemuck::cast_slice(&edge_list(data.indices)));
                mesh.bounds = bounds_of(data.positions);
            }
            _ => {
                self.meshes.insert(id, GpuMesh::new(&self.device, data));
            }
        }
    }

    /// Updates the vertices of mesh `id` in place (same vertex count), e.g. after a simulation
    /// step. Ignored if the mesh is missing or the count differs.
    pub fn update_vertices(&mut self, id: MeshId, positions: &[[f32; 3]], normals: &[[f32; 3]]) {
        if let Some(mesh) = self.meshes.get_mut(&id)
            && mesh.vertex_count == positions.len()
            && mesh.vertex_count == normals.len()
        {
            self.queue
                .write_buffer(&mesh.positions, 0, bytemuck::cast_slice(positions));
            self.queue
                .write_buffer(&mesh.normals, 0, bytemuck::cast_slice(normals));
            mesh.bounds = bounds_of(positions);
        }
    }

    /// Updates the vertex colours of mesh `id` (same vertex count).
    pub fn update_colors(&mut self, id: MeshId, colors: &[[f32; 4]]) {
        if let Some(mesh) = self.meshes.get(&id)
            && mesh.vertex_count == colors.len()
        {
            self.queue
                .write_buffer(&mesh.colors, 0, bytemuck::cast_slice(colors));
        }
    }

    /// Removes mesh `id`.
    pub fn remove_mesh(&mut self, id: MeshId) {
        self.meshes.remove(&id);
    }

    /// Ids of the uploaded meshes.
    pub fn mesh_ids(&self) -> impl Iterator<Item = MeshId> + '_ {
        self.meshes.keys().copied()
    }

    /// Draws `draws` seen from `camera` into a `width` x `height` image and returns its
    /// sRGB-encoded RGBA pixels, row by row from the top.
    pub fn render(
        &mut self,
        camera: &OrbitCamera,
        draws: &[Draw],
        overlays: Overlays,
        width: u32,
        height: u32,
    ) -> &[u8] {
        let (width, height) = (width.max(1), height.max(1));
        if self
            .targets
            .as_ref()
            .is_none_or(|t| t.width != width || t.height != height)
        {
            self.targets = Some(Targets::new(&self.device, width, height));
        }
        let light = self.light_view_proj(draws);
        self.write_uniforms(
            camera,
            draws,
            width as f32 / height as f32,
            light,
            overlays.shadows,
        );
        let Some(targets) = self.targets.as_ref() else {
            return &[];
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        if overlays.shadows {
            self.encode_shadow_pass(&mut encoder, draws);
        }
        self.encode_pass(&mut encoder, targets, draws, overlays);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &targets.resolve,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &targets.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(targets.padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        Self::read_back(&self.device, targets, &mut self.pixels);
        &self.pixels
    }

    /// The key light's view-projection, fitted around the shadow-casting draws.
    fn light_view_proj(&self, draws: &[Draw]) -> Mat4 {
        let (lo, hi) = draws
            .iter()
            .filter(|d| d.casts_shadow)
            .filter_map(|d| self.meshes.get(&d.mesh))
            .filter(|m| m.bounds.0.x <= m.bounds.1.x)
            .fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), m| {
                (lo.min(m.bounds.0), hi.max(m.bounds.1))
            });
        let (lo, hi) = if lo.x <= hi.x {
            (lo.min(Vec3::new(lo.x, 0.0, lo.z)), hi)
        } else {
            (Vec3::ZERO, Vec3::splat(2000.0))
        };
        let centre = (lo + hi) / 2.0;
        let radius = (hi - lo).length() / 2.0 + SHADOW_MARGIN_MM;
        let dir = KEY_LIGHT.normalize();
        let view = glam::camera::rh::view::look_at_mat4(centre + dir * radius * 2.0, centre, Vec3::Y);
        let proj = glam::camera::rh::proj::directx::orthographic(
            -radius,
            radius,
            -radius,
            radius,
            0.0,
            radius * 4.0,
        );
        proj * view
    }

    fn write_uniforms(
        &mut self,
        camera: &OrbitCamera,
        draws: &[Draw],
        aspect: f32,
        light: Mat4,
        shadows: bool,
    ) {
        let eye = camera.eye();
        let dir = KEY_LIGHT.normalize();
        let frame = FrameUniform {
            view_proj: camera.view_projection(aspect).to_cols_array_2d(),
            light_view_proj: light.to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, camera.distance],
            light: [dir.x, dir.y, dir.z, if shadows { 1.0 } else { 0.0 }],
        };
        self.queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame));
        let needed = draws.len() as u64;
        if needed > self.draw_capacity {
            self.draw_capacity = needed.next_power_of_two();
            (self.draw_buffer, self.draw_group) = Self::draw_slots(
                &self.device,
                &self.pipelines.draw_layout,
                self.draw_stride,
                self.draw_capacity,
            );
        }
        let mut bytes = vec![0u8; (self.draw_stride * needed) as usize];
        for (i, d) in draws.iter().enumerate() {
            let uniform = DrawUniform {
                color: [
                    srgb_to_linear(d.color[0]),
                    srgb_to_linear(d.color[1]),
                    srgb_to_linear(d.color[2]),
                    d.color[3],
                ],
                params: [d.highlight, d.vertex_colors, d.inside, d.specular],
                material: [
                    d.weave.code(),
                    d.weave_mm,
                    d.roughness,
                    if self.has_decal(d) { 1.0 } else { 0.0 },
                ],
                decal: d.decal.map_or([0.0; 4], |(_, rect)| rect),
            };
            let at = i * self.draw_stride as usize;
            bytes[at..at + DRAW_UNIFORM_SIZE as usize].copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        if !bytes.is_empty() {
            self.queue.write_buffer(&self.draw_buffer, 0, &bytes);
        }
    }

    /// Whether draw `d` names a decal that is uploaded.
    fn has_decal(&self, d: &Draw) -> bool {
        d.decal.is_some_and(|(id, _)| self.decals.contains_key(&id))
    }

    fn encode_shadow_pass(&self, encoder: &mut wgpu::CommandEncoder, draws: &[Draw]) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.shadow_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipelines.shadow);
        pass.set_bind_group(0, &self.light_group, &[]);
        for d in draws.iter().filter(|d| d.casts_shadow) {
            let Some(mesh) = self.meshes.get(&d.mesh) else {
                continue;
            };
            pass.set_vertex_buffer(0, mesh.positions.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
    }

    fn encode_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets,
        draws: &[Draw],
        overlays: Overlays,
    ) {
        let bg = BACKGROUND_SRGB.map(|c| f64::from(srgb_to_linear(c)));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewport"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.msaa,
                resolve_target: Some(&targets.resolve_view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: bg[0],
                        g: bg[1],
                        b: bg[2],
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, &self.frame_group, &[]);
        pass.set_pipeline(&self.pipelines.mesh);
        for (i, d) in draws.iter().enumerate() {
            let Some(mesh) = self.meshes.get(&d.mesh) else {
                continue;
            };
            pass.set_bind_group(1, &self.draw_group, &[(i as u64 * self.draw_stride) as u32]);
            let decal = d
                .decal
                .and_then(|(id, _)| self.decals.get(&id))
                .unwrap_or(&self.no_decal);
            pass.set_bind_group(2, decal, &[]);
            pass.set_vertex_buffer(0, mesh.positions.slice(..));
            pass.set_vertex_buffer(1, mesh.normals.slice(..));
            pass.set_vertex_buffer(2, mesh.colors.slice(..));
            pass.set_vertex_buffer(3, mesh.uvs.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
        if overlays.wireframe {
            pass.set_pipeline(&self.pipelines.wire);
            for d in draws {
                let Some(mesh) = self.meshes.get(&d.mesh) else {
                    continue;
                };
                pass.set_vertex_buffer(0, mesh.positions.slice(..));
                pass.set_index_buffer(mesh.edges.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.edge_count, 0, 0..1);
            }
        }
        if overlays.grid || overlays.shadows {
            pass.set_pipeline(&self.pipelines.grid);
            pass.draw(0..6, 0..1);
        }
    }

    fn read_back(device: &wgpu::Device, targets: &Targets, pixels: &mut Vec<u8>) {
        let slice = targets.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);
        let row = (targets.width * BYTES_PER_PIXEL) as usize;
        pixels.clear();
        pixels.reserve(row * targets.height as usize);
        {
            let mapped = slice.get_mapped_range();
            for y in 0..targets.height as usize {
                let start = y * targets.padded_row as usize;
                pixels.extend_from_slice(&mapped[start..start + row]);
            }
        }
        targets.readback.unmap();
    }
}

impl Pipelines {
    fn new(device: &wgpu::Device) -> Self {
        let uniform = |binding, dynamic| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: None,
            },
            count: None,
        };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
                uniform(0, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("light"),
            entries: &[uniform(0, false)],
        });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw"),
            entries: &[uniform(0, true)],
        });
        let decal_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("decal"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let mesh = Self::mesh_pipeline(device, &frame_layout, &draw_layout, &decal_layout);
        let grid = Self::grid_pipeline(device, &frame_layout);
        let shadow = Self::shadow_pipeline(device, &light_layout);
        let wire = Self::wire_pipeline(device, &frame_layout);
        Self {
            mesh,
            grid,
            shadow,
            wire,
            frame_layout,
            light_layout,
            draw_layout,
            decal_layout,
        }
    }

    fn depth_state(write: bool) -> wgpu::DepthStencilState {
        wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: write,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }
    }

    fn color_target() -> [Option<wgpu::ColorTargetState>; 1] {
        [Some(wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })]
    }

    fn mesh_pipeline(
        device: &wgpu::Device,
        frame_layout: &wgpu::BindGroupLayout,
        draw_layout: &wgpu::BindGroupLayout,
        decal_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/mesh.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[frame_layout, draw_layout, decal_layout],
            push_constant_ranges: &[],
        });
        let attribute = |location, format| wgpu::VertexAttribute {
            format,
            offset: 0,
            shader_location: location,
        };
        let positions = [attribute(0, wgpu::VertexFormat::Float32x3)];
        let normals = [attribute(1, wgpu::VertexFormat::Float32x3)];
        let colors = [attribute(2, wgpu::VertexFormat::Float32x4)];
        let uvs = [attribute(3, wgpu::VertexFormat::Float32x2)];
        let buffer = |stride: u64, attributes| wgpu::VertexBufferLayout {
            array_stride: stride,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes,
        };
        let targets = Self::color_target();
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    buffer(12, &positions),
                    buffer(12, &normals),
                    buffer(16, &colors),
                    buffer(8, &uvs),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(Self::depth_state(true)),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            multiview: None,
            cache: None,
        })
    }

    fn grid_pipeline(device: &wgpu::Device, frame_layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/grid.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid"),
            bind_group_layouts: &[frame_layout],
            push_constant_ranges: &[],
        });
        let targets = Self::color_target();
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(Self::depth_state(false)),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            multiview: None,
            cache: None,
        })
    }

    fn wire_pipeline(device: &wgpu::Device, frame_layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/wire.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wire"),
            bind_group_layouts: &[frame_layout],
            push_constant_ranges: &[],
        });
        let positions = [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        }];
        let targets = Self::color_target();
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wire"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &positions,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(Self::depth_state(false)),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            multiview: None,
            cache: None,
        })
    }

    fn shadow_pipeline(device: &wgpu::Device, light_layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/shadow.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow"),
            bind_group_layouts: &[light_layout],
            push_constant_ranges: &[],
        });
        let positions = [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        }];
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &positions,
                }],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }
}
