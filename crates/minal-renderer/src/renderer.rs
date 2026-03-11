//! High-level terminal renderer orchestrating text and rectangle pipelines.
//!
//! [`Renderer`] owns the glyph atlas, font system, and GPU pipelines.
//! It reads terminal state (grid, cursor) and produces GPU draw calls.

use std::collections::HashMap;

use cosmic_text as ct;
use minal_core::ansi::{Color, NamedColor};
use minal_core::cell::Cell;
use minal_core::cursor::{Cursor, CursorStyle};
use minal_core::grid::Grid;

use crate::RendererError;
use crate::atlas::{self, GlyphAtlas, GlyphKey};
use crate::rect::{RectInstance, RectPipeline};
use crate::text::{TextInstance, TextPipeline};

/// Default font size in pixels.
const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Catppuccin Mocha color palette for ANSI color mapping.
mod palette {
    /// Default foreground: Catppuccin Mocha "Text" (#cdd6f4).
    pub const FG: [f32; 4] = [0.804, 0.839, 0.957, 1.0];
    /// Default background: Catppuccin Mocha "Base" (#1e1e2e).
    pub const BG: [f32; 4] = [0.118, 0.118, 0.180, 1.0];
    /// Cursor color: Catppuccin Mocha "Rosewater" (#f5e0dc).
    pub const CURSOR: [f32; 4] = [0.961, 0.878, 0.863, 1.0];

    /// Map ANSI named colors to Catppuccin Mocha palette.
    pub fn named_color(c: super::NamedColor) -> [f32; 4] {
        match c {
            super::NamedColor::Black => [0.180, 0.192, 0.247, 1.0], // Surface1
            super::NamedColor::Red => [0.953, 0.545, 0.659, 1.0],   // Red
            super::NamedColor::Green => [0.655, 0.890, 0.631, 1.0], // Green
            super::NamedColor::Yellow => [0.976, 0.886, 0.686, 1.0], // Yellow
            super::NamedColor::Blue => [0.537, 0.706, 0.980, 1.0],  // Blue
            super::NamedColor::Magenta => [0.796, 0.651, 0.969, 1.0], // Mauve
            super::NamedColor::Cyan => [0.580, 0.886, 0.929, 1.0],  // Teal
            super::NamedColor::White => [0.729, 0.749, 0.831, 1.0], // Subtext0
            super::NamedColor::BrightBlack => [0.427, 0.443, 0.537, 1.0], // Overlay0
            super::NamedColor::BrightRed => [0.953, 0.545, 0.659, 1.0],
            super::NamedColor::BrightGreen => [0.655, 0.890, 0.631, 1.0],
            super::NamedColor::BrightYellow => [0.976, 0.886, 0.686, 1.0],
            super::NamedColor::BrightBlue => [0.537, 0.706, 0.980, 1.0],
            super::NamedColor::BrightMagenta => [0.796, 0.651, 0.969, 1.0],
            super::NamedColor::BrightCyan => [0.580, 0.886, 0.929, 1.0],
            super::NamedColor::BrightWhite => [0.804, 0.839, 0.957, 1.0], // Text
        }
    }
}

/// High-level renderer that draws terminal content using GPU pipelines.
pub struct Renderer {
    rect_pipeline: RectPipeline,
    text_pipeline: TextPipeline,
    glyph_atlas: GlyphAtlas,
    atlas_sampler: wgpu::Sampler,
    font_system: ct::FontSystem,
    swash_cache: ct::SwashCache,
    /// Cell width in pixels (determined by font metrics).
    cell_width: f32,
    /// Cell height in pixels (determined by font metrics).
    cell_height: f32,
    /// Font size in pixels.
    font_size: f32,
    /// Baseline offset from cell top.
    baseline_y: f32,
    /// Whether atlas bind group needs recreation.
    atlas_dirty: bool,
    /// Cache mapping characters to their GlyphKey to avoid per-frame layout allocations.
    char_glyph_cache: HashMap<char, Option<GlyphKey>>,
}

impl Renderer {
    /// Creates a new renderer with all pipelines initialized.
    ///
    /// # Errors
    /// Returns `RendererError` if pipeline creation or font loading fails.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Result<Self, RendererError> {
        let rect_pipeline = RectPipeline::new(device, surface_format)?;
        let text_pipeline = TextPipeline::new(device, surface_format)?;
        let mut glyph_atlas = GlyphAtlas::new(device);
        let atlas_sampler = atlas::create_atlas_sampler(device);
        let mut font_system = atlas::create_font_system()?;
        let swash_cache = ct::SwashCache::new();

        let font_size = DEFAULT_FONT_SIZE;

        // Compute cell dimensions from font metrics.
        let (cell_width, cell_height, baseline_y) =
            compute_cell_metrics(&mut font_system, font_size);

        tracing::info!(
            "Cell metrics: {:.1}x{:.1} px, baseline at {:.1}",
            cell_width,
            cell_height,
            baseline_y
        );

