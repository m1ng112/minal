//! Rectangle rendering pipeline for terminal cell backgrounds and cursors.
//!
//! Renders filled rectangles using instanced drawing. Each instance specifies
//! position, size, and color. Used for cell backgrounds and cursor overlay.

use crate::RendererError;

/// A single rectangle instance for GPU rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RectInstance {
    /// Top-left position in pixels.
    pub pos: [f32; 2],
    /// Size in pixels.
    pub size: [f32; 2],
    /// RGBA color (0.0-1.0).
    pub color: [f32; 4],
}

/// Uniform data for the rectangle shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RectUniforms {
    screen_size: [f32; 2],
    _padding: [f32; 2],
}

/// GPU pipeline for rendering filled rectangles.
pub struct RectPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    /// Maximum number of instances the buffer can hold.
    max_instances: u32,
}

impl RectPipeline {
    /// Creates the rectangle render pipeline.
    ///
    /// # Errors
    /// Returns `RendererError::Shader` if shader compilation fails.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Result<Self, RendererError> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rect.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect-uniform-buffer"),
            size: std::mem::size_of::<RectUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rect-bind-group-layout"),
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
            label: Some("rect-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RectInstance>() as u64,
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
                        // color
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 2,
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

        // Pre-allocate instance buffer for 80*24 = 1920 cells + some extra.
        let max_instances = 4096u32;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect-instance-buffer"),
            size: (max_instances as usize * std::mem::size_of::<RectInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            instance_buffer,
            max_instances,
        })
    }

    /// Updates the screen-size uniform and uploads instance data, growing the buffer if needed.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_width: f32,
        screen_height: f32,
        instances: &[RectInstance],
    ) {
        let uniforms = RectUniforms {
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
                label: Some("rect-instance-buffer"),
                size: (new_max as usize * std::mem::size_of::<RectInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.max_instances = new_max;
            tracing::debug!("Rect instance buffer grown to {new_max} instances");
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
        let count = instance_count.min(self.max_instances);
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..6, 0..count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_instance_is_pod() {
        static_assertions::assert_impl_all!(RectInstance: bytemuck::Pod, bytemuck::Zeroable);
    }

    #[test]
    fn rect_instance_size() {
        // 2+2+4 = 8 floats * 4 bytes = 32 bytes
        assert_eq!(std::mem::size_of::<RectInstance>(), 32);
    }
}
