//! The Timeline editor, as Blender's: the recorded simulation's frames on a bar to scrub, with
//! playback, the frame shown, and the Record switch. Scrubbing and playing show recorded frames
//! in the 3D view without touching the simulation; simulating again shows its own state.

use std::time::Instant;

use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::operators::Operator;

/// Height of the frame bar, in logical pixels.
const BAR_PX: f32 = 34.0;
/// A tick on the bar every this many frames (labelled every fifth tick).
const TICK_FRAMES: usize = 10;

/// Where the frame bar was drawn (its left edge and width, window pixels), shared with the
/// paint callback.
pub type BarCell = std::sync::Arc<std::sync::Mutex<Option<(f32, f32)>>>;

/// Playback and the frame shown.
#[derive(Clone, Debug, Default)]
pub struct Timeline {
    /// The recorded frame shown.
    pub frame: usize,
    /// When playback started and at which frame, while playing.
    pub playing: Option<(Instant, usize)>,
    /// Where the bar was last drawn: its left edge and width, window pixels.
    pub bar: Option<(f32, f32)>,
    /// Whether the pointer is scrubbing the bar.
    pub scrubbing: bool,
}

impl AzamApp {
    /// The recorded frame shown in the timeline.
    #[must_use]
    pub fn timeline_frame(&self) -> usize {
        self.timeline.frame
    }

    /// Whether the recording is playing back.
    #[must_use]
    pub fn playback_running(&self) -> bool {
        self.timeline.playing.is_some()
    }

    /// Shows recorded frame `frame` (clamped to the recording).
    pub fn show_frame(&mut self, frame: usize) {
        let frames = self.scene3d.recorded_frames();
        if frames == 0 {
            return;
        }
        let frame = frame.min(frames - 1);
        self.timeline.frame = frame;
        self.playing = false;
        if let Some(renderer) = self.viewport.renderer() {
            self.scene3d.show_recorded_frame(renderer, frame);
        }
    }

    /// Starts or stops playing the recording back (from the start once it has reached the end).
    pub fn toggle_playback(&mut self) {
        let frames = self.scene3d.recorded_frames();
        if self.timeline.playing.take().is_some() || frames == 0 {
            if frames == 0 {
                self.status = "Nothing recorded yet: Record, then play the simulation (Space)".into();
            }
            return;
        }
        let from = if self.timeline.frame + 1 >= frames {
            0
        } else {
            self.timeline.frame
        };
        self.timeline.playing = Some((Instant::now(), from));
        self.playing = false;
    }

    /// Before a frame: advances playback by the time passed. `true` while it plays (keep drawing).
    pub(crate) fn advance_playback(&mut self) -> bool {
        let Some((since, from)) = self.timeline.playing else {
            return false;
        };
        let frames = self.scene3d.recorded_frames();
        let dt = self
            .scene3d
            .recording
            .as_ref()
            .map_or(1.0 / 30.0, |r| r.dt.max(1e-3));
        let frame = from + (since.elapsed().as_secs_f64() / dt) as usize;
        if frames == 0 || frame >= frames {
            self.timeline.playing = None;
            if frames > 0 {
                self.show_frame(frames - 1);
            }
            return false;
        }
        if frame != self.timeline.frame {
            self.timeline.frame = frame;
            if let Some(renderer) = self.viewport.renderer() {
                self.scene3d.show_recorded_frame(renderer, frame);
            }
        }
        true
    }

    /// The frame under window x `x` on the bar, if it has been drawn.
    fn frame_at(&self, x: f32) -> Option<usize> {
        let (left, width) = self.timeline.bar?;
        let frames = self.scene3d.recorded_frames();
        if frames == 0 || width <= 0.0 {
            return None;
        }
        let t = ((x - left) / width).clamp(0.0, 1.0);
        Some(((t * (frames - 1) as f32).round()) as usize)
    }

