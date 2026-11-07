//! Collisions: cloth against the body (distance grid and skin vertices), the floor, and cloth
//! against cloth (particle spheres, vertex-triangle contacts and layer order).

pub(crate) mod body_sdf;
pub(crate) mod body_tri;
pub(crate) mod layers;
pub(crate) mod particle;
pub(crate) mod triangle;

use azam_avatar::{FAR_DISTANCE_MM, SdfGrid};
use glam::DVec3;

/// Distance to the body at `p` (mm, negative inside); the gradient goes into `g`. Outside the
/// grid the distance is [`FAR_DISTANCE_MM`] and `g` keeps its previous value, as the reference
/// sampler does (callers that read `g` there see the last gradient sampled).
#[inline]
pub(crate) fn sample_into(grid: &SdfGrid, p: [f64; 3], g: &mut [f64; 3]) -> f64 {
    match grid.sample(DVec3::new(p[0], p[1], p[2])) {
        Some(s) => {
            *g = [s.gradient.x, s.gradient.y, s.gradient.z];
            s.distance
        }
        None => FAR_DISTANCE_MM,
    }
}
