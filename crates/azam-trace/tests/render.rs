//! Renders tiny scenes and compares them with what the light transport must give analytically.

use std::f32::consts::PI;

use azam_trace::{
    AreaLight, Backdrop, Camera, Floor, Lighting, Material, Mesh, Renderer, Scene, Settings, Sky, Weave,
};
use glam::Vec3;

/// A square in the plane y = `y`, `half` mm from the origin each way, facing up.
fn plane(y: f32, half: f32, material: Material) -> Mesh {
    Mesh {
        positions: vec![
            [-half, y, -half],
            [half, y, -half],
            [half, y, half],
            [-half, y, half],
        ],
        normals: vec![[0.0, 1.0, 0.0]; 4],
        uvs: Vec::new(),
        // Counter-clockwise seen from above, so the face normal points up.
        indices: vec![0, 2, 1, 0, 3, 2],
        material,
        decal: None,
        relief: None,
    }
}

/// A UV sphere.
fn sphere(center: Vec3, radius: f32, material: Material) -> Mesh {
    let (rings, segments) = (48u32, 96u32);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    for r in 0..=rings {
        let theta = PI * r as f32 / rings as f32;
        for s in 0..=segments {
            let phi = 2.0 * PI * s as f32 / segments as f32;
            let n = Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin());
            positions.push((center + n * radius).to_array());
            normals.push(n.to_array());
        }
    }
    let mut indices = Vec::new();
    let row = segments + 1;
    for r in 0..rings {
        for s in 0..segments {
            let (a, b, c, d) = (
                r * row + s,
                r * row + s + 1,
                (r + 1) * row + s,
                (r + 1) * row + s + 1,
            );
            indices.extend([a, b, c, b, d, c]);
        }
    }
    Mesh {
        positions,
        normals,
        uvs: Vec::new(),
        indices,
        material,
        decal: None,
        relief: None,
    }
}

fn dark_lighting(lights: Vec<AreaLight>) -> Lighting {
    Lighting {
        lights,
        sky: Sky::uniform([0.0; 3]),
        floor: None,
        backdrop: None,
    }
}

fn settings(size: u32, samples: u32) -> Settings {
    Settings {
        width: size,
        height: size,
        samples,
        denoise: false,
        ..Settings::default()
    }
}

/// A camera straight above the origin looking down.
fn top_camera(height: f32) -> Camera {
    Camera::look_at(Vec3::new(0.0, height, 0.0), Vec3::ZERO, 30.0, 1.0)
}

/// The mean radiance (green channel) over a square of pixels.
fn mean(r: &Renderer, x0: u32, y0: u32, n: u32) -> f32 {
    let mut sum = 0.0;
    for y in y0..y0 + n {
        for x in x0..x0 + n {
            sum += r.radiance(x, y)[1];
        }
    }
    sum / (n * n) as f32
}

#[test]
fn diffuse_plane_under_a_uniform_sky_reflects_its_albedo() {
    // A plane sees only the sky above it: L = ρ · L_sky, whatever the bounces.
    let scene = Scene {
        meshes: vec![plane(0.0, 1e6, Material::diffuse([0.5; 3]))],
        lighting: Lighting {
            sky: Sky::uniform([1.0; 3]),
            ..dark_lighting(Vec::new())
        },
    };
    let mut r = Renderer::new(&scene, top_camera(2000.0), settings(32, 256));
    r.step(256);
    let l = mean(&r, 8, 8, 16);
    assert!((l - 0.5).abs() < 0.01, "radiance {l}");
}

#[test]
fn diffuse_plane_under_a_small_area_light_matches_the_inverse_square_law() {
    // A 100 mm square light 2 m up: E ≈ Le·A·cos²/d²·cos²… = Le·A·h²/d⁴; L = ρ/π · E.
    let (h, side) = (2000.0f32, 100.0f32);
    let le = PI * h * h / (side * side); // E = π straight below
    let light = AreaLight::aimed(Vec3::new(0.0, h, 0.0), Vec3::ZERO, side, side, [le; 3]);
    let scene = Scene {
        meshes: vec![plane(0.0, 1e5, Material::diffuse([0.8; 3]))],
        lighting: dark_lighting(vec![light]),
    };
    let camera = Camera::look_at(Vec3::new(0.0, 3000.0, 0.0), Vec3::ZERO, 60.0, 1.0);
    let size = 64;
    let mut r = Renderer::new(&scene, camera, settings(size, 128));
    r.step(128);
    let centre = mean(&r, size / 2 - 2, size / 2 - 2, 4);
    assert!((centre - 0.8).abs() < 0.02, "centre {centre}");
    // Off-centre: find where the pixel's ray meets the floor and compare with the falloff.
    let (px, py) = (6u32, size / 2);
    let ray = camera.ray((px as f32 + 0.5) / size as f32, (py as f32 + 0.5) / size as f32);
    let p = ray.at(-ray.origin.y / ray.dir.y);
    let d2 = h * h + p.x * p.x + p.z * p.z;
    let expected = 0.8 / PI * le * side * side * h * h / (d2 * d2);
    let got = mean(&r, px - 1, py - 1, 3);
    assert!(
        (got - expected).abs() < 0.04 * expected,
        "edge {got} vs {expected}"
    );
    assert!(expected < 0.7 * centre, "the test must sample the falloff");
}

