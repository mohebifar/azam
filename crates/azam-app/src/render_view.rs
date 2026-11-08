//! The Render window (F12): the 3D view path-traced by `azam-trace` on a worker thread, refined
//! progressively and shown in place of the editor areas with its progress, Save and Close, as
//! Blender's Render ▸ Render Image does (rendering into a maximized area).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use azam_render::OrbitCamera;
use azam_trace::{Camera, Lighting, Renderer, Scene, Settings};
use blenderui::prelude::*;
use blenderui::{FileDialogMode, FileDialogResult, Image, file_dialog};

use crate::app::AzamApp;

/// The longer side of a render, in pixels; the other follows the viewport's shape.
pub const RENDER_WIDTH: u32 = 1280;
/// Samples per pixel of a render.
pub const RENDER_SAMPLES: u32 = 64;
/// Samples taken between preview updates (after a first quick one).
const SAMPLES_PER_UPDATE: u32 = 4;

/// One update from the render thread.
struct Progress {
    samples: u32,
    elapsed: Duration,
    image: azam_trace::Image,
}

/// A render in progress or finished.
pub struct RenderView {
    /// Width / height of the image.
    aspect: f32,
    total: u32,
    samples: u32,
    elapsed: Duration,
    image: Option<azam_trace::Image>,
    preview: Option<Arc<Image>>,
    receiver: Receiver<Progress>,
    cancel: Arc<AtomicBool>,
}

impl Drop for RenderView {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// What a finished headless render took.
#[derive(Clone, Copy, Debug)]
pub struct RenderStats {
    /// Triangles traced.
    pub triangles: usize,
    /// Seconds spent tracing samples.
    pub trace_s: f64,
    /// Seconds spent denoising and tone mapping.
    pub develop_s: f64,
}

/// The scene and camera for path-tracing `meshes` as `orbit` sees them in a `width` × `height`
/// frame, lit by the studio rig turned with the camera; `None` when there is nothing to render.
#[must_use]
pub fn render_setup(
    meshes: Vec<azam_trace::Mesh>,
    orbit: &OrbitCamera,
    width: u32,
    height: u32,
) -> Option<(Scene, Camera)> {
    let mut scene = Scene {
        meshes,
        lighting: Lighting::studio(glam::Vec3::ZERO, glam::Vec3::ONE, 0.0),
    };
    let (lo, hi) = scene.bounds()?;
    scene.lighting = Lighting::studio(lo, hi, orbit.yaw);
    let camera = Camera::from_orbit(orbit, width as f32 / height.max(1) as f32);
    Some((scene, camera))
}

impl RenderView {
    /// Starts rendering `scene` from `camera` with `settings` on a worker thread.
    #[must_use]
    pub fn start(scene: Scene, camera: Camera, settings: Settings) -> Self {
        let (tx, receiver) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            let mut renderer = Renderer::new(&scene, camera, settings);
            while !renderer.is_done() && !stop.load(Ordering::Relaxed) {
                let step = if renderer.samples_done() == 0 {
                    1
                } else {
                    SAMPLES_PER_UPDATE
                };
                renderer.step(step);
                // Previews stay raw (as Blender's); the last image is denoised.
                let image = if renderer.is_done() {
                    renderer.image()
                } else {
                    renderer.image_with(false)
                };
                let progress = Progress {
                    samples: renderer.samples_done(),
                    elapsed: start.elapsed(),
                    image,
                };
                if tx.send(progress).is_err() {
                    return;
                }
            }
        });
        Self {
            aspect: settings.width as f32 / settings.height.max(1) as f32,
            total: settings.samples,
            samples: 0,
            elapsed: Duration::ZERO,
            image: None,
            preview: None,
            receiver,
            cancel,
        }
    }

    /// Takes the updates that have arrived; returns whether the render is still running.
    pub fn poll(&mut self) -> bool {
        let mut latest = None;
        while let Ok(p) = self.receiver.try_recv() {
            latest = Some(p);
        }
        if let Some(p) = latest {
            self.samples = p.samples;
            self.elapsed = p.elapsed;
            self.preview = Image::from_rgba(p.image.width, p.image.height, &p.image.rgba).map(Arc::new);
            self.image = Some(p.image);
        }
        !self.is_done()
    }

    /// Whether every sample has been taken.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.samples >= self.total
    }

    /// The progress line: `Sample 12/64 · 3.1 s`.
    #[must_use]
    pub fn progress(&self) -> String {
        let secs = self.elapsed.as_secs_f32();
        if self.image.is_none() {
            "Preparing…".into()
        } else if self.is_done() {
            format!("Done · {} samples · {secs:.1} s", self.total)
        } else {
            format!("Sample {}/{} · {secs:.1} s", self.samples, self.total)
        }
    }

    /// The canvas showing the latest image, fitted and centred.
    fn canvas(&self) -> Div {
        let preview = self.preview.clone();
        canvas(move |b, p| {
            p.fill(b, rgb(0x1d1d1d));
            let Some(img) = &preview else { return };
            let scale = (b.size.width / img.width as f32).min(b.size.height / img.height as f32);
            let (w, h) = (img.width as f32 * scale, img.height as f32 * scale);
            let fitted = Bounds {
                origin: point(
                    b.origin.x + (b.size.width - w) / 2.0,
                    b.origin.y + (b.size.height - h) / 2.0,
                ),
                size: size(w, h),
            };
            p.image(fitted, img);
        })
        .id("render-image")
        .flex_1()
        .w_full()
    }
}

