//! The GPU solver against the CPU solver on the same build. The GPU solves each colour of
//! constraints at once and uses vertex-triangle cloth contacts, so positions cannot match the
//! CPU's; these tests compare aggregates with documented tolerances instead. Every test passes
//! with a note when no GPU adapter is available.

use azam_avatar::{Avatar, AvatarParams};
use azam_doc::{Sex, Template, from_template};
use azam_sim::{SimParams, SimPreset, Simulation, preset_settings, resolve_settings};
use azam_sim_gpu::GpuSolver;
use glam::DVec3;

/// Screen frames simulated (4 s at 60 frames per second).
const FRAMES: usize = 240;
/// Every frame is compared up to this one (the seams close and the tee falls onto the body);
/// later frames every [`LATE_SAMPLE`] frames.
const EARLY_FRAMES: usize = 30;
/// Sampling interval after the early frames.
const LATE_SAMPLE: usize = 20;
/// Energy (RMS speed) may differ by this fraction of the larger value plus
/// [`ENERGY_ABS_TOL_MM_S`]. Observed on the tee: within 5 % while falling.
const ENERGY_REL_TOL: f64 = 0.15;
/// Energy floor of the comparison, mm/s: at rest the CPU's particle-sphere contacts keep a
/// residual jitter of about 19 mm/s, the GPU's vertex-triangle contacts about 5 mm/s.
const ENERGY_ABS_TOL_MM_S: f64 = 20.0;
/// The mean seam gap may differ by this fraction of the larger value plus [`SEAM_ABS_TOL_MM`]
/// on every compared frame. Observed: 14.7 vs 14.1 mm on frame 2, 0.23 vs 0.35 mm at rest.
const SEAM_REL_TOL: f64 = 0.1;
/// Seam gap floor of the comparison, mm.
const SEAM_ABS_TOL_MM: f64 = 0.5;
/// Both solvers close the seams to below this mean gap, mm.
const SEAM_CLOSED_MM: f64 = 1.0;
/// Bounding boxes may differ by this much per face after settling, mm (observed: 5.3 mm at the
/// shoulders, under 1 mm elsewhere).
const BBOX_TOL_MM: f64 = 10.0;
/// Mean positions may differ by this much after settling, mm (observed: 0.4 mm).
const MEAN_TOL_MM: f64 = 3.0;
/// No GPU particle may end deeper than this inside the body's distance field, mm (observed: all
/// particles at least 0.7 mm outside, as on the CPU).
const PENETRATION_TOL_MM: f64 = 0.5;
/// At rest the GPU's RMS particle speed stays below this, mm/s (observed: 5 mm/s).
const REST_SPEED_MM_S: f64 = 30.0;

fn solver() -> Option<GpuSolver> {
    match GpuSolver::new_default() {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("note: GPU unavailable ({e}); test skipped");
            None
        }
    }
}

/// Axis-aligned bounds and mean of the positions.
fn stats(x: &[f32]) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut sum = [0.0; 3];
    for p in x.chunks_exact(3) {
        for c in 0..3 {
            let v = f64::from(p[c]);
            lo[c] = lo[c].min(v);
            hi[c] = hi[c].max(v);
            sum[c] += v;
        }
    }
    let n = (x.len() / 3) as f64;
    (lo, hi, sum.map(|s| s / n))
}

