//! The 3D viewport: renders the scene offscreen and shows it as an image, with Blender's mouse
//! controls (middle drag orbits, Shift+middle pans, the wheel dollies).

use std::sync::Arc;

use azam_render::{Draw, OrbitCamera, Overlays, Renderer};
use blenderui::Image;
use blenderui::prelude::*;

/// Element id of the viewport's canvas.
pub const VIEWPORT_ID: &str = "viewport3d";

/// A mouse drag in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    /// Middle drag: orbit.
    Orbit {
        /// Last pointer position.
        last: (f32, f32),
    },
    /// Shift + middle drag: pan.
    Pan {
        /// Last pointer position.
        last: (f32, f32),
    },
    /// Left drag with the grab tool: pulling a cloth vertex.
    Grab,
}

/// The viewport's renderer, camera and last frame.
pub struct Viewport {
    renderer: Result<Renderer, String>,
    /// The camera.
    pub camera: OrbitCamera,
    /// Viewport overlays.
    pub overlays: Overlays,
    /// Where the canvas was last laid out, in logical pixels.
    pub bounds: Option<Bounds<f32>>,
    /// The drag in progress.
    pub drag: Option<Drag>,
    frame: Option<Arc<Image>>,
    /// Time the last frame took to render, in ms.
    pub frame_ms: f32,
    /// Collaborators' 3D cursors, painted over the frame.
    pub markers: Vec<CursorMarker>,
    /// Extra room kept clear on the right (an open sidebar), so the gizmo sits beside it.
    pub right_inset: f32,
}

/// A collaborator's 3D cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct CursorMarker {
    /// World position, in mm.
    pub at: glam::Vec3,
    /// Colour, `0xRRGGBB`.
    pub color: u32,
    /// The collaborator's name.
    pub name: String,
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}

impl Viewport {
    /// Starts the renderer. A missing GPU is reported in the viewport, not fatal.
    #[must_use]
    pub fn new() -> Self {
        Self {
            renderer: Renderer::new().map_err(|e| e.to_string()),
            camera: OrbitCamera::default(),
            overlays: Overlays::default(),
            bounds: None,
            drag: None,
            frame: None,
            frame_ms: 0.0,
            markers: Vec::new(),
            right_inset: 0.0,
        }
    }

    /// The renderer, if the GPU started.
    pub fn renderer(&mut self) -> Option<&mut Renderer> {
        self.renderer.as_mut().ok()
    }

    /// Why the GPU did not start, if it didn't.
    pub fn error(&self) -> Option<&str> {
        self.renderer.as_ref().err().map(String::as_str)
    }

    /// Renders `draws` at the canvas's current size and keeps the frame for painting.
    pub fn render(&mut self, draws: &[Draw], scale: f32) {
        let Some(b) = self.bounds else { return };
        let (w, h) = (
            (b.size.width * scale).round() as u32,
            (b.size.height * scale).round() as u32,
        );
        if w < 2 || h < 2 {
            return;
        }
        let Ok(renderer) = self.renderer.as_mut() else {
            return;
        };
        let start = std::time::Instant::now();
        let pixels = renderer.render(&self.camera, draws, self.overlays, w, h);
        self.frame = Image::from_rgba(w, h, pixels).map(Arc::new);
        self.frame_ms = start.elapsed().as_secs_f32() * 1000.0;
    }

    /// Shows `image` (scaled to the canvas) in place of the last rendered frame.
    pub fn show_image(&mut self, image: Arc<Image>) {
        self.frame = Some(image);
    }

    /// The canvas element showing the last frame.
    #[must_use]
    pub fn canvas(&self) -> Div {
        let frame = self.frame.clone();
        let axes = self.gizmo_axes();
        let camera = self.camera;
        let markers = self.markers.clone();
        let inset = GIZMO_INSET + self.right_inset;
        canvas(move |b, p| {
            p.fill(b, rgb(0x3d3d3d));
            if let Some(frame) = &frame {
                p.image(b, frame);
            }
            paint_markers(p, b, &camera, &markers);
            paint_gizmo(p, b, &axes, inset);
        })
        .id(VIEWPORT_ID)
        .flex_1()
        .h_full()
    }

    /// The pointer position relative to the canvas, in logical pixels.
    #[must_use]
    pub fn local(&self, position: Point<Pixels>) -> Option<(f32, f32)> {
        let b = self.bounds?;
        Some((position.x.0 - b.origin.x, position.y.0 - b.origin.y))
    }