/// Resolution of Rendered shading as a share of the viewport's (it refines in place).
const LIVE_SCALE: f32 = 0.5;
/// Samples Rendered shading refines to.
const LIVE_SAMPLES: u32 = 48;

/// The viewport's path trace in Rendered shading, and what it was started for.
pub struct LiveRender {
    key: String,
    view: RenderView,
}

impl RenderView {
    /// The latest image, if one arrived.
    #[must_use]
    pub fn preview(&self) -> Option<Arc<Image>> {
        self.preview.clone()
    }
}

impl AzamApp {
    /// Rendered shading: keeps a path trace of what the viewport shows refining in its place,
    /// started again whenever the camera, the project or the drape changes; the raster frame
    /// shows while the simulation plays. Returns whether it wants another frame.
    pub(crate) fn update_live_render(&mut self) -> bool {
        if self.shading != crate::operators::Shading::Rendered || self.playing || self.render_view.is_some() {
            self.live_render = None;
            return false;
        }
        let Some(b) = self.viewport.bounds else {
            return false;
        };
        let key = format!(
            "{:?}|{}|{:?}|{}|{}x{}",
            self.viewport.camera,
            self.session.revision,
            self.sim_time(),
            self.piece_drag.is_some(),
            b.size.width.round(),
            b.size.height.round()
        );
        if self.live_render.as_ref().is_none_or(|l| l.key != key) {
            let (w, h) = (
                (b.size.width * LIVE_SCALE).round().max(8.0) as u32,
                (b.size.height * LIVE_SCALE).round().max(8.0) as u32,
            );
            let meshes = self.scene3d.trace_meshes(&self.session.project);
            self.live_render = render_setup(meshes, &self.viewport.camera, w, h).map(|(scene, camera)| {
                let settings = Settings {
                    width: w,
                    height: h,
                    samples: LIVE_SAMPLES,
                    ..Settings::default()
                };
                LiveRender {
                    key,
                    view: RenderView::start(scene, camera, settings),
                }
            });
        }
        let Some(live) = self.live_render.as_mut() else {
            return false;
        };
        let running = live.view.poll();
        if let Some(img) = live.view.preview() {
            self.viewport.show_image(img);
        }
        running || live.view.preview().is_none()
    }

    /// The viewport shading.
    #[must_use]
    pub fn shading(&self) -> crate::operators::Shading {
        self.shading
    }

    /// Switches the viewport shading.
    pub fn set_shading(&mut self, shading: crate::operators::Shading) {
        self.shading = shading;
    }

    /// Samples the Rendered shading has refined to, while it shows.
    #[must_use]
    pub fn live_render_samples(&self) -> Option<u32> {
        self.live_render.as_ref().map(|l| l.view.samples)
    }

    /// Starts a render of the 3D view (F12), replacing any render in progress.
    pub(crate) fn start_render(&mut self) {
        // The viewport's shape, or the open render's when it hides the viewport (F12 again).
        let aspect = self
            .viewport
            .bounds
            .map(|b| b.size.width / b.size.height.max(1.0))
            .or_else(|| self.render_view.as_ref().map(|v| v.aspect))
            .unwrap_or(1.6)
            .clamp(0.5, 3.0);
        // The longer side gets the full resolution.
        let (width, height) = if aspect >= 1.0 {
            (RENDER_WIDTH, (RENDER_WIDTH as f32 / aspect).round() as u32)
        } else {
            ((RENDER_WIDTH as f32 * aspect).round() as u32, RENDER_WIDTH)
        };
        let meshes = self.scene3d.trace_meshes(&self.session.project);
        match render_setup(meshes, &self.viewport.camera, width, height) {
            Some((scene, camera)) => {
                let settings = Settings {
                    width,
                    height,
                    samples: RENDER_SAMPLES,
                    ..Settings::default()
                };
                self.render_view = Some(RenderView::start(scene, camera, settings));
                self.status = format!("Rendering {width}×{height}…");
            }
            None => self.status = "Nothing to render yet".into(),
        }
    }

    /// The Render window's progress line, while it is open.
    #[must_use]
    pub fn render_progress(&self) -> Option<String> {
        self.render_view.as_ref().map(RenderView::progress)
    }

