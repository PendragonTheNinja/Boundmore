// Shared by every shader: the per-frame globals and the sky model. Rust
// prepends this file to each shader (`render::shader_source`), so the sky
// and the fog that fades terrain into it are the same function.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    // xyz: unit vector towards the sun; w: daylight, 0 at night to 1 by day
    sun_dir: vec4<f32>,
    sky_zenith: vec4<f32>,
    sky_horizon: vec4<f32>,
    // rgb: sunlight colour; w: sky-light brightness (DayCycle::sky_light)
    sun_color: vec4<f32>,
    // x: fog start, y: fog end, z: time in seconds, w: star visibility
    params: vec4<f32>,
    // x, y: framebuffer size in pixels; z: planets in the sky; w: point lights
    screen: vec4<f32>,
    // The night sky's frame to the world's (stars and planets turn with it)
    sky_rotation: mat4x4<f32>,
    planets: array<SkyPlanet, 16>,
    // Held torches: xyz camera-relative position.
    point_lights: array<vec4<f32>, 16>,
    // Their light levels per channel, rgb (0..15; milestone 26).
    point_levels: array<vec4<f32>, 16>,
};

struct SkyPlanet {
    // xyz: unit direction, w: apparent radius in radians
    dir: vec4<f32>,
    // rgb: linear colour, w: 1 for a wandering world (a halo)
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

// Sky colour seen along `dir` (unit, camera-relative), without the sun and
// moon discs: a gradient from horizon to zenith, a glow around the sun, and
// a darker band below the horizon.
fn sky_gradient(dir: vec3<f32>) -> vec3<f32> {
    let up = dir.y;
    let t = sqrt(clamp(up, 0.0, 1.0));
    var c = mix(g.sky_horizon.rgb, g.sky_zenith.rgb, t);
    if (up < 0.0) {
        c = mix(g.sky_horizon.rgb, g.sky_horizon.rgb * 0.55, clamp(-up * 3.0, 0.0, 1.0));
    }
    let toward_sun = max(dot(dir, g.sun_dir.xyz), 0.0);
    c += g.sun_color.rgb * (pow(toward_sun, 12.0) * 0.35 + pow(toward_sun, 128.0) * 0.6);
    return c;
}

// ---- lighting shared by terrain and boxes (creatures, players) ----

// Classic per-face shading: the top is brightest, the underside darkest.
// Slanted faces blend by how much they face each way (the squares of a unit
// normal sum to one), so a cube face gets exactly its old value.
fn face_shade(n: vec3<f32>) -> f32 {
    let q = n * n;
    return q.x * 0.82 + q.z * 0.70 + q.y * select(0.52, 1.0, n.y > 0.0);
}

// Perceived brightness of a light level: each step down is a fixed fraction
// darker, so falloff reads as a smooth glow rather than a staircase.
fn light_curve(l: f32) -> f32 {
    return pow(l, 1.7);
}

// The same per channel of coloured block light (milestone 26).
fn light_curve3(l: vec3<f32>) -> vec3<f32> {
    return pow(l, vec3<f32>(1.7));
}

// The darkest anything gets: sealed caves are dark, but never pure black.
const AMBIENT: f32 = 0.012;
// Block light has the colour of what gives it off (milestone 26: each
// light's `light_colour`, a glow crystal's world colour), so it is not tinted
// here.

// Block light from the held torches (`Renderer::set_point_lights`), 0..1 per
// channel: each falls off one level per block, as block light spreads,
// measured to the point half a block out from the surface (where a face
// samples its block light). No occlusion: a held torch lights through walls
// within its reach, a fair trade for never re-meshing a light that moves
// every frame.
fn point_light(rel: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let p = rel + n * 0.5;
    let count = min(u32(g.screen.w), 16u);
    var best = vec3<f32>(0.0);
    for (var i = 0u; i < count; i++) {
        let d = length(p - g.point_lights[i].xyz);
        best = max(best, g.point_levels[i].rgb - vec3<f32>(d));
    }
    return clamp(best, vec3<f32>(0.0), vec3<f32>(15.0)) / 15.0;
}

