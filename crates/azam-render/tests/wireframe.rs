//! The wireframe overlay draws the mesh's edges over its shading.

use azam_render::{Draw, MeshData, MeshId, OrbitCamera, Overlays, Renderer};

#[test]
fn the_wireframe_overlay_draws_edges_over_the_shading() {
    let Ok(mut renderer) = Renderer::new() else {
        eprintln!("skipping: no GPU");
        return;
    };
    // a 1 m square of 10 × 10 cells facing the camera
    let n = 10;
    let mut positions = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            positions.push([i as f32 * 100.0 - 500.0, j as f32 * 100.0 + 100.0, 0.0]);
        }
    }
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];
    let mut indices = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let a = j * (n + 1) + i;
            indices.extend_from_slice(&[a, a + n + 1, a + 1, a + 1, a + n + 1, a + n + 2]);
        }
    }
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
        target: [0.0, 600.0, 0.0].into(),
        distance: 2200.0,
        yaw: 0.0,
        pitch: 0.0,
        ..OrbitCamera::default()
    };
    let draws = [Draw::new(MeshId(0), [0.9, 0.9, 0.9])];
    let dark = |wire: bool, r: &mut Renderer| {
        let overlays = Overlays {
            wireframe: wire,
            ..Overlays::default()
        };
        r.render(&camera, &draws, overlays, 200, 200)
            .chunks_exact(4)
            .filter(|p| p[0] < 40 && p[1] < 40 && p[2] < 40)
            .count()
    };
    let (plain, wired) = (dark(false, &mut renderer), dark(true, &mut renderer));
    assert!(
        wired > plain + 500,
        "edges drawn: {plain} dark pixels without, {wired} with"
    );
}
