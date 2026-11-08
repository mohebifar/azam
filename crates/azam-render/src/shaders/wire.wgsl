// Blender's wireframe overlay: the mesh's edges as thin dark lines, drawn a hair in front of the
// shaded surface so they are not buried in it.

struct Frame {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;

// How far towards the eye the lines are pulled, as a fraction of clip w.
const PULL = 0.0004;
const WIRE = vec4<f32>(0.012, 0.012, 0.012, 1.0);

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    var clip = frame.view_proj * vec4<f32>(position, 1.0);
    clip.z = clip.z - PULL * clip.w;
    return clip;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return WIRE;
}
