// Boxes (players, creatures, shots), lines (the block highlight, claim outlines) and the crosshair: small things drawn with
// plain positions.


// ---- boxes (players, creatures, shots, the hook), instanced unit cube ----

struct BoxIn {
    @location(0) corner: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) center: vec3<f32>,
    @location(3) half_size: vec3<f32>,
    @location(4) color: vec4<f32>,
    // A unit quaternion (x, y, z, w) turning the box about its centre.
    @location(5) rotation: vec4<f32>,
    // x: sky light, y: block light (0..1 white, or from 16 three levels
    // packed as 16 + r + 16 g + 256 b; `box_block_light`), z: brightness,
    // w: glow (0 lit like terrain, 1 shines alone).
    @location(6) light: vec4<f32>,
};

struct BoxOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) rel: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) light: vec4<f32>,
};

// A box's block light per channel, 0..1 (`BoxInstance::light_of`).
fn box_block_light(y: f32) -> vec3<f32> {
    if (y < 16.0) {
        return vec3<f32>(y);
    }
    let v = u32(round(y - 16.0));
    return vec3<f32>(f32(v & 15u), f32((v >> 4u) & 15u), f32((v >> 8u) & 15u)) / 15.0;
}

fn rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

@vertex
fn vs_box(in: BoxIn) -> BoxOut {
    let rel = in.center + rotate(in.rotation, in.corner * in.half_size);
    var out: BoxOut;
    out.clip = g.view_proj * vec4<f32>(rel, 1.0);
    out.color = in.color.rgb;
    out.rel = rel;
    out.normal = rotate(in.rotation, in.normal);
    out.light = in.light;
    return out;
}

// Lit as terrain is (`shade` in terrain.wgsl): the light curve on sky and
// block light, held torches, sunlit faces, then fog.
@fragment
fn fs_box(in: BoxOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let sky = light_curve(in.light.x);
    let block = light_curve3(max(box_block_light(in.light.y), point_light(in.rel, n)));
    let sun = max(dot(n, g.sun_dir.xyz), 0.0) * g.sun_dir.w;
    let sky_light = sky_light_colour(sky, sun);
    let light = max(sky_light, block) + AMBIENT;
    let lit = in.color * face_shade(n) * light * in.light.z;
    let c = mix(lit, in.color, clamp(in.light.w, 0.0, 1.0));
    let d = length(in.rel);
    let fog = clamp((d - g.params.x) / max(g.params.y - g.params.x, 1.0), 0.0, 1.0);
    return vec4<f32>(mix(c, sky_gradient(in.rel / max(d, 0.001)), fog * fog), 1.0);
}

// ---- lines (block highlight, claim outlines), camera-relative positions ----

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>) -> LineOut {
    var out: LineOut;
    out.clip = g.view_proj * vec4<f32>(pos, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    return in.color;
}

// ---- crosshair, generated from the vertex index ----

@vertex
fn vs_cross(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // Two rectangles, six vertices each.
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[i % 6u];
    var half_px = vec2<f32>(9.0, 1.0);
    if (i >= 6u) {
        half_px = vec2<f32>(1.0, 9.0);
    }
    let ndc = c * half_px * 2.0 / g.screen.xy;
    return vec4<f32>(ndc, 0.0, 1.0);
}

@fragment
fn fs_cross() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 0.85);
}
