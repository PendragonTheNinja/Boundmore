// Items held in first person (milestone 31; `held.rs`): drawn after the
// world with depth cleared, in their own fixed projection, lit as boxes are
// where the player stands.

struct Held {
    // Model space to view space.
    model: mat4x4<f32>,
    // View space to camera-relative world space (lighting, held torches).
    to_world: mat4x4<f32>,
    // The held items' own projection (reversed-Z, `render::projection`).
    proj: mat4x4<f32>,
    // As `BoxInstance::light`: sky, block light (packed), brightness, glow.
    light: vec4<f32>,
    // x: the model's first texture layer.
    params: vec4<f32>,
};

@group(1) @binding(0) var<uniform> h: Held;
@group(1) @binding(1) var held_tex: texture_2d_array<f32>;
@group(1) @binding(2) var held_sampler: sampler;

struct HeldIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: u32,
    @location(4) color: vec4<f32>,
};

struct HeldOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) color: vec4<f32>,
    @location(3) rel: vec3<f32>,
    @location(4) normal: vec3<f32>,
};

@vertex
fn vs_held(in: HeldIn) -> HeldOut {
    let view = h.model * vec4<f32>(in.pos, 1.0);
    var out: HeldOut;
    out.clip = h.proj * view;
    out.uv = in.uv;
    out.layer = in.layer;
    out.color = in.color;
    out.rel = (h.to_world * view).xyz;
    out.normal = (h.to_world * h.model * vec4<f32>(in.normal, 0.0)).xyz;
    return out;
}

// A box's block light per channel, as `box_block_light` in simple.wgsl.
fn held_block_light(y: f32) -> vec3<f32> {
    if (y < 16.0) {
        return vec3<f32>(y);
    }
    let v = u32(round(y - 16.0));
    return vec3<f32>(f32(v & 15u), f32((v >> 4u) & 15u), f32((v >> 8u) & 15u)) / 15.0;
}

@fragment
fn fs_held(in: HeldOut) -> @location(0) vec4<f32> {
    var base = vec4<f32>(1.0);
    if (in.layer != 0xffffffffu) {
        base = textureSample(held_tex, held_sampler, in.uv, i32(in.layer) + i32(h.params.x));
    }
    if (base.a < 0.5) {
        discard;
    }
    let albedo = base.rgb * in.color.rgb;
    let n = normalize(in.normal);
    let sky = light_curve(h.light.x);
    let block = light_curve3(max(held_block_light(h.light.y), point_light(in.rel, n)));
    let sun = max(dot(n, g.sun_dir.xyz), 0.0) * g.sun_dir.w;
    let light = max(sky_light_colour(sky, sun), block) + AMBIENT;
    let lit = albedo * face_shade(n) * light * h.light.z;
    let glow = clamp(max(in.color.a, h.light.w), 0.0, 1.0);
    return vec4<f32>(mix(lit, albedo, glow), 1.0);
}
