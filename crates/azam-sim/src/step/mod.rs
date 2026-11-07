//! Advancing the simulation: one frame is `substeps` substeps of predict, collide, solve
//! (`iterations` stretch + sewing passes, then bending and fasteners), cloth contacts, grab, guard,
//! pins and velocity update.

pub(crate) mod anchors;
mod constraints;
mod integrate;

use azam_avatar::Avatar;

pub use anchors::GRAB_RADIUS_MM;

use crate::collide::triangle::TriangleContacts;
use crate::collide::{body_sdf, particle};
use crate::state::Simulation;

/// Frame time used when none is given, in seconds.
pub const DEFAULT_DT_S: f64 = 1.0 / 60.0;
/// The particle contacts are re-detected on every this many substeps (resolved on the others).
const PARTICLE_DETECT_EVERY: u32 = 5;
/// Last-good positions are saved every this many frames (a power of two).
const GOOD_SAVE_INTERVAL: u64 = 16;

impl Simulation {
    /// Advances one screen frame on the CPU: [`SimParams::frames_per_advance`] frames of `dt`
    /// seconds each.
    ///
    /// [`SimParams::frames_per_advance`]: crate::SimParams::frames_per_advance
    pub fn advance(&mut self, avatar: &Avatar, dt: f64) {
        for _ in 0..self.params.frames_per_advance() {
            self.step(avatar, dt);
        }
    }

    /// Simulates one frame of `dt` seconds (0 or NaN means [`DEFAULT_DT_S`]).
    ///
    /// `avatar` must be the avatar the simulation was built on.
    pub fn step(&mut self, avatar: &Avatar, dt: f64) {
        let dt = if dt == 0.0 || dt.is_nan() {
            DEFAULT_DT_S
        } else {
            dt
        };
        let params = self.params;
        let n = params.substeps;
        let h = dt / f64::from(n);
        let ih2 = 1.0 / (h * h);
        let grid = &avatar.grid;
        let gy = integrate::piece_gravity(
            &mut integrate::Holdback {
                gravity_scale: &mut self.gravity_scale,
                piece_gravity: &mut self.piece_gravity,
                sew_gap: &mut self.sew_gap,
            },
            &self.particles.x,
            &self.particles.piece,
            &self.sewing,
            self.records.len(),
            params.gravity,
        );
        let max_sew = params.sew_speed * h;
        let has_pressure = self.particles.pressure.iter().any(|&a| a != 0.0);
        if has_pressure {
            self.compute_normals();
        }
        for s in 0..n {
            let p = &mut self.particles;
            integrate::predict(p, &gy, params.air, h, has_pressure, &self.normals);
            body_sdf::body_and_floor(p, grid, avatar.velocity.as_ref().map(|v| (v, h)));
            for _ in 0..params.iterations {
                constraints::stretch(p, &self.stretch, ih2);
                constraints::sew(p, &self.sewing, max_sew);
            }
            constraints::bend(p, &self.bend);
            constraints::fasten(p, &self.fasteners, max_sew);
            if params.self_collision {
                self.cloth_contacts(avatar, s);
            }
            if let Some(grab) = &mut self.grab {
                let anchor = if s == 0 {
                    grab.advance_anchor(params.grab_speed, dt)
                } else {
                    grab.anchor()
                };
                grab.apply(&mut self.particles, anchor);
            }
            let p = &mut self.particles;
            body_sdf::guard(p, grid);
            if (s + 1 == n || s + 1 == n / 2)
                && let Some(bt) = &mut self.body_triangles
            {
                self.body_fixes = bt.collide(p, avatar);
            }
            anchors::apply_pins(p, &self.pins.vertex, &self.pins.pos);
            integrate::update_velocities(p, h);
        }
        self.finish_frame(dt);
    }

    /// Cloth-cloth contacts of substep `s`: detection on some substeps, resolution of the recorded
    /// contacts on the others.
    fn cloth_contacts(&mut self, avatar: &Avatar, s: u32) {
        let p = &mut self.particles;
        if self.params.cpu_triangle_collision {
            let every = self.params.triangle_detect_every.max(1);
            if s % every == every - 1 {
                let tc = self
                    .triangle_contacts
                    .get_or_insert_with(|| TriangleContacts::new(&self.records, p, &self.sewing));
                tc.detect(p, &self.records, &avatar.grid, &self.params);
            } else if let Some(tc) = &self.triangle_contacts {
                tc.resolve(p, &self.params);
            }
        } else if s % PARTICLE_DETECT_EVERY == PARTICLE_DETECT_EVERY - 1 {
            particle::detect(
                p,
                &mut self.particle_contacts,
                &self.records,
                &avatar.grid,
                &self.params,
            );
        } else {
            particle::resolve(p, &self.particle_contacts, &avatar.grid, &self.params);
        }
    }

    /// NaN guard (restores the last good positions), motion energy and clocks.
    fn finish_frame(&mut self, dt: f64) {
        let p = &mut self.particles;
        let mut bad = false;
        let (mut e, mut cnt) = (0.0, 0_usize);
        for q in 0..p.len() {
            let k = q * 3;
            if p.x[k..k + 3].iter().any(|v| v.is_nan()) {
                bad = true;
                break;
            }
            if p.inv_mass[q] > 0.0 {
                let (vx, vy, vz) = (f64::from(p.v[k]), f64::from(p.v[k + 1]), f64::from(p.v[k + 2]));
                e += vx * vx + vy * vy + vz * vz;
                cnt += 1;
            }
        }
        if bad {
            p.x.copy_from_slice(&self.good);
            p.prev.copy_from_slice(&self.good);
            p.v.fill(0.0);
        } else if self.frames % GOOD_SAVE_INTERVAL == 0 {
            self.good.copy_from_slice(&p.x);
        }
        self.energy = if cnt > 0 { (e / cnt as f64).sqrt() } else { 0.0 };
        self.frames += 1;
        self.time += dt;
    }
}
