//! Physical sanity checks independent of the recorded reference data, and checks of the
//! interactive API (pins, rigid moves, reset, the background untangler).

use std::time::{Duration, Instant};

use azam_avatar::{Avatar, AvatarParams};
use azam_doc::{Piece, PieceId, Project, Sex};
use azam_geom::OutlinePoint;
use azam_sim::untangle::{UntangleSetup, UntangleWorker};
use azam_sim::{AutoStop, BuildOptions, SimParams, SimPreset, Simulation, StopReason};

/// Side of the small test square, mm.
const SQUARE_MM: f64 = 400.0;
/// Side of the square draped over head and shoulders, mm.
const DRAPE_MM: f64 = 700.0;
/// Friction of the draped fabric (0–100): high, so it does not slide off the smooth head.
const DRAPE_FRICTION: f64 = 90.0;
/// The squares start this high above the head, mm.
const DROP_HEIGHT_MM: f64 = 40.0;
/// Frames simulated in the drape tests (5 s).
const DRAPE_FRAMES: usize = 300;
/// Particles may end at most this deep inside the body's distance field, mm.
const PENETRATION_TOL_MM: f64 = 0.5;
/// Triangle centres may end at most this deep inside the body, mm (the surface between particles
/// can sag a little into convex body parts).
const SURFACE_PENETRATION_TOL_MM: f64 = 3.0;
/// RMS particle speed below which the cloth counts as at rest, mm/s.
const REST_SPEED_MM_S: f64 = 10.0;
/// With cloth self-contacts the particle-sphere contacts of a folded drape keep a residual jitter
/// (as in the reference solver); the RMS speed stays below this, mm/s.
const SELF_CONTACT_JITTER_MM_S: f64 = 200.0;

fn female_avatar() -> Avatar {
    Avatar::parametric(&AvatarParams::preset(azam_avatar::Sex::Female))
}

fn square(id: &str, side: f64) -> Piece {
    let pts = vec![
        OutlinePoint::corner("a", 0.0, 0.0),
        OutlinePoint::corner("b", side, 0.0),
        OutlinePoint::corner("c", side, side),
        OutlinePoint::corner("d", 0.0, side),
    ];
    Piece::new(PieceId::new(id), id, pts)
}

fn square_project(ids: &[&str], side: f64) -> Project {
    let mut p = Project::new(None, Sex::Female, 0.0);
    for id in ids {
        p.add_piece(square(id, side));
    }
    p
}

/// Lays piece `ri` flat and horizontal at height `y`, centred on the body axis.
fn lay_flat(sim: &mut Simulation, ri: usize, y: f64) {
    let r = sim.records[ri].clone();
    let side = r.mesh.outline.total / 4.0;
    let mut x = sim.particles.x.clone();
    for i in 0..r.count {
        let (u, v) = (r.mesh.xy[2 * i], r.mesh.xy[2 * i + 1]);
        let k = (r.offset + i) * 3;
        x[k] = (u - side / 2.0) as f32;
        x[k + 1] = y as f32;
        x[k + 2] = (v - side / 2.0) as f32;
    }
    sim.set_positions(&x);
}

fn distance(av: &Avatar, p: [f64; 3]) -> f64 {
    av.grid.distance(glam::DVec3::from_array(p))
}

/// Drops a high-friction square over the head and shoulders and simulates 5 s; returns the
/// simulation and the peak RMS speed.
fn drape(av: &Avatar, self_collision: bool) -> (Simulation, f64) {
    let mut project = square_project(&["pc_sq"], DRAPE_MM);
    for f in project.fabrics.values_mut() {
        f.phys.friction = DRAPE_FRICTION;
    }
    let params = SimParams {
        self_collision,
        ..SimParams::default()
    };
    let mut sim = Simulation::build(&project, av, None, &params);
    lay_flat(&mut sim, 0, av.height() + DROP_HEIGHT_MM);
    let mut peak: f64 = 0.0;
    for _ in 0..DRAPE_FRAMES {
        sim.step(av, 1.0 / 60.0);
        peak = peak.max(sim.energy);
    }
    (sim, peak)
}

