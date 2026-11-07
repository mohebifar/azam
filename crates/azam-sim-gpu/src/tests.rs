//! GPU checks of single passes against CPU computations. Every test passes with a note when no
//! GPU adapter is available.

#![allow(clippy::float_cmp, reason = "copies and untouched values compare exactly")]
#![allow(clippy::cast_possible_wrap, reason = "fixed-point words and small counts")]

use std::sync::OnceLock;

use azam_avatar::{Avatar, AvatarParams, BodyCollisionMesh};
use azam_doc::{Piece, PieceId, Project, Sex, Template, from_template};
use azam_geom::OutlinePoint;
use azam_sim::{BodyTriangles, SimParams, Simulation};

use crate::buffers::Buf;
use crate::frame::Dispatch;
use crate::layout::{
    SCAN_WORKGROUP, STATE_LEN, STATE_PIECE_GRAVITY, TRIANGLE_HASH_BUCKETS, from_fixed, to_fixed,
};
use crate::pack::{DESC_APPLY_SHIFT, bucket};
use crate::passes::Pass;
use crate::{GpuOptions, GpuSim, GpuSolver};

/// Frame time of the pass tests, s.
const DT: f64 = 1.0 / 60.0;
/// Side of the test square, mm.
const SQUARE_MM: f64 = 300.0;
/// Height at which the square is laid flat, well clear of the avatar, mm.
const FLAT_Y_MM: f32 = 2500.0;

fn solver() -> Option<&'static GpuSolver> {
    static S: OnceLock<Option<GpuSolver>> = OnceLock::new();
    S.get_or_init(|| match GpuSolver::new_default() {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("note: GPU unavailable ({e}); GPU pass tests skipped");
            None
        }
    })
    .as_ref()
}

fn avatar() -> &'static Avatar {
    static A: OnceLock<Avatar> = OnceLock::new();
    A.get_or_init(|| Avatar::parametric(&AvatarParams::preset(azam_avatar::Sex::Female)))
}

/// A simulation of one square laid flat at [`FLAT_Y_MM`].
fn flat_square() -> Simulation {
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
        x[k] = r.mesh.xy[2 * i] as f32;
        x[k + 1] = FLAT_Y_MM;
        x[k + 2] = r.mesh.xy[2 * i + 1] as f32;
    }
    sim.set_positions(&x);
    sim
}

fn floats(words: &[u32]) -> Vec<f32> {
    words.iter().map(|&w| f32::from_bits(w)).collect()
}

fn read4(g: &GpuSim, which: Buf, n: usize) -> Vec<f32> {
    floats(&g.read_words(which, 0, 4 * n as u64).expect("read back"))
}

fn d(pass: Pass, count: u32) -> Dispatch {
    Dispatch { pass, count, desc: 0 }
}

#[test]
fn integration_matches_the_cpu_formula() {
    let Some(solver) = solver() else { return };
    let mut sim = flat_square();
    let n = sim.particle_count();
    for q in 0..n {
        let s = q as f32;
        sim.particles.v[3 * q] = 100.0 * (s * 0.37).sin();
        sim.particles.v[3 * q + 1] = 50.0 * (s * 0.11).cos();
        sim.particles.v[3 * q + 2] = if q == 3 { 9000.0 } else { -20.0 };
    }
    assert!(sim.pin(0));
    let mut g = solver.upload(&sim, avatar()).expect("upload");
    g.run_passes(&mut sim, DT, &[d(Pass::Integrate, n as u32)]);
    let (x, p, v) = (
        read4(&g, Buf::X, n),
        read4(&g, Buf::Prev, n),
        read4(&g, Buf::Vel, n),
    );
    let pr = &sim.particles;
    let h = DT / f64::from(sim.params.substeps);
    let gravity = -sim.params.gravity;
    for q in 0..n {
        let x0 = [0, 1, 2].map(|c| f64::from(pr.x[3 * q + c]));
        for c in 0..3 {
            assert_eq!(p[4 * q + c], pr.x[3 * q + c], "substep start of {q}");
        }
        if pr.inv_mass[q] == 0.0 {
            for c in 0..3 {
                assert_eq!(x[4 * q + c], pr.x[3 * q + c], "fixed particle {q} stays");
            }
            continue;
        }
        let dmp = (1.0 - (f64::from(pr.damping[q]) + sim.params.air) * h).max(0.0);
        let v0 = [0, 1, 2].map(|c| f64::from(pr.v[3 * q + c]));
        let mut ve = [v0[0] * dmp, (v0[1] + gravity * h) * dmp, v0[2] * dmp];
        let sp = (ve[0] * ve[0] + ve[1] * ve[1] + ve[2] * ve[2]).sqrt();
        if sp > 6000.0 {
            ve = ve.map(|c| c * 6000.0 / sp);
        }
        for c in 0..3 {
            let got_v = f64::from(v[4 * q + c]);
            assert!(
                (got_v - ve[c]).abs() <= 1e-4 * (1.0 + ve[c].abs()),
                "v[{q}][{c}] {got_v} vs {}",
                ve[c]
            );
            let xe = x0[c] + ve[c] * h;
            let got_x = f64::from(x[4 * q + c]);
            assert!((got_x - xe).abs() <= 1e-3, "x[{q}][{c}] {got_x} vs {xe}");
        }
        assert_eq!(x[4 * q + 3], pr.inv_mass[q], "inverse mass kept");
    }
    // the fast particle was clamped to the speed limit
    let v3 = [v[12], v[13], v[14]].map(f64::from);
    let s3 = (v3[0] * v3[0] + v3[1] * v3[1] + v3[2] * v3[2]).sqrt();
    assert!((s3 - 6000.0).abs() < 0.5, "clamped speed {s3}");
}

