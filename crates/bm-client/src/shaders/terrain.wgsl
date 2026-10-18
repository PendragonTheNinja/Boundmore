// Terrain. Vertices are packed by bm-mesh (see `Vertex`); each draw is one
// chunk, whose camera-relative offset and world origin arrive as instance
// attributes. Everything is drawn relative to the camera (floating origin),
// so f32 precision never runs out however far from the planet's origin we are.

// Four entries per block: side, top and bottom colour (linear rgb), then
// parameters (x: opacity, y: fringe). Written by `Renderer::write_palette`.
@group(0) @binding(1) var<uniform> palette: array<vec4<f32>, 1024>;
// Per block: top, side, bottom texture as `first_layer | variants << 16`, or
// 0xffff for "use the palette colour". Written by `Renderer::set_textures`.
@group(0) @binding(2) var<uniform> faces: array<vec4<u32>, 256>;
@group(0) @binding(3) var block_tex: texture_2d_array<f32>;
@group(0) @binding(4) var block_samp: sampler;

const NO_TEXTURE: u32 = 0xffffu;

struct VsIn {
    @location(0) a: u32,
    @location(1) b: u32,
    @location(2) offset: vec3<f32>,
    @location(3) origin: vec3<i32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
    @location(1) local: vec3<f32>,
    @location(2) @interpolate(flat) block: u32,
    @location(3) @interpolate(flat) face: u32,
    @location(4) ao: f32,
    @location(5) @interpolate(flat) origin: vec3<i32>,
    // x: sky light, y: block light, each 0..1 before the curve
    @location(6) light: vec2<f32>,
};

const NORMALS = array<vec3<f32>, 6>(
    vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(-1.0, 0.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, -1.0, 0.0),
    vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 0.0, -1.0),
);

// Classic per-face shading: the top is brightest, the underside darkest.
const FACE_SHADE = array<f32, 6>(0.82, 0.82, 1.0, 0.52, 0.70, 0.70);
const AO_CURVE = array<f32, 4>(0.42, 0.62, 0.82, 1.0);

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let local = vec3<f32>(f32(in.a & 63u), f32((in.a >> 6u) & 63u), f32((in.a >> 12u) & 63u));
    let rel = in.offset + local;
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(rel, 1.0);
    out.rel = rel;
    out.local = local;
    out.block = in.b & 0xffffu;
    out.face = (in.a >> 18u) & 7u;
    out.ao = AO_CURVE[(in.a >> 21u) & 3u];
    out.origin = in.origin;
    out.light = vec2<f32>(f32((in.b >> 16u) & 15u), f32((in.b >> 20u) & 15u)) / 15.0;
    return out;
}

fn hash3(p: vec3<i32>) -> f32 {
    var h = (u32(p.x) * 73856093u) ^ (u32(p.y) * 19349663u) ^ (u32(p.z) * 83492791u);
    h = (h ^ (h >> 13u)) * 1274126177u;
    h = h ^ (h >> 16u);
    return f32(h & 0xffffu) / 65535.0;
}

// Placeholder surface detail until the texture pass: a per-block tint and a
// fine per-texel grain, both fixed to the world so nothing swims.
fn surface(c: vec3<f32>, local: vec3<f32>, n: vec3<f32>, origin: vec3<i32>) -> vec3<f32> {
    let inside = local - n * 0.01;
    let cell = vec3<i32>(floor(inside));
    let block = origin + cell;
    let tint = 0.92 + 0.12 * hash3(block);
    let texel = vec3<i32>(floor(inside * 8.0));
    let grain = 0.94 + 0.08 * hash3(block * 8 + texel);
    return c * tint * grain;
}

// Texture coordinates on a face, from the position on it: textures are
// upright on the sides (v runs down) and read from above on the top.
fn face_uv(p: vec3<f32>, face: u32) -> vec2<f32> {
    switch face {
        case 0u: { return vec2<f32>(-p.z, -p.y); }
        case 1u: { return vec2<f32>(p.z, -p.y); }
        case 2u: { return vec2<f32>(p.x, p.z); }
        case 3u: { return vec2<f32>(p.x, -p.z); }
        case 4u: { return vec2<f32>(p.x, -p.y); }
        default: { return vec2<f32>(-p.x, -p.y); }
    }
}