#[test]
fn a_spheres_shadow_falls_where_geometry_puts_it() {
    let center = Vec3::new(0.0, 400.0, 0.0);
    let radius = 200.0;
    let light_pos = Vec3::new(0.447, 0.894, 0.0) * 4000.0;
    let side = 60.0;
    let le = PI * 4000.0f32.powi(2) / (side * side);
    let light = AreaLight::aimed(light_pos, Vec3::ZERO, side, side, [le; 3]);
    let scene = Scene {
        meshes: vec![
            plane(0.0, 1e5, Material::diffuse([0.8; 3])),
            // Black, so no light bounces off it into its shadow.
            sphere(center, radius, Material::diffuse([0.0; 3])),
        ],
        lighting: dark_lighting(vec![light]),
    };
    let size = 96;
    let camera = top_camera(5000.0);
    let mut r = Renderer::new(&scene, camera, settings(size, 32));
    r.step(32);
    // Does the segment from floor point `p` to the light's centre pass through a sphere of `rad`?
    let blocked = |p: Vec3, rad: f32| {
        let d = (light_pos - p).normalize();
        let oc = p - center;
        let b = oc.dot(d);
        let c = oc.length_squared() - rad * rad;
        b * b - c > 0.0 && -b > 0.0
    };
    let (mut shadowed, mut lit) = (0, 0);
    for py in (2..size - 2).step_by(3) {
        for px in (2..size - 2).step_by(3) {
            let ray = camera.ray((px as f32 + 0.5) / size as f32, (py as f32 + 0.5) / size as f32);
            let p = ray.at(-ray.origin.y / ray.dir.y);
            // Skip pixels that see the sphere, and the penumbra (decided by a bigger and a
            // smaller sphere disagreeing).
            let sees_sphere = {
                let oc = ray.origin - center;
                let b = oc.dot(ray.dir);
                b * b - (oc.length_squared() - (radius * 1.1).powi(2)) > 0.0
            };
            if sees_sphere || blocked(p, radius * 1.1) != blocked(p, radius * 0.9) {
                continue;
            }
            let l = r.radiance(px, py)[1];
            if blocked(p, radius) {
                shadowed += 1;
                assert!(l < 0.02, "pixel {px},{py} should be in shadow: {l}");
            } else {
                lit += 1;
                assert!(l > 0.3, "pixel {px},{py} should be lit: {l}");
            }
        }
    }
    assert!(shadowed > 10 && lit > 100, "{shadowed} shadowed, {lit} lit");
}

#[test]
fn shadow_catcher_darkens_the_backdrop_under_the_subject() {
    // One small light, no sky: in the umbra the catcher shows the backdrop × (1 − strength).
    let light_pos = Vec3::new(0.447, 0.894, 0.0) * 4000.0;
    let light = AreaLight::aimed(light_pos, Vec3::ZERO, 60.0, 60.0, [1000.0; 3]);
    let mut lighting = dark_lighting(vec![light]);
    lighting.floor = Some(Floor {
        height: 0.0,
        albedo: [0.3; 3],
        shadow_catcher: true,
        shadow_strength: 0.8,
        radius: 1e5,
    });
    lighting.backdrop = Some(Backdrop {
        center: [0.25; 3],
        edge: [0.25; 3],
    });
    let scene = Scene {
        meshes: vec![sphere(
            Vec3::new(0.0, 400.0, 0.0),
            200.0,
            Material::diffuse([0.6; 3]),
        )],
        lighting,
    };
    let size = 64;
    let camera = Camera::look_at(Vec3::new(0.0, 3000.0, 1.0), Vec3::ZERO, 40.0, 1.0);
    let mut r = Renderer::new(&scene, camera, settings(size, 64));
    r.step(64);
    // A corner of the frame is open floor far from the sphere: exactly the backdrop.
    let corner = mean(&r, 0, 0, 4);
    assert!((corner - 0.25).abs() < 0.02, "corner {corner}");
    let ray_to = |p: Vec3| {
        // Project a world point to pixel coordinates by searching the pixel whose ray passes
        // closest to it (tiny image, brute force).
        let mut best = (0, 0, f32::MAX);
        for py in 0..size {
            for px in 0..size {
                let ray = camera.ray((px as f32 + 0.5) / size as f32, (py as f32 + 0.5) / size as f32);
                let d = (p - ray.origin).cross(ray.dir).length();
                if d < best.2 {
                    best = (px, py, d);
                }
            }
        }
        (best.0, best.1)
    };
    // The light (to +x, 63° up) throws the shadow towards −x.
    let (cx, cy) = ray_to(Vec3::new(-400.0, 0.0, 0.0));
    let shadow = mean(&r, cx - 1, cy - 1, 3);
    assert!(
        (shadow - 0.05).abs() < 0.01,
        "shadow {shadow} vs open floor {corner}"
    );
    // The opposite side, towards the light, is open floor.
    let (lx, ly) = ray_to(Vec3::new(400.0, 0.0, 0.0));
    let open = mean(&r, lx - 1, ly - 1, 3);
    assert!(
        (open - corner).abs() < 0.03,
        "lit side {open} vs open floor {corner}"
    );
}

