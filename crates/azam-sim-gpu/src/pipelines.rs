//! Compute pipelines and bind group layouts of every pass, built once per device.

use std::borrow::Cow;

use crate::error::{GpuError, scoped};
use crate::layout::DispatchDesc;
use crate::passes::Pass;

/// One pass's pipeline.
#[derive(Debug)]
pub struct PassPipeline {
    /// The pipeline.
    pub pipeline: wgpu::ComputePipeline,
    /// Layout of its group 0 (frame uniform plus storage bindings).
    pub layout: wgpu::BindGroupLayout,
    /// Workgroup size along x.
    pub workgroup: u32,
}

/// Pipelines of every pass.
#[derive(Debug)]
pub struct Pipelines {
    passes: Vec<PassPipeline>,
    /// Layout of group 1: the dispatch descriptor uniform with a dynamic offset.
    pub dispatch_layout: wgpu::BindGroupLayout,
}

fn buffer_entry(
    binding: u32,
    ty: wgpu::BufferBindingType,
    dynamic: bool,
    min: Option<u64>,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: dynamic,
            min_binding_size: min.and_then(wgpu::BufferSize::new),
        },
        count: None,
    }
}

impl Pipelines {
    /// Builds every pass's shader module and pipeline on `device`.
    ///
    /// # Errors
    /// [`GpuError::Pipeline`] names the first pass whose shader or pipeline fails validation.
    pub fn new(device: &wgpu::Device) -> Result<Self, GpuError> {
        let dispatch_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("azam-sim-gpu dispatch"),
            entries: &[buffer_entry(
                0,
                wgpu::BufferBindingType::Uniform,
                true,
                Some(std::mem::size_of::<DispatchDesc>() as u64),
            )],
        });
        let mut passes = Vec::with_capacity(Pass::ALL.len());
        for pass in Pass::ALL {
            let spec = pass.spec();
            let built = scoped(
                device,
                |message| GpuError::Pipeline {
                    pass: spec.label,
                    message,
                },
                || Self::build(device, pass, &dispatch_layout),
            )?;
            passes.push(built);
        }
        Ok(Self {
            passes,
            dispatch_layout,
        })
    }

    fn build(device: &wgpu::Device, pass: Pass, dispatch_layout: &wgpu::BindGroupLayout) -> PassPipeline {
        let spec = pass.spec();
        let mut entries = vec![buffer_entry(0, wgpu::BufferBindingType::Uniform, false, None)];
        for (i, b) in spec.binds.iter().enumerate() {
            entries.push(buffer_entry(
                i as u32 + 1,
                wgpu::BufferBindingType::Storage { read_only: !b.write },
                false,
                None,
            ));
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(spec.label),
            entries: &entries,
        });
        let mut groups = vec![&layout];
        if spec.includes.dispatch {
            groups.push(dispatch_layout);
        }
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(spec.label),
            bind_group_layouts: &groups,
            push_constant_ranges: &[],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(spec.label),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(pass.source())),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(spec.label),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        PassPipeline {
            pipeline,
            layout,
            workgroup: spec.workgroup,
        }
    }

    /// The pipeline of `pass`.
    #[must_use]
    pub fn get(&self, pass: Pass) -> &PassPipeline {
        &self.passes[pass.index()]
    }
}
