//! The interactive API of the GPU solver: pins, grab, CPU edits, reset, the NaN guard, the split
//! frame and error cases. Every test passes with a note when no GPU adapter is available.

#![allow(clippy::float_cmp, reason = "pinned and restored positions are exact copies")]

use std::sync::OnceLock;

use azam_avatar::{Avatar, AvatarParams};
use azam_doc::{Piece, PieceId, Project, Sex, Template, from_template};
use azam_geom::OutlinePoint;
use azam_sim::{SimParams, Simulation};
use azam_sim_gpu::{GpuError, GpuSolver};

/// Frame time, s.
const DT: f64 = 1.0 / 60.0;
/// Side of the test square, mm.
const SQUARE_MM: f64 = 300.0;
/// Height at which the square starts, well above the avatar, mm.
const START_Y_MM: f32 = 2500.0;

fn solver() -> Option<&'static GpuSolver> {
    static S: OnceLock<Option<GpuSolver>> = OnceLock::new();
    S.get_or_init(|| match GpuSolver::new_default() {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("note: GPU unavailable ({e}); test skipped");
            None
        }
    })
    .as_ref()
}

fn avatar() -> &'static Avatar {
    static A: OnceLock<Avatar> = OnceLock::new();
    A.get_or_init(|| Avatar::parametric(&AvatarParams::preset(azam_avatar::Sex::Female)))
}

/// One square laid flat high above the avatar.
fn square() -> Simulation {
    let pts = vec![
        OutlinePoint::corner("a", 0.0, 0.0),
        OutlinePoint::corner("b", SQUARE_MM, 0.0),
        OutlinePoint::corner("c", SQUARE_MM, SQUARE_MM),
        OutlinePoint::corner("d", 0.0, SQUARE_MM),
    ];
    let mut project = Project::new(None, Sex::Female, 0.0);
    project.add_piece(Piece::new(PieceId::new("sq"), "sq", pts));
    let mut sim = Simulation::build(&project, avatar(), None, &SimParams::default());
    let r = sim.records[0].clone();
    let mut x = sim.particles.x.clone();
    for i in 0..r.count {
        let k = (r.offset + i) * 3;
        x[k] = r.mesh.xy[2 * i] as f32 - SQUARE_MM as f32 / 2.0;
        x[k + 1] = START_Y_MM;
        x[k + 2] = r.mesh.xy[2 * i + 1] as f32 - SQUARE_MM as f32 / 2.0;
    }
    sim.set_positions(&x);
    sim
}

fn mean(sim: &Simulation) -> [f64; 3] {
    let n = sim.particle_count() as f64;
    let mut m = [0.0; 3];
    for p in sim.particles.x.chunks_exact(3) {
        for c in 0..3 {
            m[c] += f64::from(p[c]) / n;
        }
    }
    m
}

#[test]
fn pins_hold_and_follow_pin_and_unpin() {
    let Some(solver) = solver() else { return };
    let mut sim = square();
    assert!(sim.pin(0));
    let held = sim.particles.position(0);
    let mut g = solver.upload(&sim, avatar()).expect("upload");
    for _ in 0..30 {
        g.step_frame(&mut sim, DT).expect("frame");
    }
    assert_eq!(sim.particles.position(0), held, "the pinned particle stays");
    assert!(mean(&sim)[1] < f64::from(START_Y_MM) - 50.0, "the rest falls");
    // pinning during the run
    assert!(sim.pin(7));
    let held7 = sim.particles.position(7);
    for _ in 0..10 {
        g.step_frame(&mut sim, DT).expect("frame");
    }
    assert_eq!(
        sim.particles.position(7),
        held7,
        "a particle pinned mid-run stays"
    );
    assert!(sim.unpin(0));
    for _ in 0..10 {
        g.step_frame(&mut sim, DT).expect("frame");
    }
    assert!(
        sim.particles.position(0)[1] < held[1] - 1.0,
        "the unpinned particle falls"
    );
}

