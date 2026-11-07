//! GPU buffers of one simulation and the bind groups that connect them to the passes.

use crate::error::GpuError;
use crate::layout::{
    CONTROL_LEN, DISPATCH_STRIDE, DispatchDesc, FrameParams, GRAB_CAP, SKIN_HASH_BUCKETS, STATE_LEN,
    TRIANGLE_HASH_BUCKETS, WORKGROUP, padded,
};
use crate::pack::Packed;
use crate::passes::Pass;
use crate::pipelines::Pipelines;

/// A storage buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Buf {
    /// Positions and inverse masses (`X`).
    X,
    /// Substep starts (`P`).
    Prev,
    /// Velocities (`V`).
    Vel,
    /// Damping, margin, friction, thickness (`PP`).
    Pp,
    /// Piece, layer, flags, sewn mask; flat position, particle distance, pressure (`PD`).
    Pd,
    /// Triangles of the active pieces (`TRI`).
    Tri,
    /// Distance constraints in colour order (`CON`).
    Con,
    /// Sewing constraints in natural order.
    Sew,
    /// Sewing constraints in colour order.
    SewColored,
    /// Body distance grid.
    Grid,
    /// Skin vertices: position and normal.
    Skin,
    /// Skin hash bucket heads.
    SkinHead,
    /// Skin hash next links.
    SkinNext,
    /// Pins.
    Pin,
    /// Grabbed particles.
    Grab,
    /// Solver state (`ST`): hold-back scales, seam gap, per-piece accelerations.
    State,
    /// Control words (`CTRL`): contact count and seam-gap sums.
    Ctrl,
    /// Fixed-point pressure normals.
    Normals,
    /// Fixed-point correction accumulators.
    Delta,
    /// Triangle hash bucket counts, then scatter cursors.
    HashCount,
    /// Triangle hash bucket starts, total, then entries.
    Hash,
    /// Prefix-sum block totals.
    BlockSums,
    /// Contacts: particle, triangle, side flags, extra gap.
    Contacts,
    /// Kinetic energy partial sums.
    Partials,
    /// Body surface velocity grid: origin and cell, dimensions, then a velocity per value.
    BodyVel,
}

impl Buf {
    /// Every buffer.
    pub const ALL: [Self; 25] = [
        Self::X,
        Self::Prev,
        Self::Vel,
        Self::Pp,
        Self::Pd,
        Self::Tri,
        Self::Con,
        Self::Sew,
        Self::SewColored,
        Self::Grid,
        Self::Skin,
        Self::SkinHead,
        Self::SkinNext,
        Self::Pin,
        Self::Grab,
        Self::State,
        Self::Ctrl,
        Self::Normals,
        Self::Delta,
        Self::HashCount,
        Self::Hash,
        Self::BlockSums,
        Self::Contacts,
        Self::Partials,
        Self::BodyVel,
    ];

    /// Name used in labels and errors.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Prev => "P",
            Self::Vel => "V",
            Self::Pp => "PP",
            Self::Pd => "PD",
            Self::Tri => "TRI",
            Self::Con => "CON",
            Self::Sew => "SW",
            Self::SewColored => "SWC",
            Self::Grid => "GRID",
            Self::Skin => "SKIN",
            Self::SkinHead => "SHEAD",
            Self::SkinNext => "SNEXT",
            Self::Pin => "PIN",
            Self::Grab => "GR",
            Self::State => "ST",
            Self::Ctrl => "CTRL",
            Self::Normals => "NRM",
            Self::Delta => "DELTA",
            Self::HashCount => "HC",
            Self::Hash => "HASH",
            Self::BlockSums => "BS",
            Self::Contacts => "CONT",
            Self::Partials => "PART",
            Self::BodyVel => "BVEL",
        }
    }
}

/// Words of the prefix-sum block totals (256 blocks, the grand total, padding).
const BLOCK_SUMS_LEN: u64 = 260;

