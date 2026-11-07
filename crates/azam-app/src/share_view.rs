//! Live collaboration in the app: Share / Join, the join prompt, collaborators in the status bar,
//! their selected pieces on the pattern and in 3D, and their 3D cursors.
//!
//! The app does not poll the session every frame: the network thread wakes the window (through
//! a [`blenderui::UiSender`]) when it has news, and the next frame takes it in.

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use azam_doc::PieceId;
use azam_render::{Draw, MeshId};
use azam_sync::{Collab, Config, JoinTarget, UpdateLog, Wake};
use blenderui::prelude::*;
use glam::Vec3;

use crate::app::AzamApp;
use crate::garment3d::FIRST_PIECE_MESH;
use crate::operators::Operator;
use crate::viewport::{CursorMarker, ray_triangle};

/// Shortest time between two 3D cursor ray casts (and presence updates), so at most 20 Hz.
const CURSOR_INTERVAL: Duration = Duration::from_millis(50);
/// How much a collaborator's colour tints the 3D pieces they have selected.
const TINT: f32 = 0.45;

/// A wake-up for the window, callable from the network thread: it re-renders every window,
/// and the frame takes in what arrived.
fn wake(cx: &mut App) -> Wake {
    let ui = cx.ui_sender();
    Wake::new(move || {
        ui.send(App::notify_all);
    })
}

impl AzamApp {
    /// Runs the sharing operators.
    pub(crate) fn run_share(&mut self, op: Operator, window: &mut Window, cx: &mut Context<Self>) {
        match op {
            Operator::Share => self.share(cx),
            Operator::ShareFile => self.share_file(cx),
            Operator::Join => {
                self.join_prompt = Some(String::new());
                window.focus_id("join-input");
            }
            Operator::StopSharing => {
                if self.session.collab.is_some() {
                    self.session.stop_collab();
                    self.collab_marks.clear();
                    self.scene_revision = 0;
                    self.status = "Stopped sharing".into();
                } else {
                    self.status = "Not sharing".into();
                }
            }
            _ => {}
        }
    }

    /// Hosts the project as a live session with `config` (the window is woken whenever the
    /// session has news). Returns the address it accepts collaborators on.
    ///
    /// # Errors
    /// Fails if the listening socket cannot be opened.
    pub fn host_session(
        &mut self,
        mut config: Config,
        cx: &mut Context<Self>,
    ) -> Result<Option<SocketAddr>, String> {
        config.wake = Some(wake(cx));
        let collab = Collab::host(&self.session.project, None, config).map_err(|e| e.to_string())?;
        let listen = collab.status().listen;
        self.session.start_collab(collab, false);
        Ok(listen)
    }

    /// Starts hosting the project on the LAN (or shows how to join when already sharing).
    fn share(&mut self, cx: &mut Context<Self>) {
        if self.session.collab.is_none()
            && let Err(e) = self.host_session(Config::default(), cx)
        {
            self.status = format!("Could not share: {e}");
            return;
        }
        if let Some(collab) = &self.session.collab {
            let status = collab.status();
            let addr = status
                .listen
                .map(|a| {
                    format!(
                        " or {}:{}",
                        lan_ip().map_or_else(|| "localhost".into(), |ip| ip.to_string()),
                        a.port()
                    )
                })
                .unwrap_or_default();
            self.status = format!("Sharing — join with code {}{addr}", status.code);
        }
    }

    /// Shares the project through an update log next to its file.
    fn share_file(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.session.path.clone() else {
            self.status = "Save the project first: the update log goes next to its file".into();
            return;
        };
        let config = Config {
            listen: None,
            discovery: false,
            wake: Some(wake(cx)),
            ..Config::default()
        };
        match Collab::open_log(&path, &self.session.project, config) {
            Ok((collab, adopted)) => {
                self.session.start_collab(collab, false);
                if let Some(project) = adopted {
                    self.session.edit("Shared edits", move |p| *p = project);
                }
                self.status = format!("Sharing through {}", UpdateLog::path_for(&path).display());
            }
            Err(e) => self.status = format!("Could not share: {e}"),
        }
    }

    /// Joins the session typed in the join prompt.
    pub(crate) fn confirm_join(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.join_prompt.take() else {
            return;
        };
        window.focus_id("root");
        let config = Config {
            wake: Some(wake(cx)),
            ..Config::default()
        };
        let joined = JoinTarget::parse(&text)
            .and_then(|target| Collab::join(target, config).map_err(|e| e.to_string()));
        match joined {
            Ok(collab) => {
                self.session.start_collab(collab, true);
                self.status = format!("Joining {}…", text.trim());
            }
            Err(e) => self.status = e,
        }
    }