#[test]
fn the_grab_lifts_cloth_towards_the_pointer() {
    let Some(solver) = solver() else { return };
    let mut free = square();
    let mut grabbed = free.clone();
    let q = grabbed.particle_count() / 2;
    assert!(grabbed.grab_at(q));
    let start = grabbed.particles.position(q).map(f64::from);
    let target = [start[0], start[1] + 150.0, start[2]];
    grabbed.set_grab_target(target);
    let mut gf = solver.upload(&free, avatar()).expect("upload");
    let mut gg = solver.upload(&grabbed, avatar()).expect("upload");
    for _ in 0..60 {
        gf.step_frame(&mut free, DT).expect("frame");
        gg.step_frame(&mut grabbed, DT).expect("frame");
    }
    let y_free = f64::from(free.particles.position(q)[1]);
    let y_grab = f64::from(grabbed.particles.position(q)[1]);
    assert!(y_free < start[1] - 200.0, "the free square falls ({y_free})");
    assert!(
        (y_grab - target[1]).abs() < 30.0,
        "the grabbed particle follows the pointer ({y_grab} vs {})",
        target[1]
    );
    let anchor = grabbed
        .grab
        .as_ref()
        .and_then(|g| g.current)
        .expect("anchor advanced");
    assert!(
        (anchor[1] - target[1]).abs() < 1e-6,
        "the grab point reached the target"
    );
    grabbed.release_grab();
    for _ in 0..30 {
        gg.step_frame(&mut grabbed, DT).expect("frame");
    }
    assert!(
        f64::from(grabbed.particles.position(q)[1]) < y_grab - 50.0,
        "released cloth falls"
    );
}

#[test]
fn cpu_edits_are_uploaded_before_the_next_frame() {
    let Some(solver) = solver() else { return };
    let mut sim = square();
    let mut g = solver.upload(&sim, avatar()).expect("upload");
    for _ in 0..5 {
        g.step_frame(&mut sim, DT).expect("frame");
    }
    let before = mean(&sim);
    sim.translate_piece(0, [0.0, 0.0, 500.0]);
    g.upload_positions(true);
    g.step_frame(&mut sim, DT).expect("frame");
    let after = mean(&sim);
    assert!(
        (after[2] - before[2] - 500.0).abs() < 1.0,
        "moved by {}",
        after[2] - before[2]
    );
    // velocities were cleared: one frame of fall from rest is small
    assert!(
        before[1] - after[1] < 10.0,
        "fell {} mm in one frame",
        before[1] - after[1]
    );
}

#[test]
fn reset_puts_the_garment_back_and_restarts() {
    let Some(solver) = solver() else { return };
    let av = avatar();
    let project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    let mut sim = Simulation::build(&project, av, None, &SimParams::default());
    let mut fresh = sim.clone();
    fresh.reset(av);
    let mut g = solver.upload(&sim, av).expect("upload");
    for _ in 0..20 {
        g.step_frame(&mut sim, DT).expect("frame");
    }
    assert!(sim.particles.x != fresh.particles.x, "the garment moved");
    g.reset(&mut sim, av).expect("reset");
    assert!(sim.particles.x == fresh.particles.x, "back at the arrangement");
    assert_eq!((sim.frames, g.frames()), (0, 0));
    assert_eq!(sim.gravity_scale, None);
    g.step_frame(&mut sim, DT).expect("frame after reset");
    assert!(
        sim.sew_gap > 20.0,
        "the seams are open again ({} mm)",
        sim.sew_gap
    );
    assert_eq!(sim.frames, 1);
}

