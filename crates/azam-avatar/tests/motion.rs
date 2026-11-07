//! Re-posing avatars for motion: the mesh, joints, collision grid and surface velocity follow
//! the pose, for the parametric mannequin and (when the bundled pack is present) the
//! morph-target body.

use azam_avatar::makehuman::DEFAULT_MH_GRID_CELL_MM;
use azam_avatar::{Avatar, AvatarParams, AvatarPose, MhModel, Motion, Sex, pose_at};
use glam::DVec3;

fn avatars() -> Vec<(&'static str, Avatar)> {
    let params = AvatarParams::preset(Sex::Female);
    let mut out = vec![("mannequin", Avatar::parametric(&params))];
    match MhModel::load_bundled() {
        Some(m) => {
            let m = m.expect("the bundled pack parses");
            out.push((
                "makehuman",
                Avatar::makehuman(&m, &params, DEFAULT_MH_GRID_CELL_MM),
            ));
        }
        None => eprintln!("no bundled avatar pack: checking the mannequin only"),
    }
    out
}

fn lowest(av: &Avatar) -> f64 {
    av.mesh
        .positions
        .chunks_exact(3)
        .map(|p| f64::from(p[1]))
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn the_rest_pose_reproduces_the_avatar() {
    for (name, av) in avatars() {
        assert!(av.can_pose(), "{name}");
        assert!(av.pose().is_rest());
        let p = av.posed_in(&AvatarPose::REST, &av.grid.frame());
        assert_eq!(p.grid.dims, av.grid.dims, "{name}");
        let worst = p
            .mesh
            .positions
            .iter()
            .zip(&av.mesh.positions)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(worst < 1e-3, "{name}: moved {worst} mm");
        // the grids agree near the surface
        let skin = av
            .body_collision_mesh()
            .map_or(av.mesh.vertex_count(), |b| b.body_vertex_count);
        for v in (0..skin).step_by(97) {
            let q = DVec3::new(
                f64::from(av.mesh.positions[v * 3]),
                f64::from(av.mesh.positions[v * 3 + 1]),
                f64::from(av.mesh.positions[v * 3 + 2]),
            );
            let (a, b) = (p.grid.distance(q), av.grid.distance(q));
            assert!((a - b).abs() < 1.5, "{name}: {a} vs {b} at vertex {v}");
        }
    }
}

#[test]
fn raised_arms_lift_the_hands_and_the_grid_follows() {
    for (name, av) in avatars() {
        let pose = pose_at(Motion::RaiseArms, Motion::RaiseArms.period() / 2.0);
        let p = av.posed(&pose);
        for (l, r) in [
            (&p.joints.left, &av.joints.left),
            (&p.joints.right, &av.joints.right),
        ] {
            assert!(l.wrist.y > l.shoulder.y + 100.0, "{name}: wrist {:?}", l.wrist);
            assert!(
                (l.shoulder - r.shoulder).length() < 1.0,
                "{name}: the shoulder stays"
            );
            let mid = (l.elbow + l.wrist) / 2.0;
            assert!(p.grid.distance(mid) < -5.0, "{name}: the forearm is in the grid");
            let old_mid = (r.elbow + r.wrist) / 2.0;
            assert!(p.grid.distance(old_mid) > 5.0, "{name}: the old forearm is gone");
        }
        assert!(lowest(&p).abs() < 1.0, "{name}: stays on the floor");
    }
}

#[test]
fn sitting_bends_hips_and_knees_and_lowers_the_body() {
    for (name, av) in avatars() {
        let p = av.posed(&pose_at(Motion::Sit, Motion::Sit.period() / 2.0));
        let (k, k0) = (p.joints.left.knee, av.joints.left.knee);
        assert!(k.z > k0.z + 200.0, "{name}: knee {k:?} vs {k0:?}");
        assert!(
            p.joints.left.hip.y < av.joints.left.hip.y - 150.0,
            "{name}: hips lowered"
        );
        assert!(lowest(&p).abs() < 1.0, "{name}: feet on the floor");
        let thigh = (p.joints.left.hip + p.joints.left.knee) / 2.0;
        assert!(p.grid.distance(thigh) < -10.0, "{name}: thigh in the grid");
    }
}

#[test]
fn the_turntable_turns_the_body_and_reports_its_velocity() {
    for (name, av) in avatars() {
        let frame = av.motion_frame(Motion::Turntable);
        let t = 3.0;
        let dt = 1.0 / 60.0;
        let a = av.posed_in(&pose_at(Motion::Turntable, t), &frame);
        let b = av
            .posed_in(&pose_at(Motion::Turntable, t + dt), &frame)
            .with_velocity_from(&a, dt);
        assert_eq!(a.grid.dims, b.grid.dims, "{name}: one layout for the motion");
        let turn = b.pose().turn.to_radians();
        let s0 = av.joints.left.shoulder;
        let s = b.joints.left.shoulder;
        let axis_z = a.joints.left.shoulder.z.midpoint(a.joints.right.shoulder.z);
        assert!((s.y - s0.y).abs() < 1.0, "{name}");
        let r = s0.x;
        let expect = DVec3::new(r * turn.cos(), s0.y, axis_z - r * turn.sin());
        assert!(
            (s - expect).length() < 25.0,
            "{name}: shoulder {s:?}, expected {expect:?}"
        );
        // the skin moves at ω × r
        let v = b.velocity.as_ref().expect("velocity");
        let w = (360.0 / Motion::Turntable.period()).to_radians();
        let sample = v.sample(s);
        assert!(
            (sample.length() - w * r).abs() < 0.25 * w * r,
            "{name}: speed {} vs {}",
            sample.length(),
            w * r
        );
        assert!(b.grid.distance(DVec3::new(0.0, av.profile.levels.waist, axis_z)) < -40.0);
    }
}

#[test]
fn walking_keeps_a_fixed_grid_layout_and_finite_values() {
    for (name, av) in avatars() {
        let frame = av.motion_frame(Motion::Walk);
        for i in 0..8 {
            let p = av.posed_in(&pose_at(Motion::Walk, 1.0 + f64::from(i) * 0.15), &frame);
            assert_eq!(p.grid.frame(), frame, "{name}");
            assert!(p.grid.data.iter().all(|d| d.is_finite()), "{name}");
            assert!(p.mesh.positions.iter().all(|d| d.is_finite()), "{name}");
            // every skin vertex lies inside the grid layout
            let hi = frame.max();
            assert!(p.mesh.positions.chunks_exact(3).all(|q| {
                let q = DVec3::new(f64::from(q[0]), f64::from(q[1]), f64::from(q[2]));
                q.cmpge(frame.origin).all() && q.cmple(hi).all()
            }));
        }
    }
}

/// Closest point on triangle `abc` to `p` (Ericson).
fn closest_point(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let den = 1.0 / (va + vb + vc);
    a + ab * (vb * den) + ac * (vc * den)
}

#[test]
fn the_fast_pose_update_matches_a_full_rebuild() {
    // the morph-target body: the fast grid against brute-force distances to the posed triangles
    // near the skin and against the full flood-filled grid away from it
    for (name, av) in avatars()
        .into_iter()
        .filter(|(_, a)| a.body_collision_mesh().is_some())
    {
        for (motion, t, band_tol) in [(Motion::Walk, 1.4, 0.01), (Motion::Turntable, 3.3, 0.01)] {
            let frame = av.motion_frame(motion);
            let p = av.posed_in(&pose_at(motion, t), &frame);
            let what = format!("{name} {}", motion.label());
            let pos = |v: usize| {
                DVec3::new(
                    f64::from(p.mesh.positions[v * 3]),
                    f64::from(p.mesh.positions[v * 3 + 1]),
                    f64::from(p.mesh.positions[v * 3 + 2]),
                )
            };
            let nrm = |v: usize| {
                DVec3::new(
                    f64::from(p.mesh.normals[v * 3]),
                    f64::from(p.mesh.normals[v * 3 + 1]),
                    f64::from(p.mesh.normals[v * 3 + 2]),
                )
            };
            // near the skin: grid nodes against brute-force distances to the posed triangles
            let skin = p
                .body_collision_mesh()
                .map_or(p.mesh.vertex_count(), |b| b.body_vertex_count);
            let tris = &p.mesh.indices;
            let mut checked = 0;
            for v in (0..skin).step_by(skin / 60) {
                for offset in [-5.0, 3.0, 6.0] {
                    let q = pos(v) + nrm(v) * offset;
                    let f = ((q - frame.origin) / frame.cell).round();
                    let node = frame.origin + f * frame.cell;
                    let (mut best, mut sign) = (f64::INFINITY, 1.0);
                    for t in tris.chunks_exact(3) {
                        let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize);
                        let x = closest_point(node, pos(a), pos(b), pos(c));
                        let d = (node - x).length();
                        if d < best {
                            best = d;
                            let n = nrm(a) + nrm(b) + nrm(c);
                            sign = if (node - x).dot(n) >= 0.0 { 1.0 } else { -1.0 };
                        }
                    }
                    if best > 8.0 {
                        continue;
                    }
                    let got = p.grid.distance(node);
                    let tol = band_tol;
                    assert!(
                        (got - sign * best).abs() < tol || (best < 1.0 && (got.abs() - best).abs() < tol),
                        "{what}: {got:.3} vs {:.3} at {node:?}",
                        sign * best
                    );
                    checked += 1;
                }
            }
            assert!(checked > 60, "{what}: {checked} nodes checked");
            // away from the skin: the same side as the full (flood-filled) grid, below the chin
            // (the flood fill also fills the closed mouth cavity, which the skin's normals face)
            let chin = av.profile.levels.chin;
            let reference = azam_avatar::mesh_sdf(&p.mesh.positions, tris, frame.cell, 90.0, &p.mesh.normals);
            let mut differ = 0;
            let [nx, ny, nz] = frame.dims;
            for k in (0..nz).step_by(3) {
                for j in (0..ny).step_by(3) {
                    for i in (0..nx).step_by(3) {
                        let node = frame.origin + DVec3::new(i as f64, j as f64, k as f64) * frame.cell;
                        let r = reference.distance(node);
                        if node.y < chin
                            && r.abs() > 15.0
                            && r < 900.0
                            && (p.grid.distance(node) > 0.0) != (r > 0.0)
                        {
                            differ += 1;
                        }
                    }
                }
            }
            assert_eq!(differ, 0, "{what}: cells on the wrong side");
        }
    }
}