    /// Begins a middle-button drag.
    pub fn begin_navigate(&mut self, position: Point<Pixels>, pan: bool) {
        let last = (position.x.0, position.y.0);
        self.drag = Some(if pan {
            Drag::Pan { last }
        } else {
            Drag::Orbit { last }
        });
    }

    /// Continues a navigation drag; returns `true` if the view changed.
    pub fn navigate(&mut self, position: Point<Pixels>) -> bool {
        let now = (position.x.0, position.y.0);
        let height = self.bounds.map_or(600.0, |b| b.size.height);
        match &mut self.drag {
            Some(Drag::Orbit { last }) => {
                self.camera.orbit(now.0 - last.0, now.1 - last.1);
                *last = now;
                true
            }
            Some(Drag::Pan { last }) => {
                self.camera.pan(now.0 - last.0, now.1 - last.1, height);
                *last = now;
                true
            }
            _ => false,
        }
    }

    /// The world ray under a pointer position, if it is over the viewport.
    #[must_use]
    pub fn ray(&self, position: Point<Pixels>) -> Option<(glam::Vec3, glam::Vec3)> {
        let b = self.bounds?;
        let (x, y) = self.local(position)?;
        (x >= 0.0 && y >= 0.0 && x <= b.size.width && y <= b.size.height)
            .then(|| self.camera.ray(x, y, b.size.width, b.size.height))
    }
}

/// Radius of the navigation gizmo's ring, in logical pixels.
const GIZMO_RADIUS: f32 = 38.0;
/// Radius of an axis ball, in logical pixels.
const GIZMO_BALL: f32 = 8.5;
/// Distance of the gizmo's centre from the viewport's top-right corner, in logical pixels.
const GIZMO_INSET: f32 = 52.0;

/// One ball of the navigation gizmo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GizmoAxis {
    /// Offset from the gizmo's centre, in logical pixels (y down).
    pub offset: (f32, f32),
    /// Depth towards the viewer (larger is nearer).
    pub depth: f32,
    /// Axis letter, as Blender names it (z up).
    pub label: &'static str,
    /// Axis colour.
    pub color: u32,
    /// Whether this is the positive end.
    pub positive: bool,
    /// The view a click gives.
    pub view: Option<azam_render::ViewPreset>,
}

impl Viewport {
    /// The gizmo's axes as seen from the camera, farthest first.
    ///
    /// The scene is y up; the gizmo uses Blender's names, so world y is "Z" and world −z is "Y".
    #[must_use]
    pub fn gizmo_axes(&self) -> Vec<GizmoAxis> {
        use azam_render::ViewPreset as V;
        let view = self.camera.view();
        let axes: [(glam::Vec3, &str, u32, Option<V>); 3] = [
            (glam::Vec3::X, "X", 0xf03a53, Some(V::Right)),
            (glam::Vec3::NEG_Z, "Y", 0x8acf3a, Some(V::Back)),
            (glam::Vec3::Y, "Z", 0x3d8bff, Some(V::Top)),
        ];
        let negative_view = |v: Option<V>| match v {
            Some(V::Right) => Some(V::Left),
            Some(V::Back) => Some(V::Front),
            _ => None,
        };
        let mut out = Vec::with_capacity(6);
        for (dir, label, color, preset) in axes {
            for positive in [true, false] {
                let d = if positive { dir } else { -dir };
                let v = view.transform_vector3(d);
                out.push(GizmoAxis {
                    offset: (v.x * GIZMO_RADIUS, -v.y * GIZMO_RADIUS),
                    depth: v.z,
                    label,
                    color,
                    positive,
                    view: if positive { preset } else { negative_view(preset) },
                });
            }
        }
        out.sort_by(|a, b| a.depth.total_cmp(&b.depth));
        out
    }

    /// The gizmo ball under a pointer position, if any.
    #[must_use]
    pub fn gizmo_hit(&self, position: Point<Pixels>) -> Option<GizmoAxis> {
        let b = self.bounds?;
        let centre = (
            b.origin.x + b.size.width - GIZMO_INSET - self.right_inset,
            b.origin.y + GIZMO_INSET,
        );
        let (x, y) = (position.x.0 - centre.0, position.y.0 - centre.1);
        self.gizmo_axes()
            .into_iter()
            .rev()
            .find(|a| (x - a.offset.0).hypot(y - a.offset.1) <= GIZMO_BALL + 2.0)
    }
}