/// Checks the drape is finite, outside the body and hanging on the upper body.
fn check_outside(sim: &Simulation, av: &Avatar) {
    assert!(
        sim.particles.x.iter().all(|v| v.is_finite()),
        "positions stay finite"
    );
    let x = &sim.particles.x;
    let pos = |q: usize| {
        [
            f64::from(x[3 * q]),
            f64::from(x[3 * q + 1]),
            f64::from(x[3 * q + 2]),
        ]
    };
    let worst = (0..sim.particle_count())
        .map(|q| distance(av, pos(q)))
        .fold(f64::INFINITY, f64::min);
    assert!(
        worst >= -PENETRATION_TOL_MM,
        "a particle is {worst} mm inside the body"
    );
    let r = &sim.records[0];
    let mut worst_surface = f64::INFINITY;
    for t in r.mesh.tris.chunks_exact(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|v| pos(r.offset + v as usize));
        let centre = [0, 1, 2].map(|d| (a[d] + b[d] + c[d]) / 3.0);
        worst_surface = worst_surface.min(distance(av, centre));
    }
    assert!(
        worst_surface >= -SURFACE_PENETRATION_TOL_MM,
        "the cloth surface is {worst_surface} mm inside the body"
    );
    let lowest = (0..sim.particle_count())
        .map(|q| pos(q)[1])
        .fold(f64::INFINITY, f64::min);
    assert!(
        lowest > av.height() * 0.6,
        "the square hangs on the upper body, lowest point at {lowest} mm"
    );
}

#[test]
fn a_square_dropped_on_the_avatar_comes_to_rest_outside_the_body() {
    let av = female_avatar();
    let (sim, peak) = drape(&av, false);
    assert!(peak > 100.0, "the square falls (peak RMS speed {peak} mm/s)");
    assert!(
        sim.energy < REST_SPEED_MM_S,
        "the square comes to rest (RMS speed {} mm/s)",
        sim.energy
    );
    check_outside(&sim, &av);
}

#[test]
fn a_square_dropped_with_cloth_contacts_stays_outside_the_body() {
    let av = female_avatar();
    let (sim, _) = drape(&av, true);
    assert!(
        sim.energy < SELF_CONTACT_JITTER_MM_S,
        "the drape stays calm (RMS speed {} mm/s)",
        sim.energy
    );
    check_outside(&sim, &av);
}

#[test]
fn auto_stop_ends_a_settled_run() {
    let av = female_avatar();
    let project = square_project(&["pc_sq"], SQUARE_MM);
    let mut sim = Simulation::build(&project, &av, None, &SimParams::default());
    lay_flat(&mut sim, 0, av.height() + DROP_HEIGHT_MM);
    let values = SimPreset::Final.values();
    let mut stop = AutoStop::new();
    let mut reason = None;
    for _ in 0..(values.max_time * 60.0) as usize + 120 {
        sim.step(&av, 1.0 / 60.0);
        reason = stop.update(sim.time, sim.energy, sim.sew_gap, &values);
        if reason.is_some() {
            break;
        }
    }
    assert!(
        matches!(reason, Some(StopReason::Settled { after, .. }) if after < values.max_time),
        "run settles before the time limit: {reason:?}"
    );
}

#[test]
#[allow(clippy::float_cmp, reason = "positions and masses are copied, not computed")]
fn pins_hold_and_release() {
    let av = female_avatar();
    let project = square_project(&["pc_sq"], SQUARE_MM);
    let mut sim = Simulation::build(&project, &av, None, &SimParams::default());
    lay_flat(&mut sim, 0, av.height() + DROP_HEIGHT_MM);
    let corner = 0;
    let free_mass = sim.particles.inv_mass[corner];
    assert!(sim.pin(corner));
    assert!(!sim.pin(corner), "a particle is pinned once");
    let held = sim.particles.position(corner);
    for _ in 0..20 {
        sim.step(&av, 1.0 / 60.0);
    }
    assert_eq!(
        sim.particles.position(corner),
        held,
        "the pinned corner stays put"
    );
    assert!(sim.unpin(corner));
    assert_eq!(
        sim.particles.inv_mass[corner], free_mass,
        "unpinning restores the mass"
    );
    for _ in 0..5 {
        sim.step(&av, 1.0 / 60.0);
    }
    assert_ne!(sim.particles.position(corner), held, "the released corner moves");
}

