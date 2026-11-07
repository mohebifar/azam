//! Checks of the bundled morph-target pack (`assets/makehuman/makehuman-cc0.bin`, built from
//! MakeHuman's CC0 assets by `tools/mhpack`): fit accuracy for the presets and size charts, a
//! finite mesh and a sane collision grid. Skipped when the pack is absent.

use std::sync::OnceLock;

use azam_avatar::makehuman::DEFAULT_MH_GRID_CELL_MM;
use azam_avatar::{Avatar, AvatarKind, AvatarParams, MEASUREMENTS, MhModel, Sex, size_chart};
use glam::DVec3;

/// A fitted measurement may miss its wanted value by at most this much, in mm (the avatar
/// panel flags larger deviations).
const FIT_TOL_MM: f64 = 3.0;

fn model() -> Option<&'static MhModel> {
    static M: OnceLock<Option<MhModel>> = OnceLock::new();
    M.get_or_init(|| {
        let pack = MhModel::load_bundled().map(|r| r.expect("the bundled pack parses"));
        if pack.is_none() {
            eprintln!("skipping: no bundled avatar pack");
        }
        pack
    })
    .as_ref()
}

fn check_fit(m: &MhModel, params: &AvatarParams, what: &str) {
    let body = m.build_body(params);
    let r = &body.report;
    for (i, d) in MEASUREMENTS.iter().enumerate() {
        let measured = r.measured.0[i];
        assert!(
            measured.is_finite() && measured > 0.0,
            "{what} {}: {measured}",
            d.key
        );
        if let Some(want) = r.fit.wanted[i] {
            assert!(
                (measured - want).abs() <= FIT_TOL_MM,
                "{what} {}: measured {measured:.2}, wanted {want:.2}",
                d.key
            );
        }
    }
}

#[test]
fn pack_has_every_fit_target_and_pose_weights() {
    let Some(m) = model() else { return };
    assert!(
        m.missing_fit_targets().is_empty(),
        "{:?}",
        m.missing_fit_targets()
    );
    assert_eq!(m.body_vertex_count, 13380);
    let pw = &m.pose_weights;
    for w in [
        &pw.lower_arm_left,
        &pw.lower_arm_right,
        &pw.lower_leg_left,
        &pw.lower_leg_right,
        &pw.spine,
    ] {
        assert_eq!(w.len(), m.vertex_count);
    }
    for j in ["joint-neck", "joint-pelvis", "joint-spine-3"] {
        assert!(m.has_joint(j), "{j}");
    }
}

#[test]
fn presets_fit_their_measurements() {
    let Some(m) = model() else { return };
    for sex in [Sex::Female, Sex::Male] {
        check_fit(m, &AvatarParams::preset(sex), &format!("{sex:?} preset"));
    }
}

#[test]
fn size_charts_fit_their_measurements() {
    let Some(m) = model() else { return };
    for sex in [Sex::Female, Sex::Male] {
        for size in size_chart(sex) {
            let mut p = AvatarParams::preset(sex);
            p.apply_size(size);
            check_fit(m, &p, &format!("{sex:?} {}", size.name));
        }
    }
}

#[test]
fn every_optional_measurement_can_be_fitted() {
    let Some(m) = model() else { return };
    let mut p = AvatarParams::preset(Sex::Female);
    p.across_front = Some(265.0);
    p.back_length = Some(425.0);
    p.waist_to_hip = Some(205.0);
    p.wrist = Some(150.0);
    p.knee_height = Some(480.0);
    p.knee = Some(355.0);
    p.calf = Some(345.0);
    p.ankle = Some(220.0);
    check_fit(m, &p, "female with every measurement");
}

