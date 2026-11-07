//! Offscreen 3D renderer for azam's viewport.
//!
//! The viewport is drawn with wgpu into a multisampled texture and read back as RGBA pixels, which
//! the UI shows in its 3D area. Meshes (avatar, garment pieces) are uploaded once and their
//! positions updated every simulation frame.
//!
//! Units are millimetres; 3D is y up, the avatar faces +z and stands on y = 0.

mod camera;
mod mesh;
mod renderer;

pub use camera::{OrbitCamera, ViewPreset};
pub use mesh::{MeshData, MeshId};
pub use renderer::{DecalId, Draw, Overlays, RenderError, Renderer, Weave, srgb_to_linear};
