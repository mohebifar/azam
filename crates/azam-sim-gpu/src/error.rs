//! Errors of the GPU solver. Every variant means the caller should fall back to the CPU solver.

use std::fmt;

/// Why the GPU solver cannot run (or stopped running).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuError {
    /// No GPU adapter is available.
    NoAdapter,
    /// The adapter cannot run compute shaders.
    NoComputeShaders,
    /// The adapter refused to create a device.
    RequestDevice(String),
    /// The device does not provide a limit the solver needs.
    Limit {
        /// Name of the limit.
        name: &'static str,
        /// Value the solver needs.
        needed: u64,
        /// Value the device provides.
        available: u64,
    },
    /// A shader or compute pipeline failed to build.
    Pipeline {
        /// The pass whose pipeline failed.
        pass: &'static str,
        /// The validation message.
        message: String,
    },
    /// The simulation has no particles.
    EmptySimulation,
    /// The simulation passed in does not match the one that was uploaded.
    Mismatch {
        /// Particles uploaded.
        expected: usize,
        /// Particles of the simulation passed in.
        found: usize,
    },
    /// A buffer of the simulation is larger than the device allows.
    TooLarge {
        /// The buffer.
        buffer: &'static str,
        /// Its size in bytes.
        size: u64,
        /// The device's limit in bytes.
        limit: u64,
    },
    /// The device reported a validation or out-of-memory error.
    Device(String),
    /// Reading results back from the GPU failed (for example because the device was lost).
    Readback(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter => write!(f, "no GPU adapter is available"),
            Self::NoComputeShaders => write!(f, "the GPU adapter cannot run compute shaders"),
            Self::RequestDevice(m) => write!(f, "the GPU refused to create a device: {m}"),
            Self::Limit {
                name,
                needed,
                available,
            } => write!(f, "GPU limit {name} is {available}, the solver needs {needed}"),
            Self::Pipeline { pass, message } => write!(f, "GPU pass {pass} failed to build: {message}"),
            Self::EmptySimulation => write!(f, "the simulation has no particles"),
            Self::Mismatch { expected, found } => write!(
                f,
                "the simulation has {found} particles but {expected} were uploaded"
            ),
            Self::TooLarge { buffer, size, limit } => write!(
                f,
                "GPU buffer {buffer} needs {size} bytes, the device allows {limit}"
            ),
            Self::Device(m) => write!(f, "GPU error: {m}"),
            Self::Readback(m) => write!(f, "GPU readback failed: {m}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// Runs `f` inside validation and out-of-memory error scopes of `device` and turns a captured
/// error into [`GpuError::Device`] (or the error `wrap` makes of its message).
pub(crate) fn scoped<T>(
    device: &wgpu::Device,
    wrap: impl FnOnce(String) -> GpuError,
    f: impl FnOnce() -> T,
) -> Result<T, GpuError> {
    device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let out = f();
    let validation = pollster::block_on(device.pop_error_scope());
    let memory = pollster::block_on(device.pop_error_scope());
    match validation.or(memory) {
        Some(e) => Err(wrap(e.to_string())),
        None => Ok(out),
    }
}
