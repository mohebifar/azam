//! A fabric's normal map changes the viewport's shading across its relief.

use azam_render::{Draw, MeshData, MeshId, OrbitCamera, Overlays, Renderer, SurfaceId, SurfaceMap};

#[test]
fn a_normal_map_shades_stripes_into_a_flat_square() {
    let Ok(mut renderer) = Renderer::new() else {
        eprintln!("skipping: no GPU");
        return;
    };
    // a 1 m square facing the camera, its UVs its own coordinates in mm
    let positions = [
        [-500.0, 100.0, 0.0],
        [500.0, 100.0, 0.0],
        [500.0, 1100.0, 0.0],
        [-500.0, 1100.0, 0.0],
    ];
    let uvs: Vec<[f32; 2]> = positions.iter().map(|p| [p[0], 1100.0 - p[1]]).collect();
    renderer.set_mesh(
        MeshId(0),
        &MeshData {
            positions: &positions,
            normals: &[[0.0, 0.0, 1.0]; 4],
            colors: None,
            uvs: Some(&uvs),
            indices: &[0, 1, 2, 0, 2, 3],
        },
    );
    // vertical stripes leaning hard left and right
    let stripes = [250, 128, 160, 255, 6, 128, 160, 255];
    let map = SurfaceMap::pack(Some((2, 1, &stripes)), None).expect("packed");
    renderer.set_surface(SurfaceId(1), &map);
    let camera = OrbitCamera {
        target: [0.0, 600.0, 0.0].into(),
        distance: 2200.0,
        yaw: 0.0,
        pitch: 0.0,
        ..OrbitCamera::default()
    };
    let row = |relief: bool, r: &mut Renderer| {
        let mut draw = Draw::new(MeshId(0), [0.7, 0.7, 0.7]);
        draw.specular = 0.0;
        if relief {
            draw.surface = Some(SurfaceId(1));
            draw.surface_mm = 200.0;
        }
        let px = r.render(&camera, &[draw], Overlays::default(), 200, 200);
        // the middle row across the square
        let y = 100;
        (60..140)
            .map(|x| f32::from(px[(y * 200 + x) * 4 + 1]))
            .collect::<Vec<_>>()
    };
    let range =
        |v: &[f32]| v.iter().copied().fold(f32::MIN, f32::max) - v.iter().copied().fold(f32::MAX, f32::min);
    let (flat, bumped) = (row(false, &mut renderer), row(true, &mut renderer));
    assert!(
        range(&bumped) > range(&flat) + 20.0,
        "stripes shaded: {:.0} levels flat, {:.0} with the map",
        range(&flat),
        range(&bumped)
    );
}
