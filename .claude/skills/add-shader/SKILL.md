---
name: add-shader
description: "Add a new wgpu rendering pipeline and WGSL shader to crates/minal-renderer/. Use when creating new visual elements like overlays, effects, or custom drawing."
argument-hint: "[pipeline-name]"
---

Add a new rendering pipeline `$ARGUMENTS` to `crates/minal-renderer/`.

## Steps

1. Create shader: `crates/minal-renderer/src/shaders/$ARGUMENTS.wgsl`
2. Create pipeline: `crates/minal-renderer/src/$ARGUMENTS.rs`
3. Add module in `crates/minal-renderer/src/lib.rs`
4. Integrate into `Renderer::draw()` call sequence
5. Verify: `cargo build -p minal-renderer`

## WGSL Shader Template

```wgsl
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> transform: mat4x4<f32>;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = transform * vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
```

## Pipeline Struct Template

```rust
pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    vertex_buffer: wgpu::Buffer,
}

impl Pipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        // 1. Load shader with device.create_shader_module()
        // 2. Create bind group layout
        // 3. Create pipeline layout
        // 4. Create render pipeline
        // 5. Create vertex buffer
        todo!()
    }

    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        // 1. Begin render pass
        // 2. Set pipeline
        // 3. Set bind groups
        // 4. Set vertex buffer
        // 5. Draw
        todo!()
    }
}
```

## Existing Pipelines

- `text.wgsl` / `text.rs`: Text drawing (glyph atlas sampling)
- `rect.wgsl` / `rect.rs`: Rectangle drawing (background, cursor, selection)
