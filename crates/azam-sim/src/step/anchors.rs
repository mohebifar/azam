//! Interactive anchors: grabbing cloth with the pointer, pins and rigid moves of a piece.

use crate::numeric::{hypot3, nudge};
use crate::state::{Grab, Particles, Simulation};

/// Radius around the grabbed point within which particles follow the pointer, in mm.
pub const GRAB_RADIUS_MM: f64 = 45.0;
/// Grip of the particle right under the pointer (fraction of the gap closed per substep).
const GRAB_GRIP: f64 = 0.5;

impl Grab {
    /// Advances the grab point towards the target by at most `speed * dt` and returns it.
    pub fn advance_anchor(&mut self, speed: f64, dt: f64) -> [f64; 3] {
        let cur = *self.current.get_or_insert(self.target);
        let mx = speed * dt;
        let (dx, dy, dz) = (
            self.target[0] - cur[0],
            self.target[1] - cur[1],
            self.target[2] - cur[2],
        );
        let l = hypot3(dx, dy, dz);
        let next = if l <= mx {
            self.target
        } else {
            let f = mx / l;
            [cur[0] + dx * f, cur[1] + dy * f, cur[2] + dz * f]
        };
        self.current = Some(next);
        next
    }

    /// The current grab point (the target before the first substep).
    #[must_use]
    pub fn anchor(&self) -> [f64; 3] {
        self.current.unwrap_or(self.target)
    }

    /// Pulls every grabbed particle towards the anchor plus its offset.
    pub(crate) fn apply(&self, p: &mut Particles, anchor: [f64; 3]) {
        for (e, &q) in self.vertices.iter().enumerate() {
            let k = q as usize * 3;
            let f = self.weights[e];
            let offset = &self.offsets[e * 3..e * 3 + 3];
            for ((xd, a), o) in p.x[k..k + 3].iter_mut().zip(anchor).zip(offset) {
                let pull = (a + o - f64::from(*xd)) * f;
                nudge(xd, pull);
            }
        }
    }
}

impl Simulation {
    /// Grabs the cloth at particle `q`: particles of the same piece within [`GRAB_RADIUS_MM`]
    /// follow the grab point, more firmly the closer they are. Returns `false` if `q` is not a
    /// particle.
    pub fn grab_at(&mut self, q: usize) -> bool {
        if q >= self.particle_count() {
            return false;
        }
        let x = &self.particles.x;
        let c = [
            f64::from(x[q * 3]),
            f64::from(x[q * 3 + 1]),
            f64::from(x[q * 3 + 2]),
        ];
        let r = &self.records[self.particles.piece[q] as usize];
        let mut grab = Grab {
            vertices: Vec::new(),
            weights: Vec::new(),
            offsets: Vec::new(),
            target: c,
            current: None,
        };
        for i in r.particles() {
            let k = i * 3;
            let rel = [
                f64::from(x[k]) - c[0],
                f64::from(x[k + 1]) - c[1],
                f64::from(x[k + 2]) - c[2],
            ];
            let d = hypot3(rel[0], rel[1], rel[2]);
            if d < GRAB_RADIUS_MM {
                let f = 1.0 - d / GRAB_RADIUS_MM;
                grab.vertices.push(i as u32);
                grab.weights.push(GRAB_GRIP * f * f);
                grab.offsets.extend(rel);
            }
        }
        self.grab = Some(grab);
        true
    }

    /// Moves the grab target (where the pointer is), mm.
    pub fn set_grab_target(&mut self, target: [f64; 3]) {
        if let Some(g) = &mut self.grab {
            g.target = target;
        }
    }

    /// Lets go of the grabbed cloth.
    pub fn release_grab(&mut self) {
        self.grab = None;
    }

    /// Pins particle `q` where it is now. Returns `false` if it is not a particle or already
    /// pinned.
    pub fn pin(&mut self, q: usize) -> bool {
        if q >= self.particle_count() || self.pins.vertex.contains(&(q as u32)) {
            return false;
        }
        self.pins.vertex.push(q as u32);
        let pos = self.particles.position(q);
        self.pins.pos.extend_from_slice(&pos);
        self.pins.fixed.push(true);
        self.particles.inv_mass[q] = 0.0;
        true
    }

    /// Removes the pin of particle `q`. Returns `false` if it was not pinned.
    pub fn unpin(&mut self, q: usize) -> bool {
        let Some(e) = self.pins.vertex.iter().position(|&v| v as usize == q) else {
            return false;
        };
        self.pins.vertex.remove(e);
        self.pins.pos.drain(e * 3..e * 3 + 3);
        if e < self.pins.fixed.len() {
            self.pins.fixed.remove(e);
        }
        if !self.pins.vertex.contains(&(q as u32)) {
            self.particles.inv_mass[q] = self.particles.inv_mass_free[q];
        }
        true
    }

    /// Moves piece `ri` rigidly by `delta` mm (while the simulation is paused); the move adds no
    /// velocity.
    pub fn translate_piece(&mut self, ri: usize, delta: [f64; 3]) {
        let Some(r) = self.records.get(ri) else { return };
        let p = &mut self.particles;
        for q in r.particles() {
            let k = q * 3;
            for ((xd, pd), dd) in p.x[k..k + 3].iter_mut().zip(&mut p.prev[k..k + 3]).zip(delta) {
                nudge(xd, dd);
                *pd = *xd;
            }
        }
    }

    /// Turns piece `ri` rigidly by `angle` radians about the vertical axis through `centre` (x and
    /// z, mm), as a piece is turned around the body (while the simulation is paused); the turn
    /// adds no velocity.
    pub fn turn_piece(&mut self, ri: usize, angle: f64, centre: [f64; 2]) {
        let Some(r) = self.records.get(ri) else { return };
        let (s, c) = angle.sin_cos();
        let p = &mut self.particles;
        for q in r.particles() {
            let k = q * 3;
            let (x, z) = (f64::from(p.x[k]) - centre[0], f64::from(p.x[k + 2]) - centre[1]);
            p.x[k] = (centre[0] + x * c + z * s) as f32;
            p.x[k + 2] = (centre[1] - x * s + z * c) as f32;
            p.prev[k] = p.x[k];
            p.prev[k + 2] = p.x[k + 2];
        }
    }
}

/// Puts every pinned particle back at its pin.
pub(crate) fn apply_pins(p: &mut Particles, vertex: &[u32], pos: &[f32]) {
    for (e, &q) in vertex.iter().enumerate() {
        let k = q as usize * 3;
        p.x[k..k + 3].copy_from_slice(&pos[e * 3..e * 3 + 3]);
    }
}
