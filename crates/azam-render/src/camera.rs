//! Orbit camera with Blender's viewport controls: orbit around a target, pan, dolly, and the
//! numpad view presets.

use glam::camera::rh::proj::directx;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

/// Vertical field of view, in degrees (Blender's default 50 mm lens on a 36 mm sensor).
const FOV_Y_DEG: f32 = 39.6;
/// Near clip distance, in mm.
const NEAR_MM: f32 = 10.0;
/// Far clip distance, in mm.
const FAR_MM: f32 = 100_000.0;
/// Closest the camera may come to its target, in mm.
const MIN_DISTANCE_MM: f32 = 100.0;
/// Farthest the camera may move from its target, in mm.
const MAX_DISTANCE_MM: f32 = 40_000.0;
/// Pitch limit, so the view never flips over the pole, in radians.
const PITCH_LIMIT: f32 = 1.55;
/// Orbit speed, in radians per logical pixel dragged.
const ORBIT_RAD_PER_PX: f32 = 0.008;
/// Dolly factor per wheel notch.
const DOLLY_PER_NOTCH: f32 = 1.12;

/// A fixed view direction, as on Blender's numpad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewPreset {
    /// Numpad 1: looking at the avatar's front (towards -z).
    Front,
    /// Ctrl+Numpad 1: looking at the back.
    Back,
    /// Numpad 3: looking from the avatar's left side.
    Right,
    /// Ctrl+Numpad 3: looking from the other side.
    Left,
    /// Numpad 7: looking down.
    Top,
}

/// A camera orbiting `target` at `distance`, turned by `yaw` and `pitch`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    /// Point the camera looks at and orbits around, in mm.
    pub target: Vec3,
    /// Distance from the target, in mm.
    pub distance: f32,
    /// Rotation about the vertical axis, in radians; 0 looks at the avatar's front.
    pub yaw: f32,
    /// Elevation, in radians; positive looks down.
    pub pitch: f32,
    /// `true` for an orthographic projection (Numpad 5).
    pub orthographic: bool,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 1000.0, 0.0),
            distance: 3400.0,
            yaw: 0.35,
            pitch: 0.12,
            orthographic: false,
        }
    }
}

impl OrbitCamera {
    /// Position of the eye, in mm.
    #[must_use]
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(sy * cp, sp, cy * cp) * self.distance
    }

    /// World-to-view matrix.
    #[must_use]
    pub fn view(&self) -> Mat4 {
        look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    /// View-to-clip matrix for a viewport `aspect` (width / height), with wgpu's 0..1 depth.
    #[must_use]
    pub fn projection(&self, aspect: f32) -> Mat4 {
        if self.orthographic {
            let half_h = self.distance * (FOV_Y_DEG.to_radians() / 2.0).tan();
            let half_w = half_h * aspect;
            directx::orthographic(-half_w, half_w, -half_h, half_h, -FAR_MM, FAR_MM)
        } else {
            directx::perspective(FOV_Y_DEG.to_radians(), aspect, NEAR_MM, FAR_MM)
        }
    }

    /// World-to-clip matrix.
    #[must_use]
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// Orbits by a mouse drag of `dx`, `dy` logical pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * ORBIT_RAD_PER_PX;
        self.pitch = (self.pitch + dy * ORBIT_RAD_PER_PX).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Pans by a mouse drag of `dx`, `dy` logical pixels in a viewport `height` pixels tall,
    /// so the point under the cursor follows it.
    pub fn pan(&mut self, dx: f32, dy: f32, height: f32) {
        let mm_per_px = 2.0 * self.distance * (FOV_Y_DEG.to_radians() / 2.0).tan() / height.max(1.0);
        let view = self.view();
        let right = view.row(0).truncate();
        let up = view.row(1).truncate();
        self.target += (-right * dx + up * dy) * mm_per_px;
    }

    /// Dollies in (positive `notches`) or out.
    pub fn dolly(&mut self, notches: f32) {
        self.distance =
            (self.distance / DOLLY_PER_NOTCH.powf(notches)).clamp(MIN_DISTANCE_MM, MAX_DISTANCE_MM);
    }

    /// Turns to a fixed view, keeping target and distance.
    pub fn set_preset(&mut self, preset: ViewPreset) {
        use std::f32::consts::{FRAC_PI_2, PI};
        (self.yaw, self.pitch) = match preset {
            ViewPreset::Front => (0.0, 0.0),
            ViewPreset::Back => (PI, 0.0),
            ViewPreset::Right => (FRAC_PI_2, 0.0),
            ViewPreset::Left => (-FRAC_PI_2, 0.0),
            ViewPreset::Top => (0.0, PITCH_LIMIT),
        };
    }

    /// Frames a bounding box: centres on it and backs off until it fits.
    pub fn frame(&mut self, min: Vec3, max: Vec3) {
        self.target = (min + max) / 2.0;
        let radius = ((max - min).length() / 2.0).max(MIN_DISTANCE_MM);
        self.distance =
            (radius / (FOV_Y_DEG.to_radians() / 2.0).sin() * 1.1).clamp(MIN_DISTANCE_MM, MAX_DISTANCE_MM);
    }

    /// The world-space ray through a viewport pixel (`x`, `y` from the top-left, in the same units
    /// as `width` and `height`), as origin and unit direction.
    #[must_use]
    pub fn ray(&self, x: f32, y: f32, width: f32, height: f32) -> (Vec3, Vec3) {
        let ndc_x = 2.0 * x / width.max(1.0) - 1.0;
        let ndc_y = 1.0 - 2.0 * y / height.max(1.0);
        let inv = self.view_projection(width / height.max(1.0)).inverse();
        let near = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
        let far = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
        (near, (far - near).normalize_or_zero())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_view_looks_down_negative_z() {
        let mut cam = OrbitCamera::default();
        cam.set_preset(ViewPreset::Front);
        let dir = (cam.target - cam.eye()).normalize();
        assert!((dir - Vec3::NEG_Z).length() < 1e-5, "{dir}");
    }

    #[test]
    fn centre_ray_hits_the_target() {
        let cam = OrbitCamera::default();
        let (origin, dir) = cam.ray(400.0, 300.0, 800.0, 600.0);
        let to_target = (cam.target - origin).normalize();
        assert!(dir.dot(to_target) > 0.999_99);
    }

    #[test]
    fn dolly_is_clamped() {
        let mut cam = OrbitCamera::default();
        cam.dolly(1000.0);
        assert!((cam.distance - MIN_DISTANCE_MM).abs() < 1e-3);
    }
}