#[test]
fn avatar_mesh_and_grid_are_sane() {
    let Some(m) = model() else { return };
    for sex in [Sex::Female, Sex::Male] {
        let params = AvatarParams::preset(sex);
        let av = Avatar::makehuman(m, &params, DEFAULT_MH_GRID_CELL_MM);
        assert_eq!(av.kind, AvatarKind::MakeHuman);
        assert!(
            av.mesh.positions.iter().all(|v| v.is_finite()),
            "{sex:?}: positions"
        );
        assert!(av.mesh.normals.iter().all(|v| v.is_finite()), "{sex:?}: normals");
        assert!(av.grid.data.iter().all(|v| v.is_finite()), "{sex:?}: grid");
        let lowest = av
            .mesh
            .positions
            .chunks_exact(3)
            .map(|p| p[1])
            .fold(f32::INFINITY, f32::min);
        assert!(lowest.abs() < 0.5, "{sex:?}: grounded at {lowest}");
        assert!(
            (av.height() - params.height).abs() < 1.0,
            "{sex:?}: height {}",
            av.height()
        );

        let lv = av.profile.levels;
        let inside = |p: DVec3, what: &str| {
            let d = av.grid.distance(p);
            assert!(d < -20.0, "{sex:?} {what}: {d} at {p:?}");
        };
        let outside = |p: DVec3, what: &str| {
            let d = av.grid.distance(p);
            assert!(d > 20.0, "{sex:?} {what}: {d}");
        };
        // front and back of the midline at height y, from the skin vertices
        let extent = |y: f64| {
            av.mesh.positions[..av.body_collision_mesh().map_or(0, |b| b.body_vertex_count) * 3]
                .chunks_exact(3)
                .filter(|p| f64::from(p[0]).abs() < 30.0 && (f64::from(p[1]) - y).abs() < 10.0)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(f64::from(p[2])), hi.max(f64::from(p[2])))
                })
        };
        for (y, what) in [(lv.waist, "waist"), (lv.bust, "bust"), (lv.high_hip, "high hip")] {
            let (back, front) = extent(y);
            assert!(front - back > 120.0, "{sex:?} {what}: depth {}", front - back);
            inside(DVec3::new(0.0, y, f64::midpoint(front, back)), what);
            outside(DVec3::new(0.0, y, front + 40.0), what);
            outside(DVec3::new(0.0, y, back - 40.0), what);
            // just outside the front of the torso the distance grows forwards
            let p = DVec3::new(0.0, y, front + 8.0);
            let g = av.grid.sample(p).expect("inside the grid").gradient;
            assert!(g.z > 0.7, "{sex:?} {what}: gradient {g:?}");
        }
        let j = &av.joints.left;
        inside((j.shoulder + j.elbow) / 2.0, "upper arm");
        inside((j.hip + j.knee) / 2.0, "thigh");
        inside(DVec3::new(0.0, lv.chin + 0.06 * av.height(), 0.0), "head");
        outside(DVec3::new(0.0, av.height() + 40.0, 0.0), "above the head");
        let negative = av.grid.data.iter().filter(|&&d| d < 0.0).count() as f64;
        let volume_l = negative * av.grid.cell.powi(3) / 1e6;
        assert!(
            (40.0..120.0).contains(&volume_l),
            "{sex:?}: body volume {volume_l:.1} l"
        );
    }
}

#[test]
fn landmarks_are_symmetric() {
    let Some(m) = model() else { return };
    let b = &m.base;
    let p = |name: &str| {
        let v = m.measure_vertices(name);
        v.iter().map(|&i| b[i as usize * 3]).sum::<f32>() / v.len() as f32
    };
    // limb loops are on the left (+x) side
    for name in [
        "measure-upperarm-circ",
        "measure-wrist-circ",
        "measure-thigh-circ",
        "measure-ankle-circ",
    ] {
        assert!(p(name) > 50.0, "{name}: {}", p(name));
    }
    let fc = m.measure_vertices("measure-frontchest-dist");
    let (a, c) = (fc[0] as usize, fc[1] as usize);
    assert!((b[a * 3] + b[c * 3]).abs() < 1.0 && (b[a * 3 + 1] - b[c * 3 + 1]).abs() < 1.0);
}
