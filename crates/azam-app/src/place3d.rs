//! Placing pieces in 3D before draping, as in CLO: drag a piece to move it, Ctrl-drag to turn it
//! around the body, or click an arrangement point on the avatar to send the selected pieces to
//! that slot.

use blenderui::prelude::*;
use glam::Vec3;

use crate::app::AzamApp;
use crate::viewport::project;

/// Radians a piece turns per logical pixel of horizontal drag.
const TURN_PER_PX: f64 = 0.01;
/// Diameter of an arrangement point, logical pixels.
const POINT_PX: f32 = 12.0;

/// A piece being moved or turned in the 3D view.
#[derive(Clone, Copy, Debug)]
pub struct PieceDrag {
    /// Record index of the piece in the simulation.
    ri: usize,
    /// Distance from the eye to the grabbed point along the view ray.
    depth: f32,
    /// The grabbed point as of the last move (moving).
    last: Vec3,
    /// Pointer x as of the last move (turning).
    last_x: f32,
    /// Turning around the body rather than moving.
    turn: bool,
    /// Whether the piece has moved yet.
    moved: bool,
}

impl AzamApp {
    /// Starts moving (or with Ctrl, turning) the piece under the pointer, if the simulation is
    /// paused and there is one. Returns `true` if a drag started.
    pub(crate) fn begin_piece_drag(&mut self, e: &MouseDownEvent) -> bool {
        if self.playing {
            return false;
        }
        let Some((origin, dir)) = self.viewport.ray(e.position) else {
            return false;
        };
        let Some((ri, _, depth)) = self.scene3d.hit(origin, dir) else {
            return false;
        };
        self.piece_drag = Some(PieceDrag {
            ri,
            depth,
            last: origin + dir * depth,
            last_x: e.position.x.0,
            turn: e.modifiers.control,
            moved: false,
        });
        true
    }

    /// Follows the pointer with the piece being dragged; returns `true` if there is one.
    pub(crate) fn piece_drag_move(&mut self, at: Point<Pixels>) -> bool {
        let Some(mut drag) = self.piece_drag else {
            return false;
        };
        let (mut delta, mut turn) = (Vec3::ZERO, 0.0);
        if drag.turn {
            turn = f64::from(at.x.0 - drag.last_x) * TURN_PER_PX;
            drag.last_x = at.x.0;
        } else if let Some((origin, dir)) = self.viewport.ray(at) {
            let here = origin + dir * drag.depth;
            delta = here - drag.last;
            drag.last = here;
        }
        if delta != Vec3::ZERO || turn != 0.0 {
            drag.moved = true;
            if let Some(renderer) = self.viewport.renderer() {
                self.scene3d.place_piece(drag.ri, delta, turn, renderer);
            }
        }
        self.piece_drag = Some(drag);
        true
    }

    /// Shows or hides the avatar's arrangement points.
    pub fn set_show_arrangement(&mut self, show: bool) {
        self.show_arrangement = show;
    }

    /// Selects piece `id` alone.
    pub fn select_piece(&mut self, id: azam_doc::PieceId) {
        self.session.selection.select_only(id);
    }

    /// The middle of piece `id`'s cloth in the 3D view, mm.
    #[must_use]
    pub fn piece_position(&self, id: &azam_doc::PieceId) -> Option<[f32; 3]> {
        let sim = self.scene3d.sim.as_ref()?;
        let view = sim.piece_views().find(|v| v.record.id == *id)?;
        let n = (view.positions.len() / 3).max(1) as f32;
        let mut sum = [0.0f32; 3];
        for p in view.positions.chunks_exact(3) {
            for k in 0..3 {
                sum[k] += p[k];
            }
        }
        Some(sum.map(|c| c / n))
    }

    /// Ends a piece drag; returns `true` if one was in progress.
    pub(crate) fn end_piece_drag(&mut self) -> bool {
        let Some(drag) = self.piece_drag.take() else {
            return false;
        };
        if drag.moved {
            self.record_placement(drag.ri, drag.turn);
            self.status = if drag.turn {
                "Turned the piece; press Space to drape from here".into()
            } else {
                "Moved the piece; press Space to drape from here".into()
            };
        }
        true
    }

