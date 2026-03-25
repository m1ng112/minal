//! Glyph atlas: packs rasterized glyphs into a GPU texture.
//!
//! Uses [`cosmic_text::SwashCache`] for rasterization and [`guillotiere`]
//! for rectangle packing within a 2048×2048 R8 texture.

use std::collections::HashMap;

use cosmic_text as ct;
use guillotiere::{AllocId, AtlasAllocator, Size as GuillotiereSize};

use crate::RendererError;

/// Default atlas texture dimension (width and height).
const ATLAS_SIZE: i32 = 2048;

/// A glyph's location within the atlas texture.
#[derive(Debug, Clone, Copy)]
pub struct GlyphEntry {
    /// X offset in atlas pixels.
    pub x: u32,
    /// Y offset in atlas pixels.
    pub y: u32,
    /// Glyph width in pixels.
    pub width: u32,
    /// Glyph height in pixels.
    pub height: u32,
    /// Horizontal bearing (offset from cell origin to glyph left edge).
    pub left: i32,
    /// Vertical bearing (offset from baseline to glyph top edge).
    pub top: i32,
    /// Allocation ID for future LRU eviction.
    #[allow(dead_code)]
    alloc_id: AllocId,
}

/// Key for looking up a cached glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    /// Font ID within cosmic-text's font system.
    pub font_id: ct::fontdb::ID,
    /// Glyph index within the font.
    pub glyph_id: u16,
    /// Font size in pixels (quantized to avoid float hashing issues).
    pub size_px: u32,
}

/// Manages a GPU glyph atlas texture with rectangle packing.
pub struct GlyphAtlas {
    /// Rectangle packer.
    allocator: AtlasAllocator,
    /// Cached glyph entries keyed by font+glyph+size.
    cache: HashMap<GlyphKey, GlyphEntry>,
    /// CPU-side pixel data (R8 format) for staging uploads.
    pixels: Vec<u8>,
    /// GPU texture (R8Unorm).
    texture: wgpu::Texture,
    /// GPU texture view.
    texture_view: wgpu::TextureView,
    /// Whether the CPU pixels have been modified since last GPU upload.
    dirty: bool,
    /// Atlas width in pixels.
    width: u32,
    /// Atlas height in pixels.
    height: u32,
}

impl GlyphAtlas {
    /// Creates a new glyph atlas with a 2048×2048 R8 texture.
    pub fn new(device: &wgpu::Device) -> Self {
        let width = ATLAS_SIZE as u32;
        let height = ATLAS_SIZE as u32;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph-atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let allocator = AtlasAllocator::new(GuillotiereSize::new(ATLAS_SIZE, ATLAS_SIZE));

        Self {
            allocator,
            cache: HashMap::new(),
            pixels: vec![0u8; (width * height) as usize],
            texture,
            texture_view,
            dirty: false,
            width,
            height,
        }
    }