fn worst_penetration(sim: &Simulation, av: &Avatar) -> f64 {
    sim.particles
        .x
        .chunks_exact(3)
        .map(|p| {
            av.grid
                .distance(DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2])))
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn the_tee_settles_on_the_body_like_on_the_cpu() {
    let Some(solver) = solver() else { return };
    let av = Avatar::parametric(&AvatarParams::preset(azam_avatar::Sex::Female));
    let mut project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    project.sim = preset_settings(SimPreset::Normal);
    let settings = resolve_settings(&project.sim);
    let params = SimParams::from(&settings);
    let dt = settings.values.dt;
    let mut cpu = Simulation::build(&project, &av, None, &params);
    let mut gpu_sim = cpu.clone();
    let mut gpu = solver.upload(&gpu_sim, &av).expect("upload");
    gpu.set_untangler(false);

    for f in 1..=FRAMES {
        cpu.advance(&av, dt);
        let report = gpu.step_frame(&mut gpu_sim, dt).expect("GPU frame");
        assert!(!report.restored, "the GPU solver blew up at frame {f}");
        if f > EARLY_FRAMES && f % LATE_SAMPLE != 0 {
            continue;
        }
        let (e_cpu, e_gpu) = (cpu.energy, gpu_sim.energy);
        let (s_cpu, s_gpu) = (cpu.sew_gap, gpu_sim.sew_gap);
        eprintln!(
            "frame {f:>3}: energy CPU {e_cpu:>7.1} GPU {e_gpu:>7.1}, seam gap CPU {s_cpu:>7.2} GPU {s_gpu:>7.2}"
        );
        let tol = ENERGY_REL_TOL * e_cpu.max(e_gpu) + ENERGY_ABS_TOL_MM_S;
        assert!(
            (e_cpu - e_gpu).abs() <= tol,
            "frame {f}: energy CPU {e_cpu} vs GPU {e_gpu}"
        );
        let tol = SEAM_REL_TOL * s_cpu.max(s_gpu) + SEAM_ABS_TOL_MM;
        assert!(
            (s_cpu - s_gpu).abs() <= tol,
            "frame {f}: seam gap CPU {s_cpu} vs GPU {s_gpu}"
        );
    }
    assert_eq!(gpu_sim.frames, cpu.frames, "both clocks advanced");
    assert!(gpu_sim.particles.x.iter().all(|v| v.is_finite()), "no NaN");

    let (clo, chi, cmean) = stats(&cpu.particles.x);
    let (glo, ghi, gmean) = stats(&gpu_sim.particles.x);
    eprintln!("bounds CPU {clo:.1?}..{chi:.1?} GPU {glo:.1?}..{ghi:.1?}");
    eprintln!("mean CPU {cmean:.2?} GPU {gmean:.2?}");
    for c in 0..3 {
        assert!(
            (clo[c] - glo[c]).abs() <= BBOX_TOL_MM,
            "low bound {c}: CPU {} GPU {}",
            clo[c],
            glo[c]
        );
        assert!(
            (chi[c] - ghi[c]).abs() <= BBOX_TOL_MM,
            "high bound {c}: CPU {} GPU {}",
            chi[c],
            ghi[c]
        );
        assert!(
            (cmean[c] - gmean[c]).abs() <= MEAN_TOL_MM,
            "mean {c}: CPU {} GPU {}",
            cmean[c],
            gmean[c]
        );
    }
    let (gc, gg) = (cpu.sew_gap, gpu_sim.sew_gap);
    assert!(
        gc < SEAM_CLOSED_MM && gg < SEAM_CLOSED_MM,
        "seams closed: CPU {gc} GPU {gg}"
    );
    let pen_gpu = worst_penetration(&gpu_sim, &av);
    let pen_cpu = worst_penetration(&cpu, &av);
    eprintln!("nearest particle to the body: CPU {pen_cpu:.2} mm, GPU {pen_gpu:.2} mm");
    assert!(
        pen_gpu >= -PENETRATION_TOL_MM,
        "a GPU particle is {pen_gpu} mm inside the body"
    );
    assert!(
        gpu_sim.energy < REST_SPEED_MM_S,
        "the GPU drape comes to rest ({} mm/s)",
        gpu_sim.energy
    );
    let gravity = gpu_sim.gravity_scale.expect("hold-back read back");
    assert!(gravity > 0.9, "full gravity once the seams closed ({gravity})");
}

/// Side of the square draped over head and shoulders, mm.
const DRAPE_MM: f64 = 700.0;
/// Fabric friction of the drape (0–100): high, so it does not slide off the smooth head.
const DRAPE_FRICTION: f64 = 90.0;
/// The square starts this high above the head, mm.
const DROP_HEIGHT_MM: f64 = 40.0;
/// Screen frames of the drape (5 s).
const DRAPE_FRAMES: usize = 300;
/// Drape bounds may differ by this much per face, mm: the folds settle differently (observed:
/// up to 16 mm).
const DRAPE_BBOX_TOL_MM: f64 = 30.0;
/// Drape means may differ by this much, mm (observed: 3 mm).
const DRAPE_MEAN_TOL_MM: f64 = 10.0;
/// The GPU drape's RMS speed at the end stays below this, mm/s (observed: 8 mm/s; the CPU's
/// particle-sphere contacts keep jittering at about 100 mm/s).
const DRAPE_REST_SPEED_MM_S: f64 = 30.0;