#[test]
fn the_turned_mannequin_matches_its_exact_distance() {
    // the mannequin's grid comes from its primitives, turned with the body
    let params = AvatarParams::preset(Sex::Female);
    let av = Avatar::parametric(&params);
    let body = azam_avatar::ParametricBody::new(&params);
    let motion = Motion::Turntable;
    let p = av.posed_in(&pose_at(motion, 3.3), &av.motion_frame(motion));
    let turn = p.pose().turn.to_radians();
    let cz = av.profile.torso.eval(av.profile.levels.waist).center_z;
    let mut checked = 0;
    for v in (0..p.mesh.vertex_count()).step_by(97) {
        let q = DVec3::new(
            f64::from(p.mesh.positions[v * 3]),
            f64::from(p.mesh.positions[v * 3 + 1]),
            f64::from(p.mesh.positions[v * 3 + 2]),
        );
        for offset in [-6.0, 0.0, 6.0] {
            let n = DVec3::new(
                f64::from(p.mesh.normals[v * 3]),
                f64::from(p.mesh.normals[v * 3 + 1]),
                f64::from(p.mesh.normals[v * 3 + 2]),
            );
            // the nearest grid value, where the grid holds the exact distance
            let g = &p.grid;
            let x = g.origin + ((q + n * offset - g.origin) / g.cell).round() * g.cell;
            // back to the rest pose: turn by -turn about the vertical axis through (0, cz)
            let (s, c) = (-turn).sin_cos();
            let (dx, dz) = (x.x, x.z - cz);
            let rest = DVec3::new(c * dx + s * dz, x.y, -s * dx + c * dz + cz);
            let exact = body.distance(rest);
            let got = p.grid.distance(x);
            assert!((got - exact).abs() < 0.01, "{got:.3} vs {exact:.3} at {x:?}");
            checked += 1;
        }
    }
    assert!(checked > 100);
}
