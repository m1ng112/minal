// Text rendering shader for glyph atlas-based terminal text.
//
// Each instance is a glyph defined by screen position, atlas UV coordinates,
// and foreground color. The glyph alpha from the atlas modulates the fg color.

struct Uniforms {
    screen_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;
@group(1) @binding(1)
var atlas_sampler: sampler;

struct InstanceInput {
    // Glyph position (top-left) in pixels.
    @location(0) pos: vec2<f32>,
    // Glyph size in pixels.
    @location(1) size: vec2<f32>,
    // Atlas UV top-left.
    @location(2) uv_pos: vec2<f32>,
    // Atlas UV size.
    @location(3) uv_size: vec2<f32>,
    // Foreground color RGBA.
    @location(4) fg_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fg_color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: InstanceInput,
) -> VertexOutput {
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
    );

    let unit = pos[vertex_index];
    let pixel_pos = instance.pos + unit * instance.size;

    let ndc = vec2<f32>(
        pixel_pos.x / uniforms.screen_size.x * 2.0 - 1.0,
        1.0 - pixel_pos.y / uniforms.screen_size.y * 2.0,
    );

    let uv = instance.uv_pos + unit * instance.uv_size;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    out.fg_color = instance.fg_color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(atlas_texture, atlas_sampler, in.uv).r;
    return vec4<f32>(in.fg_color.rgb, in.fg_color.a * alpha);
}