        if cell_width <= 0.0 || cell_height <= 0.0 {
            tracing::warn!(
                "Font metrics returned invalid cell size ({:.1}x{:.1}), using fallback",
                cell_width,
                cell_height
            );
        }

        // Perform initial empty atlas upload.
        glyph_atlas.upload(queue);

        Ok(Self {
            rect_pipeline,
            text_pipeline,
            glyph_atlas,
            atlas_sampler,
            font_system,
            swash_cache,
            cell_width,
            cell_height,
            font_size,
            baseline_y,
            atlas_dirty: true,
            char_glyph_cache: HashMap::new(),
        })
    }

    /// Returns the cell dimensions in pixels.
    pub fn cell_size(&self) -> (f32, f32) {
        (self.cell_width, self.cell_height)
    }

    /// Renders the terminal content to the given texture view.
    ///
    /// Draws background rectangles, then text glyphs, then cursor overlay.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        grid: &Grid,
        cursor: &Cursor,
    ) {
        let sw = screen_width as f32;
        let sh = screen_height as f32;

        // Build instance data from terminal state.
        let mut rect_instances = Vec::new();
        let mut text_instances = Vec::new();

        self.build_cell_instances(grid, &mut rect_instances, &mut text_instances);

        // Add cursor.
        self.build_cursor_instance(cursor, &mut rect_instances);

        // Upload atlas if glyphs were added.
        self.glyph_atlas.upload(queue);

        // Rebind atlas texture if dirty.
        if self.atlas_dirty {
            self.text_pipeline
                .bind_atlas(device, &self.glyph_atlas, &self.atlas_sampler);
            self.atlas_dirty = false;
        }

        // Prepare pipelines.
        self.rect_pipeline.prepare(queue, sw, sh, &rect_instances);
        self.text_pipeline.prepare(queue, sw, sh, &text_instances);

        // Encode render pass.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("renderer-encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terminal-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: palette::BG[0] as f64,
                            g: palette::BG[1] as f64,
                            b: palette::BG[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Draw backgrounds first, then text on top.
            self.rect_pipeline
                .draw(&mut pass, rect_instances.len() as u32);
            self.text_pipeline
                .draw(&mut pass, text_instances.len() as u32);
        }

        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Builds rect and text instances from the terminal grid.
    fn build_cell_instances(
        &mut self,
        grid: &Grid,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) {
        let atlas_w = self.glyph_atlas.size().0 as f32;
        let atlas_h = self.glyph_atlas.size().1 as f32;
        let font_size_px = self.font_size as u32;
        let cell_width = self.cell_width;
        let cell_height = self.cell_height;
        let baseline_y = self.baseline_y;

        for row_idx in 0..grid.rows() {
            let Some(row) = grid.row(row_idx) else {
                continue;
            };
            for col_idx in 0..row.len() {
                let Some(cell) = row.get(col_idx) else {
                    continue;
                };

                let x = col_idx as f32 * cell_width;
                let y = row_idx as f32 * cell_height;

                let (fg, bg) = resolve_cell_colors(cell);

                // Background rectangle (skip if default/transparent).
                if bg != palette::BG {
                    rect_instances.push(RectInstance {
                        pos: [x, y],
                        size: [cell_width, cell_height],
                        color: bg,
                    });
                }

                // Skip space characters (nothing to render).
                if cell.c == ' ' || cell.c == '\0' {
                    continue;
                }

                // Look up glyph key from cache, or resolve via font system.
                let glyph_key = match self.char_glyph_cache.get(&cell.c) {
                    Some(cached) => *cached,
                    None => {
                        let key = resolve_glyph_key(
                            &mut self.font_system,
                            cell.c,
                            self.font_size,
                            font_size_px,
                        );
                        self.char_glyph_cache.insert(cell.c, key);
                        key
                    }
                };

                if let Some(glyph_key) = glyph_key {
                    // Use explicit disjoint field borrows for the atlas lookup.
                    let atlas = &mut self.glyph_atlas;
                    let font_system = &mut self.font_system;
                    let swash_cache = &mut self.swash_cache;

                    if let Some(entry) = atlas.get_or_insert(glyph_key, font_system, swash_cache) {
                        self.atlas_dirty = true;

                        let glyph_x = x + entry.left as f32;
                        let glyph_y = y + baseline_y - entry.top as f32;

                        text_instances.push(TextInstance {
                            pos: [glyph_x, glyph_y],
                            size: [entry.width as f32, entry.height as f32],
                            uv_pos: [entry.x as f32 / atlas_w, entry.y as f32 / atlas_h],
                            uv_size: [entry.width as f32 / atlas_w, entry.height as f32 / atlas_h],
                            fg_color: fg,
                        });
                    }
                }
            }
        }
    }

    /// Builds the cursor rectangle instance.
    fn build_cursor_instance(&self, cursor: &Cursor, rect_instances: &mut Vec<RectInstance>) {
        if !cursor.visible {
            return;
        }

        let x = cursor.col as f32 * self.cell_width;
        let y = cursor.row as f32 * self.cell_height;

        let (width, height) = match cursor.style {
            CursorStyle::Block => (self.cell_width, self.cell_height),
            CursorStyle::Underline => (self.cell_width, 2.0),
            CursorStyle::Bar => (2.0, self.cell_height),
        };

        let cursor_y = match cursor.style {
            CursorStyle::Underline => y + self.cell_height - 2.0,
            _ => y,
        };

        rect_instances.push(RectInstance {
            pos: [x, cursor_y],
            size: [width, height],
            color: palette::CURSOR,
        });
    }
}

