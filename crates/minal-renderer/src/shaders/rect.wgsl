// Rectangle rendering shader for terminal backgrounds and cursors.
//
// Each instance is a rectangle defined by position, size, and color.
// The vertex shader generates quad vertices from instance data.

struct Uniforms {
    screen_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct InstanceInput {
    // Rectangle position (top-left) in pixels.
    @location(0) pos: vec2<f32>,
    // Rectangle size in pixels.
    @location(1) size: vec2<f32>,
    // RGBA color (0.0-1.0).
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: InstanceInput,
) -> VertexOutput {
    // Generate quad vertices from vertex_index (0..5 for two triangles).
    // Triangle 1: 0,1,2  Triangle 2: 2,1,3
    // 0--2
    // |\ |
    // | \|
    // 1--3
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

    // Convert pixel coordinates to NDC: (0,0)=top-left -> (-1,1), (w,h)=bottom-right -> (1,-1).
    let ndc = vec2<f32>(
        pixel_pos.x / uniforms.screen_size.x * 2.0 - 1.0,
        1.0 - pixel_pos.y / uniforms.screen_size.y * 2.0,
    );

    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
