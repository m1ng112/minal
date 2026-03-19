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

/// Parse a hex color string (with optional `#` prefix) into an `[f32; 4]` RGBA value.
///
/// Returns black (`[0.0, 0.0, 0.0, 1.0]`) for invalid input.
fn hex_to_rgba(hex: &str) -> [f32; 4] {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// Factor by which dim (SGR 2) reduces foreground color intensity.
const DIM_FACTOR: f32 = 0.66;

/// Resolved color palette from theme configuration.
///
/// All colors are stored as pre-computed `[f32; 4]` RGBA values to avoid
/// per-frame string parsing.
struct ColorPalette {
    fg: [f32; 4],
    bg: [f32; 4],
    cursor: [f32; 4],
    named: [[f32; 4]; 16],
    /// Full 256-color palette for indexed lookups.
    indexed_256: [[f32; 4]; 256],
}

impl ColorPalette {
    /// Creates a palette from a theme configuration.
    fn from_theme(theme: &minal_config::ThemeConfig) -> Self {
        let named = [
            hex_to_rgba(&theme.ansi.black),
            hex_to_rgba(&theme.ansi.red),
            hex_to_rgba(&theme.ansi.green),
            hex_to_rgba(&theme.ansi.yellow),
            hex_to_rgba(&theme.ansi.blue),
            hex_to_rgba(&theme.ansi.magenta),
            hex_to_rgba(&theme.ansi.cyan),
            hex_to_rgba(&theme.ansi.white),
            hex_to_rgba(&theme.ansi.bright_black),
            hex_to_rgba(&theme.ansi.bright_red),
            hex_to_rgba(&theme.ansi.bright_green),
            hex_to_rgba(&theme.ansi.bright_yellow),
            hex_to_rgba(&theme.ansi.bright_blue),
            hex_to_rgba(&theme.ansi.bright_magenta),
            hex_to_rgba(&theme.ansi.bright_cyan),
            hex_to_rgba(&theme.ansi.bright_white),
        ];

        // Build full 256-color palette: 0-15 from theme named colors,
        // 16-255 from the standard xterm palette.
        let base_palette = minal_core::ansi::build_256_palette();
        let mut indexed_256 = [[0.0f32; 4]; 256];
        for (i, entry) in indexed_256.iter_mut().enumerate() {
            if i < 16 {
                *entry = named[i];
            } else {
                let (r, g, b) = base_palette[i];
                *entry = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];
            }
        }

        Self {
            fg: hex_to_rgba(&theme.foreground),
            bg: hex_to_rgba(&theme.background),
            cursor: hex_to_rgba(&theme.foreground),
            named,
            indexed_256,
        }
    }

    /// Creates a palette from the default theme (Catppuccin Mocha).
    #[cfg(test)]
    fn default_palette() -> Self {
        Self::from_theme(&minal_config::ThemeConfig::default())
    }

    /// Returns the RGBA color for a named ANSI color.
    fn named_color(&self, c: NamedColor) -> [f32; 4] {
        let idx = match c {
            NamedColor::Black => 0,
            NamedColor::Red => 1,
            NamedColor::Green => 2,
            NamedColor::Yellow => 3,
            NamedColor::Blue => 4,
            NamedColor::Magenta => 5,
            NamedColor::Cyan => 6,
            NamedColor::White => 7,
            NamedColor::BrightBlack => 8,
            NamedColor::BrightRed => 9,
            NamedColor::BrightGreen => 10,
            NamedColor::BrightYellow => 11,
            NamedColor::BrightBlue => 12,
            NamedColor::BrightMagenta => 13,
            NamedColor::BrightCyan => 14,
            NamedColor::BrightWhite => 15,
        };
        self.named[idx]
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
    /// Resolved color palette from theme config.
    palette: ColorPalette,
    /// Font family name for glyph resolution.
    font_family: String,
    /// Window padding in pixels.
    padding: f32,
}

impl Renderer {
    /// Creates a new renderer with all pipelines initialized.
    ///
    /// Reads font, theme, and window settings from the provided configuration.
    ///
    /// # Errors
    /// Returns `RendererError` if pipeline creation or font loading fails.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        config: &minal_config::Config,
    ) -> Result<Self, RendererError> {
        let rect_pipeline = RectPipeline::new(device, surface_format)?;
        let text_pipeline = TextPipeline::new(device, surface_format)?;
        let mut glyph_atlas = GlyphAtlas::new(device);
        let atlas_sampler = atlas::create_atlas_sampler(device);
        let mut font_system = atlas::create_font_system()?;
        let swash_cache = ct::SwashCache::new();

        let resolved_theme = config.colors.resolve();
        let palette = ColorPalette::from_theme(&resolved_theme);
        let font_family = config.font.family.clone();
        let font_size = config.font.size;
        let line_height = config.font.effective_line_height();
        let padding = config.window.padding as f32;

        // Compute cell dimensions from font metrics.
        let (cell_width, cell_height, baseline_y) =
            compute_cell_metrics(&mut font_system, font_size, line_height, &font_family);

        tracing::info!(
            "Cell metrics: {:.1}x{:.1} px, baseline at {:.1} (font: {}, size: {:.1}, line_height: {:.1})",
            cell_width,
            cell_height,
            baseline_y,
            font_family,
            font_size,
            line_height,
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
            palette,
            font_family,
            padding,
        })
    }

    /// Returns the cell dimensions in pixels.
    pub fn cell_size(&self) -> (f32, f32) {
        (self.cell_width, self.cell_height)
    }

    /// Returns the window padding in pixels.
    pub fn padding(&self) -> f32 {
        self.padding
    }

    /// Updates the color palette from a new theme configuration.
    ///
    /// This only rebuilds the palette struct; no GPU resources are affected
    /// because colors are per-instance data, not baked into shaders.
    pub fn update_theme(&mut self, theme: &minal_config::ThemeConfig) {
        let resolved = theme.resolve();
        self.palette = ColorPalette::from_theme(&resolved);
        tracing::info!("Theme updated");
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

        // Prepare pipelines (dynamic buffer growth).
        self.rect_pipeline
            .prepare(device, queue, sw, sh, &rect_instances);
        self.text_pipeline
            .prepare(device, queue, sw, sh, &text_instances);

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
                            r: self.palette.bg[0] as f64,
                            g: self.palette.bg[1] as f64,
                            b: self.palette.bg[2] as f64,
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
        let padding = self.padding;

        for row_idx in 0..grid.rows() {
            let Some(row) = grid.row(row_idx) else {
                continue;
            };
            for col_idx in 0..row.len() {
                let Some(cell) = row.get(col_idx) else {
                    continue;
                };

                let x = col_idx as f32 * cell_width + padding;
                let y = row_idx as f32 * cell_height + padding;

                let (fg, bg) = resolve_cell_colors(cell, &self.palette);

                // Background rectangle (skip if default/transparent).
                if bg != self.palette.bg {
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
                            &self.font_family,
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

        let x = cursor.col as f32 * self.cell_width + self.padding;
        let y = cursor.row as f32 * self.cell_height + self.padding;

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
            color: self.palette.cursor,
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
    font_family: &str,
) -> Option<GlyphKey> {
    let mut buffer = ct::BufferLine::new(
        format!("{c}"),
        ct::LineEnding::None,
        ct::AttrsList::new(ct::Attrs::new().family(ct::Family::Name(font_family))),
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
///
/// Applies bold-as-bright (standard 8 named colors get promoted to bright
/// variants when bold is set) and dim (reduces RGB intensity by [`DIM_FACTOR`]).
fn resolve_cell_colors(cell: &Cell, palette: &ColorPalette) -> ([f32; 4], [f32; 4]) {
    let mut fg = resolve_color(&cell.fg, palette.fg, palette);
    let mut bg = resolve_color(&cell.bg, palette.bg, palette);

    // Bold-as-bright: promote standard 8 colors (0-7) to bright (8-15).
    if cell.attrs.bold {
        if let Color::Named(named) = &cell.fg {
            let idx = *named as usize;
            if idx < 8 {
                fg = palette.named[idx + 8];
            }
        }
    }

    // Dim: reduce foreground intensity.
    if cell.attrs.dim {
        fg = [
            fg[0] * DIM_FACTOR,
            fg[1] * DIM_FACTOR,
            fg[2] * DIM_FACTOR,
            fg[3],
        ];
    }

    if cell.attrs.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }

    if cell.attrs.hidden {
        fg = bg;
    }

    (fg, bg)
}

/// Converts a terminal `Color` to RGBA float values.
fn resolve_color(color: &Color, default: [f32; 4], palette: &ColorPalette) -> [f32; 4] {
    match color {
        Color::Default => default,
        Color::Named(named) => palette.named_color(*named),
        Color::Indexed(idx) => indexed_color(*idx, palette),
        Color::Rgb(r, g, b) => [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0],
    }
}

/// Converts a 256-color index to RGBA using the pre-computed palette.
fn indexed_color(idx: u8, palette: &ColorPalette) -> [f32; 4] {
    palette.indexed_256[idx as usize]
}

/// Computes cell width, height, and baseline offset from font metrics.
fn compute_cell_metrics(
    font_system: &mut ct::FontSystem,
    font_size: f32,
    line_height: f32,
    font_family: &str,
) -> (f32, f32, f32) {
    // Create a temporary buffer to measure a reference character.
    let mut buffer = ct::Buffer::new(font_system, ct::Metrics::new(font_size, line_height));
    buffer.set_text(
        font_system,
        "M",
        ct::Attrs::new().family(ct::Family::Name(font_family)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use minal_core::cell::CellAttributes;

    #[test]
    fn hex_to_rgba_valid() {
        let c = hex_to_rgba("#ff0000");
        assert!((c[0] - 1.0).abs() < 0.01);
        assert!(c[1].abs() < 0.01);
        assert!(c[2].abs() < 0.01);
        assert!((c[3] - 1.0).abs() < 0.01);
    }

    #[test]
    fn hex_to_rgba_without_hash() {
        let c = hex_to_rgba("00ff00");
        assert!(c[0].abs() < 0.01);
        assert!((c[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn hex_to_rgba_invalid_returns_black() {
        let c = hex_to_rgba("xyz");
        assert_eq!(c, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn color_palette_from_default_theme() {
        let palette = ColorPalette::default_palette();
        // Catppuccin Mocha background is #1e1e2e
        assert!((palette.bg[0] - 0.118).abs() < 0.01);
        assert!((palette.bg[2] - 0.180).abs() < 0.01);
    }

    #[test]
    fn resolve_color_default() {
        let palette = ColorPalette::default_palette();
        let result = resolve_color(&Color::Default, palette.fg, &palette);
        assert_eq!(result, palette.fg);
    }

    #[test]
    fn resolve_color_rgb() {
        let palette = ColorPalette::default_palette();
        let result = resolve_color(&Color::Rgb(255, 0, 0), [0.0; 4], &palette);
        assert!((result[0] - 1.0).abs() < 0.01);
        assert!(result[1].abs() < 0.01);
    }

    #[test]
    fn resolve_color_named() {
        let palette = ColorPalette::default_palette();
        let result = resolve_color(&Color::Named(NamedColor::Red), [0.0; 4], &palette);
        assert!(result[0] > 0.5); // Red should be > 0.5
    }

    #[test]
    fn resolve_cell_colors_inverse() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.attrs = CellAttributes {
            inverse: true,
            ..CellAttributes::default()
        };
        let (fg, bg) = resolve_cell_colors(&cell, &palette);
        // With inverse, fg and bg should be swapped
        assert_eq!(fg, palette.bg);
        assert_eq!(bg, palette.fg);
    }

    #[test]
    fn resolve_cell_colors_hidden() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.attrs = CellAttributes {
            hidden: true,
            ..CellAttributes::default()
        };
        let (fg, bg) = resolve_cell_colors(&cell, &palette);
        // Hidden means fg should equal bg
        assert_eq!(fg, bg);
    }

    #[test]
    fn indexed_color_cube() {
        let palette = ColorPalette::default_palette();
        // Index 16 = first entry of 6x6x6 cube (r=0,g=0,b=0) -> all zero
        let c = indexed_color(16, &palette);
        assert!(c[0].abs() < 0.01);
        assert!(c[1].abs() < 0.01);
        assert!(c[2].abs() < 0.01);
    }

    #[test]
    fn indexed_color_grayscale() {
        let palette = ColorPalette::default_palette();
        // Index 232 = first grayscale entry (8/255)
        let c = indexed_color(232, &palette);
        let expected = 8.0 / 255.0;
        assert!((c[0] - expected).abs() < 0.01);
        assert_eq!(c[0], c[1]);
        assert_eq!(c[1], c[2]);
    }

    #[test]
    fn indexed_color_named_range() {
        let palette = ColorPalette::default_palette();
        // Index 0 = Black, should match palette.named[0]
        let c = indexed_color(0, &palette);
        assert_eq!(c, palette.named[0]);
    }

    #[test]
    fn indexed_color_matches_build_256_for_16_to_255() {
        let palette = ColorPalette::default_palette();
        let base = minal_core::ansi::build_256_palette();
        for i in 16u8..=255 {
            let (r, g, b) = base[i as usize];
            let expected = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];
            let actual = indexed_color(i, &palette);
            assert!(
                (actual[0] - expected[0]).abs() < 0.001
                    && (actual[1] - expected[1]).abs() < 0.001
                    && (actual[2] - expected[2]).abs() < 0.001,
                "mismatch at index {i}: expected {expected:?}, got {actual:?}"
            );
        }
    }

    #[test]
    fn bold_as_bright_named_color() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::Red);
        cell.attrs = CellAttributes {
            bold: true,
            ..CellAttributes::default()
        };
        let (fg, _) = resolve_cell_colors(&cell, &palette);
        // Bold + Red should produce BrightRed.
        assert_eq!(fg, palette.named[NamedColor::BrightRed as usize]);
    }

    #[test]
    fn bold_does_not_affect_rgb() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.fg = Color::Rgb(100, 200, 50);
        cell.attrs = CellAttributes {
            bold: true,
            ..CellAttributes::default()
        };
        let (fg, _) = resolve_cell_colors(&cell, &palette);
        let expected = [100.0 / 255.0, 200.0 / 255.0, 50.0 / 255.0, 1.0];
        assert!((fg[0] - expected[0]).abs() < 0.01);
        assert!((fg[1] - expected[1]).abs() < 0.01);
        assert!((fg[2] - expected[2]).abs() < 0.01);
    }

    #[test]
    fn bold_does_not_affect_bright_colors() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::BrightRed);
        cell.attrs = CellAttributes {
            bold: true,
            ..CellAttributes::default()
        };
        let (fg, _) = resolve_cell_colors(&cell, &palette);
        // BrightRed should stay BrightRed (no double-promotion).
        assert_eq!(fg, palette.named[NamedColor::BrightRed as usize]);
    }

    #[test]
    fn dim_reduces_intensity() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.fg = Color::Rgb(255, 255, 255);
        cell.attrs = CellAttributes {
            dim: true,
            ..CellAttributes::default()
        };
        let (fg, _) = resolve_cell_colors(&cell, &palette);
        let expected = 1.0 * DIM_FACTOR;
        assert!((fg[0] - expected).abs() < 0.01);
        assert!((fg[1] - expected).abs() < 0.01);
        assert!((fg[2] - expected).abs() < 0.01);
        assert!((fg[3] - 1.0).abs() < 0.01); // alpha unchanged
    }

    #[test]
    fn dim_before_inverse() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.fg = Color::Rgb(200, 200, 200);
        cell.bg = Color::Rgb(50, 50, 50);
        cell.attrs = CellAttributes {
            dim: true,
            inverse: true,
            ..CellAttributes::default()
        };
        let (fg, bg) = resolve_cell_colors(&cell, &palette);
        // After dim + inverse: bg becomes the dimmed fg, fg becomes the original bg.
        let dimmed_r = (200.0 / 255.0) * DIM_FACTOR;
        assert!((bg[0] - dimmed_r).abs() < 0.01);
        let bg_r = 50.0 / 255.0;
        assert!((fg[0] - bg_r).abs() < 0.01);
    }
}