#[test]
fn a_blow_up_restores_the_last_good_positions() {
    let Some(solver) = solver() else { return };
    let mut sim = square();
    let mut g = solver.upload(&sim, avatar()).expect("upload");
    g.step_frame(&mut sim, DT).expect("frame");
    let good = sim.particles.x.clone();
    sim.particles.x[4] = f32::NAN;
    g.upload_positions(false);
    let report = g.step_frame(&mut sim, DT).expect("frame");
    assert!(report.restored, "the NaN was caught");
    assert_eq!(sim.energy, 0.0);
    assert!(sim.particles.x.iter().all(|v| v.is_finite()));
    // the last good positions were saved after the first frame (frame counter 0)
    assert!(
        sim.particles.x == good,
        "restored to the positions after the first frame"
    );
    let report = g.step_frame(&mut sim, DT).expect("frame after the restore");
    assert!(!report.restored, "the restored state runs on");
    assert!(sim.particles.x.iter().all(|v| v.is_finite()));
}

#[test]
fn split_frames_and_errors() {
    let Some(solver) = solver() else { return };
    let mut sim = square();
    let mut g = solver.upload(&sim, avatar()).expect("upload");
    assert!(g.begin_frame(&mut sim, DT).expect("begin"));
    assert!(g.is_busy());
    assert!(
        !g.begin_frame(&mut sim, DT).expect("begin while busy"),
        "one frame at a time"
    );
    let mut report = None;
    for _ in 0..1000 {
        report = g.try_finish_frame(&mut sim).expect("poll");
        if report.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let report = report.expect("the frame finishes");
    assert_eq!(report.frames, 1);
    assert!(!g.is_busy());
    assert_eq!(g.finish_frame(&mut sim).expect("nothing in flight"), None);
    assert_eq!(sim.frames, 1);

    // a different simulation is refused
    let project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    let mut other = Simulation::build(&project, avatar(), None, &SimParams::default());
    match g.step_frame(&mut other, DT) {
        Err(GpuError::Mismatch { expected, found }) => {
            assert_eq!((expected, found), (sim.particle_count(), other.particle_count()));
        }
        r => panic!("expected a mismatch, got {r:?}"),
    }
    // untangle codes of the wrong length are ignored; the right length marks particles
    g.set_tangles(&[1, 2, 3]);
    assert_eq!(g.tangled(), 0);
    let mut codes = vec![0; sim.particle_count()];
    codes[3] = (1 << 8) | 2;
    g.set_tangles(&codes);
    assert_eq!(g.tangled(), 1);
    g.step_frame(&mut sim, DT).expect("frame with tangle codes");
    let stats = g.contact_stats().expect("stats");
    assert_eq!(stats.particles as usize, sim.particle_count());
    assert!(stats.hash_entries > 0, "contacts were detected");
}

#[test]
fn an_empty_simulation_is_refused() {
    let Some(solver) = solver() else { return };
    let project = Project::new(None, Sex::Female, 0.0);
    let sim = Simulation::build(&project, avatar(), None, &SimParams::default());
    assert!(matches!(
        solver.upload(&sim, avatar()),
        Err(GpuError::EmptySimulation)
    ));
}

#[test]
fn a_shared_device_runs_the_solver() {
    let Some(own) = solver() else { return };
    // the app hands over clones of its renderer's device and queue
    let shared = GpuSolver::new(own.device().clone(), own.queue().clone()).expect("shared device");
    assert_eq!(shared.adapter_label(), None);
    let mut sim = square();
    let mut g = shared.upload(&sim, avatar()).expect("upload");
    g.step_frame(&mut sim, DT).expect("frame");
    assert!(mean(&sim)[1] < f64::from(START_Y_MM));
}

#[test]
fn the_background_untangler_runs() {
    let Some(solver) = solver() else { return };
    let av = avatar();
    let project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    let mut sim = Simulation::build(&project, av, None, &SimParams::default());
    assert!(sim.params.untangle);
    let mut g = solver.upload(&sim, av).expect("upload");
    for _ in 0..120 {
        g.step_frame(&mut sim, DT).expect("frame");
        if g.untangle_totals().runs > 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("no untangler run finished in 120 frames");
}
