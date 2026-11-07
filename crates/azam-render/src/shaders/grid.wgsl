// Blender's floor grid: anti-aliased minor and major lines on the y = 0 plane, the x axis in red
// and the z axis in green, fading out with distance from the eye. The floor also catches the
// key light's shadow.

struct Frame {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;

// Half the side of the grid quad, in mm.
const EXTENT = 20000.0;
const MINOR_MM = 100.0;
const MAJOR_MM = 1000.0;
const LINE = vec3<f32>(0.090, 0.090, 0.090);
const AXIS_X = vec3<f32>(0.62, 0.10, 0.13);
const AXIS_Z = vec3<f32>(0.22, 0.52, 0.06);
const SHADOW_TEXEL = 1.0 / 2048.0;
/// How dark a full shadow makes the floor.
const SHADOW_STRENGTH = 0.38;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[index] * EXTENT;
    var out: VertexOut;
    out.world = vec3<f32>(c.x, 0.0, c.y);
    out.clip = frame.view_proj * vec4<f32>(out.world, 1.0);
    return out;
}

// Coverage of grid lines `spacing` apart at `coord`, about one pixel wide.
fn lines(coord: vec2<f32>, spacing: f32) -> f32 {
    let g = coord / spacing;
    let width = fwidth(g);
    let d = abs(fract(g - 0.5) - 0.5) / max(width, vec2<f32>(1e-6));
    return 1.0 - min(min(d.x, d.y), 1.0);
}

// Coverage of the line where `value` is zero.
fn axis(value: f32) -> f32 {
    return 1.0 - min(abs(value) / max(fwidth(value), 1e-6), 1.0);
}

// Fraction of the floor point `world` in the key light's shadow.
fn shadowed(world: vec3<f32>) -> f32 {
    if frame.light.w < 0.5 {
        return 0.0;
    }
    let p = frame.light_view_proj * vec4<f32>(world, 1.0);
    let ndc = p.xyz / p.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0 {
        return 0.0;
    }
    var sum = 0.0;
    for (var dy = -2; dy <= 2; dy++) {
        for (var dx = -2; dx <= 2; dx++) {
            let o = vec2<f32>(f32(dx), f32(dy)) * SHADOW_TEXEL * 2.0;
            sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + o, ndc.z);
        }
    }
    return 1.0 - sum / 25.0;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let xz = in.world.xz;
    let dist = length(frame.eye.xyz - in.world);
    let fade = clamp(1.0 - dist / (frame.eye.w * 4.0), 0.0, 1.0);
    let minor = lines(xz, MINOR_MM) * 0.35;
    let major = lines(xz, MAJOR_MM) * 0.8;
    var color = LINE;
    var alpha = max(minor, major) * fade;
    let ax = axis(in.world.z);
    let az = axis(in.world.x);
    if ax > 0.0 {
        color = mix(color, AXIS_X, ax);
        alpha = max(alpha, ax * fade);
    }
    if az > 0.0 {
        color = mix(color, AXIS_Z, az);
        alpha = max(alpha, az * fade);
    }
    // the shadow darkens the floor under the lines
    let shade = shadowed(in.world) * SHADOW_STRENGTH;
    let out_alpha = alpha + shade * (1.0 - alpha);
    let out_color = (color * alpha) / max(out_alpha, 1e-4);
    return vec4<f32>(out_color, out_alpha);
}
