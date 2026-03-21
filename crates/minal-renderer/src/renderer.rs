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
use minal_core::selection::Selection;
use minal_core::term::GhostText;

use crate::RendererError;
use crate::atlas::{self, GlyphAtlas, GlyphKey};
use crate::rect::{RectInstance, RectPipeline};
use crate::text::{TextInstance, TextPipeline};

/// A rectangular viewport region in pixel coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Information needed to render a single tab in the tab bar.
#[derive(Debug, Clone)]
pub struct TabBarInfo {
    pub title: String,
    pub is_active: bool,
}

/// Height of the tab bar in pixels.
pub const TAB_BAR_HEIGHT: f32 = 28.0;

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

/// Resolved color palette from theme configuration.
///
/// All colors are stored as pre-computed `[f32; 4]` RGBA values to avoid
/// per-frame string parsing.
struct ColorPalette {
    fg: [f32; 4],
    bg: [f32; 4],
    cursor: [f32; 4],
    selection: [f32; 4],
    named: [[f32; 4]; 16],
}

impl ColorPalette {
    /// Creates a palette from a theme configuration.
    fn from_theme(theme: &minal_config::ThemeConfig) -> Self {
        Self {
            fg: hex_to_rgba(&theme.foreground),
            bg: hex_to_rgba(&theme.background),
            cursor: hex_to_rgba(&theme.foreground), // Use foreground as cursor color
            selection: [0.4, 0.5, 0.7, 0.3],
            named: [
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
            ],
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
    pub(crate) glyph_atlas: GlyphAtlas,
    atlas_sampler: wgpu::Sampler,
    pub(crate) font_system: ct::FontSystem,
    pub(crate) swash_cache: ct::SwashCache,
    /// Cell width in pixels (determined by font metrics).
    pub(crate) cell_width: f32,
    /// Cell height in pixels (determined by font metrics).
    pub(crate) cell_height: f32,
    /// Font size in pixels.
    pub(crate) font_size: f32,
    /// Baseline offset from cell top.
    pub(crate) baseline_y: f32,
    /// Whether atlas bind group needs recreation.
    pub(crate) atlas_dirty: bool,
    /// Cache mapping characters to their GlyphKey to avoid per-frame layout allocations.
    pub(crate) char_glyph_cache: HashMap<char, Option<GlyphKey>>,
    /// Resolved color palette from theme config.
    palette: ColorPalette,
    /// Font family name for glyph resolution.
    pub(crate) font_family: String,
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

        let palette = ColorPalette::from_theme(&config.colors);
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
    /// Call this when the user changes the theme preset or the config file
    /// is hot-reloaded. The next `render()` call will use the new colors.
    pub fn update_theme(&mut self, theme: &minal_config::ThemeConfig) {
        self.palette = ColorPalette::from_theme(theme);
    }

    /// Renders the terminal content to the given texture view (single-pane legacy path).
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
        ghost_text: Option<&GhostText>,
        selection: Option<&Selection>,
    ) {
        let sw = screen_width as f32;
        let sh = screen_height as f32;

        // Build instance data from terminal state.
        let mut rect_instances = Vec::new();
        let mut text_instances = Vec::new();

        self.build_cell_instances(grid, &mut rect_instances, &mut text_instances);

        // Add ghost text overlay before cursor.
        if let Some(gt) = ghost_text {
            self.build_ghost_text_instances(gt, grid, &mut text_instances);
        }

        // Add selection highlight overlay.
        if let Some(sel) = selection {
            self.build_selection_instances(sel, grid, &mut rect_instances);
        }

        // Add cursor.
        self.build_cursor_instance(cursor, &mut rect_instances);

        self.submit_frame(
            device,
            queue,
            view,
            sw,
            sh,
            &rect_instances,
            &text_instances,
        );
    }

    /// Renders multiple panes, an optional tab bar, and dividers to the given texture view.
    #[allow(clippy::too_many_arguments)]
    pub fn render_multi_pane<F>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        tabs: &[TabBarInfo],
        show_tab_bar: bool,
        divider_rects: &[(f32, f32, f32, f32)],
        focused_pane_viewport: Option<Viewport>,
        mut each_pane: F,
    ) where
        F: FnMut(&mut Self, &mut Vec<RectInstance>, &mut Vec<TextInstance>),
    {
        let sw = screen_width as f32;
        let sh = screen_height as f32;

        let mut rect_instances = Vec::new();
        let mut text_instances = Vec::new();

        // Render tab bar if visible.
        if show_tab_bar {
            self.build_tab_bar_instances(tabs, sw, &mut rect_instances, &mut text_instances);
        }

        // Let caller add pane instances.
        each_pane(self, &mut rect_instances, &mut text_instances);

        // Render dividers.
        self.build_divider_instances(divider_rects, &mut rect_instances);

        // Render focused pane border indicator.
        if let Some(vp) = focused_pane_viewport {
            self.build_focus_indicator(vp, &mut rect_instances);
        }

        self.submit_frame(
            device,
            queue,
            view,
            sw,
            sh,
            &rect_instances,
            &text_instances,
        );
    }

