//! Driving the app from a program rather than a person: the 3D view rendered off screen from any
//! camera, the simulation stepped without drawing the window, and live sessions joined by address.
//! The MCP server (`azam-mcp`) and scripts use these; the window never calls them.

use std::time::{Duration, Instant};

use azam_render::{OrbitCamera, Overlays};
use azam_sync::{Collab, Config, JoinTarget};
use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::operators::Shading;
use crate::selection::Selection;
use crate::session::Session;

/// How long [`AzamApp::prepare_scene`] waits for the avatar to be built.
const SCENE_TIMEOUT: Duration = Duration::from_secs(60);

/// What a call to [`AzamApp::simulate`] did.
#[derive(Clone, Debug, PartialEq)]
pub struct SimReport {
    /// Frames stepped.
    pub frames: usize,
    /// Simulated time at the end, in seconds.
    pub time: f64,
    /// Why the run stopped early (settled, time limit…), if it did.
    pub stopped: Option<String>,
    /// Share of the garment that is tight and that is over the fabric's limit, when known.
    pub fit: Option<(f64, f64)>,
}

/// The 3D view's renderer, or why there is none.
fn renderer_of(viewport: &mut crate::viewport::Viewport) -> Result<&mut azam_render::Renderer, String> {
    if let Some(e) = viewport.error() {
        return Err(format!("no 3D renderer: {e}"));
    }
    viewport
        .renderer()
        .ok_or_else(|| "no 3D renderer: the GPU could not be opened".to_owned())
}

impl AzamApp {
    /// The open project, to change directly (for tools; prefer [`Session::edit`] so the change can
    /// be undone).
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// The 3D view's camera.
    #[must_use]
    pub fn camera(&self) -> OrbitCamera {
        self.viewport.camera
    }

    /// Replaces the 3D view's camera.
    pub fn set_camera(&mut self, camera: OrbitCamera) {
        self.viewport.camera = camera;
    }

    /// One line on the simulation's state, as the status bar shows it.
    #[must_use]
    pub fn simulation_status(&self) -> String {
        self.sim_status()
    }

    /// Frames the pattern editor on every piece the next time it is drawn.
    pub fn reframe_pattern(&mut self) {
        self.framed = false;
    }

    /// Brings the avatar and the garment's simulation up to date with the project, waiting (up to
    /// a minute) while the avatar is built on its worker thread.
    ///
    /// # Errors
    /// Fails without a GPU renderer, or when the avatar takes too long.
    pub fn prepare_scene(&mut self) -> Result<(), String> {
        let started = Instant::now();
        loop {
            let renderer = renderer_of(&mut self.viewport)?;
            let pending = self
                .scene3d
                .sync(&self.session.project, self.session.revision, renderer);
            if !pending && !self.scene3d.building() {
                return Ok(());
            }
            if started.elapsed() > SCENE_TIMEOUT {
                return Err("the avatar is still being built; try again".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Puts the garment back in its arranged pose, before it was simulated.
    ///
    /// # Errors
    /// Fails without a GPU renderer.
    pub fn reset_simulation(&mut self) -> Result<(), String> {
        self.prepare_scene()?;
        let renderer = renderer_of(&mut self.viewport)?;
        self.scene3d.reset(renderer);
        self.playing = false;
        Ok(())
    }

    /// Steps the cloth simulation until `seconds` more of simulated time have passed (or at most
    /// `max_frames` screen frames), stopping early when the garment settles.
    ///
    /// # Errors
    /// Fails without a GPU renderer or before the avatar exists.
    pub fn simulate(&mut self, seconds: f64, max_frames: usize) -> Result<SimReport, String> {
        self.prepare_scene()?;
        self.scene3d.restart_auto_stop();
        let until = self.sim_time().unwrap_or(0.0) + seconds;
        let mut stopped = None;
        let mut done = 0;
        while done < max_frames && self.sim_time().unwrap_or(0.0) < until {
            let renderer = renderer_of(&mut self.viewport)?;
            done += 1;
            if let Some(reason) = self.scene3d.step(&self.session.project, renderer) {
                stopped = Some(format!("{reason:?}"));
                break;
            }
        }
        let renderer = renderer_of(&mut self.viewport)?;
        self.scene3d
            .update_fit(&self.session.project, Some(azam_sim::FitMode::Fit), renderer);
        Ok(SimReport {
            frames: done,
            time: self.sim_time().unwrap_or(0.0),
            stopped,
            fit: self.scene3d.fit_summary.map(|f| (f.tight, f.over_limit)),
        })
    }

    /// Renders the 3D view off screen from `camera` at `width` x `height` pixels in `shading`
    /// (Solid or Fit Map), with nothing selected. Returns sRGB RGBA pixels, top row first.
    ///
    /// # Errors
    /// Fails without a GPU renderer or before the avatar exists.
    pub fn render_view3d(
        &mut self,
        camera: &OrbitCamera,
        width: u32,
        height: u32,
        shading: Shading,
        grid: bool,
    ) -> Result<Vec<u8>, String> {
        self.prepare_scene()?;
        let shading = if shading == Shading::Rendered {
            Shading::Solid
        } else {
            shading
        };
        let renderer = renderer_of(&mut self.viewport)?;
        let fit = (shading == Shading::FitMap).then_some(azam_sim::FitMode::Fit);
        self.scene3d.update_fit(&self.session.project, fit, renderer);
        let draws = self
            .scene3d
            .draws(&self.session.project, &Selection::default(), shading);
        let overlays = Overlays {
            grid,
            ..Overlays::default()
        };
        let renderer = renderer_of(&mut self.viewport)?;
        Ok(renderer.render(camera, &draws, overlays, width, height).to_vec())
    }

    /// The bounding box of the avatar and garment in 3D, in metres, once the scene exists.
    #[must_use]
    pub fn scene_bounds(&self) -> Option<(glam::Vec3, glam::Vec3)> {
        let mut points = self.scene3d.points().peekable();
        let first = *points.peek()?;
        Some(points.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// Joins the live session at `target` (`host:port` or a share code): the session's project
    /// replaces this one and edits flow both ways from then on.
    ///
    /// # Errors
    /// Fails when the address does not resolve or the connection cannot start.
    pub fn join_session(&mut self, target: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let ui = cx.ui_sender();
        let config = Config {
            wake: Some(azam_sync::Wake::new(move || {
                ui.send(App::notify_all);
            })),
            ..Config::default()
        };
        let collab =
            JoinTarget::parse(target).and_then(|t| Collab::join(t, config).map_err(|e| e.to_string()))?;
        self.session.start_collab(collab, true);
        self.status = format!("Joining {}…", target.trim());
        Ok(())
    }

    /// Takes in collaborators' edits now, without waiting for a frame. Returns whether the
    /// project changed.
    pub fn pull_collaborators(&mut self) -> bool {
        self.session.pull_sync()
    }
}