/// Every buffer of one simulation, the frame uniform, the dispatch table and the bind groups.
#[derive(Debug)]
pub struct Buffers {
    storage: Vec<wgpu::Buffer>,
    /// The frame parameters.
    pub uniform: wgpu::Buffer,
    /// Bind group of the dispatch table (group 1, dynamic offset); it keeps the table, one
    /// descriptor per [`DISPATCH_STRIDE`] bytes, alive.
    pub dispatch_group: wgpu::BindGroup,
    /// Group 0 of every pass, indexed like [`Pass::ALL`].
    pub groups: Vec<wgpu::BindGroup>,
    /// Pins the pin buffer holds.
    pub pin_capacity: usize,
    /// Workgroups of a per-particle dispatch (energy partial sums).
    pub particle_workgroups: u32,
}

/// The bytes of `data`.
fn bytes_of<T: bytemuck::Pod>(data: &[T]) -> &[u8] {
    bytemuck::cast_slice(data)
}

struct Maker<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    limit: u64,
}

impl Maker<'_> {
    /// A storage buffer of at least `size` bytes (padded to 16) holding `data` at its start.
    fn make(&self, which: Buf, data: &[u8], size: u64) -> Result<wgpu::Buffer, GpuError> {
        let size = padded(size.max(data.len() as u64));
        if size > self.limit {
            return Err(GpuError::TooLarge {
                buffer: which.label(),
                size,
                limit: self.limit,
            });
        }
        let b = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(which.label()),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        if !data.is_empty() {
            self.queue.write_buffer(&b, 0, data);
        }
        Ok(b)
    }
}