/// Radius of a collaborator's cursor dot, in logical pixels.
const MARKER_RADIUS: f32 = 5.0;

/// Where world point `at` shows in canvas bounds `b`, if it is in front of the camera.
#[must_use]
pub fn project(camera: &OrbitCamera, b: Bounds<f32>, at: glam::Vec3) -> Option<(f32, f32)> {
    let clip = camera.view_projection(b.size.width / b.size.height.max(1.0)) * at.extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    (ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z)).then(|| {
        (
            b.origin.x + f32::midpoint(ndc.x, 1.0) * b.size.width,
            b.origin.y + f32::midpoint(1.0, -ndc.y) * b.size.height,
        )
    })
}

/// Collaborators' cursors: a coloured dot with the name beside it.
fn paint_markers(p: &mut blenderui::Painter, b: Bounds<f32>, camera: &OrbitCamera, markers: &[CursorMarker]) {
    for m in markers {
        let Some((x, y)) = project(camera, b, m.at) else {
            continue;
        };
        let dot = Bounds {
            origin: point(x - MARKER_RADIUS, y - MARKER_RADIUS),
            size: size(2.0 * MARKER_RADIUS, 2.0 * MARKER_RADIUS),
        };
        p.oval(dot, rgb(m.color), rgb(0xffffff), 1.5);
        p.text(x + MARKER_RADIUS + 4.0, y - 7.0, &m.name, 11.0, rgb(m.color));
    }
}

fn paint_gizmo(p: &mut blenderui::Painter, b: Bounds<f32>, axes: &[GizmoAxis], inset: f32) {
    let (cx, cy) = (b.origin.x + b.size.width - inset, b.origin.y + GIZMO_INSET);
    for a in axes {
        let (x, y) = (cx + a.offset.0, cy + a.offset.1);
        let ball = Bounds {
            origin: point(x - GIZMO_BALL, y - GIZMO_BALL),
            size: size(2.0 * GIZMO_BALL, 2.0 * GIZMO_BALL),
        };
        if a.positive {
            p.line(&[(cx, cy), (x, y)], rgb(a.color), 2.0);
            p.oval(ball, rgb(a.color), rgb(a.color), 1.0);
            p.text(x - 3.5, y - 7.0, a.label, 11.0, rgb(0x111111));
        } else {
            p.oval(ball, rgba((a.color << 8) | 0x55), rgb(a.color), 1.0);
        }
    }
}

/// Ray–triangle intersection (Möller–Trumbore): distance along the ray, if it hits.
#[must_use]
pub fn ray_triangle(
    origin: glam::Vec3,
    dir: glam::Vec3,
    a: glam::Vec3,
    b: glam::Vec3,
    c: glam::Vec3,
) -> Option<f32> {
    const EPSILON: f32 = 1e-7;
    let e1 = b - a;
    let e2 = c - a;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < EPSILON {
        return None;
    }
    let inv = 1.0 / det;
    let t_vec = origin - a;
    let u = t_vec.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t_vec.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > EPSILON).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn projected_points_match_the_pointer_ray() {
        let camera = OrbitCamera::default();
        let b = Bounds {
            origin: point(100.0, 50.0),
            size: size(800.0, 600.0),
        };
        let (x, y) = project(&camera, b, camera.target).expect("the target is in view");
        assert!(
            (x - 500.0).abs() < 0.5 && (y - 350.0).abs() < 0.5,
            "target at the centre: {x}, {y}"
        );
        let (origin, dir) = camera.ray(200.0, 150.0, 800.0, 600.0);
        let (x, y) = project(&camera, b, origin + dir * camera.distance).expect("in view");
        assert!(
            (x - 300.0).abs() < 0.5 && (y - 200.0).abs() < 0.5,
            "back where the ray started: {x}, {y}"
        );
        assert!(
            project(&camera, b, camera.eye() + (camera.eye() - camera.target)).is_none(),
            "behind"
        );
    }

    #[test]
    fn ray_hits_triangle_in_front() {
        let t = ray_triangle(
            Vec3::new(0.2, 0.2, 5.0),
            Vec3::NEG_Z,
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
        );
        assert!((t.unwrap() - 5.0).abs() < 1e-5);
    }

    #[test]
    fn ray_misses_triangle_beside_it() {
        assert!(
            ray_triangle(
                Vec3::new(2.0, 2.0, 5.0),
                Vec3::NEG_Z,
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y
            )
            .is_none()
        );
    }
}
