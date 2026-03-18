//! Text rendering pipeline for terminal glyph display.
//!
//! Renders glyph quads using instanced drawing. Each instance maps a region
//! of the glyph atlas texture to a screen-space rectangle with a foreground color.

use crate::RendererError;
use crate::atlas::GlyphAtlas;

/// A single text glyph instance for GPU rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextInstance {
    /// Top-left position in pixels.
    pub pos: [f32; 2],
    /// Glyph size in pixels.
    pub size: [f32; 2],
    /// Atlas UV top-left (normalized 0.0-1.0).
    pub uv_pos: [f32; 2],
    /// Atlas UV size (normalized 0.0-1.0).
    pub uv_size: [f32; 2],
    /// Foreground RGBA color (0.0-1.0).
    pub fg_color: [f32; 4],
}

/// Uniform data for the text shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TextUniforms {
    screen_size: [f32; 2],
    _padding: [f32; 2],
}

/// GPU pipeline for rendering textured glyph quads.
pub struct TextPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    atlas_bind_group_layout: wgpu::BindGroupLayout,
    atlas_bind_group: Option<wgpu::BindGroup>,
    instance_buffer: wgpu::Buffer,
    /// Maximum number of instances the buffer can hold.
    max_instances: u32,
}

impl TextPipeline {
    /// Creates the text render pipeline.
    ///
    /// # Errors
    /// Returns `RendererError::Shader` if shader compilation fails.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Result<Self, RendererError> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
        });

        // Group 0: uniforms (screen size).
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text-uniform-buffer"),
            size: std::mem::size_of::<TextUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("text-uniform-bind-group-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text-uniform-bind-group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Group 1: atlas texture + sampler.
        let atlas_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("text-atlas-bind-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text-pipeline-layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &atlas_bind_group_layout],
            ..Default::default()
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TextInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        // pos
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        // size
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        // uv_pos
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 16,
                            shader_location: 2,
                        },
                        // uv_size
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 3,
                        },
                        // fg_color
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 4,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let max_instances = 4096u32;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text-instance-buffer"),
            size: (max_instances as usize * std::mem::size_of::<TextInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            atlas_bind_group_layout,
            atlas_bind_group: None,
            instance_buffer,
            max_instances,
        })
    }

    /// Binds the glyph atlas texture and sampler for rendering.
    pub fn bind_atlas(
        &mut self,
        device: &wgpu::Device,
        atlas: &GlyphAtlas,
        sampler: &wgpu::Sampler,
    ) {
        self.atlas_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text-atlas-bind-group"),
            layout: &self.atlas_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(atlas.texture_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        }));
    }

    /// Updates uniforms and uploads instance data, growing the buffer if needed.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_width: f32,
        screen_height: f32,
        instances: &[TextInstance],
    ) {
        let uniforms = TextUniforms {
            screen_size: [screen_width, screen_height],
            _padding: [0.0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let count = instances.len();
        if count == 0 {
            return;
        }

        // Grow buffer if needed.
        if count > self.max_instances as usize {
            let new_max = count.next_power_of_two() as u32;
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text-instance-buffer"),
                size: (new_max as usize * std::mem::size_of::<TextInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.max_instances = new_max;
            tracing::debug!("Text instance buffer grown to {new_max} instances");
        }

        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..count]),
        );
    }

    /// Records draw commands into the given render pass.
    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, instance_count: u32) {
        if instance_count == 0 {
            return;
        }
        let atlas_bind_group = match &self.atlas_bind_group {
            Some(bg) => bg,
            None => {
                tracing::warn!("Text pipeline: atlas not bound, skipping draw");
                return;
            }
        };

        let count = instance_count.min(self.max_instances);
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, atlas_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..6, 0..count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_instance_is_pod() {
        static_assertions::assert_impl_all!(TextInstance: bytemuck::Pod, bytemuck::Zeroable);
    }

    #[test]
    fn text_instance_size() {
        // 2+2+2+2+4 = 12 floats * 4 bytes = 48 bytes
        assert_eq!(std::mem::size_of::<TextInstance>(), 48);
    }
}