    /// Builds cell instances for a pane at a specific viewport offset.
    ///
    /// This is the viewport-aware version of `build_cell_instances`.
    #[allow(clippy::too_many_arguments)]
    pub fn build_pane_instances(
        &mut self,
        viewport: Viewport,
        grid: &Grid,
        cursor: &Cursor,
        ghost_text: Option<&GhostText>,
        selection: Option<&Selection>,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) {
        let atlas_w = self.glyph_atlas.size().0 as f32;
        let atlas_h = self.glyph_atlas.size().1 as f32;
        let font_size_px = self.font_size as u32;
        let cell_width = self.cell_width;
        let cell_height = self.cell_height;
        let baseline_y = self.baseline_y;
        let pane_padding = self.padding;

        for row_idx in 0..grid.rows() {
            let Some(row) = grid.row(row_idx) else {
                continue;
            };
            for col_idx in 0..row.len() {
                let Some(cell) = row.get(col_idx) else {
                    continue;
                };

                let x = viewport.x + col_idx as f32 * cell_width + pane_padding;
                let y = viewport.y + row_idx as f32 * cell_height + pane_padding;

                let (fg, bg) = resolve_cell_colors(cell, &self.palette);

                if bg != self.palette.bg {
                    rect_instances.push(RectInstance {
                        pos: [x, y],
                        size: [cell_width, cell_height],
                        color: bg,
                    });
                }

                if cell.c == ' ' || cell.c == '\0' {
                    continue;
                }

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

        // Ghost text.
        if let Some(gt) = ghost_text {
            self.build_ghost_text_at_viewport(gt, grid, viewport, text_instances);
        }

        // Selection.
        if let Some(sel) = selection {
            self.build_selection_at_viewport(sel, grid, viewport, rect_instances);
        }

        // Cursor.
        self.build_cursor_at_viewport(cursor, viewport, rect_instances);
    }

    fn build_ghost_text_at_viewport(
        &mut self,
        ghost_text: &GhostText,
        grid: &Grid,
        viewport: Viewport,
        text_instances: &mut Vec<TextInstance>,
    ) {
        let ghost_color: [f32; 4] = [0.6, 0.6, 0.6, 0.5];
        let atlas_w = self.glyph_atlas.size().0 as f32;
        let atlas_h = self.glyph_atlas.size().1 as f32;
        let font_size_px = self.font_size as u32;
        let max_col = grid.cols();

        for (i, c) in ghost_text.text.chars().enumerate() {
            let col = ghost_text.col + i;
            if col >= max_col {
                break;
            }
            if c == ' ' || c == '\0' {
                continue;
            }

            let x = viewport.x + col as f32 * self.cell_width + self.padding;
            let y = viewport.y + ghost_text.row as f32 * self.cell_height + self.padding;

            let glyph_key = match self.char_glyph_cache.get(&c) {
                Some(cached) => *cached,
                None => {
                    let key = resolve_glyph_key(
                        &mut self.font_system,
                        c,
                        self.font_size,
                        font_size_px,
                        &self.font_family,
                    );
                    self.char_glyph_cache.insert(c, key);
                    key
                }
            };

            if let Some(glyph_key) = glyph_key {
                let atlas = &mut self.glyph_atlas;
                let font_system = &mut self.font_system;
                let swash_cache = &mut self.swash_cache;

                if let Some(entry) = atlas.get_or_insert(glyph_key, font_system, swash_cache) {
                    self.atlas_dirty = true;
                    text_instances.push(TextInstance {
                        pos: [
                            x + entry.left as f32,
                            y + self.baseline_y - entry.top as f32,
                        ],
                        size: [entry.width as f32, entry.height as f32],
                        uv_pos: [entry.x as f32 / atlas_w, entry.y as f32 / atlas_h],
                        uv_size: [entry.width as f32 / atlas_w, entry.height as f32 / atlas_h],
                        fg_color: ghost_color,
                    });
                }
            }
        }
    }

    fn build_selection_at_viewport(
        &self,
        selection: &Selection,
        grid: &Grid,
        viewport: Viewport,
        rect_instances: &mut Vec<RectInstance>,
    ) {
        use minal_core::selection::SelectionType;

        let (start, end) = selection.bounds();
        let color = self.palette.selection;
        let grid_cols = grid.cols();

        for row_idx in start.row..=end.row {
            if row_idx < 0 || row_idx as usize >= grid.rows() {
                continue;
            }
            let row_usize = row_idx as usize;

            let (col_start, col_end) = match selection.ty {
                SelectionType::Lines => (0, grid_cols),
                SelectionType::Block => {
                    let min_col = start.col.min(end.col);
                    let max_col = (start.col.max(end.col) + 1).min(grid_cols);
                    (min_col, max_col)
                }
                SelectionType::Simple => {
                    let cs = if row_idx == start.row { start.col } else { 0 };
                    let ce = if row_idx == end.row {
                        (end.col + 1).min(grid_cols)
                    } else {
                        grid_cols
                    };
                    (cs, ce)
                }
            };

            if col_start >= col_end {
                continue;
            }

            let x = viewport.x + col_start as f32 * self.cell_width + self.padding;
            let y = viewport.y + row_usize as f32 * self.cell_height + self.padding;
            let width = (col_end - col_start) as f32 * self.cell_width;

            rect_instances.push(RectInstance {
                pos: [x, y],
                size: [width, self.cell_height],
                color,
            });
        }
    }

    fn build_cursor_at_viewport(
        &self,
        cursor: &Cursor,
        viewport: Viewport,
        rect_instances: &mut Vec<RectInstance>,
    ) {
        if !cursor.visible {
            return;
        }

        let x = viewport.x + cursor.col as f32 * self.cell_width + self.padding;
        let y = viewport.y + cursor.row as f32 * self.cell_height + self.padding;

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

    /// Builds instances for the tab bar.
    fn build_tab_bar_instances(
        &mut self,
        tabs: &[TabBarInfo],
        screen_width: f32,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) {
        if tabs.is_empty() {
            return;
        }

        // Tab bar background.
        let tab_bar_bg = [
            self.palette.bg[0] * 0.7,
            self.palette.bg[1] * 0.7,
            self.palette.bg[2] * 0.7,
            1.0,
        ];
        rect_instances.push(RectInstance {
            pos: [0.0, 0.0],
            size: [screen_width, TAB_BAR_HEIGHT],
            color: tab_bar_bg,
        });

        // Individual tab rectangles.
        let tab_width = (screen_width / tabs.len() as f32).min(200.0);
        let atlas_w = self.glyph_atlas.size().0 as f32;
        let atlas_h = self.glyph_atlas.size().1 as f32;
        let font_size_px = self.font_size as u32;

        for (i, tab) in tabs.iter().enumerate() {
            let tab_x = i as f32 * tab_width;

            // Active tab is slightly brighter.
            let tab_color = if tab.is_active {
                self.palette.bg
            } else {
                tab_bar_bg
            };

            rect_instances.push(RectInstance {
                pos: [tab_x, 0.0],
                size: [tab_width - 1.0, TAB_BAR_HEIGHT],
                color: tab_color,
            });

            // Tab title text.
            let text_x = tab_x + 8.0;
            let text_y = 4.0;
            let text_color = if tab.is_active {
                self.palette.fg
            } else {
                [
                    self.palette.fg[0] * 0.6,
                    self.palette.fg[1] * 0.6,
                    self.palette.fg[2] * 0.6,
                    1.0,
                ]
            };

            // Render title characters.
            let max_chars = ((tab_width - 16.0) / self.cell_width) as usize;
            let title: String = tab.title.chars().take(max_chars).collect();
            for (ci, c) in title.chars().enumerate() {
                if c == ' ' || c == '\0' {
                    continue;
                }
                let cx = text_x + ci as f32 * self.cell_width;

                let glyph_key = match self.char_glyph_cache.get(&c) {
                    Some(cached) => *cached,
                    None => {
                        let key = resolve_glyph_key(
                            &mut self.font_system,
                            c,
                            self.font_size,
                            font_size_px,
                            &self.font_family,
                        );
                        self.char_glyph_cache.insert(c, key);
                        key
                    }
                };

                if let Some(glyph_key) = glyph_key {
                    let atlas = &mut self.glyph_atlas;
                    let font_system = &mut self.font_system;
                    let swash_cache = &mut self.swash_cache;

                    if let Some(entry) = atlas.get_or_insert(glyph_key, font_system, swash_cache) {
                        self.atlas_dirty = true;
                        text_instances.push(TextInstance {
                            pos: [
                                cx + entry.left as f32,
                                text_y + self.baseline_y - entry.top as f32,
                            ],
                            size: [entry.width as f32, entry.height as f32],
                            uv_pos: [entry.x as f32 / atlas_w, entry.y as f32 / atlas_h],
                            uv_size: [entry.width as f32 / atlas_w, entry.height as f32 / atlas_h],
                            fg_color: text_color,
                        });
                    }
                }
            }
        }
    }

    /// Builds divider rectangles between panes.
    fn build_divider_instances(
        &self,
        dividers: &[(f32, f32, f32, f32)],
        rect_instances: &mut Vec<RectInstance>,
    ) {
        let divider_color = [
            self.palette.fg[0] * 0.3,
            self.palette.fg[1] * 0.3,
            self.palette.fg[2] * 0.3,
            1.0,
        ];
        for &(x, y, w, h) in dividers {
            rect_instances.push(RectInstance {
                pos: [x, y],
                size: [w, h],
                color: divider_color,
            });
        }
    }

    /// Builds a subtle focus indicator border around the focused pane.
    fn build_focus_indicator(&self, viewport: Viewport, rect_instances: &mut Vec<RectInstance>) {
        let color = [
            self.palette.cursor[0] * 0.5,
            self.palette.cursor[1] * 0.5,
            self.palette.cursor[2] * 0.5,
            0.6,
        ];
        let thickness = 1.0;

        // Top edge.
        rect_instances.push(RectInstance {
            pos: [viewport.x, viewport.y],
            size: [viewport.width, thickness],
            color,
        });
        // Bottom edge.
        rect_instances.push(RectInstance {
            pos: [viewport.x, viewport.y + viewport.height - thickness],
            size: [viewport.width, thickness],
            color,
        });
        // Left edge.
        rect_instances.push(RectInstance {
            pos: [viewport.x, viewport.y],
            size: [thickness, viewport.height],
            color,
        });
        // Right edge.
        rect_instances.push(RectInstance {
            pos: [viewport.x + viewport.width - thickness, viewport.y],
            size: [thickness, viewport.height],
            color,
        });
    }

    /// Submits a frame with the given instances to the GPU.
    #[allow(clippy::too_many_arguments)]
    fn submit_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        sw: f32,
        sh: f32,
        rect_instances: &[RectInstance],
        text_instances: &[TextInstance],
    ) {
        // Upload atlas if glyphs were added.
        self.glyph_atlas.upload(queue);

        // Rebind atlas texture if dirty.
        if self.atlas_dirty {
            self.text_pipeline
                .bind_atlas(device, &self.glyph_atlas, &self.atlas_sampler);
            self.atlas_dirty = false;
        }

        // Prepare pipelines.
        self.rect_pipeline
            .prepare(device, queue, sw, sh, rect_instances);
        self.text_pipeline
            .prepare(device, queue, sw, sh, text_instances);

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

    /// Builds text instances for ghost text (AI completion suggestion).
    fn build_ghost_text_instances(
        &mut self,
        ghost_text: &GhostText,
        grid: &Grid,
        text_instances: &mut Vec<TextInstance>,
    ) {
        let ghost_color: [f32; 4] = [0.6, 0.6, 0.6, 0.5];
        let atlas_w = self.glyph_atlas.size().0 as f32;
        let atlas_h = self.glyph_atlas.size().1 as f32;
        let font_size_px = self.font_size as u32;
        let cell_width = self.cell_width;
        let cell_height = self.cell_height;
        let baseline_y = self.baseline_y;
        let padding = self.padding;
        let max_col = grid.cols();

        for (i, c) in ghost_text.text.chars().enumerate() {
            let col = ghost_text.col + i;
            if col >= max_col {
                break;
            }

            if c == ' ' || c == '\0' {
                continue;
            }

            let x = col as f32 * cell_width + padding;
            let y = ghost_text.row as f32 * cell_height + padding;

            let glyph_key = match self.char_glyph_cache.get(&c) {
                Some(cached) => *cached,
                None => {
                    let key = resolve_glyph_key(
                        &mut self.font_system,
                        c,
                        self.font_size,
                        font_size_px,
                        &self.font_family,
                    );
                    self.char_glyph_cache.insert(c, key);
                    key
                }
            };

            if let Some(glyph_key) = glyph_key {
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
                        fg_color: ghost_color,
                    });
                }
            }
        }
    }

