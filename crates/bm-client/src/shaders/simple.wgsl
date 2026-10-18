// Players, the block highlight and the crosshair: small things drawn with
// plain positions.


// ---- boxes (other players), instanced unit cube ----

struct BoxIn {
    @location(0) corner: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) center: vec3<f32>,
    @location(3) half_size: vec3<f32>,
    @location(4) color: vec4<f32>,
};

struct BoxOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_box(in: BoxIn) -> BoxOut {
    let rel = in.center + in.corner * in.half_size;
    var out: BoxOut;
    out.clip = g.view_proj * vec4<f32>(rel, 1.0);
    let light = (0.6 + 0.4 * max(dot(in.normal, g.sun_dir.xyz), 0.0) * g.sun_dir.w) * g.sun_color.w;
    out.color = in.color.rgb * light;
    return out;
}

@fragment
fn fs_box(in: BoxOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}

// ---- lines (block highlight), camera-relative positions ----

@vertex
fn vs_line(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return g.view_proj * vec4<f32>(pos, 1.0);
}

@fragment
fn fs_line() -> @location(0) vec4<f32> {
    return vec4<f32>(0.05, 0.05, 0.07, 0.9);
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
