//! Renders a test scene (a sphere on the floor grid) to a PNG.
//!
//! `cargo run -p azam-render --example snapshot -- out.png`

use std::f32::consts::PI;
use std::fs::File;
use std::io::BufWriter;

use azam_render::{Draw, MeshData, MeshId, OrbitCamera, Overlays, Renderer};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn sphere(
    radius: f32,
    centre: [f32; 3],
    rings: u32,
    segments: u32,
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    let (mut pos, mut nor, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for r in 0..=rings {
        let phi = PI * r as f32 / rings as f32;
        for s in 0..=segments {
            let theta = 2.0 * PI * s as f32 / segments as f32;
            let n = [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()];
            nor.push(n);
            pos.push([
                centre[0] + n[0] * radius,
                centre[1] + n[1] * radius,
                centre[2] + n[2] * radius,
            ]);
        }
    }
    let row = segments + 1;
    for r in 0..rings {
        for s in 0..segments {
            let a = r * row + s;
            idx.extend_from_slice(&[a, a + 1, a + row, a + 1, a + row + 1, a + row]);
        }
    }
    (pos, nor, idx)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "snapshot.png".into());
    let mut renderer = Renderer::new()?;
    let (positions, normals, indices) = sphere(400.0, [0.0, 400.0, 0.0], 48, 96);
    renderer.set_mesh(
        MeshId(0),
        &MeshData {
            positions: &positions,
            normals: &normals,
            colors: None,
            uvs: None,
            indices: &indices,
        },
    );
    let camera = OrbitCamera {
        target: [0.0, 400.0, 0.0].into(),
        distance: 2600.0,
        ..OrbitCamera::default()
    };
    let draws = [Draw::new(MeshId(0), [0.8, 0.8, 0.8])];
    let start = std::time::Instant::now();
    let pixels = renderer
        .render(&camera, &draws, Overlays::default(), WIDTH, HEIGHT)
        .to_vec();
    let first = start.elapsed();
    let start = std::time::Instant::now();
    for _ in 0..10 {
        renderer.render(&camera, &draws, Overlays::default(), WIDTH, HEIGHT);
    }
    println!("first frame {first:?}, then {:?} per frame", start.elapsed() / 10);
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(&path)?), WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    println!("wrote {path}");
    Ok(())
}
