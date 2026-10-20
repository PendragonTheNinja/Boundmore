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
    // Held torches: xyz camera-relative position, w light level (0..15).
    point_lights: array<vec4<f32>, 16>,
};

struct SkyPlanet {
    // xyz: unit direction, w: apparent radius in radians
    dir: vec4<f32>,
    // rgb: linear colour
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
