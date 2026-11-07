//! Times re-posing the avatar (mesh, joints, collision grid and surface velocity) for every
//! motion, for the parametric mannequin and, when the pack is present, the morph-target body.
//!
//! `cargo run --release -p azam-avatar --example bench_pose [frames]`

use std::time::Instant;

use azam_avatar::makehuman::DEFAULT_MH_GRID_CELL_MM;
use azam_avatar::{Avatar, AvatarParams, MhModel, Motion, Sex, pose_at};

/// Poses timed per motion when no count is given.
const DEFAULT_FRAMES: usize = 60;

fn bench(name: &str, avatar: &Avatar, frames: usize) {
    for motion in Motion::ALL {
        let frame = avatar.motion_frame(motion);
        let dt = 1.0 / 60.0;
        // warm up (the rest grid that rigid poses resample is built on first use)
        let mut prev = avatar.posed_in(&pose_at(motion, 1.0), &frame);
        let (mut pose_ms, mut vel_ms) = (0.0, 0.0);
        let mut full = Vec::with_capacity(frames);
        for i in 1..=frames {
            let pose = pose_at(motion, 1.0 + i as f64 * dt);
            let t = Instant::now();
            let next = avatar.posed_in(&pose, &frame);
            pose_ms += t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let _ = next.with_velocity_from(&prev, dt);
            vel_ms += t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            prev = avatar.posed_after(&pose, &frame, &prev, dt);
            full.push(t.elapsed().as_secs_f64() * 1e3);
        }
        let n = frames as f64;
        full.sort_by(f64::total_cmp);
        println!(
            "{name:<11} {:<10} grid {:?} ({:.1} M values): pose {:>5.2} ms, velocity {:>4.2} ms; \
             full update (posed_after) mean {:>5.2} ms, median {:>5.2} ms",
            motion.label(),
            frame.dims,
            frame.len() as f64 / 1e6,
            pose_ms / n,
            vel_ms / n,
            full.iter().sum::<f64>() / n,
            full[full.len() / 2],
        );
    }
}

fn main() {
    let frames = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FRAMES);
    let params = AvatarParams::preset(Sex::Female);
    bench("mannequin", &Avatar::parametric(&params), frames);
    match MhModel::load_bundled() {
        Some(Ok(model)) => {
            let t = Instant::now();
            let avatar = Avatar::makehuman(&model, &params, DEFAULT_MH_GRID_CELL_MM);
            println!("makehuman build: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
            bench("makehuman", &avatar, frames);
        }
        Some(Err(e)) => eprintln!("avatar pack: {e}"),
        None => eprintln!("no avatar pack; skipping the morph-target body"),
    }
}
