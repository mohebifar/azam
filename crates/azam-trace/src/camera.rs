//! The render camera: a pinhole (or orthographic) camera matching the viewport's
//! [`OrbitCamera`].

use glam::Vec3;

use crate::{OrbitCamera, Ray};

/// Vertical field of view of the viewport, in degrees (Blender's 50 mm lens on a 36 mm sensor;
/// keep in sync with `azam-render`'s camera).
pub(crate) const FOV_Y_DEG: f32 = 39.6;

/// A camera generating primary rays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    /// Half the frame's height on the image plane at distance 1 (perspective) or in mm
    /// (orthographic).
    half_height: f32,
    aspect: f32,
    orthographic: bool,
    /// Distance from the eye to the orbit target, in mm.
    focus: f32,
}

impl Camera {
    /// The camera that sees what the viewport's `orbit` camera shows in a frame `aspect`
    /// (width / height) wide.
    #[must_use]
    pub fn from_orbit(orbit: &OrbitCamera, aspect: f32) -> Self {
        let half = (FOV_Y_DEG.to_radians() / 2.0).tan();
        let mut camera = Self::look_at(orbit.eye(), orbit.target, FOV_Y_DEG, aspect);
        if orbit.orthographic {
            camera.orthographic = true;
            camera.half_height = orbit.distance * half;
        }
        camera
    }

    /// A perspective camera at `eye` looking at `target` with a vertical field of view of
    /// `fov_y_deg` degrees, for a frame `aspect` (width / height) wide.
    #[must_use]
    pub fn look_at(eye: Vec3, target: Vec3, fov_y_deg: f32, aspect: f32) -> Self {
        let forward = (target - eye).normalize_or(Vec3::NEG_Z);
        let right = forward
            .cross(Vec3::Y)
            .try_normalize()
            .unwrap_or_else(|| forward.cross(Vec3::Z).normalize_or(Vec3::X));
        let up = right.cross(forward);
        Self {
            eye,
            forward,
            right,
            up,
            half_height: (fov_y_deg.to_radians() / 2.0).tan(),
            aspect: aspect.max(1e-3),
            orthographic: false,
            focus: (target - eye).length(),
        }
    }

    /// Position of the eye, in mm.
    #[must_use]
    pub const fn eye(&self) -> Vec3 {
        self.eye
    }

    /// The camera's yaw about the vertical axis (as [`OrbitCamera::yaw`]: 0 looks along −z).
    #[must_use]
    pub fn yaw(&self) -> f32 {
        (-self.forward.x).atan2(-self.forward.z)
    }

    /// The primary ray through screen position `x`, `y` in 0..1 from the top-left.
    #[must_use]
    pub fn ray(&self, x: f32, y: f32) -> Ray {
        let sx = (2.0 * x - 1.0) * self.half_height * self.aspect;
        let sy = (1.0 - 2.0 * y) * self.half_height;
        if self.orthographic {
            // Start well behind the eye so nothing between it and the target is clipped.
            let origin = self.eye + self.right * sx + self.up * sy - self.forward * self.focus * 4.0;
            Ray::new(origin, self.forward)
        } else {
            let dir = (self.forward + self.right * sx + self.up * sy).normalize();
            Ray::new(self.eye, dir)
        }
    }

    /// The width a pixel covers at distance `t` along a ray, in mm, for a frame `height` pixels
    /// tall (the footprint used to fade out weaves finer than a pixel).
    #[must_use]
    pub fn footprint(&self, t: f32, height: u32) -> f32 {
        let per_pixel = 2.0 * self.half_height / height.max(1) as f32;
        if self.orthographic {
            per_pixel
        } else {
            per_pixel * t
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_ray_hits_the_orbit_target() {
        let orbit = OrbitCamera::default();
        let cam = Camera::from_orbit(&orbit, 1.6);
        let ray = cam.ray(0.5, 0.5);
        assert!(ray.dir.dot((orbit.target - orbit.eye()).normalize()) > 0.999_99);
        assert!(
            (cam.yaw() - orbit.yaw).abs() < 1e-4,
            "{} vs {}",
            cam.yaw(),
            orbit.yaw
        );
    }

    #[test]
    fn matches_the_viewport_projection() {
        // A corner ray agrees with the viewport camera's ray through the same pixel.
        let orbit = OrbitCamera::default();
        let cam = Camera::from_orbit(&orbit, 800.0 / 500.0);
        let (_, expected) = orbit.ray(0.0, 0.0, 800.0, 500.0);
        let got = cam.ray(0.0, 0.0).dir;
        assert!(got.dot(expected) > 0.999_99, "{got} vs {expected}");
    }

    #[test]
    fn orthographic_rays_are_parallel() {
        let orbit = OrbitCamera {
            orthographic: true,
            ..OrbitCamera::default()
        };
        let cam = Camera::from_orbit(&orbit, 1.0);
        assert!((cam.ray(0.0, 0.0).dir - cam.ray(1.0, 1.0).dir).length() < 1e-6);
    }
}
