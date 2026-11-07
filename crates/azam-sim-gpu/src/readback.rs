//! Reading a frame's results back: positions, solver state and energy partial sums, copied into
//! one staging buffer at the end of the frame and mapped once the GPU is done.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::buffers::{Buf, Buffers};
use crate::error::GpuError;
use crate::layout::{STATE_LEN, padded};

/// What a frame leaves for the CPU.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Results {
    /// Positions, `x, y, z` per particle; `None` when any is NaN.
    pub positions: Option<Vec<f32>>,
    /// The solver state (`ST`).
    pub state: Vec<f32>,
    /// Sum of squared speeds of the free particles, mm²/s².
    pub energy_sum: f64,
    /// Number of free particles.
    pub energy_count: f64,
}

/// The staging buffer and the pending map request.
#[derive(Debug)]
pub struct Readback {
    buffer: wgpu::Buffer,
    x_bytes: u64,
    state_bytes: u64,
    partial_bytes: u64,
    pending: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
}

impl Readback {
    /// A staging buffer for `particles` particles and `workgroups` energy partial sums.
    pub fn new(device: &wgpu::Device, particles: usize, workgroups: u32) -> Self {
        let x_bytes = particles as u64 * 16;
        let state_bytes = STATE_LEN as u64 * 4;
        let partial_bytes = u64::from(workgroups) * 8;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("azam-sim-gpu readback"),
            size: padded(x_bytes + state_bytes + partial_bytes),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            x_bytes,
            state_bytes,
            partial_bytes,
            pending: None,
        }
    }

    /// Records the copies of the frame's results into the staging buffer.
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, buffers: &Buffers) {
        encoder.copy_buffer_to_buffer(buffers.get(Buf::X), 0, &self.buffer, 0, self.x_bytes);
        encoder.copy_buffer_to_buffer(
            buffers.get(Buf::State),
            0,
            &self.buffer,
            self.x_bytes,
            self.state_bytes,
        );
        encoder.copy_buffer_to_buffer(
            buffers.get(Buf::Partials),
            0,
            &self.buffer,
            self.x_bytes + self.state_bytes,
            self.partial_bytes,
        );
    }

    /// Requests the map (call after the frame's submission).
    pub fn request(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.pending = Some(rx);
    }

    /// The frame's results once mapped: waits for the GPU when `wait`, otherwise returns
    /// `Ok(None)` while it is still busy.
    ///
    /// # Errors
    /// [`GpuError::Readback`] when the map fails (for example on a lost device).
    pub fn poll(&mut self, device: &wgpu::Device, wait: bool) -> Result<Option<Results>, GpuError> {
        let Some(rx) = &self.pending else {
            return Ok(None);
        };
        device.poll(if wait {
            wgpu::Maintain::Wait
        } else {
            wgpu::Maintain::Poll
        });
        let mapped = if wait {
            rx.recv().map_err(|e| GpuError::Readback(e.to_string()))?
        } else {
            match rx.try_recv() {
                Ok(r) => r,
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    return Err(GpuError::Readback("map request dropped".into()));
                }
            }
        };
        self.pending = None;
        mapped.map_err(|e| GpuError::Readback(e.to_string()))?;
        let results = {
            let view = self.buffer.slice(..).get_mapped_range();
            self.decode(&view)
        };
        self.buffer.unmap();
        Ok(Some(results))
    }

    fn decode(&self, bytes: &[u8]) -> Results {
        let words = |from: u64, len: u64| -> Vec<f32> {
            bytes[from as usize..(from + len) as usize]
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        };
        let x = words(0, self.x_bytes);
        let mut positions = Vec::with_capacity(x.len() / 4 * 3);
        let mut bad = false;
        for row in x.chunks_exact(4) {
            if row[..3].iter().any(|v| v.is_nan()) {
                bad = true;
                break;
            }
            positions.extend_from_slice(&row[..3]);
        }
        let state = words(self.x_bytes, self.state_bytes);
        let partials = words(self.x_bytes + self.state_bytes, self.partial_bytes);
        let (mut e, mut cnt) = (0.0, 0.0);
        for p in partials.chunks_exact(2) {
            e += f64::from(p[0]);
            cnt += f64::from(p[1]);
        }
        Results {
            positions: (!bad).then_some(positions),
            state,
            energy_sum: e,
            energy_count: cnt,
        }
    }
}