    /// The join prompt.
    pub(crate) fn join_popup(&mut self, text: &str, cx: &mut Context<Self>) -> Div {
        div()
            .class("search")
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .child(
                div()
                    .class("prop-title")
                    .child(self.icons.icon("LINKED"))
                    .child("Join Session"),
            )
            .child(
                input()
                    .id("join-input")
                    .attr("autofocus", "")
                    .attr("live", "")
                    .placeholder("host:port or share code")
                    .value(text.to_owned())
                    .w_full()
                    .on_input(cx.listener(|this, e: &ChangeEvent, _, cx| {
                        if let Some(t) = &mut this.join_prompt {
                            t.clone_from(&e.value);
                        }
                        cx.notify();
                    })),
            )
            .child(div().class("pbuttons").mt(px(6.0)).child(
                div().class("pbtn primary").child("Join").on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| {
                        this.confirm_join(window, cx);
                        cx.notify();
                    },
                )),
            ))
    }

    /// Before a frame: takes in collaborators' changes and shares this user's selection.
    pub(crate) fn sync_collab(&mut self) {
        let Some(collab) = &mut self.session.collab else {
            self.viewport.markers.clear();
            return;
        };
        let mut presence = collab.presence().clone();
        presence.piece = self.session.selection.active.as_ref().map(|p| p.0.clone());
        collab.set_presence(presence);
        let joined = !collab.has_document();
        if self.session.pull_sync() && joined {
            self.framed = false;
            self.status = "Joined the session".into();
        }
        let marks: Vec<(PieceId, u32)> = self
            .session
            .collab
            .iter()
            .flat_map(Collab::peers)
            .filter_map(|p| Some((PieceId::new(p.presence.piece.clone()?), p.presence.rgb())))
            .collect();
        if marks != self.collab_marks {
            self.collab_marks = marks;
            self.scene_revision = 0;
        }
        self.viewport.markers = self
            .session
            .collab
            .iter()
            .flat_map(Collab::peers)
            .filter_map(|p| {
                Some(CursorMarker {
                    at: Vec3::from_array(p.presence.cursor?),
                    color: p.presence.rgb(),
                    name: p.presence.name.clone(),
                })
            })
            .collect();
    }

    /// Shares where the pointer meets the garment or avatar in the 3D view (at most 20 times a
    /// second), or that it is off them.
    pub(crate) fn publish_cursor(&mut self, position: Point<Pixels>) {
        if self.session.collab.is_none() || self.cursor_cast.is_some_and(|t| t.elapsed() < CURSOR_INTERVAL) {
            return;
        }
        self.cursor_cast = Some(Instant::now());
        let at = self
            .viewport
            .ray(position)
            .and_then(|(origin, dir)| self.surface_hit(origin, dir));
        if let Some(collab) = &mut self.session.collab {
            let mut presence = collab.presence().clone();
            presence.cursor = at.map(|v| v.to_array());
            collab.set_presence(presence);
        }
    }

    /// The nearest point where a ray meets the garment or the shown avatar.
    fn surface_hit(&self, origin: Vec3, dir: Vec3) -> Option<Vec3> {
        let mut best = self.scene3d.hit(origin, dir).map(|(_, _, d)| d);
        if self.session.project.avatar.show
            && let Some(body) = self.scene3d.body()
        {
            let p = &body.mesh.positions;
            let v = |i: u32| {
                let k = i as usize * 3;
                Vec3::new(p[k], p[k + 1], p[k + 2])
            };
            for t in body.mesh.indices.chunks_exact(3) {
                if let Some(d) = ray_triangle(origin, dir, v(t[0]), v(t[1]), v(t[2]))
                    && best.is_none_or(|b| d < b)
                {
                    best = Some(d);
                }
            }
        }
        best.map(|d| origin + dir * d)
    }

    /// Tints the 3D pieces collaborators have selected with their colours.
    pub(crate) fn tint_collaborator_pieces(&self, draws: &mut [Draw]) {
        let Some(sim) = &self.scene3d.sim else {
            return;
        };
        for (id, color) in &self.collab_marks {
            let Some(ri) = sim.records.iter().position(|r| &r.id == id) else {
                continue;
            };
            let mesh = MeshId(FIRST_PIECE_MESH + ri as u32);
            let rgb = [(color >> 16) & 0xff, (color >> 8) & 0xff, color & 0xff].map(|c| c as f32 / 255.0);
            for draw in draws.iter_mut().filter(|d| d.mesh == mesh) {
                for (c, t) in draw.color.iter_mut().zip(rgb) {
                    *c = *c * (1.0 - TINT) + t * TINT;
                }
            }
        }
    }

    /// The status-bar line on the session, while one runs.
    pub(crate) fn collab_status(&self) -> Option<String> {
        let collab = self.session.collab.as_ref()?;
        let status = collab.status();
        if !collab.has_document() {
            return Some("Joining…".into());
        }
        let n = collab.peers().len();
        let who = match n {
            0 if status.log.is_some() => "shared via file".to_owned(),
            0 => format!("sharing {} · no collaborators", status.code),
            1 => "1 collaborator".to_owned(),
            n => format!("{n} collaborators"),
        };
        Some(if status.online {
            who
        } else {
            format!("{who} · offline")
        })
    }
}

/// This machine's address on the LAN (no packet is sent).
fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    Some(socket.local_addr().ok()?.ip())
}
