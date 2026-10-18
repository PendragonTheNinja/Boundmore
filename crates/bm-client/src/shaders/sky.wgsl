// The sky: drawn after opaque terrain, only where nothing was drawn (depth
// still at the cleared 0, reversed-Z), as one full-screen triangle.

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: SkyOut;
    out.clip = vec4<f32>(p, 0.0, 1.0);
    out.ndc = p;
    return out;
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    // A point on the near plane in camera-relative world space gives the
    // view direction (the camera sits at the origin).
    let p = g.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let dir = normalize(p.xyz / p.w);
    var c = sky_gradient(dir);

    let sun = g.sun_dir.xyz;
    let ds = dot(dir, sun);
    c = mix(c, g.sun_color.rgb * 2.2 + vec3<f32>(0.3, 0.25, 0.1), smoothstep(0.9992, 0.9996, ds));

    // The moon rides opposite the sun.
    let dm = dot(dir, -sun);
    let moon = smoothstep(0.99935, 0.99965, dm);
    c = mix(c, vec3<f32>(0.78, 0.82, 0.95), moon * (1.0 - g.sun_dir.w * 0.8));

    // Stars on a direction grid, fading in at dusk, never below the horizon.
    let stars = g.params.w;
    if (stars > 0.0 && dir.y > 0.0) {
        let cell = floor(vec2<f32>(atan2(dir.z, dir.x) * 180.0, dir.y * 260.0));
        let h = hash2(cell);
        if (h > 0.9965) {
            let twinkle = 0.6 + 0.4 * sin(g.params.z * (2.0 + h * 5.0) + h * 40.0);
            c += vec3<f32>(0.9, 0.92, 1.0) * stars * twinkle * (h - 0.9965) * 220.0 * clamp(dir.y * 4.0, 0.0, 1.0);
        }
    }
    return vec4<f32>(c, 1.0);
}
