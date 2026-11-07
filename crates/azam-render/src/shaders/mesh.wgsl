// Solid shading in the style of Blender's studio lighting — a sky/ground hemisphere, a key, fill
// and back light, a soft specular and a rim — with a shadow map from the key light and procedural
// fabric weaves drawn from each piece's pattern coordinates. Cloth is two-sided: the inside is
// shaded darker.

struct Frame {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    // xyz: eye position, w: camera distance.
    eye: vec4<f32>,
    // xyz: direction towards the key light, w: 1 when shadows are on.
    light: vec4<f32>,
};

struct Draw {
    // Base colour (linear) and alpha.
    color: vec4<f32>,
    // x: highlight amount, y: vertex colour weight, z: inside brightness, w: specular strength.
    params: vec4<f32>,
    // x: weave kind, y: weave repeat in mm, z: roughness, w: unused.
    material: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(1) @binding(0) var<uniform> draw: Draw;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = frame.view_proj * vec4<f32>(v.position, 1.0);
    out.world = v.position;
    out.normal = v.normal;
    out.color = v.color;
    out.uv = v.uv;
    return out;
}

const FILL_DIR = vec3<f32>(-0.62, 0.32, 0.42);
const BACK_DIR = vec3<f32>(0.0, 0.45, -0.89);
const SKY = vec3<f32>(0.40, 0.41, 0.44);
const GROUND = vec3<f32>(0.16, 0.15, 0.14);
const HIGHLIGHT = vec3<f32>(1.0, 0.55, 0.16);
const TAU = 6.2831853;
/// Shadow map texel size (2048²).
const SHADOW_TEXEL = 1.0 / 2048.0;

// Weave kinds (keep in sync with `Weave` in renderer.rs).
const PLAIN = 1u;
const TWILL = 2u;
const DENIM = 3u;
const SATIN = 4u;
const KNIT = 5u;
const RIB = 6u;
const HERRINGBONE = 7u;
const CORDUROY = 8u;
const CHECK = 9u;
const STRIPE = 10u;
const PLAID = 11u;
const FLEECE = 12u;

/// Fraction of the key light reaching `world` (3×3 PCF on the shadow map).
fn lit(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if frame.light.w < 0.5 {
        return 1.0;
    }
    let offset = world + n * 2.0;
    let p = frame.light_view_proj * vec4<f32>(offset, 1.0);
    let ndc = p.xyz / p.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0 {
        return 1.0;
    }
    var sum = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let o = vec2<f32>(f32(dx), f32(dy)) * SHADOW_TEXEL * 1.5;
            sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + o, ndc.z);
        }
    }
    return sum / 9.0;
}

/// A thread wave across `t` (repeats per unit), anti-aliased away when smaller than a pixel.
fn wave(t: f32, fade: f32) -> f32 {
    return sin(t * TAU) * fade;
}

/// Brightness change of the weave at pattern point `uv` (mm), around 1.
fn weave(uv: vec2<f32>, kind: u32, repeat_mm: f32) -> f32 {
    if kind == 0u {
        return 1.0;
    }
    let p = uv / max(repeat_mm, 0.1);
    // fade the pattern out as threads get smaller than about two pixels
    let width = max(fwidth(p.x), fwidth(p.y));
    let fade = clamp(1.0 - (width - 0.25) * 2.0, 0.0, 1.0);
    switch kind {
        case PLAIN: { return 1.0 + 0.05 * wave(p.x, fade) * wave(p.y, fade); }
        case TWILL: { return 1.0 + 0.07 * wave(p.x + p.y, fade); }
        case DENIM: { return 1.0 + 0.10 * wave(p.x + p.y, fade) + 0.04 * wave(p.y * 0.37, fade); }
        case SATIN: { return 1.0 + 0.015 * wave(p.x * 0.5, fade); }
        case KNIT: {
            let v = abs(fract(p.x) - 0.5) * 2.0;
            return 1.0 + 0.08 * fade * (abs(fract(p.y * 2.0 + v) - 0.5) * 2.0 - 0.5);
        }
        case RIB: { return 1.0 + 0.14 * wave(p.x, fade); }
        case HERRINGBONE: {
            let flip = select(-1.0, 1.0, fract(p.x * 0.25) < 0.5);
            return 1.0 + 0.08 * wave(p.y + p.x * flip, fade);
        }
        case CORDUROY: { return 1.0 + 0.22 * max(wave(p.x, fade), -0.4); }
        case CHECK: {
            let a = step(0.5, fract(p.x * 0.125));
            let b = step(0.5, fract(p.y * 0.125));
            return 1.0 - 0.18 * (a + b) * 0.5 - 0.1 * a * b;
        }
        case STRIPE: { return 1.0 - 0.22 * step(0.5, fract(p.x * 0.1)); }
        case PLAID: {
            let a = step(0.7, fract(p.x * 0.08)) + 0.5 * step(0.92, fract(p.x * 0.08 + 0.3));
            let b = step(0.7, fract(p.y * 0.08)) + 0.5 * step(0.92, fract(p.y * 0.08 + 0.3));
            return 1.0 - 0.16 * (a + b);
        }
        case FLEECE: { return 1.0 + 0.04 * wave(p.x * 2.3 + sin(p.y * 1.7), fade); }
        default: { return 1.0; }
    }
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    let v = normalize(frame.eye.xyz - in.world);
    var inside = 1.0;
    if dot(n, v) < 0.0 {
        n = -n;
        inside = draw.params.z;
    }
    let kind = u32(draw.material.x + 0.5);
    let pattern = weave(in.uv, kind, draw.material.y);
    let base = draw.color.rgb * mix(vec3<f32>(1.0), in.color.rgb, draw.params.y) * pattern;
    let key = normalize(frame.light.xyz);
    let shadow = lit(in.world, n);
    let hemi = mix(GROUND, SKY, n.y * 0.5 + 0.5);
    let diffuse = hemi
        + vec3<f32>(0.80) * max(dot(n, key), 0.0) * shadow
        + vec3<f32>(0.24) * max(dot(n, normalize(FILL_DIR)), 0.0)
        + vec3<f32>(0.18) * max(dot(n, normalize(BACK_DIR)), 0.0);
    let half_dir = normalize(key + v);
    let shininess = mix(80.0, 8.0, clamp(draw.material.z, 0.0, 1.0));
    let specular = pow(max(dot(n, half_dir), 0.0), shininess) * draw.params.w * shadow;
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.12;
    var color = base * diffuse * inside + vec3<f32>(specular + rim);
    color = mix(color, HIGHLIGHT, draw.params.x * 0.35);
    return vec4<f32>(color, draw.color.a);
}