    /// Returns the GPU texture view for binding in the render pipeline.
    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    /// Looks up or rasterizes a glyph, returning its atlas entry.
    ///
    /// If the glyph is not cached, it is rasterized via `swash_cache` and
    /// packed into the atlas. Returns `None` if the glyph has no visible
    /// pixels (e.g., space character) or if the atlas is full.
    pub fn get_or_insert(
        &mut self,
        key: GlyphKey,
        font_system: &mut ct::FontSystem,
        swash_cache: &mut ct::SwashCache,
    ) -> Option<GlyphEntry> {
        if let Some(&entry) = self.cache.get(&key) {
            return Some(entry);
        }

        // Rasterize the glyph (uses SwashCache's internal LRU).
        let image = swash_cache
            .get_image(font_system, key.cache_key())
            .as_ref()?;

        let glyph_width = image.placement.width;
        let glyph_height = image.placement.height;

        if glyph_width == 0 || glyph_height == 0 {
            return None;
        }

        // Allocate space in the atlas (add 1px padding to avoid bleeding).
        let alloc = self
            .allocator
            .allocate(GuillotiereSize::new(
                glyph_width as i32 + 1,
                glyph_height as i32 + 1,
            ))
            .or_else(|| {
                // Atlas full: clear everything and retry.
                tracing::warn!("Glyph atlas full, clearing all entries");
                self.clear();
                self.allocator.allocate(GuillotiereSize::new(
                    glyph_width as i32 + 1,
                    glyph_height as i32 + 1,
                ))
            })?;

        let rect = alloc.rectangle;
        let atlas_x = rect.min.x as u32;
        let atlas_y = rect.min.y as u32;

        // Copy glyph pixels into the CPU-side atlas buffer.
        match image.content {
            ct::SwashContent::Mask => {
                for row in 0..glyph_height {
                    for col in 0..glyph_width {
                        let src_idx = (row * glyph_width + col) as usize;
                        let dst_idx = ((atlas_y + row) * self.width + (atlas_x + col)) as usize;
                        if src_idx < image.data.len() && dst_idx < self.pixels.len() {
                            self.pixels[dst_idx] = image.data[src_idx];
                        }
                    }
                }
            }
            ct::SwashContent::Color => {
                // For color glyphs (emoji), take the alpha channel only for now.
                for row in 0..glyph_height {
                    for col in 0..glyph_width {
                        let src_idx = ((row * glyph_width + col) * 4 + 3) as usize;
                        let dst_idx = ((atlas_y + row) * self.width + (atlas_x + col)) as usize;
                        if src_idx < image.data.len() && dst_idx < self.pixels.len() {
                            self.pixels[dst_idx] = image.data[src_idx];
                        }
                    }
                }
            }
            ct::SwashContent::SubpixelMask => {
                // For subpixel, average the RGB channels.
                for row in 0..glyph_height {
                    for col in 0..glyph_width {
                        let base = ((row * glyph_width + col) * 3) as usize;
                        let dst_idx = ((atlas_y + row) * self.width + (atlas_x + col)) as usize;
                        if base + 2 < image.data.len() && dst_idx < self.pixels.len() {
                            let avg = ((image.data[base] as u16
                                + image.data[base + 1] as u16
                                + image.data[base + 2] as u16)
                                / 3) as u8;
                            self.pixels[dst_idx] = avg;
                        }
                    }
                }
            }
        }

        self.dirty = true;

        let entry = GlyphEntry {
            x: atlas_x,
            y: atlas_y,
            width: glyph_width,
            height: glyph_height,
            left: image.placement.left,
            top: image.placement.top,
            alloc_id: alloc.id,
        };

        self.cache.insert(key, entry);
        Some(entry)
    }

    /// Uploads any dirty pixels to the GPU texture.
    pub fn upload(&mut self, queue: &wgpu::Queue) {
        if !self.dirty {
            return;
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.dirty = false;
    }

    /// Pre-rasterize printable ASCII glyphs (0x20..=0x7E) into the atlas.
    ///
    /// This avoids first-frame rasterization latency for the most common characters.
    pub fn prewarm_ascii(
        &mut self,
        keys: &[GlyphKey],
        font_system: &mut ct::FontSystem,
        swash_cache: &mut ct::SwashCache,
    ) {
        for &key in keys {
            let _ = self.get_or_insert(key, font_system, swash_cache);
        }
    }

    /// Clears all cached glyphs and resets the allocator.
    pub fn clear(&mut self) {
        self.allocator.clear();
        self.cache.clear();
        self.pixels.fill(0);
        self.dirty = true;
    }

    /// Returns the atlas dimensions.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl GlyphKey {
    /// Converts to a `cosmic_text::CacheKey` for swash rasterization.
    fn cache_key(&self) -> ct::CacheKey {
        ct::CacheKey {
            font_id: self.font_id,
            glyph_id: self.glyph_id,
            font_size_bits: (self.size_px as f32).to_bits(),
            x_bin: ct::SubpixelBin::Zero,
            y_bin: ct::SubpixelBin::Zero,
            flags: ct::CacheKeyFlags::empty(),
        }
    }
}

/// Creates a wgpu sampler suitable for glyph atlas sampling.
pub fn create_atlas_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("glyph-atlas-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    })
}

/// Creates a `FontSystem` with system fonts loaded.
///
/// # Errors
/// Returns a `RendererError` if font system creation fails.
pub fn create_font_system() -> Result<ct::FontSystem, RendererError> {
    let font_system = ct::FontSystem::new();
    tracing::info!("Font system initialized");
    Ok(font_system)
}
