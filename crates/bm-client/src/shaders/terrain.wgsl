// Terrain. Vertices are packed by bm-mesh (see `Vertex`); each draw is one
// chunk, whose camera-relative offset and world origin arrive as instance
// attributes. Everything is drawn relative to the camera (floating origin),
// so f32 precision never runs out however far from the planet's origin we are.

// Four entries per block: side, top and bottom colour (linear rgb), then
// parameters (x: opacity, y: fringe). Written by `Renderer::write_palette`.
@group(0) @binding(1) var<uniform> palette: array<vec4<f32>, 1024>;

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

fn base_colour(in: VsOut) -> vec3<f32> {
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

fn shade(in: VsOut) -> vec3<f32> {
    let base = base_colour(in);
    let n = NORMALS[in.face];
    var c = surface(base, in.local, n, in.origin);
    // Until per-voxel light lands (M2), the whole scene follows the sky's
    // brightness; direct sun adds warmth to faces turned towards it by day.
    let sun = max(dot(n, g.sun_dir.xyz), 0.0) * g.sun_dir.w;
    let light = g.sun_color.w * (0.82 + 0.18 * sun);
    c = c * FACE_SHADE[in.face] * light * in.ao;
    let d = length(in.rel);
    let fog = clamp((d - g.params.x) / max(g.params.y - g.params.x, 1.0), 0.0, 1.0);
    return mix(c, sky_gradient(in.rel / max(d, 0.001)), fog * fog);
}

@fragment
fn fs_opaque(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(shade(in), 1.0);
}

@fragment
fn fs_translucent(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(shade(in), palette[in.block * 4u + 3u].x);
}