#[test]
fn triangle_hash_matches_cpu_counts_and_entries() {
    let Some(solver) = solver() else { return };
    let mut sim = flat_square();
    let mut g = solver.upload(&sim, avatar()).expect("upload");
    let fp = *g.frame_params();
    let nt = fp.n_tri;
    let hs = TRIANGLE_HASH_BUCKETS;
    g.run_passes(
        &mut sim,
        DT,
        &[
            d(Pass::HashClear, hs),
            d(Pass::HashCount, nt),
            d(Pass::Scan1, hs),
            d(Pass::Scan2, SCAN_WORKGROUP),
            d(Pass::Scan3, hs),
            d(Pass::Scatter, nt),
        ],
    );
    // CPU: every triangle in every cell of its bounds grown by the search radius
    let x = &sim.particles.x;
    let tris = azam_sim::TriangleContacts::new(&sim.records, &sim.particles, &sim.sewing).tris;
    let mut lists = vec![Vec::new(); hs as usize];
    for t in 0..nt as usize {
        let corner = |c: usize, k: usize| x[3 * tris[3 * t + c] as usize + k];
        let lo =
            |k: usize| ((corner(0, k).min(corner(1, k)).min(corner(2, k)) - fp.radius) / fp.cell).floor();
        let hi =
            |k: usize| ((corner(0, k).max(corner(1, k)).max(corner(2, k)) + fp.radius) / fp.cell).floor();
        let (l, h) = ([lo(0), lo(1), lo(2)], [hi(0), hi(1), hi(2)]);
        let cells = (h[0] - l[0] + 1.0) * (h[1] - l[1] + 1.0) * (h[2] - l[2] + 1.0);
        if cells > 64.0 {
            continue;
        }
        let mut i = l[0];
        while i <= h[0] {
            let mut j = l[1];
            while j <= h[1] {
                let mut k = l[2];
                while k <= h[2] {
                    lists[bucket(i, j, k, hs)].push(t as u32);
                    k += 1.0;
                }
                j += 1.0;
            }
            i += 1.0;
        }
    }
    let total: usize = lists.iter().map(Vec::len).sum();
    assert!(
        total > nt as usize,
        "the square covers several cells per triangle"
    );
    let starts = g
        .read_words(Buf::Hash, 0, u64::from(hs) + 1)
        .expect("read starts");
    assert_eq!(starts[hs as usize] as usize, total, "grand total");
    let entries = g
        .read_words(Buf::Hash, u64::from(hs) + 1, total as u64)
        .expect("read entries");
    let mut start = 0;
    for (b, list) in lists.iter_mut().enumerate() {
        assert_eq!(starts[b] as usize, start, "start of bucket {b}");
        let mut got = entries[start..start + list.len()].to_vec();
        got.sort_unstable();
        list.sort_unstable();
        assert_eq!(&got, list, "entries of bucket {b}");
        start += list.len();
    }
}

