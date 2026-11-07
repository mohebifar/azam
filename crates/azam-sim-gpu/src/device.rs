//! The GPU device the solver runs on: limits, pipelines, and attaching simulations.

use std::sync::Arc;

use azam_avatar::{Avatar, BodyCollisionMesh, SdfGrid};
use azam_sim::Simulation;

use crate::error::GpuError;
use crate::layout::SCAN_WORKGROUP;
use crate::options::GpuOptions;
use crate::pipelines::Pipelines;
use crate::sim::GpuSim;

/// Storage buffers bound by the busiest pass (contact detection).
pub const STORAGE_BUFFERS_PER_STAGE: u32 = 8;
/// Storage binding and buffer size requested from an adapter that allows it, bytes.
const PREFERRED_BUFFER_BYTES: u64 = 1 << 28;
/// Workgroup memory of the prefix-sum and energy passes, bytes.
const WORKGROUP_STORAGE_BYTES: u32 = 1024;

/// A GPU device with the solver's pipelines built. Cheap to clone; every clone shares the
/// device and pipelines.
#[derive(Clone, Debug)]
pub struct GpuSolver {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) pipelines: Arc<Pipelines>,
    adapter: Option<wgpu::AdapterInfo>,
}

/// Checks that `limits` cover what the solver needs.
fn check_limits(limits: &wgpu::Limits) -> Result<(), GpuError> {
    let checks: [(&'static str, u64, u64); 8] = [
        (
            "max_storage_buffers_per_shader_stage",
            u64::from(STORAGE_BUFFERS_PER_STAGE),
            u64::from(limits.max_storage_buffers_per_shader_stage),
        ),
        ("max_bind_groups", 2, u64::from(limits.max_bind_groups)),
        (
            "max_dynamic_uniform_buffers_per_pipeline_layout",
            1,
            u64::from(limits.max_dynamic_uniform_buffers_per_pipeline_layout),
        ),
        (
            "max_uniform_buffers_per_shader_stage",
            2,
            u64::from(limits.max_uniform_buffers_per_shader_stage),
        ),
        (
            "max_compute_workgroup_size_x",
            u64::from(SCAN_WORKGROUP),
            u64::from(limits.max_compute_workgroup_size_x),
        ),
        (
            "max_compute_invocations_per_workgroup",
            u64::from(SCAN_WORKGROUP),
            u64::from(limits.max_compute_invocations_per_workgroup),
        ),
        (
            "max_compute_workgroup_storage_size",
            u64::from(WORKGROUP_STORAGE_BYTES),
            u64::from(limits.max_compute_workgroup_storage_size),
        ),
        (
            "min_uniform_buffer_offset_alignment (at most)",
            crate::layout::DISPATCH_STRIDE,
            if u64::from(limits.min_uniform_buffer_offset_alignment) <= crate::layout::DISPATCH_STRIDE {
                crate::layout::DISPATCH_STRIDE
            } else {
                0
            },
        ),
    ];
    for (name, needed, available) in checks {
        if available < needed {
            return Err(GpuError::Limit {
                name,
                needed,
                available,
            });
        }
    }
    Ok(())
}

impl GpuSolver {
    /// The limits the solver needs: wgpu's defaults with the largest storage buffers `adapter`
    /// allows up to 256 MiB. Request these when creating a device to share with the solver.
    #[must_use]
    pub fn required_limits(adapter: &wgpu::Limits) -> wgpu::Limits {
        let big = PREFERRED_BUFFER_BYTES.min(adapter.max_buffer_size);
        wgpu::Limits {
            max_storage_buffer_binding_size: (PREFERRED_BUFFER_BYTES as u32)
                .min(adapter.max_storage_buffer_binding_size),
            max_buffer_size: big,
            ..wgpu::Limits::default()
        }
    }

    /// Uses an existing device (for example the renderer's). Builds every pipeline now.
    ///
    /// # Errors
    /// [`GpuError::Limit`] when the device lacks a limit the solver needs, [`GpuError::Pipeline`]
    /// when a pass fails to build.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, GpuError> {
        Self::with_adapter_info(device, queue, None)
    }

    fn with_adapter_info(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter: Option<wgpu::AdapterInfo>,
    ) -> Result<Self, GpuError> {
        check_limits(&device.limits())?;
        let pipelines = Arc::new(Pipelines::new(&device)?);
        Ok(Self {
            device,
            queue,
            pipelines,
            adapter,
        })
    }

    /// Creates its own device on the default high-performance adapter (`WGPU_BACKEND` and the
    /// other wgpu environment variables are honoured).
    ///
    /// # Errors
    /// [`GpuError::NoAdapter`] without a GPU, [`GpuError::NoComputeShaders`] on adapters without
    /// compute support, [`GpuError::RequestDevice`] when the device cannot be created, and the
    /// errors of [`GpuSolver::new`].
    pub fn new_default() -> Result<Self, GpuError> {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                })
                .await
                .ok_or(GpuError::NoAdapter)?;
            if !adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
            {
                return Err(GpuError::NoComputeShaders);
            }
            let limits = Self::required_limits(&adapter.limits());
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("azam-sim-gpu"),
                        required_features: wgpu::Features::empty(),
                        required_limits: limits,
                        memory_hints: wgpu::MemoryHints::Performance,
                    },
                    None,
                )
                .await
                .map_err(|e| GpuError::RequestDevice(e.to_string()))?;
            Self::with_adapter_info(device, queue, Some(adapter.get_info()))
        })
    }

    /// The device.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// A short description of the adapter ("Apple M2 (Metal)"), when the solver created the
    /// device itself.
    #[must_use]
    pub fn adapter_label(&self) -> Option<String> {
        self.adapter
            .as_ref()
            .map(|a| format!("{} ({:?})", a.name, a.backend))
    }

    /// Uploads `sim` on `avatar` with the default contact options.
    ///
    /// # Errors
    /// See [`GpuSolver::upload_with`].
    pub fn upload(&self, sim: &Simulation, avatar: &Avatar) -> Result<GpuSim, GpuError> {
        self.upload_with(
            sim,
            &avatar.grid,
            avatar.body_collision_mesh().as_ref(),
            GpuOptions::default(),
        )
    }

    /// Uploads `sim` with the body distance grid `grid` and, for morph-target avatars, the skin
    /// `skin` (its vertices push cloth triangles off the body).
    ///
    /// # Errors
    /// [`GpuError::EmptySimulation`] without particles, [`GpuError::TooLarge`] when a buffer
    /// exceeds the device's limits, [`GpuError::Device`] when the device rejects a buffer.
    pub fn upload_with(
        &self,
        sim: &Simulation,
        grid: &SdfGrid,
        skin: Option<&BodyCollisionMesh<'_>>,
        options: GpuOptions,
    ) -> Result<GpuSim, GpuError> {
        GpuSim::new(self.clone(), sim, grid, skin, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits_are_enough() {
        assert_eq!(check_limits(&wgpu::Limits::default()), Ok(()));
        let low = wgpu::Limits {
            max_storage_buffers_per_shader_stage: 4,
            ..wgpu::Limits::default()
        };
        assert!(matches!(
            check_limits(&low),
            Err(GpuError::Limit { needed: 8, .. })
        ));
        let downlevel = wgpu::Limits::downlevel_webgl2_defaults();
        assert!(check_limits(&downlevel).is_err());
    }
}
