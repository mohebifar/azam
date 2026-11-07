//! Drapes the T-shirt template on the CPU solver and path-traces it, reporting timings: the
//! realistic benchmark for the tracer.
//!
//! `cargo run --release -p azam-trace --example tee -- [out.png] [frames] [samples] [WxH]
//! [close|full] [weave,rough,shine,#rrggbb]`
//!
//! Writes the denoised image to `out.png` and the raw one next to it (`out-raw.png`).

use std::path::Path;
use std::time::Instant;

use azam_avatar::{Avatar, AvatarParams};
use azam_doc::{Sex, Template, from_template};
use azam_sim::{SimParams, Simulation, resolve_settings};
use azam_trace::{Camera, Lighting, Material, Mesh, OrbitCamera, Renderer, Scene, Settings, Weave};
use glam::Vec3;

/// `#rrggbb` to linear RGB.
fn hex_linear(hex: &str) -> Option<[f32; 3]> {
    let v = u32::from_str_radix(hex.trim().strip_prefix('#')?, 16).ok()?;
    let c = |s: u32| azam_render::srgb_to_linear((((v >> s) & 0xff) as f32) / 255.0);
    Some([c(16), c(8), c(0)])
}

fn triples(flat: &[f32]) -> Vec<[f32; 3]> {
    flat.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()
}

/// A fabric look given as `weave,rough,shine,#rrggbb`, keeping `base`'s weave repeat.
fn parse_look(look: &str, base: &Material) -> Option<Material> {
    let parts: Vec<&str> = look.split(',').collect();
    let [weave, rough, shine, color] = parts[..] else {
        return None;
    };
    Some(Material {
        weave: Weave::from_name(weave),
        weave_mm: base.weave_mm,
        ..Material::fabric(
            hex_linear(color).unwrap_or(base.base_color),
            rough.parse().ok()?,
            shine.parse().ok()?,
        )
    })
}

/// The tee template draped for `frames` frames, as meshes (avatar first), optionally with every
/// piece in fabric `look`.
fn draped_tee(frames: usize, look: Option<&str>) -> Vec<Mesh> {
    let project = from_template(Template::Tee, Sex::Female, None, None, 0.0);
    let params: AvatarParams = serde_json::to_value(&project.avatar)
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| AvatarParams::preset(azam_avatar::Sex::Female));
    let avatar = Avatar::build(&params, None);
    let settings = resolve_settings(&project.sim);
    let mut sim = Simulation::build(&project, &avatar, None, &SimParams::from(&settings));
    let t = Instant::now();
    for _ in 0..frames {
        sim.advance(&avatar, settings.values.dt);
    }
    sim.compute_normals();
    println!("drape: {frames} frames in {:.2} s", t.elapsed().as_secs_f64());

    let mut meshes = vec![Mesh {
        positions: triples(&avatar.mesh.positions),
        normals: triples(&avatar.mesh.normals),
        uvs: Vec::new(),
        indices: avatar.mesh.indices.clone(),
        material: Material::skin(hex_linear(&project.avatar.skin).unwrap_or([0.45; 3])),
        decal: None,
    }];
    for view in sim.piece_views() {
        let mut material = Material::cloth([0.27, 0.32, 0.45]);
        if let Some(f) = project.effective_fabric(&view.record.fabric) {
            let color = hex_linear(&f.color).unwrap_or(material.base_color);
            material = Material::fabric(color, f.rough as f32, f.sheen as f32);
            material.weave = Weave::from_name(&f.weave);
            material.weave_mm = (f.scale as f32).clamp(0.2, 20.0) * 1.6;
            material.opacity = (f.opacity as f32).clamp(0.2, 1.0);
        }
        if let Some(m) = look.and_then(|l| parse_look(l, &material)) {
            material = m;
        }
        meshes.push(Mesh {
            positions: triples(view.positions),
            normals: triples(view.normals),
            uvs: view
                .record
                .mesh
                .xy
                .chunks_exact(2)
                .map(|c| [c[0] as f32, c[1] as f32])
                .collect(),
            indices: view.triangles.to_vec(),
            material,
            decal: None,
        });
    }
    meshes
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "tee.png".into());
    let frames: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(90);
    let samples: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(64);
    let (width, height) = args
        .get(4)
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1280, 800));
    let orbit = if args.get(5).is_some_and(|a| a == "close") {
        OrbitCamera {
            target: Vec3::new(0.0, 1250.0, 0.0),
            distance: 1300.0,
            ..OrbitCamera::default()
        }
    } else {
        OrbitCamera::default()
    };

    let mut scene = Scene {
        meshes: draped_tee(frames, args.get(6).map(String::as_str)),
        lighting: Lighting::studio(Vec3::ZERO, Vec3::ONE, 0.0),
    };
    let (lo, hi) = scene.bounds().unwrap_or((Vec3::ZERO, Vec3::ONE));
    scene.lighting = Lighting::studio(lo, hi, orbit.yaw);
    println!(
        "scene: {} triangles, {} meshes",
        scene.triangle_count(),
        scene.meshes.len()
    );

    let camera = Camera::from_orbit(&orbit, width as f32 / height as f32);
    let t = Instant::now();
    let settings = Settings {
        width,
        height,
        samples,
        ..Settings::default()
    };
    let mut renderer = Renderer::new(&scene, camera, settings);
    println!("bvh: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    while !renderer.is_done() {
        renderer.step(8);
    }
    let trace_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let image = renderer.image();
    let denoise_s = t.elapsed().as_secs_f64();
    println!(
        "trace: {width}x{height} @ {samples} spp in {trace_s:.2} s ({:.1} Msamples/s), \
         denoise + tonemap {denoise_s:.2} s",
        f64::from(width * height) * f64::from(samples) / trace_s / 1e6
    );
    let raw_path = out.replace(".png", "-raw.png");
    for (path, img) in [(&out, image), (&raw_path, renderer.image_with(false))] {
        if let Err(e) = img.write_png(Path::new(path)) {
            eprintln!("could not write {path}: {e}");
        }
    }
    println!("wrote {out} and {raw_path}");
}