#[test]
fn apply_averages_fixed_point_corrections() {
    let Some(solver) = solver() else { return };
    let mut sim = flat_square();
    let n = sim.particle_count();
    let mut g = solver.upload(&sim, avatar()).expect("upload");
    // particle 5 gets three corrections, particle 9 one
    let parts5 = [[0.25_f32, -1.0, 2.0], [0.5, 0.125, -0.75], [-0.003, 0.0, 1.0]];
    let parts9 = [[-3.062_5_f32, 0.0001, 7.5]];
    let mut words = vec![0_u32; 4 * n];
    for (q, parts) in [(5_usize, &parts5[..]), (9, &parts9[..])] {
        for p in parts {
            for c in 0..3 {
                words[4 * q + c] = (words[4 * q + c] as i32 + to_fixed(p[c])) as u32;
            }
            words[4 * q + 3] += 1;
        }
    }
    g.write_words(Buf::Delta, 0, &words);
    g.run_passes(
        &mut sim,
        DT,
        &[Dispatch {
            pass: Pass::Apply,
            count: n as u32,
            desc: DESC_APPLY_SHIFT,
        }],
    );
    let (x, p) = (read4(&g, Buf::X, n), read4(&g, Buf::Prev, n));
    for (q, parts) in [(5_usize, &parts5[..]), (9, &parts9[..])] {
        for c in 0..3 {
            let mean: f32 = parts.iter().map(|v| v[c]).sum::<f32>() / parts.len() as f32;
            let fixed: i32 = parts.iter().map(|v| to_fixed(v[c])).sum();
            let expect = from_fixed(fixed, parts.len() as i32);
            assert!(
                (expect - mean).abs() <= 0.5 / crate::FIXED_POINT_SCALE,
                "fixed point is exact to half a step"
            );
            let got = x[4 * q + c] - sim.particles.x[3 * q + c];
            assert!(
                (got - expect).abs() < 1e-3,
                "x[{q}][{c}] moved {got}, expected {expect}"
            );
            let got_p = p[4 * q + c] - sim.particles.prev[3 * q + c];
            assert!(
                (got_p - expect).abs() < 1e-3,
                "substep start of {q} moved {got_p}"
            );
        }
    }
    assert_eq!(x[0], sim.particles.x[0], "untouched particles stay");
    let left = g
        .read_words(Buf::Delta, 0, 4 * n as u64)
        .expect("read accumulators");
    assert!(left.iter().all(|&w| w == 0), "the accumulators are cleared");
}

#[test]
fn skin_collision_matches_the_cpu_push() {
    let Some(solver) = solver() else { return };
    let mut sim = flat_square();
    let r = sim.records[0].clone();
    // a skin vertex 1 mm under the centroid of triangle 7, pointing up
    let t = 7;
    let corners: Vec<usize> = r.mesh.tris[3 * t..3 * t + 3]
        .iter()
        .map(|&v| r.offset + v as usize)
        .collect();
    let centroid = [0, 1, 2].map(|c| corners.iter().map(|&q| sim.particles.x[3 * q + c]).sum::<f32>() / 3.0);
    let positions = [centroid[0], centroid[1] - 1.0, centroid[2]];
    let normals = [0.0_f32, 1.0, 0.0];
    let skin = BodyCollisionMesh {
        positions: &positions,
        normals: &normals,
        body_vertex_count: 1,
    };
    let n = sim.particle_count();
    let mut g = solver
        .upload_with(&sim, &avatar().grid, Some(&skin), GpuOptions::default())
        .expect("upload");
    let nt = g.triangle_count() as u32;
    g.run_passes(
        &mut sim,
        DT,
        &[
            d(Pass::BodyTri, nt),
            Dispatch {
                pass: Pass::Apply,
                count: n as u32,
                desc: DESC_APPLY_SHIFT,
            },
        ],
    );
    let (x, p) = (read4(&g, Buf::X, n), read4(&g, Buf::Prev, n));
    // CPU
    let mut tris = Vec::new();
    let mut margin = Vec::new();
    for t in r.mesh.tris.chunks_exact(3) {
        let c = [t[0], t[1], t[2]].map(|v| v + r.offset as u32);
        tris.extend(c);
        margin.push(
            c.iter()
                .map(|&q| sim.particles.margin[q as usize])
                .fold(f32::INFINITY, f32::min),
        );
    }
    let mut cpu = sim.particles.clone();
    let fixes =
        BodyTriangles::from_triangles(tris, margin).collide_with_skin(&mut cpu, &positions, &normals, 1);
    assert_eq!(fixes, 1, "the CPU pushes exactly one triangle");
    let mut moved = 0;
    for q in 0..n {
        for c in 0..3 {
            let (got, want) = (x[4 * q + c], cpu.x[3 * q + c]);
            assert!((got - want).abs() < 1e-3, "x[{q}][{c}] {got} vs CPU {want}");
            let (got_p, want_p) = (p[4 * q + c], cpu.prev[3 * q + c]);
            assert!(
                (got_p - want_p).abs() < 1e-3,
                "prev[{q}][{c}] {got_p} vs CPU {want_p}"
            );
        }
        if (cpu.x[3 * q + 1] - sim.particles.x[3 * q + 1]).abs() > 1e-4 {
            moved += 1;
        }
    }
    assert_eq!(moved, 3, "the triangle's three corners move");
}

