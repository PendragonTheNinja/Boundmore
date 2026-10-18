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

// Three pseudo-random values in 0..1 for a grid cell.
fn hash3f(p: vec3<f32>) -> vec3<f32> {
    var q = vec3<u32>(vec3<i32>(p) + vec3<i32>(1000));
    q = q * vec3<u32>(1597334673u, 3812015801u, 2798796415u);
    let n = (q.x ^ q.y ^ q.z) * 1597334673u;
    let m = vec3<u32>(n, n * 16807u, n * 48271u);
    return vec3<f32>(m >> vec3<u32>(8u)) / 16777216.0;
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

    // Stars: one candidate per cell of a 3D grid around the camera, each a
    // point of fixed angular size at a jittered spot in its cell. A grid on
    // the sphere's latitude and longitude would crowd them at the zenith.
    let stars = g.params.w;
    if (stars > 0.0 && dir.y > -0.05) {
        let scaled = dir * 60.0;
        let base = floor(scaled);
        var best = 0.0;
        for (var i = 0; i < 8; i++) {
            let cell = base + vec3<f32>(f32(i & 1), f32((i >> 1) & 1), f32((i >> 2) & 1));
            let h = hash3f(cell);
            if (h.x > 0.92) {
                let star_dir = normalize(cell + h.yzx * 0.8 + 0.1);
                let d = 1.0 - dot(dir, star_dir);
                let size = 0.0000035 + h.y * 0.000006;
                let twinkle = 0.75 + 0.25 * sin(g.params.z * (1.5 + h.z * 3.0) + h.x * 50.0);
                best = max(best, smoothstep(size, 0.0, d) * twinkle * (0.4 + 0.6 * h.z));
            }
        }
        c += vec3<f32>(0.92, 0.94, 1.0) * best * stars * clamp(dir.y * 6.0 + 0.3, 0.0, 1.0);
    }
    return vec4<f32>(c, 1.0);
}