    /// Saves where piece record `ri` now sits (its 3D move and turn, as the simulation tracked
    /// them) on the piece, as one undoable edit; saved pin positions on it move along.
    fn record_placement(&mut self, ri: usize, turned_it: bool) {
        let Some(record) = self.scene3d.sim.as_ref().and_then(|s| s.records.get(ri)) else {
            return;
        };
        let (id, moved, turned) = (
            record.id.clone(),
            record.arrangement.moved,
            record.arrangement.turned,
        );
        self.session
            .edit(if turned_it { "Turn Piece" } else { "Move Piece" }, |p| {
                let Some(pc) = p.pieces.get_mut(&id) else { return };
                let as_placed = |moved: Option<[f64; 3]>, turned: Option<f64>| azam_avatar::Arrangement {
                    moved,
                    turned,
                    ..azam_avatar::Arrangement::default()
                };
                let (old, new) = (as_placed(pc.arr.moved, pc.arr.turned), as_placed(moved, turned));
                for pin in &mut pc.pins {
                    if let Some(at) = pin.at {
                        let m = old.moved.unwrap_or([0.0; 3]);
                        let unmoved = glam::DVec3::new(at[0] - m[0], at[1] - m[1], at[2] - m[2]);
                        let step = azam_avatar::Arrangement {
                            turned: Some(new.turned.unwrap_or(0.0) - old.turned.unwrap_or(0.0)),
                            moved: new.moved,
                            ..azam_avatar::Arrangement::default()
                        };
                        let q = step.place(unmoved);
                        pin.at = Some([q.x, q.y, q.z]);
                    }
                }
                pc.arr.moved = moved;
                pc.arr.turned = turned;
            });
    }

    /// The avatar's arrangement points over the 3D view, while they are shown: clicking one sends
    /// the selected pieces to its slot.
    pub(crate) fn arrangement_overlay(&self, cx: &mut Context<Self>) -> Vec<Div> {
        let mut dots = Vec::new();
        if !self.show_arrangement {
            return dots;
        }
        let (Some(avatar), Some(b)) = (self.scene3d.avatar.as_ref(), self.viewport.bounds) else {
            return dots;
        };
        let local = Bounds {
            origin: Point { x: 0.0, y: 0.0 },
            size: b.size,
        };
        let eye = self.viewport.camera.eye();
        for point in azam_avatar::arrangement_points(avatar) {
            let at = point.position.as_vec3();
            // points on the far side of the torso are hidden, as CLO hides them (limb points
            // stay: the limbs stand clear of the body)
            let outward = Vec3::new(at.x, 0.0, at.z);
            if point.h.is_some() && outward.length_squared() > 1.0 && outward.dot(eye - at) < 0.0 {
                continue;
            }
            let Some((x, y)) = project(&self.viewport.camera, local, at) else {
                continue;
            };
            let level = point.label.rsplit("· ").next().unwrap_or(&point.label).to_owned();
            let label = point.label.clone();
            let dot = div()
                .class("arr-point")
                .id(format!("arr-{}-{level}", point.slot))
                .css("left", format!("{}px", x - POINT_PX / 2.0))
                .css("top", format!("{}px", y - POINT_PX / 2.0))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.arrange_selected(point.slot, point.h, point.t);
                    cx.notify();
                }));
            dots.push(Self::with_tooltip(dot, label, cx));
        }
        dots
    }

    /// Sends the selected pieces to `slot` at height `h` or limb distance `t`, at the slot's
    /// own angle; the simulation re-places them there.
    fn arrange_selected(&mut self, slot: &'static str, h: Option<f64>, t: Option<f64>) {
        let ids: Vec<_> = self.session.selection.pieces.iter().cloned().collect();
        if ids.is_empty() {
            self.status = "Select a piece, then click an arrangement point".into();
            return;
        }
        self.session.edit("Arrange Pieces", |p| {
            for id in &ids {
                if let Some(pc) = p.pieces.get_mut(id) {
                    slot.clone_into(&mut pc.arr.slot);
                    pc.arr.h = h;
                    pc.arr.t = t;
                    pc.arr.ang = None;
                }
            }
        });
        self.status = format!(
            "Arranged {} piece(s) at {}",
            ids.len(),
            azam_avatar::slot_by_id(slot).label
        );
    }
}