#[test]
fn a_square_drapes_over_the_shoulders_like_on_the_cpu() {
    let Some(solver) = solver() else { return };
    let av = Avatar::parametric(&AvatarParams::preset(azam_avatar::Sex::Female));
    let pts = vec![
        azam_geom::OutlinePoint::corner("a", 0.0, 0.0),
        azam_geom::OutlinePoint::corner("b", DRAPE_MM, 0.0),
        azam_geom::OutlinePoint::corner("c", DRAPE_MM, DRAPE_MM),
        azam_geom::OutlinePoint::corner("d", 0.0, DRAPE_MM),
    ];
    let mut project = azam_doc::Project::new(None, Sex::Female, 0.0);
    project.add_piece(azam_doc::Piece::new(azam_doc::PieceId::new("sq"), "sq", pts));
    for f in project.fabrics.values_mut() {
        f.phys.friction = DRAPE_FRICTION;
    }
    let mut cpu = Simulation::build(&project, &av, None, &SimParams::default());
    let r = cpu.records[0].clone();
    let mut x = cpu.particles.x.clone();
    let half = DRAPE_MM / 2.0;
    for i in 0..r.count {
        let k = (r.offset + i) * 3;
        x[k] = (r.mesh.xy[2 * i] - half) as f32;
        x[k + 1] = (av.height() + DROP_HEIGHT_MM) as f32;
        x[k + 2] = (r.mesh.xy[2 * i + 1] - half) as f32;
    }
    cpu.set_positions(&x);
    let mut gpu_sim = cpu.clone();
    let mut gpu = solver.upload(&gpu_sim, &av).expect("upload");
    let mut peak: f64 = 0.0;
    for _ in 0..DRAPE_FRAMES {
        cpu.step(&av, 1.0 / 60.0);
        let report = gpu.step_frame(&mut gpu_sim, 1.0 / 60.0).expect("GPU frame");
        assert!(!report.restored);
        peak = peak.max(gpu_sim.energy);
    }
    let (clo, chi, cmean) = stats(&cpu.particles.x);
    let (glo, ghi, gmean) = stats(&gpu_sim.particles.x);
    let pen = worst_penetration(&gpu_sim, &av);
    let setup = azam_sim::untangle::UntangleSetup::new(&gpu_sim);
    let (cross_cpu, cross_gpu) = (
        setup.run(&cpu.particles.x).crossings,
        setup.run(&gpu_sim.particles.x).crossings,
    );
    eprintln!("drape bounds CPU {clo:.1?}..{chi:.1?} GPU {glo:.1?}..{ghi:.1?}");
    eprintln!(
        "drape mean CPU {cmean:.1?} GPU {gmean:.1?}; energy CPU {:.1} GPU {:.1}, GPU peak {peak:.0}; nearest {pen:.2} mm; crossings CPU {cross_cpu} GPU {cross_gpu}",
        cpu.energy, gpu_sim.energy
    );
    assert!(gpu_sim.particles.x.iter().all(|v| v.is_finite()), "no NaN");
    assert!(peak > 100.0, "the square falls (peak {peak} mm/s)");
    assert!(
        pen >= -PENETRATION_TOL_MM,
        "a particle is {pen} mm inside the body"
    );
    assert!(
        glo[1] > av.height() * 0.6,
        "the square hangs on the upper body (lowest {} mm)",
        glo[1]
    );
    for c in 0..3 {
        assert!(
            (clo[c] - glo[c]).abs() <= DRAPE_BBOX_TOL_MM,
            "low bound {c}: CPU {} GPU {}",
            clo[c],
            glo[c]
        );
        assert!(
            (chi[c] - ghi[c]).abs() <= DRAPE_BBOX_TOL_MM,
            "high bound {c}: CPU {} GPU {}",
            chi[c],
            ghi[c]
        );
        assert!(
            (cmean[c] - gmean[c]).abs() <= DRAPE_MEAN_TOL_MM,
            "mean {c}: CPU {} GPU {}",
            cmean[c],
            gmean[c]
        );
    }
    assert!(
        gpu_sim.energy < DRAPE_REST_SPEED_MM_S,
        "the GPU drape comes to rest ({} mm/s)",
        gpu_sim.energy
    );
    // the GPU's vertex-triangle contacts keep the folds apart at least as well as the CPU's
    // particle spheres (observed: 43 edge-triangle crossings against 325)
    assert!(
        cross_gpu <= cross_cpu,
        "crossings: CPU {cross_cpu}, GPU {cross_gpu}"
    );
}