#[test]
#[allow(clippy::float_cmp, reason = "positions and masses are copied, not computed")]
fn translating_a_piece_adds_no_velocity_and_reset_keeps_the_placement() {
    let av = female_avatar();
    let project = square_project(&["pc_sq"], SQUARE_MM);
    let fresh = Simulation::build(&project, &av, None, &SimParams::default());
    let mut sim = fresh.clone();
    for _ in 0..10 {
        sim.step(&av, 1.0 / 60.0);
    }
    sim.translate_piece(0, [0.0, 50.0, 0.0]);
    assert_eq!(
        sim.particles.x, sim.particles.prev,
        "a rigid move is not a motion"
    );
    sim.grab_at(5);
    sim.reset(&av);
    // reset re-arranges the piece and keeps its 3D placement (the move)
    for (i, (a, b)) in sim.particles.x.iter().zip(&fresh.particles.x).enumerate() {
        let want = if i % 3 == 1 { b + 50.0 } else { *b };
        assert!(
            (a - want).abs() < 1e-3,
            "reset puts the piece at its arrangement plus the move"
        );
    }
    assert!(sim.grab.is_none() && sim.frames == 0 && sim.gravity_scale.is_none());
    assert!(sim.particles.v.iter().all(|&v| v == 0.0));
}

#[test]
fn rebuilding_an_unchanged_project_keeps_the_drape() {
    let av = female_avatar();
    let project = square_project(&["pc_a", "pc_b"], SQUARE_MM);
    let mut sim = Simulation::build(&project, &av, None, &SimParams::default());
    for _ in 0..15 {
        sim.step(&av, 1.0 / 60.0);
    }
    let again = Simulation::build(&project, &av, Some(&sim), &SimParams::default());
    let a = &again.records[0];
    assert!(
        std::sync::Arc::ptr_eq(&a.mesh, &sim.records[0].mesh),
        "the mesh is reused"
    );
    assert_eq!(
        again.gravity_scale, sim.gravity_scale,
        "the gravity hold-back carries over"
    );
    let opts = BuildOptions {
        rearrange_ids: vec![PieceId::new("pc_b")],
        ..BuildOptions::default()
    };
    let re = Simulation::build_with(&project, &av, Some(&sim), &SimParams::default(), &opts);
    assert!(
        re.gravity_scale.is_none(),
        "re-arranging restarts the gravity hold-back"
    );
}

/// Two squares crossing so that the last 50 mm of the second poke through the first.
fn crossing_squares() -> (Simulation, Avatar) {
    let av = female_avatar();
    let project = square_project(&["pc_a", "pc_b"], SQUARE_MM);
    let opts = BuildOptions {
        project_out: false,
        ..BuildOptions::default()
    };
    let mut sim = Simulation::build_with(&project, &av, None, &SimParams::default(), &opts);
    let mut x = sim.particles.x.clone();
    for (ri, r) in sim.records.iter().enumerate() {
        for i in 0..r.count {
            let (u, v) = (r.mesh.xy[2 * i], r.mesh.xy[2 * i + 1]);
            let k = (r.offset + i) * 3;
            let p = if ri == 0 {
                [u - 200.0 + 0.013 * v, 1300.0 - v, 0.001 * u]
            } else {
                [0.3 * v - 40.0 + 0.01 * u, 1290.0 - 0.9 * v, 0.4 * u - 140.0]
            };
            x[k..k + 3].copy_from_slice(&p.map(|c| c as f32));
        }
    }
    sim.set_positions(&x);
    (sim, av)
}

