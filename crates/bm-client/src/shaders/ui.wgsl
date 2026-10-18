// The HUD: pixel-space triangles over the finished frame (built by ui.rs).

@group(0) @binding(3) var ui_tex: texture_2d_array<f32>;
@group(0) @binding(4) var ui_samp: sampler;

struct UiIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: u32,
    @location(3) color: vec4<f32>,
};

struct UiOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_ui(in: UiIn) -> UiOut {
    var out: UiOut;
    let ndc = vec2<f32>(in.pos.x / g.screen.x * 2.0 - 1.0, 1.0 - in.pos.y / g.screen.y * 2.0);
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.layer = in.layer;
    out.color = in.color;
    return out;
}

@fragment
fn fs_ui(in: UiOut) -> @location(0) vec4<f32> {
    let textured = in.layer != 0xffffu;
    let t = textureSample(ui_tex, ui_samp, in.uv, select(0u, in.layer, textured));
    if (textured) {
        return t * in.color;
    }
    return in.color;
}