    /// Takes the render's progress; returns whether it is still running.
    pub(crate) fn poll_render(&mut self) -> bool {
        let Some(view) = self.render_view.as_mut() else {
            return false;
        };
        let was_running = !view.is_done();
        let running = view.poll();
        if was_running && !running {
            self.status = format!("Render finished: {}", view.progress());
        }
        running
    }

    /// Renders the current scene from the viewport's camera to an image, synchronously (the
    /// `--render` command line).
    ///
    /// # Errors
    /// Fails when there is nothing to render.
    pub fn render_still(
        &self,
        width: u32,
        height: u32,
        samples: u32,
    ) -> Result<(azam_trace::Image, RenderStats), String> {
        let meshes = self.scene3d.trace_meshes(&self.session.project);
        let (scene, camera) =
            render_setup(meshes, &self.viewport.camera, width, height).ok_or("nothing to render")?;
        let mut renderer = Renderer::new(
            &scene,
            camera,
            Settings {
                width,
                height,
                samples,
                ..Settings::default()
            },
        );
        let start = Instant::now();
        while !renderer.is_done() {
            renderer.step(samples);
        }
        let trace_s = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let image = renderer.image();
        let stats = RenderStats {
            triangles: renderer.triangle_count(),
            trace_s,
            develop_s: start.elapsed().as_secs_f64(),
        };
        Ok((image, stats))
    }

    /// Asks where to save the finished render and writes it as PNG.
    fn save_render(&mut self) {
        let Some(image) = self.render_view.as_ref().and_then(|v| v.image.clone()) else {
            return;
        };
        let name = azam_io::format::slug(&self.session.project.meta.name);
        let file = format!("{name}-render.png");
        let dir = self
            .session
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from));
        let path = match file_dialog(
            "Save Render",
            FileDialogMode::Save,
            dir.as_deref(),
            Some(&file),
            &["PNG\t*.png"],
            false,
        ) {
            FileDialogResult::Chosen(paths) => match paths.into_iter().next() {
                Some(p) => p,
                None => return,
            },
            FileDialogResult::Cancelled => {
                self.status = "Save cancelled".into();
                return;
            }
            FileDialogResult::Unavailable => dir.unwrap_or_default().join(file),
        };
        self.status = match image.write_png(&path) {
            Ok(()) => format!("Saved {}", path.display()),
            Err(e) => format!("Could not save {}: {e}", path.display()),
        };
    }

    /// The Render window: while a render is open it takes the place of the editor areas (as
    /// Blender's "Render in Maximized Area"), with the image, its progress, Save and Close.
    pub(crate) fn render_area(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(view) = self.render_view.as_ref() else {
            return div().class("areas");
        };
        let progress = view.progress();
        let done = view.is_done();
        let canvas = view.canvas();
        let header = div()
            .class("area-header")
            .child(div().class("editor-type").child(self.icons.icon("RENDER_RESULT")))
            .child(div().class("mode-label").child("Render Result"))
            .child(div().class("header-spacer"))
            .child(div().class("mode-label").child(progress.clone()));
        let button = |label: &'static str, class: &'static str| {
            div().class(class).css("flex", "0 0 90px").child(label)
        };
        let footer = div()
            .class("area-header")
            .css("gap", "6px")
            .child(
                div()
                    .css("flex", "1")
                    .css("padding", "0 6px")
                    .css("color", if done { "#9fe07a" } else { "#d0d0d0" })
                    .child(progress),
            )
            .child(
                button("Save", if done { "pbtn primary" } else { "pbtn" }).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.save_render();
                        cx.notify();
                    },
                )),
            )
            .child(
                button("Close", "pbtn").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.render_view = None;
                    cx.notify();
                })),
            );
        div().class("areas").child(
            div()
                .class("area")
                .id("render-window")
                .flex_1()
                .child(header)
                .child(canvas)
                .child(footer),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> azam_trace::Mesh {
        azam_trace::Mesh {
            positions: vec![[-100.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 200.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: Vec::new(),
            indices: vec![0, 1, 2],
            material: azam_trace::Material::cloth([0.5; 3]),
            decal: None,
            relief: None,
        }
    }

    #[test]
    fn nothing_to_render_without_meshes() {
        assert!(render_setup(Vec::new(), &OrbitCamera::default(), 64, 40).is_none());
        assert!(render_setup(vec![quad()], &OrbitCamera::default(), 64, 40).is_some());
    }

    #[test]
    fn a_render_runs_to_completion_on_its_thread() {
        let (scene, camera) = render_setup(vec![quad()], &OrbitCamera::default(), 32, 20).expect("a scene");
        let settings = Settings {
            width: 32,
            height: 20,
            samples: 6,
            ..Settings::default()
        };
        let mut view = RenderView::start(scene, camera, settings);
        let deadline = Instant::now() + Duration::from_secs(30);
        while view.poll() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(view.is_done());
        assert!(
            view.progress().starts_with("Done · 6 samples"),
            "{}",
            view.progress()
        );
        assert_eq!(view.image.as_ref().map(|i| (i.width, i.height)), Some((32, 20)));
    }
}