#[test]
fn seam_gap_and_gravity_hold_back_match_the_cpu() {
    let Some(solver) = solver() else { return };
    let project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    let av = avatar();
    let mut sim = Simulation::build(&project, av, None, &SimParams::default());
    assert!(!sim.sewing.is_empty());
    let mut g = solver.upload(&sim, av).expect("upload");
    let nsew = g.frame_params().n_sew;
    g.run_passes(&mut sim, DT, &[d(Pass::Gap, nsew.div_ceil(2)), d(Pass::Grav, 1)]);
    let st = floats(&g.read_words(Buf::State, 0, STATE_LEN as u64).expect("read state"));
    // the CPU computes the same hold-back at the start of its frame
    let mut cpu = sim.clone();
    cpu.step(av, DT);
    let gap = cpu.sew_gap;
    assert!(gap > 20.0, "the seams start open ({gap} mm)");
    assert!(
        (f64::from(st[1]) - gap).abs() < 0.07,
        "seam gap {} vs CPU {gap}",
        st[1]
    );
    let scale = cpu.gravity_scale.expect("hold-back set");
    assert!(
        (f64::from(st[0]) - scale).abs() < 0.003,
        "hold-back {} vs CPU {scale}",
        st[0]
    );
    for (r, &pg) in cpu.piece_gravity.iter().enumerate() {
        let got = st[STATE_PIECE_GRAVITY + r];
        assert!((got - pg).abs() < 0.003, "piece {r} hold-back {got} vs CPU {pg}");
    }
}

#[test]
fn a_moving_body_is_swapped_in_between_frames() {
    let Some(solver) = solver() else { return };
    let rest = avatar();
    let project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    let mut sim = Simulation::build(&project, rest, None, &SimParams::default());
    let mut g = solver.upload(&sim, rest).expect("upload");
    for _ in 0..30 {
        g.step_frame(&mut sim, DT).expect("frame");
    }
    let motion = azam_avatar::Motion::Turntable;
    let frame = rest.motion_frame(motion);
    let mut prev = rest.posed_in(&azam_avatar::pose_at(motion, 0.0), &frame);
    for i in 1..=90 {
        let next = rest
            .posed_in(&azam_avatar::pose_at(motion, f64::from(i) * DT), &frame)
            .with_velocity_from(&prev, DT);
        g.update_avatar(&next).expect("body update");
        g.step_frame(&mut sim, DT).expect("frame");
        prev = next;
    }
    let f = g.frame_params();
    assert_eq!(f.grid_dims, frame.dims.map(|d| d as u32));
    assert_eq!(f.body_motion, 1);
    assert!(sim.particles.x.iter().all(|v| v.is_finite()));
    let worst = sim
        .particles
        .x
        .chunks_exact(3)
        .map(|p| {
            prev.grid.distance(glam::DVec3::new(
                f64::from(p[0]),
                f64::from(p[1]),
                f64::from(p[2]),
            ))
        })
        .fold(f64::INFINITY, f64::min);
    assert!(worst > -3.0, "a particle is {worst:.2} mm inside the moving body");
    // back to rest: the velocity is dropped
    g.update_avatar(rest).expect("body update");
    assert_eq!(g.frame_params().body_motion, 0);
    assert_eq!(g.frame_params().grid_dims, rest.grid.dims.map(|d| d as u32));
}
