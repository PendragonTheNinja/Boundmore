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

// Smooth value noise, about 0..1, for markings on the planets.
fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash3f(i).x, hash3f(i + vec3<f32>(1.0, 0.0, 0.0)).x, u.x);
    let b = mix(hash3f(i + vec3<f32>(0.0, 1.0, 0.0)).x, hash3f(i + vec3<f32>(1.0, 1.0, 0.0)).x, u.x);
    let c = mix(hash3f(i + vec3<f32>(0.0, 0.0, 1.0)).x, hash3f(i + vec3<f32>(1.0, 0.0, 1.0)).x, u.x);
    let d = mix(hash3f(i + vec3<f32>(0.0, 1.0, 1.0)).x, hash3f(i + vec3<f32>(1.0, 1.0, 1.0)).x, u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

// The other planets of the universe: lit spheres hanging among the stars.
// `c` is the sky so far (stars, sun, moon), `behind` the sky's own glow
// without them: a planet hides the stars and the sun, but by day the sky's
// light still lies over it, as over the moon. `pixel` is one pixel's angle.
fn draw_planets(dir: vec3<f32>, c_in: vec3<f32>, behind: vec3<f32>, pixel: f32) -> vec3<f32> {
    var c = c_in;
    let sun = g.sun_dir.xyz;
    let count = min(u32(g.screen.z), 16u);
    let above = clamp(dir.y * 6.0 + 0.3, 0.0, 1.0);
    for (var i = 0u; i < count; i++) {
        let p = g.planets[i];
        let centre = p.dir.xyz;
        let r = p.dir.w;
        let cosang = dot(dir, centre);
        if (cosang < cos(r * 1.5)) {
            continue;
        }
        // Position on the disc: 0 at the centre, 1 at the limb.
        let refv = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(centre.y) > 0.99);
        let side = normalize(cross(centre, refv));
        let upv = cross(side, centre);
        let off = dir - centre * cosang;
        let uv = vec2<f32>(dot(off, side), dot(off, upv)) / sin(r);
        let t = length(uv);
        let aa = max(pixel / sin(r), 0.002);

        // A faint atmosphere: brightest on the limb facing the sun.
        let out_dir = off / max(length(off), 1e-6);
        let rim_lit = 0.12 + 0.88 * pow(clamp(dot(out_dir, sun) * 0.5 + 0.5, 0.0, 1.0), 2.0);
        let air = mix(p.color.rgb, vec3<f32>(0.72, 0.84, 1.0), 0.55);
        let glow = exp(-max(t - 1.0, 0.0) / 0.06) * smoothstep(1.45, 1.0, t);

        if (t < 1.0 + aa) {
            let mu = sqrt(max(1.0 - t * t, 0.0));
            // The sphere's normal, on the side facing us.
            let n = normalize(side * uv.x + upv * uv.y - centre * mu);
            let lambert = max(dot(n, sun), 0.0);
            // Markings fixed to the planet, not to the screen: its normal in
            // the night sky's frame, which turns with it.
            let nc = (transpose(g.sky_rotation) * vec4<f32>(n, 0.0)).xyz;
            let seed = vec3<f32>(f32(i) * 17.0, f32(i) * 5.0, 3.0);
            let land = value_noise(nc * 2.6 + seed) * 0.65 + value_noise(nc * 6.5 + seed) * 0.35;
            let albedo = p.color.rgb * mix(0.72, 1.18, smoothstep(0.38, 0.62, land));
            let limb = 0.5 + 0.5 * mu;
            // The night side keeps a faint glow so it hides the stars.
            let lit = albedo * (lambert * 1.3 * limb + 0.03);
            let inner_air = air * pow(1.0 - mu, 3.0) * rim_lit * 0.5;
            let disc = behind + lit + inner_air;
            let cover = smoothstep(1.0 + aa, 1.0 - aa, t) * above;
            c = mix(c, disc, cover);
        }
        c += air * glow * rim_lit * 0.35 * above * step(1.0, t + aa * 0.5);
    }
    return c;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    // A point on the near plane in camera-relative world space gives the
    // view direction (the camera sits at the origin).
    let p = g.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let dir = normalize(p.xyz / p.w);
    // One pixel to the right, for the planets' edges.
    let q = g.inv_view_proj * vec4<f32>(in.ndc + vec2<f32>(2.0 / max(g.screen.x, 1.0), 0.0), 1.0, 1.0);
    let pixel = length(normalize(q.xyz / q.w) - dir);
    let behind = sky_gradient(dir);
    var c = behind;

    let sun = g.sun_dir.xyz;
    let ds = dot(dir, sun);
    c = mix(c, g.sun_color.rgb * 2.2 + vec3<f32>(0.3, 0.25, 0.1), smoothstep(0.9992, 0.9996, ds));

    // The moon rides opposite the sun.
    let dm = dot(dir, -sun);
    let moon = smoothstep(0.99935, 0.99965, dm);
    c = mix(c, vec3<f32>(0.78, 0.82, 0.95), moon * (1.0 - g.sun_dir.w * 0.8));

    // Stars: one candidate per cell of a 3D grid in the night sky's frame,
    // each a point of fixed angular size at a jittered spot in its cell. A
    // grid on the sphere's latitude and longitude would crowd them at the
    // zenith. The frame turns once a day, so they rise and set.
    let stars = g.params.w;
    if (stars > 0.0 && dir.y > -0.05) {
        let sdir = (transpose(g.sky_rotation) * vec4<f32>(dir, 0.0)).xyz;
        let scaled = sdir * 60.0;
        let base = floor(scaled);
        var best = 0.0;
        for (var i = 0; i < 8; i++) {
            let cell = base + vec3<f32>(f32(i & 1), f32((i >> 1) & 1), f32((i >> 2) & 1));
            let h = hash3f(cell);
            if (h.x > 0.92) {
                let star_dir = normalize(cell + h.yzx * 0.8 + 0.1);
                let d = 1.0 - dot(sdir, star_dir);
                let size = 0.0000035 + h.y * 0.000006;
                let twinkle = 0.75 + 0.25 * sin(g.params.z * (1.5 + h.z * 3.0) + h.x * 50.0);
                best = max(best, smoothstep(size, 0.0, d) * twinkle * (0.4 + 0.6 * h.z));
            }
        }
        c += vec3<f32>(0.92, 0.94, 1.0) * best * stars * clamp(dir.y * 6.0 + 0.3, 0.0, 1.0);
    }
    c = draw_planets(dir, c, behind, pixel);
    return vec4<f32>(c, 1.0);
}
