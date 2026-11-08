//! Offline path tracer for presentation renders of the draped garment, like Blender's F12
//! "Render Image".
//!
//! The input is a [`Scene`] independent of the app: triangle meshes with per-vertex normals and
//! pattern coordinates, each with a cloth [`Material`], lit by a studio [`Lighting`] rig (soft key,
//! fill and rim area lights, a sky gradient, a shadow-catching floor in front of a grey backdrop).
//! A [`Renderer`] traces it progressively — call [`Renderer::step`] until [`Renderer::is_done`],
//! showing [`Renderer::image`] in between — with all the work spread over tiles on `rayon`'s pool.
//!
//! The light transport is a unidirectional path tracer over a SAH bounding volume hierarchy:
//! next-event estimation of the area lights combined with BSDF sampling by multiple importance
//! sampling, Russian roulette after the first bounces, a cloth BSDF (Lambert, a Charlie sheen lobe
//! and GGX specular), procedural weaves matching the viewport's, and an ACES-like filmic tone
//! curve. An edge-avoiding À-trous filter guided by albedo, normal and depth cleans up the noise.
//!
//! Every random number comes from a hash of the seed, the pixel and the sample index, so renders
//! are deterministic whatever the thread count.
//!
//! Units are millimetres; y is up and the floor is the plane y = 0, as in `azam-render`.

mod bsdf;
mod bvh;
mod camera;
mod denoise;
mod integrator;
mod renderer;
mod rng;
mod scene;
mod tonemap;
mod weave;

pub use bvh::{Bvh, Hit, Ray};
pub use camera::Camera;
pub use renderer::{Image, Renderer, Settings};
pub use rng::Rng;
pub use scene::{AreaLight, Backdrop, Decal, Floor, Lighting, Material, Mesh, Relief, Scene, Sky};
pub use tonemap::{aces_filmic, linear_to_srgb};

pub use azam_render::{OrbitCamera, Weave};
