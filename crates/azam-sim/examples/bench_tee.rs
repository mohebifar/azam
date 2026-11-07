//! Times the CPU solver on the tee template for every quality preset.
//!
//! `cargo run --release -p azam-sim --example bench_tee [frames]`

use std::time::Instant;

use azam_avatar::{Avatar, AvatarParams};
use azam_doc::{Sex, Template, from_template};
use azam_sim::{SimParams, SimPreset, Simulation, preset_settings, resolve_settings};

/// Screen frames timed per preset when none are given.
const DEFAULT_FRAMES: usize = 120;

fn main() {
    let frames = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FRAMES);
    let t = Instant::now();
    let avatar = Avatar::parametric(&AvatarParams::preset(azam_avatar::Sex::Female));
    println!("avatar: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let mut project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    for preset in SimPreset::ALL {
        project.sim = preset_settings(preset);
        let settings = resolve_settings(&project.sim);
        let params = SimParams::from(&settings);
        let t = Instant::now();
        let mut sim = Simulation::build(&project, &avatar, None, &params);
        let build_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        for _ in 0..frames {
            sim.advance(&avatar, settings.values.dt);
        }
        let per_screen = t.elapsed().as_secs_f64() * 1e3 / frames as f64;
        let per_sim = per_screen / f64::from(params.frames_per_advance());
        println!(
            "{:<8} {:>5} particles, {:>2} substeps x {} iterations: build {:>6.1} ms, {:>6.2} ms per simulated frame, {:>6.2} ms per screen frame ({} sim frames), energy {:.1}",
            preset.name(),
            sim.particle_count(),
            params.substeps,
            params.iterations,
            build_ms,
            per_sim,
            per_screen,
            params.frames_per_advance(),
            sim.energy
        );
    }
}