    /// Builds semi-transparent rectangle instances for the selection highlight.
    fn build_selection_instances(
        &self,
        selection: &Selection,
        grid: &Grid,
        rect_instances: &mut Vec<RectInstance>,
    ) {
        use minal_core::selection::SelectionType;

        let (start, end) = selection.bounds();
        let color = self.palette.selection;
        let cell_width = self.cell_width;
        let cell_height = self.cell_height;
        let padding = self.padding;
        let grid_cols = grid.cols();

        for row_idx in start.row..=end.row {
            if row_idx < 0 || row_idx as usize >= grid.rows() {
                continue;
            }
            let row_usize = row_idx as usize;

            let (col_start, col_end) = match selection.ty {
                SelectionType::Lines => (0, grid_cols),
                SelectionType::Block => {
                    let min_col = start.col.min(end.col);
                    let max_col = (start.col.max(end.col) + 1).min(grid_cols);
                    (min_col, max_col)
                }
                SelectionType::Simple => {
                    let cs = if row_idx == start.row { start.col } else { 0 };
                    let ce = if row_idx == end.row {
                        (end.col + 1).min(grid_cols)
                    } else {
                        grid_cols
                    };
                    (cs, ce)
                }
            };

            if col_start >= col_end {
                continue;
            }

            let x = col_start as f32 * cell_width + padding;
            let y = row_usize as f32 * cell_height + padding;
            let width = (col_end - col_start) as f32 * cell_width;

            rect_instances.push(RectInstance {
                pos: [x, y],
                size: [width, cell_height],
                color,
            });
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
pub(crate) fn resolve_glyph_key(
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
fn resolve_cell_colors(cell: &Cell, palette: &ColorPalette) -> ([f32; 4], [f32; 4]) {
    let mut fg = resolve_color(&cell.fg, palette.fg, palette);
    let mut bg = resolve_color(&cell.bg, palette.bg, palette);

    // Bold-as-bright: map standard named colors (0-7) to their bright
    // variants when the bold attribute is set.
    if cell.attrs.bold {
        if let Color::Named(named) = &cell.fg {
            if let Some(bright) = named.to_bright() {
                fg = palette.named_color(bright);
            }
        }
    }

    // Dim: reduce foreground intensity by ~34%.
    // Applied before inverse so that dim+inverse dims the text, not the background.
    if cell.attrs.dim {
        fg[0] *= 0.66;
        fg[1] *= 0.66;
        fg[2] *= 0.66;
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

/// Converts a 256-color index to RGBA.
fn indexed_color(idx: u8, palette: &ColorPalette) -> [f32; 4] {
    match idx {
        0..=15 => palette.named[idx as usize],
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
    fn resolve_cell_colors_dim() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.fg = Color::Rgb(255, 255, 255);
        cell.attrs = CellAttributes {
            dim: true,
            ..CellAttributes::default()
        };
        let (fg, _bg) = resolve_cell_colors(&cell, &palette);
        // Dim reduces RGB channels by ~34%
        let expected = 1.0 * 0.66;
        assert!((fg[0] - expected).abs() < 0.01);
        assert!((fg[1] - expected).abs() < 0.01);
        assert!((fg[2] - expected).abs() < 0.01);
    }

    #[test]
    fn resolve_cell_colors_bold_as_bright() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::Red);
        cell.attrs = CellAttributes {
            bold: true,
            ..CellAttributes::default()
        };
        let (fg, _bg) = resolve_cell_colors(&cell, &palette);
        // Bold + Named red should resolve to bright red
        assert_eq!(fg, palette.named_color(NamedColor::BrightRed));
    }

    #[test]
    fn resolve_cell_colors_bold_already_bright_unchanged() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::BrightRed);
        cell.attrs = CellAttributes {
            bold: true,
            ..CellAttributes::default()
        };
        let (fg, _bg) = resolve_cell_colors(&cell, &palette);
        // Already bright, should stay bright red
        assert_eq!(fg, palette.named_color(NamedColor::BrightRed));
    }

    #[test]
    fn resolve_cell_colors_bold_rgb_no_bright() {
        let palette = ColorPalette::default_palette();
        let mut cell = Cell::default();
        cell.fg = Color::Rgb(128, 0, 0);
        cell.attrs = CellAttributes {
            bold: true,
            ..CellAttributes::default()
        };
        let (fg, _bg) = resolve_cell_colors(&cell, &palette);
        // RGB colors should not be affected by bold-as-bright
        assert!((fg[0] - 128.0 / 255.0).abs() < 0.01);
        assert!(fg[1].abs() < 0.01);
    }

    #[test]
    fn update_theme_changes_palette() {
        let palette1 = ColorPalette::from_theme(&minal_config::ThemeConfig::default());
        let dracula = minal_config::builtin_theme(minal_config::ThemePreset::Dracula);
        let palette2 = ColorPalette::from_theme(&dracula);
        // Backgrounds should differ between Catppuccin and Dracula
        assert_ne!(palette1.bg, palette2.bg);
    }

    #[test]
    fn indexed_color_full_256_range() {
        let palette = ColorPalette::default_palette();
        // Verify all 256 indices produce valid RGBA values
        for i in 0..=255u8 {
            let c = indexed_color(i, &palette);
            for channel in &c[..3] {
                assert!(
                    (0.0..=1.0).contains(channel),
                    "index {i} has out-of-range channel value {channel}"
                );
            }
            assert!((c[3] - 1.0).abs() < f32::EPSILON, "alpha should be 1.0");
        }
    }
}
