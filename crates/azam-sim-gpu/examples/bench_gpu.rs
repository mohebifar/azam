//! Times the GPU solver against the CPU solver on the tee template for every quality preset.
//!
//! `cargo run --release -p azam-sim-gpu --example bench_gpu [frames]`

use std::time::Instant;

use azam_avatar::{Avatar, AvatarParams};
use azam_doc::{Sex, Template, from_template};
use azam_sim::{SimParams, SimPreset, Simulation, preset_settings, resolve_settings};
use azam_sim_gpu::GpuSolver;

/// Screen frames timed per preset when none are given.
const DEFAULT_FRAMES: usize = 120;
/// Untimed screen frames before timing the GPU (pipeline warm-up).
const WARMUP_FRAMES: usize = 3;

fn main() {
    let frames = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FRAMES);
    let t = Instant::now();
    let solver = match GpuSolver::new_default() {
        Ok(s) => s,
        Err(e) => {
            println!("no GPU solver: {e}");
            return;
        }
    };
    println!(
        "GPU: {} (pipelines built in {:.1} ms)",
        solver.adapter_label().unwrap_or_default(),
        t.elapsed().as_secs_f64() * 1e3
    );
    let avatar = Avatar::parametric(&AvatarParams::preset(azam_avatar::Sex::Female));
    let mut project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    for preset in SimPreset::ALL {
        project.sim = preset_settings(preset);
        let settings = resolve_settings(&project.sim);
        let params = SimParams::from(&settings);
        let dt = settings.values.dt;
        let base = Simulation::build(&project, &avatar, None, &params);

        let mut cpu = base.clone();
        let t = Instant::now();
        for _ in 0..frames {
            cpu.advance(&avatar, dt);
        }
        let cpu_ms = t.elapsed().as_secs_f64() * 1e3 / frames as f64;

        let mut sim = base.clone();
        let t = Instant::now();
        let mut gpu = match solver.upload(&sim, &avatar) {
            Ok(g) => g,
            Err(e) => {
                println!("{:<8} upload failed: {e}", preset.name());
                continue;
            }
        };
        gpu.set_untangler(false);
        let upload_ms = t.elapsed().as_secs_f64() * 1e3;
        for _ in 0..WARMUP_FRAMES {
            if let Err(e) = gpu.step_frame(&mut sim, dt) {
                println!("{:<8} frame failed: {e}", preset.name());
            }
        }
        let t = Instant::now();
        for _ in 0..frames {
            if let Err(e) = gpu.step_frame(&mut sim, dt) {
                println!("{:<8} frame failed: {e}", preset.name());
                break;
            }
        }
        let gpu_ms = t.elapsed().as_secs_f64() * 1e3 / frames as f64;
        let contacts = gpu.contact_stats().map(|c| c.contacts).unwrap_or(0);
        println!(
            "{:<8} {:>5} particles, {:>2} substeps x {} iterations, {} sim frames per screen frame: CPU {:>7.2} ms, GPU {:>7.2} ms per screen frame ({:.1}x; upload {:.1} ms), energy CPU {:.1} GPU {:.1}, contacts {contacts}",
            preset.name(),
            sim.particle_count(),
            params.substeps,
            params.iterations,
            params.frames_per_advance(),
            cpu_ms,
            gpu_ms,
            cpu_ms / gpu_ms,
            upload_ms,
            cpu.energy,
            sim.energy,
        );
    }
}
