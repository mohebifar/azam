//! Drapes the tee on the morph-target avatar built from the bundled pack
//! (`assets/makehuman/makehuman-cc0.bin`). Skipped when the pack is absent.

use azam_avatar::makehuman::DEFAULT_MH_GRID_CELL_MM;
use azam_avatar::{Avatar, AvatarParams, MhModel};
use azam_doc::{Sex, Template, from_template};
use azam_sim::{SimParams, Simulation, resolve_settings};
use glam::DVec3;

/// Screen frames simulated.
const FRAMES: usize = 240;
/// Particles may end at most this deep inside the body's distance field, mm.
const PENETRATION_TOL_MM: f64 = 3.0;

#[test]
fn tee_drapes_on_the_makehuman_avatar_without_sinking_in() {
    let Some(model) = MhModel::load_bundled() else {
        eprintln!("skipping: no bundled avatar pack");
        return;
    };
    let model = model.expect("the bundled pack parses");
    let avatar = Avatar::makehuman(
        &model,
        &AvatarParams::preset(azam_avatar::Sex::Female),
        DEFAULT_MH_GRID_CELL_MM,
    );
    let project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    let settings = resolve_settings(&project.sim);
    let params = SimParams::from(&settings);
    let mut sim = Simulation::build(&project, &avatar, None, &params);
    for _ in 0..FRAMES {
        sim.advance(&avatar, settings.values.dt);
    }
    let x = &sim.particles.x;
    assert!(x.iter().all(|v| v.is_finite()), "positions stay finite");
    let pos = |q: usize| {
        DVec3::new(
            f64::from(x[3 * q]),
            f64::from(x[3 * q + 1]),
            f64::from(x[3 * q + 2]),
        )
    };
    let (worst, at) = (0..sim.particle_count())
        .map(|q| (avatar.grid.distance(pos(q)), q))
        .fold((f64::INFINITY, 0), |a, b| if b.0 < a.0 { b } else { a });
    eprintln!("deepest particle: {worst:.2} mm");
    assert!(
        worst >= -PENETRATION_TOL_MM,
        "particle {at} is {worst:.2} mm inside the body at {:?}",
        pos(at)
    );
    // the tee hangs on the torso: between the hips and the neck
    let ys: Vec<f64> = (0..sim.particle_count()).map(|q| pos(q).y).collect();
    let (lo, hi) = ys.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &y| {
        (a.min(y), b.max(y))
    });
    let lv = avatar.profile.levels;
    assert!(
        lo > lv.crotch - 150.0 && hi < lv.neck + 80.0,
        "tee spans {lo:.0}..{hi:.0} mm"
    );
}