fn face_slot(face: u32) -> u32 {
    if (face == 2u) {
        return 0u;
    }
    if (face == 3u) {
        return 2u;
    }
    return 1u;
}

// The palette colour path, used for blocks without textures.
fn palette_colour(in: VsOut) -> vec3<f32> {
    let i = in.block * 4u;
    if (in.face == 2u) {
        return palette[i + 1u].rgb;
    }
    if (in.face == 3u) {
        return palette[i + 2u].rgb;
    }
    // Turf-like sides: the top colour hangs a little way down the side,
    // with a ragged edge that varies per texel column.
    if (palette[i + 3u].y > 0.5) {
        let n = NORMALS[in.face];
        let inside = in.local - n * 0.01;
        let column = vec3<i32>(floor(inside * 8.0));
        let edge = 0.78 - 0.12 * hash3(in.origin * 8 + vec3<i32>(column.x, 0, column.z));
        if (fract(inside.y) > edge) {
            return palette[i + 1u].rgb;
        }
    }
    return palette[i].rgb;
}

// Base colour and alpha of the face at this fragment: the block's texture
// (one of its variants, chosen per block position), or its palette colour.
fn albedo(in: VsOut) -> vec4<f32> {
    let n = NORMALS[in.face];
    let inside = in.local - n * 0.01;
    let block = in.origin + vec3<i32>(floor(inside));
    let entry = faces[in.block][face_slot(in.face)];
    let textured = entry != NO_TEXTURE;
    let variants = max(entry >> 16u, 1u);
    let variant = min(u32(hash3(block) * f32(variants)), variants - 1u);
    let layer = select(0u, (entry & 0xffffu) + variant, textured);
    // Sampled unconditionally: derivatives need uniform control flow.
    let t = textureSample(block_tex, block_samp, face_uv(in.local, in.face), layer);
    if (textured) {
        return t;
    }
    let c = surface(palette_colour(in), in.local, n, in.origin);
    return vec4<f32>(c, palette[in.block * 4u + 3u].x);
}

// Perceived brightness of a light level: each step down is a fixed fraction
// darker, so falloff reads as a smooth glow rather than a staircase.
fn light_curve(l: f32) -> f32 {
    return pow(l, 1.7);
}

// The darkest anything gets: sealed caves are dark, but never pure black.
const AMBIENT: f32 = 0.012;
// Block light is warm, like lamplight on stone.
const BLOCK_TINT: vec3<f32> = vec3<f32>(1.0, 0.88, 0.70);

fn shade(in: VsOut, base: vec3<f32>) -> vec3<f32> {
    let n = NORMALS[in.face];
    // Sky and block light arrive as separate channels so night dims only the
    // sky (a uniform, never a re-mesh). Direct sun warms faces turned to it,
    // but only where the sky reaches.
    let sky = light_curve(in.light.x);
    let block = light_curve(in.light.y);
    let sun = max(dot(n, g.sun_dir.xyz), 0.0) * g.sun_dir.w;
    let sky_light = sky * g.sun_color.w * (0.82 + 0.18 * sun);
    let light = max(vec3<f32>(sky_light), BLOCK_TINT * block) + AMBIENT;
    let c = base * FACE_SHADE[in.face] * light * in.ao;
    let d = length(in.rel);
    let fog = clamp((d - g.params.x) / max(g.params.y - g.params.x, 1.0), 0.0, 1.0);
    return mix(c, sky_gradient(in.rel / max(d, 0.001)), fog * fog);
}

@fragment
fn fs_opaque(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(shade(in, albedo(in).rgb), 1.0);
}

@fragment
fn fs_translucent(in: VsOut) -> @location(0) vec4<f32> {
    let a = albedo(in);
    return vec4<f32>(shade(in, a.rgb), a.a);
}

// ---- crack overlay on the block being broken ----

struct OverlayIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: u32,
};

struct OverlayOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
};

@vertex
fn vs_overlay(in: OverlayIn) -> OverlayOut {
    var out: OverlayOut;
    out.clip = g.view_proj * vec4<f32>(in.pos, 1.0);
    out.uv = in.uv;
    out.layer = in.layer;
    return out;
}

@fragment
fn fs_overlay(in: OverlayOut) -> @location(0) vec4<f32> {
    let t = textureSample(block_tex, block_samp, in.uv, in.layer);
    return vec4<f32>(t.rgb * g.sun_color.w, t.a);
}