impl Buffers {
    /// Creates and fills every buffer for `packed`.
    ///
    /// # Errors
    /// [`GpuError::TooLarge`] when a buffer exceeds the device's storage binding size.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipelines: &Pipelines,
        packed: &Packed,
        grid: &[f32],
    ) -> Result<Self, GpuError> {
        let limits = device.limits();
        let m = Maker {
            device,
            queue,
            limit: u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size),
        };
        let n = packed.x.len() as u64;
        let fp = &packed.params;
        let particle_workgroups = (n as u32).div_ceil(WORKGROUP);
        let head = if packed.skin.head.is_empty() {
            vec![-1_i32; SKIN_HASH_BUCKETS as usize]
        } else {
            packed.skin.head.clone()
        };
        let pin_capacity = packed.pins.len().max(1);
        let mut storage = Vec::with_capacity(Buf::ALL.len());
        for which in Buf::ALL {
            let b = match which {
                Buf::X => m.make(which, bytes_of(&packed.x), 0)?,
                Buf::Prev => m.make(which, bytes_of(&packed.prev), 0)?,
                Buf::Vel => m.make(which, bytes_of(&packed.vel), 0)?,
                Buf::Pp => m.make(which, bytes_of(&packed.pp), 0)?,
                Buf::Pd => m.make(which, bytes_of(&packed.pd), 0)?,
                Buf::Tri => m.make(which, bytes_of(&packed.tri), 16)?,
                Buf::Con => m.make(which, bytes_of(&packed.con), 32)?,
                Buf::Sew => m.make(which, bytes_of(&packed.sew_natural), 16)?,
                Buf::SewColored => m.make(which, bytes_of(&packed.sew_colored), 16)?,
                Buf::Grid => m.make(which, bytes_of(grid), 0)?,
                Buf::Skin => m.make(which, bytes_of(&packed.skin.rows), 32)?,
                Buf::SkinHead => m.make(which, bytes_of(&head), 0)?,
                Buf::SkinNext => m.make(which, bytes_of(&packed.skin.next), 4)?,
                Buf::Pin => m.make(which, bytes_of(&packed.pins), 16)?,
                Buf::Grab => m.make(which, &[], GRAB_CAP as u64 * 32)?,
                Buf::State => m.make(which, bytes_of(&packed.state), STATE_LEN as u64 * 4)?,
                Buf::Ctrl => m.make(which, &[], CONTROL_LEN as u64 * 4)?,
                Buf::Normals | Buf::Delta => m.make(which, &[], n * 16)?,
                Buf::HashCount => m.make(which, &[], u64::from(TRIANGLE_HASH_BUCKETS) * 4)?,
                Buf::Hash => m.make(
                    which,
                    &[],
                    (u64::from(TRIANGLE_HASH_BUCKETS) + 1 + u64::from(fp.entry_cap)) * 4,
                )?,
                Buf::BlockSums => m.make(which, &[], BLOCK_SUMS_LEN * 4)?,
                Buf::Contacts => m.make(which, &[], u64::from(fp.contact_cap) * 16)?,
                Buf::Partials => m.make(which, &[], u64::from(particle_workgroups) * 8)?,
                Buf::BodyVel => m.make(which, bytes_of(&packed.body_velocity), 0)?,
            };
            storage.push(b);
        }
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("FP"),
            size: padded(std::mem::size_of::<FrameParams>() as u64),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut table = vec![0_u8; packed.dispatch.len() * DISPATCH_STRIDE as usize];
        for (i, d) in packed.dispatch.iter().enumerate() {
            let at = i * DISPATCH_STRIDE as usize;
            table[at..at + std::mem::size_of::<DispatchDesc>()].copy_from_slice(bytemuck::bytes_of(d));
        }
        let dispatch = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("DP"),
            size: table.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&dispatch, 0, &table);
        let dispatch_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DP"),
            layout: &pipelines.dispatch_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &dispatch,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<DispatchDesc>() as u64),
                }),
            }],
        });
        let mut out = Self {
            storage,
            uniform,
            dispatch_group,
            groups: Vec::new(),
            pin_capacity,
            particle_workgroups,
        };
        out.rebind(device, pipelines);
        Ok(out)
    }

    /// The storage buffer `which`.
    #[must_use]
    pub fn get(&self, which: Buf) -> &wgpu::Buffer {
        &self.storage[which as usize]
    }

    /// Recreates every pass's bind group (after a buffer was replaced).
    pub fn rebind(&mut self, device: &wgpu::Device, pipelines: &Pipelines) {
        self.groups = Pass::ALL
            .iter()
            .map(|&pass| {
                let spec = pass.spec();
                let mut entries = vec![wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                }];
                for (i, b) in spec.binds.iter().enumerate() {
                    entries.push(wgpu::BindGroupEntry {
                        binding: i as u32 + 1,
                        resource: self.get(b.buffer).as_entire_binding(),
                    });
                }
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(spec.label),
                    layout: &pipelines.get(pass).layout,
                    entries: &entries,
                })
            })
            .collect();
    }

    /// Writes `data` at the start of buffer `which`, replacing the buffer with a larger one (and
    /// rebinding) when it does not fit.
    ///
    /// # Errors
    /// [`GpuError::TooLarge`] when the data exceeds the device's storage binding size.
    pub fn write_growing(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipelines: &Pipelines,
        which: Buf,
        data: &[u8],
    ) -> Result<(), GpuError> {
        if data.len() as u64 > self.get(which).size() {
            let limits = device.limits();
            let m = Maker {
                device,
                queue,
                limit: u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size),
            };
            self.storage[which as usize] = m.make(which, data, 0)?;
            self.rebind(device, pipelines);
        } else if !data.is_empty() {
            queue.write_buffer(self.get(which), 0, data);
        }
        Ok(())
    }

    /// Writes the pins, growing the pin buffer (and rebinding) when they no longer fit.
    pub fn write_pins(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipelines: &Pipelines,
        rows: &[[f32; 4]],
    ) {
        if rows.len() > self.pin_capacity {
            self.pin_capacity = rows.len().next_power_of_two();
            self.storage[Buf::Pin as usize] = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(Buf::Pin.label()),
                size: padded(self.pin_capacity as u64 * 16),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            self.rebind(device, pipelines);
        }
        if !rows.is_empty() {
            queue.write_buffer(self.get(Buf::Pin), 0, bytemuck::cast_slice(rows));
        }
    }
}
