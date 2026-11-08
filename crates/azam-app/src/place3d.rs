//! Placing pieces in 3D before draping, as in CLO: drag a piece to move it, Ctrl-drag to turn it
//! around the body, or click an arrangement point on the avatar to send the selected pieces to
//! that slot. As in Blender, G and R over the 3D view move or turn the piece under the pointer
//! (or the selected one) without holding a button: X, Y or Z locks the move to an axis, a typed
//! number sets it exactly (mm, or degrees for a turn), a click or Enter keeps it and Esc or a
//! right-click puts the piece back.

use blenderui::prelude::*;
use glam::Vec3;

use crate::app::AzamApp;
use crate::viewport::project;

/// Radians a piece turns per logical pixel of horizontal drag.
const TURN_PER_PX: f64 = 0.01;
/// Diameter of an arrangement point, logical pixels.
const POINT_PX: f32 = 12.0;

/// A piece being moved or turned in the 3D view.
#[derive(Clone, Debug)]
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
    /// A modal G or R (no button held): ended by a click or Enter, undone by Esc.
    modal: bool,
    /// How far the piece has moved and turned since the drag began.
    total: Vec3,
    total_turn: f64,
    /// The axis a modal move is locked to (0 x, 1 y up, 2 z).
    axis: Option<usize>,
    /// A number typed during a modal move or turn.
    typed: String,
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
            modal: false,
            total: Vec3::ZERO,
            total_turn: 0.0,
            axis: None,
            typed: String::new(),
        });
        true
    }

    /// Starts a modal move (or `turn`) of the piece under the pointer at `at` in the 3D view, or
    /// of the selected piece. Returns `true` if one started.
    pub(crate) fn begin_piece_modal(&mut self, at: Point<Pixels>, turn: bool) -> bool {
        if self.playing || self.piece_drag.is_some() {
            return false;
        }
        let Some((origin, dir)) = self.viewport.ray(at) else {
            return false;
        };
        let hit = self.scene3d.hit(origin, dir).map(|(ri, _, depth)| (ri, depth));
        let selected = || {
            let id = self.session.selection.active.clone()?;
            let ri = self
                .scene3d
                .sim
                .as_ref()?
                .records
                .iter()
                .position(|r| r.id == id)?;
            let centre = Vec3::from(self.piece_position(&id)?);
            Some((ri, (centre - origin).dot(dir).max(1.0)))
        };
        let Some((ri, depth)) = hit.or_else(selected) else {
            return false;
        };
        self.piece_drag = Some(PieceDrag {
            ri,
            depth,
            last: origin + dir * depth,
            last_x: at.x.0,
            turn,
            moved: false,
            modal: true,
            total: Vec3::ZERO,
            total_turn: 0.0,
            axis: None,
            typed: String::new(),
        });
        self.status = if turn {
            "Turn around the body: type degrees · click or Enter keeps · Esc cancels".into()
        } else {
            "Move: X/Y/Z lock an axis · type mm · click or Enter keeps · Esc cancels".into()
        };
        true
    }

    /// Whether a modal 3D move or turn is running.
    #[must_use]
    pub fn piece_modal(&self) -> bool {
        self.piece_drag.as_ref().is_some_and(|d| d.modal)
    }

    /// Takes a key during a modal 3D move or turn; returns `true` if one is running.
    pub(crate) fn piece_modal_key(&mut self, key: &str, ch: Option<char>) -> bool {
        let Some(mut drag) = self.piece_drag.clone().filter(|d| d.modal) else {
            return false;
        };
        match key {
            "escape" => self.cancel_piece_modal(),
            "return" | "kp_enter" => {
                self.end_piece_drag();
            }
            "x" | "y" | "z" if !drag.turn => {
                let axis = usize::from(key == "y") + 2 * usize::from(key == "z");
                drag.axis = if drag.axis == Some(axis) { None } else { Some(axis) };
                let axis = drag.axis;
                self.piece_drag = Some(drag);
                self.status = match axis {
                    Some(a) => format!("Locked to {}", ["X", "Y", "Z"][a]),
                    None => "Free".into(),
                };
                self.reapply_typed_3d();
            }
            k => {
                match ch {
                    _ if k == "backspace" => {
                        drag.typed.pop();
                    }
                    Some('-') => {
                        if drag.typed.starts_with('-') {
                            drag.typed.remove(0);
                        } else {
                            drag.typed.insert(0, '-');
                        }
                    }
                    Some('.') if !drag.typed.contains('.') => drag.typed.push('.'),
                    Some(c) if c.is_ascii_digit() => drag.typed.push(c),
                    _ => return true,
                }
                self.piece_drag = Some(drag);
                self.reapply_typed_3d();
            }
        }
        true
    }

    /// Puts the piece of the modal drag back where it began and, if a number is typed, moves or
    /// turns it by exactly that.
    fn reapply_typed_3d(&mut self) {
        let Some(mut drag) = self.piece_drag.clone() else {
            return;
        };
        if drag.typed.is_empty() {
            return;
        }
        let value = drag.typed.trim_end_matches('.').parse::<f64>().unwrap_or(0.0);
        let Some(renderer) = self.viewport.renderer() else {
            return;
        };
        // undo in reverse order: the turn, then the move
        self.scene3d
            .place_piece(drag.ri, Vec3::ZERO, -drag.total_turn, renderer);
        self.scene3d.place_piece(drag.ri, -drag.total, 0.0, renderer);
        if drag.turn {
            drag.total = Vec3::ZERO;
            drag.total_turn = value.to_radians();
            self.scene3d
                .place_piece(drag.ri, Vec3::ZERO, drag.total_turn, renderer);
            self.status = format!("Turn {}°  (Enter keeps, Esc cancels)", drag.typed);
        } else {
            let mut d = Vec3::ZERO;
            d[drag.axis.unwrap_or(0)] = value as f32;
            drag.total = d;
            drag.total_turn = 0.0;
            self.scene3d.place_piece(drag.ri, d, 0.0, renderer);
            self.status = format!(
                "Move {}: {} mm  (Enter keeps, Esc cancels)",
                ["X", "Y", "Z"][drag.axis.unwrap_or(0)],
                drag.typed
            );
        }
        drag.moved = true;
        self.piece_drag = Some(drag);
    }

    /// Ends a modal 3D move or turn, putting the piece back where it began.
    pub(crate) fn cancel_piece_modal(&mut self) {
        let Some(drag) = self.piece_drag.take() else {
            return;
        };
        if let Some(renderer) = self.viewport.renderer() {
            self.scene3d
                .place_piece(drag.ri, Vec3::ZERO, -drag.total_turn, renderer);
            self.scene3d.place_piece(drag.ri, -drag.total, 0.0, renderer);
        }
        self.status = "Cancelled".into();
    }

    /// Follows the pointer with the piece being dragged; returns `true` if there is one.
    pub(crate) fn piece_drag_move(&mut self, at: Point<Pixels>) -> bool {
        let Some(mut drag) = self.piece_drag.clone() else {
            return false;
        };
        if drag.modal && !drag.typed.is_empty() {
            // a typed number holds the piece; the pointer does not move it
            return true;
        }
        let (mut delta, mut turn) = (Vec3::ZERO, 0.0);
        if drag.turn {
            turn = f64::from(at.x.0 - drag.last_x) * TURN_PER_PX;
            drag.last_x = at.x.0;
        } else if let Some((origin, dir)) = self.viewport.ray(at) {
            let here = origin + dir * drag.depth;
            delta = here - drag.last;
            drag.last = here;
            if let Some(a) = drag.axis {
                let keep = delta[a];
                delta = Vec3::ZERO;
                delta[a] = keep;
            }
        }
        drag.total += delta;
        drag.total_turn += turn;
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
                if let Some(pc) = p.pieces.get_mut(&id) {
                    set_placement(pc, moved, turned);
                }
            });
    }

    /// The avatar's arrangement points over the 3D view, while they are shown: clicking one sends
    /// the selected pieces to its slot.
    pub(crate) fn arrangement_overlay(&self, cx: &mut Context<Self>) -> Vec<Div> {
        let mut dots = Vec::new();
        // the points sit on the resting avatar: hidden while a motion moves it
        if !self.show_arrangement || self.scene3d.motion.motion() != azam_avatar::Motion::None {
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

/// Gives `pc` the 3D placement `moved` / `turned`; its saved pin positions move along, so pins
/// keep holding the same spot of the cloth.
pub(crate) fn set_placement(pc: &mut azam_doc::Piece, moved: Option<[f64; 3]>, turned: Option<f64>) {
    let (old_moved, old_turned) = (pc.arr.moved.unwrap_or([0.0; 3]), pc.arr.turned.unwrap_or(0.0));
    let step = azam_avatar::Arrangement {
        turned: Some(turned.unwrap_or(0.0) - old_turned),
        moved,
        ..azam_avatar::Arrangement::default()
    };
    for pin in &mut pc.pins {
        if let Some(at) = pin.at {
            let unmoved = glam::DVec3::new(at[0] - old_moved[0], at[1] - old_moved[1], at[2] - old_moved[2]);
            let q = step.place(unmoved);
            pin.at = Some([q.x, q.y, q.z]);
        }
    }
    pc.arr.moved = moved;
    pc.arr.turned = turned;
}

/// Turning less than this at a boundary sample, in degrees, drops it from a flattened outline.
const FLAT_KEEP_TURN_DEG: f64 = 4.0;

impl AzamApp {
    /// Lays the active piece's drape flat as a new piece beside the pattern (unsewn, same
    /// fabric and grain): the pattern the drape implies, to compare with the one drafted.
    pub fn flatten_active_piece(&mut self) {
        let Some(id) = self.session.selection.active.clone() else {
            self.status = "Select a piece to flatten".into();
            return;
        };
        let Some(sim) = self.scene3d.sim.as_ref() else {
            self.status = "There is no drape to flatten yet".into();
            return;
        };
        let Some(view) = sim.piece_views().find(|v| v.record.id == id) else {
            self.status = "That piece is not simulated".into();
            return;
        };
        let flat = azam_sim::flatten_piece(&view);
        let ring: Vec<azam_geom::Vec2> = view
            .record
            .mesh
            .boundary
            .iter()
            .map(|&v| flat[v as usize])
            .collect();
        let outline = simplify_ring(&ring, FLAT_KEEP_TURN_DEG);
        let project = &self.session.project;
        let Some(src) = project.pieces.get(&id).cloned() else {
            return;
        };
        // beside everything on the board
        let right = project
            .pieces
            .values()
            .flat_map(|pc| pc.pts.iter().map(|p| p.pos.x))
            .fold(f64::MIN, f64::max);
        let left = outline.iter().map(|p| p.x).fold(f64::MAX, f64::min);
        let shift = azam_geom::Vec2::new(right + 100.0 - left, 0.0);
        let points: Vec<(azam_geom::Vec2, bool)> = outline.iter().map(|p| (*p + shift, false)).collect();
        let mut made = None;
        self.session.edit("Flatten Piece", |p| {
            made = azam_doc::create_piece(p, &points);
            if let Some(pc) = made.as_ref().and_then(|m| p.pieces.get_mut(m)) {
                pc.name = format!("{} (flattened)", src.name);
                // a pattern to compare, not cloth to drape over the one it came from
                pc.state = azam_doc::PieceState::Off;
                pc.fabric = src.fabric.clone();
                pc.grain = src.grain;
                pc.sa = src.sa;
            }
        });
        let area = |pts: &[azam_geom::Vec2]| azam_geom::signed_area(pts).abs();
        let drafted = area(&src.outline().poly);
        self.status = match made {
            Some(m) => {
                self.session.selection.select_only(m);
                format!(
                    "Flattened {}: {:+.1} % area against the draft",
                    src.name,
                    (area(&outline) / drafted.max(1e-9) - 1.0) * 100.0
                )
            }
            None => "Could not flatten the piece".into(),
        };
        self.framed = false;
    }
}

/// `ring` (a closed polygon) without the points where it turns by less than `deg` degrees.
fn simplify_ring(ring: &[azam_geom::Vec2], deg: f64) -> Vec<azam_geom::Vec2> {
    let n = ring.len();
    if n < 4 {
        return ring.to_vec();
    }
    let keep = |i: usize| {
        let (a, b, c) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
        let (u, v) = ((b - a).normalize_or_zero(), (c - b).normalize_or_zero());
        u.dot(v).clamp(-1.0, 1.0).acos().to_degrees() >= deg
    };
    let kept: Vec<azam_geom::Vec2> = (0..n).filter(|&i| keep(i)).map(|i| ring[i]).collect();
    if kept.len() >= 3 { kept } else { ring.to_vec() }
}