#[test]
fn renders_are_deterministic_and_finite_with_awkward_input() {
    let mut glossy = Material::cloth([0.9, 0.1, 0.1]);
    glossy.roughness = 0.0;
    glossy.specular = 1.0;
    glossy.sheen = 1.0;
    let mut sheer = Material::cloth([0.2, 0.3, 0.9]);
    sheer.opacity = 0.4;
    sheer.weave = Weave::Plaid;
    let mut broken = plane(300.0, 300.0, sheer);
    // Zero and NaN normals, a degenerate triangle, an out-of-range index, UVs.
    broken.normals[0] = [0.0; 3];
    broken.normals[1] = [f32::NAN; 3];
    broken.positions.push([0.0, 300.0, 0.0]);
    broken.normals.push([0.0, 1.0, 0.0]);
    broken.indices.extend([4, 4, 4, 0, 1, 99]);
    broken.uvs = vec![
        [0.0, 0.0],
        [600.0, 0.0],
        [600.0, 600.0],
        [0.0, 600.0],
        [300.0, 300.0],
    ];
    let mut nan_mesh = sphere(Vec3::new(0.0, 500.0, 0.0), 100.0, glossy);
    nan_mesh.positions[10] = [f32::NAN, 0.0, 0.0];
    let scene = Scene {
        meshes: vec![
            broken,
            nan_mesh,
            sphere(Vec3::new(200.0, 150.0, 0.0), 150.0, glossy),
        ],
        lighting: Lighting::studio(
            Vec3::new(-300.0, 0.0, -300.0),
            Vec3::new(300.0, 700.0, 300.0),
            0.4,
        ),
    };
    let camera = Camera::look_at(
        Vec3::new(800.0, 900.0, 1600.0),
        Vec3::new(0.0, 300.0, 0.0),
        40.0,
        1.5,
    );
    let s = Settings {
        width: 48,
        height: 32,
        samples: 8,
        ..Settings::default()
    };
    let mut a = Renderer::new(&scene, camera, s);
    a.step(3);
    a.step(5);
    assert!(a.is_done());
    assert!(a.radiance_buffer(false).iter().flatten().all(|v| v.is_finite()));
    assert!(a.radiance_buffer(true).iter().flatten().all(|v| v.is_finite()));
    let b = Renderer::new(&scene, camera, s).render();
    let c = Renderer::new(&scene, camera, s).render();
    assert_eq!(b, c, "same seed, same image");
    // Progressive steps take the same samples as one big step.
    let ia = a.image();
    let diff = ia
        .rgba
        .iter()
        .zip(&b.rgba)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0);
    assert!(diff <= 1, "progressive and one-shot renders differ by {diff}");
    let other = Renderer::new(&scene, camera, Settings { seed: 9, ..s }).render();
    assert_ne!(b, other, "a different seed gives different noise");
}

/// The standard deviation of the green radiance over a square of pixels, relative to its mean.
fn spread(r: &Renderer, x0: u32, y0: u32, n: u32) -> f32 {
    let m = mean(r, x0, y0, n);
    let mut sum = 0.0;
    for y in y0..y0 + n {
        for x in x0..x0 + n {
            sum += (r.radiance(x, y)[1] - m).powi(2);
        }
    }
    (sum / (n * n) as f32).sqrt() / m.max(1e-6)
}

#[test]
fn a_normal_map_shades_its_relief() {
    // stripes leaning towards +x and -x, under a light low on the +x side
    let stripes = [218, 128, 218, 255, 38, 128, 218, 255];
    let map =
        std::sync::Arc::new(azam_render::SurfaceMap::pack(Some((2, 1, &stripes)), None).expect("packed"));
    let lit = |relief: bool| {
        let mut quad = plane(0.0, 300.0, Material::diffuse([0.6; 3]));
        quad.uvs = quad.positions.iter().map(|p| [p[0], p[2]]).collect();
        if relief {
            quad.relief = Some(azam_trace::Relief {
                map: map.clone(),
                tile_mm: 150.0,
                strength: 1.0,
            });
        }
        let light = AreaLight {
            center: Vec3::new(3000.0, 1200.0, 0.0),
            half_u: Vec3::new(0.0, 0.0, 300.0),
            half_v: Vec3::new(-110.0, 280.0, 0.0),
            radiance: [40.0; 3],
        };
        let scene = Scene {
            meshes: vec![quad],
            lighting: dark_lighting(vec![light]),
        };
        let mut r = Renderer::new(&scene, top_camera(1400.0), settings(48, 64));
        r.step(64);
        spread(&r, 16, 16, 16)
    };
    let (flat, bumped) = (lit(false), lit(true));
    assert!(bumped > 3.0 * flat + 0.05, "relief {bumped:.3} vs flat {flat:.3}");
}