#[test]
fn the_background_untangler_matches_the_synchronous_one() {
    let (sim, _) = crossing_squares();
    let expected = UntangleSetup::new(&sim).run(&sim.particles.x);
    assert!(
        expected.crossings > 0 && expected.resolved > 0 && expected.marked > 0,
        "{expected:?}"
    );
    let mut worker = UntangleWorker::spawn(&sim);
    let start = Instant::now();
    let result = loop {
        if let Some(r) = worker.tick(&sim) {
            break r;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "the worker answers");
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(result, expected);
    assert_eq!(worker.totals.runs, 1);
}

#[test]
#[allow(clippy::float_cmp, reason = "positions are copied, not computed")]
fn a_saved_pin_holds_its_spot_through_build_and_reset() {
    let av = female_avatar();
    let mut project = square_project(&["pc_sq"], SQUARE_MM);
    let spot = [150.0, 1300.0, 220.0];
    project
        .pieces
        .get_mut(&PieceId::new("pc_sq"))
        .expect("square")
        .pins
        .push(azam_doc::Pin {
            x: 0.0,
            y: 0.0,
            at: Some(spot),
            extra: serde_json::Map::new(),
        });
    let mut sim = Simulation::build(&project, &av, None, &SimParams::default());
    let q = sim.pins.vertex[0] as usize;
    let held = spot.map(|c| c as f32);
    assert_eq!(
        sim.particles.position(q),
        held,
        "the build puts the pinned cloth at its spot"
    );
    for _ in 0..10 {
        sim.step(&av, 1.0 / 60.0);
    }
    sim.reset(&av);
    assert_eq!(
        sim.particles.position(q),
        held,
        "a reset keeps it there, not at the arrangement"
    );
}

#[test]
fn a_new_arrangement_moves_only_that_piece_on_rebuild() {
    let av = female_avatar();
    let mut project = square_project(&["pc_a", "pc_b"], SQUARE_MM);
    let first = Simulation::build(&project, &av, None, &SimParams::default());
    let mut moved = first.clone();
    moved.translate_piece(0, [0.0, 0.0, 50.0]);
    moved.translate_piece(1, [0.0, 0.0, 50.0]);
    // the app records a 3D move on the piece; the re-slotted piece starts over
    project
        .pieces
        .get_mut(&PieceId::new("pc_a"))
        .expect("a")
        .arr
        .moved = Some([0.0, 0.0, 50.0]);
    project.pieces.get_mut(&PieceId::new("pc_b")).expect("b").arr.slot = "back".into();
    let rebuilt = Simulation::build(&project, &av, Some(&moved), &SimParams::default());
    let span = |s: &Simulation, ri: usize| {
        let r = &s.records[ri];
        s.particles.x[r.offset * 3..(r.offset + r.count) * 3].to_vec()
    };
    let ri = |s: &Simulation, id: &str| s.records.iter().position(|r| r.id.0 == id).expect("record");
    assert_eq!(
        span(&rebuilt, ri(&rebuilt, "pc_a")),
        span(&moved, ri(&moved, "pc_a")),
        "a moved piece whose move is saved keeps its place"
    );
    let fresh = Simulation::build(&project, &av, None, &SimParams::default());
    assert_eq!(
        span(&rebuilt, ri(&rebuilt, "pc_b")),
        span(&fresh, ri(&fresh, "pc_b")),
        "the re-slotted piece starts at its new arrangement"
    );
}

#[test]
fn turning_a_piece_keeps_its_height_and_distance_from_the_axis() {
    let av = female_avatar();
    let project = square_project(&["pc_sq"], SQUARE_MM);
    let mut sim = Simulation::build(&project, &av, None, &SimParams::default());
    let before = sim.particles.position(0);
    sim.turn_piece(0, std::f64::consts::FRAC_PI_2, [0.0, 0.0]);
    let after = sim.particles.position(0);
    assert!((after[1] - before[1]).abs() < 1e-4, "same height");
    let r = |p: [f32; 3]| f64::from(p[0]).hypot(f64::from(p[2]));
    assert!((r(after) - r(before)).abs() < 1e-3, "same distance from the axis");
    assert!(
        (f64::from(after[0]) - f64::from(before[2])).abs() < 1e-3,
        "a quarter turn moves z onto x"
    );
}

#[test]
fn an_unsaved_move_is_undone_by_a_rebuild() {
    let av = female_avatar();
    let project = square_project(&["pc_sq"], SQUARE_MM);
    let first = Simulation::build(&project, &av, None, &SimParams::default());
    let mut moved = first.clone();
    moved.turn_piece(0, 0.3, [0.0, 0.0]);
    moved.translate_piece(0, [10.0, 0.0, 40.0]);
    // the project never heard of the move (an undo, say): the piece goes back
    let rebuilt = Simulation::build(&project, &av, Some(&moved), &SimParams::default());
    for (a, b) in rebuilt.particles.x.iter().zip(&first.particles.x) {
        assert!((a - b).abs() < 0.01, "{a} vs {b}");
    }
}

/// The square of the drape tests, with a zipper half along its first edge if `zipped`.
fn drape_project(zipped: bool) -> Project {
    let mut project = square_project(&["pc_sq"], DRAPE_MM);
    for f in project.fabrics.values_mut() {
        f.phys.friction = DRAPE_FRICTION;
    }
    if zipped {
        azam_doc::add_zipper_half(
            &mut project,
            &PieceId::new("pc_sq"),
            &azam_geom::PointId::new("a"),
        )
        .expect("zipper on the first edge");
    }
    project
}

/// Mean relative stretch of the constraints along the first edge of the square after draping it
/// over the head (with a zipper half on that edge or without), and how many there are.
fn first_edge_strain(av: &Avatar, zipped: bool) -> (f64, usize) {
    let mut sim = Simulation::build(&drape_project(zipped), av, None, &SimParams::default());
    lay_flat(&mut sim, 0, av.height() + DROP_HEIGHT_MM);
    for _ in 0..DRAPE_FRAMES {
        sim.step(av, 1.0 / 60.0);
    }
    let r = &sim.records[0];
    let on_edge = |v: u32| {
        let v = v as usize - r.offset;
        v < r.mesh.boundary_count() && r.mesh.boundary_segment[v] == 0
    };
    let x = &sim.particles.x;
    let at = |v: u32| {
        let v = v as usize * 3;
        glam::DVec3::new(f64::from(x[v]), f64::from(x[v + 1]), f64::from(x[v + 2]))
    };
    let (mut sum, mut n) = (0.0, 0);
    for c in 0..sim.stretch.i.len() {
        let (i, j) = (sim.stretch.i[c], sim.stretch.j[c]);
        if on_edge(i) && on_edge(j) {
            sum += (at(i).distance(at(j)) / f64::from(sim.stretch.rest[c]) - 1.0).abs();
            n += 1;
        }
    }
    (sum / n.max(1) as f64, n)
}

#[test]
fn a_zipper_holds_its_edge_and_weighs_on_it() {
    let av = female_avatar();
    let (plain, n) = first_edge_strain(&av, false);
    let (zipped, m) = first_edge_strain(&av, true);
    assert!(
        n > 5 && n == m,
        "the same edge constraints in both runs ({n}, {m})"
    );
    assert!(
        zipped < plain * 0.8,
        "the zipper's tape stretches less than plain cloth: {zipped:.5} vs {plain:.5}"
    );
    // its weight: a half zipper's 11 g/m along the 700 mm edge
    let mass = |zipped: bool| {
        let sim = Simulation::build(&drape_project(zipped), &av, None, &SimParams::default());
        sim.particles
            .inv_mass
            .iter()
            .filter(|&&w| w > 0.0)
            .map(|&w| 1.0 / f64::from(w))
            .sum::<f64>()
    };
    let added = mass(true) - mass(false);
    assert!(
        (added - 0.011 * DRAPE_MM).abs() < 0.5,
        "the zipper adds {added:.2} g"
    );
}

#[test]
fn flattening_gives_back_the_pattern_or_its_stretch() {
    let av = female_avatar();
    let project = square_project(&["pc_sq"], SQUARE_MM);
    let mut sim = Simulation::build(&project, &av, None, &SimParams::default());
    let area = |pts: &[azam_geom::Vec2], sim: &Simulation| {
        let r = &sim.records[0];
        let poly: Vec<azam_geom::Vec2> = r.mesh.boundary.iter().map(|&v| pts[v as usize]).collect();
        azam_geom::signed_area(&poly).abs()
    };
    let flat = azam_sim::flatten_piece(&sim.piece_views().next().expect("a piece"));
    let a0 = area(&flat, &sim);
    assert!(
        (a0 - SQUARE_MM * SQUARE_MM).abs() < 0.01 * SQUARE_MM * SQUARE_MM,
        "as drafted: {a0}"
    );
    // stretched 10 % each way about its middle: the flat shape is 21 % larger
    let n = sim.particles.x.len() / 3;
    let mid = (0..3)
        .map(|k| (0..n).map(|q| sim.particles.x[q * 3 + k]).sum::<f32>() / n as f32)
        .collect::<Vec<_>>();
    let mut x = sim.particles.x.clone();
    for q in 0..n {
        for k in 0..3 {
            x[q * 3 + k] = mid[k] + (x[q * 3 + k] - mid[k]) * 1.1;
        }
    }
    sim.set_positions(&x);
    let flat = azam_sim::flatten_piece(&sim.piece_views().next().expect("a piece"));
    let ratio = area(&flat, &sim) / a0;
    assert!((ratio - 1.21).abs() < 0.02, "stretched: {ratio}");
}