    /// The Timeline editor's area.
    pub(crate) fn timeline_area(&mut self, area: crate::areas::AreaId, cx: &mut Context<Self>) -> Div {
        let frames = self.scene3d.recorded_frames();
        let header = self.timeline_header(area, frames, cx);
        let current = self.timeline.frame;
        let bar_cell = std::sync::Arc::new(std::sync::Mutex::new(None));
        let shared = std::sync::Arc::clone(&bar_cell);
        self.timeline_bar_cell = Some(bar_cell);
        let bar = canvas(move |b, p| {
            if let Ok(mut c) = shared.lock() {
                *c = Some((b.origin.x + 8.0, (b.size.width - 16.0).max(1.0)));
            }
            paint_bar(b, p, frames, current);
        })
        .w_full()
        .h(px(BAR_PX));
        let body = div()
            .class("timeline-body")
            .id("timeline-bar")
            .child(bar)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    this.note_timeline_bar();
                    if let Some(f) = this.frame_at(e.position.x.0) {
                        this.timeline.scrubbing = true;
                        this.timeline.playing = None;
                        this.show_frame(f);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.timeline.scrubbing
                    && let Some(f) = this.frame_at(e.position.x.0)
                    && f != this.timeline.frame
                {
                    this.show_frame(f);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.timeline.scrubbing = false;
                    cx.notify();
                }),
            );
        div().class("area").flex_1().child(header).child(body)
    }

    /// The Timeline's header: play, Record and the frame readout.
    fn timeline_header(&mut self, area: crate::areas::AreaId, frames: usize, cx: &mut Context<Self>) -> Div {
        let recording = self.scene3d.record;
        let fps = self
            .scene3d
            .recording
            .as_ref()
            .filter(|r| r.dt > 0.0)
            .map_or(30.0, |r| 1.0 / r.dt);
        let play = div()
            .class("pbtn header-action")
            .id("timeline-play")
            .child(self.icons.icon(if self.timeline.playing.is_some() {
                "PAUSE"
            } else {
                "PLAY"
            }))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.toggle_playback();
                cx.notify();
            }));
        let record = div()
            .class(if recording {
                "pbtn header-action recording"
            } else {
                "pbtn header-action"
            })
            .id("timeline-record")
            .child(self.icons.icon("RADIOBUT_ON"))
            .child(if recording { "Recording" } else { "Record" })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.run(Operator::RecordAnimation, window, cx);
                cx.notify();
            }));
        let readout = if frames == 0 {
            "No frames recorded".to_owned()
        } else {
            format!(
                "Frame {} / {} · {:.1} s · {:.0} fps",
                self.timeline.frame + 1,
                frames,
                self.timeline.frame as f64 / fps,
                fps
            )
        };
        div()
            .class("area-header")
            .child(self.editor_type_button(area, cx))
            .child(div().class("mode-label").child("Timeline"))
            .child(play)
            .child(record)
            .child(
                div()
                    .class("timeline-readout")
                    .id("timeline-readout")
                    .child(readout),
            )
            .child(div().class("header-spacer"))
    }

    /// Takes where the bar was last drawn.
    fn note_timeline_bar(&mut self) {
        if let Some(cell) = &self.timeline_bar_cell
            && let Ok(c) = cell.lock()
            && let Some(bar) = *c
        {
            self.timeline.bar = Some(bar);
        }
    }

    /// Where the timeline bar's frame `frame` sits, window x (once drawn; for scripts and tests).
    #[must_use]
    pub fn timeline_x(&mut self, frame: usize) -> Option<f32> {
        self.note_timeline_bar();
        let (left, width) = self.timeline.bar?;
        let frames = self.scene3d.recorded_frames();
        (frames > 1).then(|| left + width * frame as f32 / (frames - 1) as f32)
    }
}

/// The frame bar: a tick every ten frames, numbers every fifty, the current frame marked.
fn paint_bar(b: Bounds<f32>, p: &mut Painter, frames: usize, current: usize) {
    let (left, width) = (b.origin.x + 8.0, (b.size.width - 16.0).max(1.0));
    let (top, h) = (b.origin.y + 4.0, b.size.height - 8.0);
    p.quad(
        Bounds {
            origin: point(left, top),
            size: size(width, h),
        },
        3.0,
        rgb(0x2b2b2b),
        rgb(0x3d3d3d),
        1.0,
    );
    if frames < 2 {
        return;
    }
    let x_of = |f: usize| left + width * f as f32 / (frames - 1) as f32;
    let mut f = 0;
    while f < frames {
        let major = f % (TICK_FRAMES * 5) == 0;
        let x = x_of(f);
        p.line(
            &[(x, top + if major { 2.0 } else { h * 0.55 }), (x, top + h - 2.0)],
            rgb(if major { 0x6a6a6a } else { 0x4a4a4a }),
            1.0,
        );
        if major {
            p.text(x + 3.0, top + 1.0, &(f + 1).to_string(), 10.0, rgb(0x9a9a9a));
        }
        f += TICK_FRAMES;
    }
    let x = x_of(current.min(frames - 1));
    p.line(&[(x, top - 2.0), (x, top + h + 2.0)], rgb(0x4772b3), 2.0);
    p.quad(
        Bounds {
            origin: point(x - 14.0, top - 3.0),
            size: size(28.0, 13.0),
        },
        3.0,
        rgb(0x4772b3),
        rgb(0x4772b3),
        0.0,
    );
    p.text(
        x - 11.0,
        top - 3.0,
        &(current + 1).to_string(),
        10.0,
        rgb(0xffffff),
    );
}
