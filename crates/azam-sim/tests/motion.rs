//! The solver against a moving body: the tee stays outside the re-posed avatar and is carried
//! along by friction when the avatar turns.

use azam_avatar::{Avatar, AvatarParams, Motion, pose_at};
use azam_doc::{Sex, Template, from_template};
use azam_sim::{SimParams, Simulation, resolve_settings};
use glam::DVec3;

/// Frames the tee settles on the resting avatar first.
const SETTLE_FRAMES: usize = 40;
/// The body is re-posed every this many frames (as the app may when posing is slow).
const POSE_EVERY: usize = 2;
/// Particles may end at most this deep inside the moving body, mm.
const PENETRATION_TOL_MM: f64 = 3.0;

fn tee_on(avatar: &Avatar) -> (Simulation, f64) {
    let project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    let settings = resolve_settings(&project.sim);
    let params = SimParams::from(&settings);
    let mut sim = Simulation::build(&project, avatar, None, &params);
    for _ in 0..SETTLE_FRAMES {
        sim.advance(avatar, settings.values.dt);
    }
    (sim, settings.values.dt)
}

fn positions(sim: &Simulation) -> Vec<DVec3> {
    sim.particles
        .x
        .chunks_exact(3)
        .map(|p| DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2])))
        .collect()
}

/// Runs `motion` for `seconds`, re-posing every [`POSE_EVERY`] frames; returns the last posed
/// avatar.
fn run(sim: &mut Simulation, rest: &Avatar, motion: Motion, seconds: f64, dt: f64) -> Avatar {
    let frame = rest.motion_frame(motion);
    let mut body = rest.posed_in(&pose_at(motion, 0.0), &frame);
    let frames = (seconds / dt).round() as usize;
    let step = POSE_EVERY as f64 * dt;
    for i in 0..frames {
        if i % POSE_EVERY == 0 {
            // the pose at the end of this stretch, moving at its average velocity
            let t = (i + POSE_EVERY) as f64 * dt;
            body = rest
                .posed_in(&pose_at(motion, t), &frame)
                .with_velocity_from(&body, step);
        }
        sim.advance(&body, dt);
    }
    body
}

fn check_outside(sim: &Simulation, avatar: &Avatar, what: &str) {
    let x = positions(sim);
    assert!(x.iter().all(|p| p.is_finite()), "{what}: positions stay finite");
    let (worst, at) = x
        .iter()
        .enumerate()
        .map(|(q, &p)| (avatar.grid.distance(p), q))
        .fold((f64::INFINITY, 0), |a, b| if b.0 < a.0 { b } else { a });
    assert!(
        worst >= -PENETRATION_TOL_MM,
        "{what}: particle {at} is {worst:.2} mm inside the body"
    );
}

#[test]
fn a_turning_avatar_carries_the_tee_along() {
    let rest = Avatar::parametric(&AvatarParams::preset(azam_avatar::Sex::Female));
    let (mut sim, dt) = tee_on(&rest);
    let before = positions(&sim);
    let last = run(&mut sim, &rest, Motion::Turntable, 2.0, dt);
    check_outside(&sim, &last, "turntable");
    let after = positions(&sim);
    // mean turn of the cloth about the vertical axis
    let cz = rest.profile.torso.eval(rest.profile.levels.waist).center_z;
    let angle = |p: DVec3| p.x.atan2(p.z - cz);
    let mut turned = 0.0;
    for (a, b) in before.iter().zip(&after) {
        let mut d = angle(*b) - angle(*a);
        while d > std::f64::consts::PI {
            d -= std::f64::consts::TAU;
        }
        while d < -std::f64::consts::PI {
            d += std::f64::consts::TAU;
        }
        turned += d;
    }
    let turned = (turned / before.len() as f64).to_degrees();
    let body = last.pose().turn;
    eprintln!("the tee turned {turned:.1}° with the body's {body:.1}°");
    assert!(
        (turned - body).abs() < 0.25 * body,
        "the tee turned {turned:.1}° with the body's {body:.1}°"
    );
}

#[test]
fn the_tee_stays_outside_a_walking_and_arm_raising_avatar() {
    let rest = Avatar::parametric(&AvatarParams::preset(azam_avatar::Sex::Female));
    for motion in [Motion::Walk, Motion::RaiseArms] {
        let (mut sim, dt) = tee_on(&rest);
        let last = run(&mut sim, &rest, motion, 1.5, dt);
        check_outside(&sim, &last, motion.label());
    }
}
