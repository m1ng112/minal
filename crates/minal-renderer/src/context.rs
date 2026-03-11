//! GPU rendering context management.
//!
//! Provides [`GpuContext`] which owns wgpu Device, Queue, and Surface.
//! Created from a winit Window via `Arc<Window>`.

use std::sync::Arc;
use winit::window::Window;

use crate::RendererError;

/// Encodes a render pass that clears the given view with the specified color.
///
/// This is a Surface-independent helper used by both [`GpuContext::render_clear`]
/// and offscreen tests.
pub(crate) fn encode_clear_pass(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    r: f64,
    g: f64,
    b: f64,
) {
    let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("clear-pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

/// A surface frame acquired for rendering.
///
/// Holds the surface texture output and its view. Call [`present`](SurfaceFrame::present)
/// after submitting render commands.
pub struct SurfaceFrame {
    output: wgpu::SurfaceTexture,
    /// The texture view for the current frame.
    pub view: wgpu::TextureView,
}

impl SurfaceFrame {
    /// Presents the rendered frame to the screen.
    pub fn present(self) {
        self.output.present();
    }
}

/// Manages the wgpu rendering context: device, queue, and surface.
///
/// Created from an `Arc<winit::window::Window>`. Owns all GPU state needed
/// for rendering. Call [`resize`](GpuContext::resize) when the window
/// size changes.
pub struct GpuContext {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: (u32, u32),
}

impl GpuContext {
    /// Creates a new GPU context from a winit window.
    ///
    /// Initializes the full wgpu pipeline: Instance -> Adapter -> Device -> Queue -> Surface.
    /// Uses `pollster::block_on` for async wgpu calls (resolves immediately on native).
    ///
    /// # Note
    /// `pollster` is used for Phase 1. Will be replaced by tokio's block_on
    /// when the async runtime is introduced in later phases.
    pub fn new(window: Arc<Window>) -> Result<Self, RendererError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Window(Box::new(window.clone())))
            .map_err(|e| RendererError::SurfaceInit(e.to_string()))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|e| {
            tracing::debug!("Adapter request error detail: {e}");
            RendererError::AdapterNotFound
        })?;

        let info = adapter.get_info();
        tracing::info!(
            "GPU adapter: {} ({:?}, {:?})",
            info.name,
            info.device_type,
            info.backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("minal-device"),
            ..Default::default()
        }))
        .map_err(|e: wgpu::RequestDeviceError| RendererError::DeviceRequest(e.to_string()))?;

        let phys_size = window.inner_size();
        let width = phys_size.width.max(1);
        let height = phys_size.height.max(1);

        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| {
                RendererError::SurfaceConfig("No compatible surface configuration found".into())
            })?;

        surface.configure(&device, &config);

        tracing::info!("wgpu surface configured: {}x{}", width, height);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size: (width, height),
        })
    }

    /// Resizes the surface. Skips if width or height is zero (minimized window).
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            tracing::debug!("Skipping resize to zero-size: {}x{}", width, height);
            return;
        }
        self.size = (width, height);
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        tracing::debug!("Surface resized to {}x{}", width, height);
    }

    /// Clears the screen with the given RGB color (0.0-1.0 range).
    ///
    /// # TODO
    /// Move to a dedicated `Renderer` struct in Step 1.3 (text rendering pipeline).
    pub fn render_clear(&self, r: f64, g: f64, b: f64) -> Result<(), RendererError> {
        let output = self.surface.get_current_texture().map_err(|e| match e {
            wgpu::SurfaceError::OutOfMemory => RendererError::OutOfMemory,
            wgpu::SurfaceError::Lost => RendererError::SurfaceLost,
            wgpu::SurfaceError::Outdated => RendererError::SurfaceOutdated,
            wgpu::SurfaceError::Timeout => RendererError::SurfaceTimeout,
            wgpu::SurfaceError::Other => RendererError::SurfaceOther("backend error".into()),
        })?;

        if output.suboptimal {
            tracing::debug!("Surface texture is suboptimal; reconfiguration recommended");
        }

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("clear-encoder"),
            });

        encode_clear_pass(&mut encoder, &view, r, g, b);

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Returns a reference to the wgpu device.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Returns a reference to the wgpu queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Returns a reference to the current surface configuration.
    pub fn config(&self) -> &wgpu::SurfaceConfiguration {
        &self.config
    }

    /// Returns the current surface size as (width, height).
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Acquires the next surface texture for rendering.
    ///
    /// Returns the surface texture output which must be presented after rendering,
    /// along with a texture view for the render pass.
    pub fn begin_frame(&self) -> Result<SurfaceFrame, RendererError> {
        let output = self.surface.get_current_texture().map_err(|e| match e {
            wgpu::SurfaceError::OutOfMemory => RendererError::OutOfMemory,
            wgpu::SurfaceError::Lost => RendererError::SurfaceLost,
            wgpu::SurfaceError::Outdated => RendererError::SurfaceOutdated,
            wgpu::SurfaceError::Timeout => RendererError::SurfaceTimeout,
            wgpu::SurfaceError::Other => RendererError::SurfaceOther("backend error".into()),
        })?;

        if output.suboptimal {
            tracing::debug!("Surface texture is suboptimal; reconfiguration recommended");
        }

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        Ok(SurfaceFrame { output, view })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_impl_all;

    assert_impl_all!(GpuContext: Send);

    /// Helper to create an offscreen context, skipping if GPU is unavailable.
    macro_rules! gpu_context {
        () => {
            match crate::test_harness::OffscreenContext::new() {
                Ok(ctx) => ctx,
                Err(e) => {
                    eprintln!("Skipping GPU test: {e}");
                    return;
                }
            }
        };
    }

    /// Helper to assert pixel color with tolerance.
    fn assert_pixel_eq(pixel: &[u8], expected: [u8; 4], tolerance: u8) {
        for (i, (&actual, &exp)) in pixel.iter().zip(expected.iter()).enumerate() {
            let channel = ["R", "G", "B", "A"][i];
            assert!(
                actual.abs_diff(exp) <= tolerance,
                "{channel} channel: expected {exp} +/- {tolerance}, got {actual}"
            );
        }
    }

    #[test]
    #[ignore] // Requires GPU -- run with: cargo test -p minal-renderer -- --ignored
    fn test_clear_red() {
        let ctx = gpu_context!();
        let (width, height) = (64, 64);
        let texture = ctx.create_texture(width, height, wgpu::TextureFormat::Rgba8Unorm);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("test-encoder"),
            });
        encode_clear_pass(&mut encoder, &view, 1.0, 0.0, 0.0);
        ctx.queue.submit(std::iter::once(encoder.finish()));

        let pixels = ctx.read_pixels(&texture, width, height).unwrap();
        for chunk in pixels.chunks_exact(4) {
            assert_pixel_eq(chunk, [255, 0, 0, 255], 0);
        }
    }

    #[test]
    #[ignore]
    fn test_clear_green() {
        let ctx = gpu_context!();
        let (width, height) = (64, 64);
        let texture = ctx.create_texture(width, height, wgpu::TextureFormat::Rgba8Unorm);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("test-encoder"),
            });
        encode_clear_pass(&mut encoder, &view, 0.0, 1.0, 0.0);
        ctx.queue.submit(std::iter::once(encoder.finish()));

        let pixels = ctx.read_pixels(&texture, width, height).unwrap();
        for chunk in pixels.chunks_exact(4) {
            assert_pixel_eq(chunk, [0, 255, 0, 255], 0);
        }
    }

    #[test]
    #[ignore]
    fn test_clear_black() {
        let ctx = gpu_context!();
        let (width, height) = (64, 64);
        let texture = ctx.create_texture(width, height, wgpu::TextureFormat::Rgba8Unorm);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("test-encoder"),
            });
        encode_clear_pass(&mut encoder, &view, 0.0, 0.0, 0.0);
        ctx.queue.submit(std::iter::once(encoder.finish()));

        let pixels = ctx.read_pixels(&texture, width, height).unwrap();
        for chunk in pixels.chunks_exact(4) {
            assert_pixel_eq(chunk, [0, 0, 0, 255], 0);
        }
    }

    #[test]
    #[ignore]
    fn test_clear_custom_color() {
        let ctx = gpu_context!();
        let (width, height) = (64, 64);
        let texture = ctx.create_texture(width, height, wgpu::TextureFormat::Rgba8Unorm);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("test-encoder"),
            });
        // Catppuccin Mocha base: #1e1e2e = (30/255, 30/255, 46/255)
        encode_clear_pass(
            &mut encoder,
            &view,
            30.0 / 255.0,
            30.0 / 255.0,
            46.0 / 255.0,
        );
        ctx.queue.submit(std::iter::once(encoder.finish()));

        let pixels = ctx.read_pixels(&texture, width, height).unwrap();
        for chunk in pixels.chunks_exact(4) {
            // Tolerance of 1 for float-to-unorm rounding differences across GPU backends
            assert_pixel_eq(chunk, [30, 30, 46, 255], 1);
        }
    }
}
