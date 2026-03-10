# シェーダー追加スキル

新しい wgpu レンダリングパイプライン/シェーダーを追加する。

## 手順

1. `crates/minal-renderer/src/shaders/<name>.wgsl` にシェーダーを作成
2. `crates/minal-renderer/src/<pipeline>.rs` にパイプライン構造体を作成
3. `crates/minal-renderer/src/lib.rs` にモジュール追加
4. `Renderer::draw()` から新パイプラインを呼び出し
5. `cargo build -p minal-renderer` で確認

## WGSL シェーダーテンプレート

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

## パイプライン構造体テンプレート

```rust
pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    vertex_buffer: wgpu::Buffer,
}

impl Pipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self { ... }
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) { ... }
}
```

## 既存パイプライン

- `text.wgsl` / `text.rs`: テキスト描画 (グリフアトラスからサンプリング)
- `rect.wgsl` / `rect.rs`: 矩形描画 (背景色、カーソル、選択範囲)