/// Resolves a character to its `GlyphKey` via cosmic-text font matching.
///
/// This is called once per unique character and the result is cached.
fn resolve_glyph_key(
    font_system: &mut ct::FontSystem,
    c: char,
    font_size: f32,
    size_px: u32,
) -> Option<GlyphKey> {
    let mut buffer = ct::BufferLine::new(
        format!("{c}"),
        ct::LineEnding::None,
        ct::AttrsList::new(ct::Attrs::new().family(ct::Family::Monospace)),
        ct::Shaping::Advanced,
    );
    let layout_lines = buffer.layout(
        font_system,
        font_size,
        Some(f32::MAX),
        ct::Wrap::None,
        None,
        8, // tab width
    );

    let layout_line = layout_lines.first()?;
    let glyph = layout_line.glyphs.first()?;

    Some(GlyphKey {
        font_id: glyph.font_id,
        glyph_id: glyph.glyph_id,
        size_px,
    })
}

/// Resolves cell foreground and background colors to RGBA.
fn resolve_cell_colors(cell: &Cell) -> ([f32; 4], [f32; 4]) {
    let mut fg = resolve_color(&cell.fg, palette::FG);
    let mut bg = resolve_color(&cell.bg, palette::BG);

    if cell.attrs.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }

    if cell.attrs.hidden {
        fg = bg;
    }

    (fg, bg)
}

/// Converts a terminal `Color` to RGBA float values.
fn resolve_color(color: &Color, default: [f32; 4]) -> [f32; 4] {
    match color {
        Color::Default => default,
        Color::Named(named) => palette::named_color(*named),
        Color::Indexed(idx) => indexed_color(*idx),
        Color::Rgb(r, g, b) => [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0],
    }
}

/// Converts a 256-color index to RGBA.
fn indexed_color(idx: u8) -> [f32; 4] {
    match idx {
        0..=15 => {
            // Map to named colors via Catppuccin palette.
            let named = match idx {
                0 => NamedColor::Black,
                1 => NamedColor::Red,
                2 => NamedColor::Green,
                3 => NamedColor::Yellow,
                4 => NamedColor::Blue,
                5 => NamedColor::Magenta,
                6 => NamedColor::Cyan,
                7 => NamedColor::White,
                8 => NamedColor::BrightBlack,
                9 => NamedColor::BrightRed,
                10 => NamedColor::BrightGreen,
                11 => NamedColor::BrightYellow,
                12 => NamedColor::BrightBlue,
                13 => NamedColor::BrightMagenta,
                14 => NamedColor::BrightCyan,
                _ => NamedColor::BrightWhite,
            };
            palette::named_color(named)
        }
        16..=231 => {
            // 6x6x6 color cube.
            let idx = idx - 16;
            let r = (idx / 36) % 6;
            let g = (idx / 6) % 6;
            let b = idx % 6;
            let to_f = |v: u8| -> f32 {
                if v == 0 {
                    0.0
                } else {
                    (55.0 + 40.0 * v as f32) / 255.0
                }
            };
            [to_f(r), to_f(g), to_f(b), 1.0]
        }
        232..=255 => {
            // Grayscale ramp.
            let v = (8 + 10 * (idx - 232) as u16) as f32 / 255.0;
            [v, v, v, 1.0]
        }
    }
}

/// Computes cell width, height, and baseline offset from font metrics.
fn compute_cell_metrics(font_system: &mut ct::FontSystem, font_size: f32) -> (f32, f32, f32) {
    // Create a temporary buffer to measure a reference character.
    let mut buffer = ct::Buffer::new(font_system, ct::Metrics::new(font_size, font_size * 1.2));
    buffer.set_text(
        font_system,
        "M",
        ct::Attrs::new().family(ct::Family::Monospace),
        ct::Shaping::Advanced,
    );
    buffer.shape_until_scroll(font_system, false);

    let metrics = buffer.metrics();
    let cell_height = metrics.line_height;

    // Get the advance width from the first glyph.
    let cell_width = buffer
        .layout_runs()
        .next()
        .and_then(|run| run.glyphs.first())
        .map(|g| g.w)
        .unwrap_or(font_size * 0.6);

    // Baseline = ascent portion of line height.
    // Use font_size as a reasonable approximation for ascent.
    let baseline_y = font_size * 0.8;

    (cell_width, cell_height, baseline_y)
}
