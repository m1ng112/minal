//! Test utilities for headless GPU rendering and pixel verification.
//!
//! Provides [`OffscreenContext`] for creating wgpu devices without a window
//! and reading back rendered pixels from offscreen textures.

use crate::RendererError;

/// Headless wgpu context for offscreen rendering tests.
///
/// Creates a GPU device without a window/surface. Used to verify rendering
/// output by reading back pixel data from offscreen textures.
pub(crate) struct OffscreenContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl OffscreenContext {
    /// Creates a new headless GPU context.
    ///
    /// Returns an error if no GPU adapter is available (e.g., headless CI
    /// without GPU drivers installed).
    pub fn new() -> Result<Self, RendererError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| {
            tracing::debug!("Offscreen adapter request failed: {e}");
            RendererError::AdapterNotFound
        })?;

        let info = adapter.get_info();
        tracing::info!(
            "Offscreen GPU adapter: {} ({:?}, {:?})",
            info.name,
            info.device_type,
            info.backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("test-device"),
            ..Default::default()
        }))
        .map_err(|e| RendererError::DeviceRequest(e.to_string()))?;

        Ok(Self { device, queue })
    }

    /// Creates an offscreen texture suitable for rendering and readback.
    pub fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Reads pixel data from a texture as RGBA bytes.
    ///
    /// Copies the texture to a staging buffer, maps it, and returns the raw
    /// pixel data with row padding stripped.
    pub fn read_pixels(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, RendererError> {
        let bytes_per_pixel = 4u32; // RGBA
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = 256u32; // wgpu COPY_BYTES_PER_ROW_ALIGNMENT
        let padded_bytes_per_row = (unpadded_bytes_per_row + align - 1) / align * align;
        let buffer_size = (padded_bytes_per_row * height) as u64;

        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback-buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback-encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = staging_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });

        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());

        rx.recv()
            .map_err(|e| RendererError::BufferMap(e.to_string()))?
            .map_err(|e| RendererError::BufferMap(e.to_string()))?;

        let data = buffer_slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((width * height * bytes_per_pixel) as usize);
        for row in 0..height {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + unpadded_bytes_per_row as usize;
            pixels.extend_from_slice(&data[start..end]);
        }

        drop(data);
        staging_buffer.unmap();

        Ok(pixels)
    }
}
