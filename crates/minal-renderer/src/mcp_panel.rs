//! MCP tools panel overlay rendering.
//!
//! Builds rect and text instances for the MCP (Model Context Protocol) tools
//! panel that overlays the terminal window, listing available tools grouped
//! by server.

use crate::Viewport;
use crate::rect::RectInstance;
use crate::renderer::Renderer;
use crate::text::TextInstance;

// -- Layout constants --

/// Height of the header bar in pixels.
const HEADER_HEIGHT: f32 = 24.0;

/// Horizontal padding inside the panel in pixels.
const PADDING_X: f32 = 8.0;

/// Vertical gap between tool entries in pixels.
const ENTRY_GAP: f32 = 4.0;

/// Internal padding within each tool entry in pixels.
const ENTRY_PADDING: f32 = 4.0;

// -- Colors --

/// Panel background color.
const COLOR_PANEL_BG: [f32; 4] = [0.08, 0.10, 0.16, 0.95];

/// Header bar background color.
const COLOR_HEADER_BG: [f32; 4] = [0.12, 0.18, 0.28, 1.0];

/// Header title text color.
const COLOR_HEADER_TEXT: [f32; 4] = [0.5, 0.8, 1.0, 1.0];

/// Close button text color.
const COLOR_CLOSE_TEXT: [f32; 4] = [0.6, 0.6, 0.65, 1.0];

/// Server group header background color.
const COLOR_SERVER_HEADER_BG: [f32; 4] = [0.14, 0.20, 0.32, 1.0];

/// Server name text color.
const COLOR_SERVER_TEXT: [f32; 4] = [0.6, 0.9, 1.0, 1.0];

/// Tool entry background color.
const COLOR_TOOL_BG: [f32; 4] = [0.10, 0.13, 0.20, 1.0];

/// Tool name text color.
const COLOR_TOOL_NAME_TEXT: [f32; 4] = [0.9, 0.9, 1.0, 1.0];

/// Tool description text color.
const COLOR_TOOL_DESC_TEXT: [f32; 4] = [0.6, 0.65, 0.75, 1.0];

/// An entry in the MCP tool list panel.
#[derive(Debug, Clone)]
pub struct McpPanelEntry {
    /// Name of the MCP server providing this tool.
    pub server_name: String,
    /// Name of the tool.
    pub tool_name: String,
    /// Human-readable tool description.
    pub description: String,
}

/// Clickable regions in the MCP panel.
#[derive(Debug, Clone)]
pub enum McpPanelHitRegion {
    /// Close button in the panel header.
    CloseButton {
        /// Bounding rectangle.
        rect: Viewport,
    },
}

impl Renderer {
    /// Builds rect and text instances for the MCP tools panel overlay.
    ///
    /// The panel is drawn within `viewport` and shows a header bar followed
    /// by a scrollable list of available MCP tools grouped by server.
    ///
    /// Returns a list of [`McpPanelHitRegion`]s for click handling.
    pub fn build_mcp_panel_instances(
        &mut self,
        viewport: Viewport,
        entries: &[McpPanelEntry],
        scroll_offset: f32,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) -> Vec<McpPanelHitRegion> {
        let mut hit_regions = Vec::new();

        // --- Panel background ---
        rect_instances.push(RectInstance {
            pos: [viewport.x, viewport.y],
            size: [viewport.width, viewport.height],
            color: COLOR_PANEL_BG,
        });

        // --- Header bar ---
        let header_y = viewport.y;
        rect_instances.push(RectInstance {
            pos: [viewport.x, header_y],
            size: [viewport.width, HEADER_HEIGHT],
            color: COLOR_HEADER_BG,
        });

        // Header title.
        let title = format!("MCP Tools ({})", entries.len());
        let title_x = viewport.x + PADDING_X;
        let title_y = header_y + (HEADER_HEIGHT - self.cell_height) / 2.0;
        self.render_text_run(&title, title_x, title_y, COLOR_HEADER_TEXT, text_instances);

        // Close button "[X]" at right side.
        let close_label = "[X]";
        let close_btn_width = close_label.len() as f32 * self.cell_width;
        let close_x = viewport.x + viewport.width - close_btn_width - PADDING_X;
        let close_y = title_y;
        self.render_text_run(
            close_label,
            close_x,
            close_y,
            COLOR_CLOSE_TEXT,
            text_instances,
        );
        hit_regions.push(McpPanelHitRegion::CloseButton {
            rect: Viewport {
                x: close_x,
                y: header_y,
                width: close_btn_width,
                height: HEADER_HEIGHT,
            },
        });

        // --- Entries area ---
        let entries_top = viewport.y + HEADER_HEIGHT;
        let entries_bottom = viewport.y + viewport.height;
        let content_width = viewport.width - PADDING_X * 2.0;

        let mut cursor_y = entries_top + ENTRY_GAP + scroll_offset;
        let entry_x = viewport.x + PADDING_X;

        // Group entries by server and render server headers.
        let mut current_server = String::new();

        for entry in entries {
            // Server group header when server changes.
            if entry.server_name != current_server {
                current_server = entry.server_name.clone();

                let server_h = self.cell_height + 4.0;
                if cursor_y + server_h > entries_top && cursor_y < entries_bottom {
                    rect_instances.push(RectInstance {
                        pos: [entry_x, cursor_y],
                        size: [content_width, server_h],
                        color: COLOR_SERVER_HEADER_BG,
                    });
                    let server_label = format!("  {}", current_server);
                    let text_y = cursor_y + (server_h - self.cell_height) / 2.0;
                    self.render_text_run(
                        &server_label,
                        entry_x + 2.0,
                        text_y,
                        COLOR_SERVER_TEXT,
                        text_instances,
                    );
                }
                cursor_y += server_h + ENTRY_GAP;
            }

            // Tool entry.
            let line_height = self.cell_height;
            let has_desc = !entry.description.is_empty();

            // Compute entry height.
            let desc_lines = if has_desc {
                let desc_width = content_width - ENTRY_PADDING * 2.0 - self.cell_width * 2.0;
                self.count_mcp_wrapped_lines(&entry.description, desc_width)
            } else {
                0
            };

            let entry_h = ENTRY_PADDING
                + line_height // tool name
                + if has_desc {
                    line_height * 0.3 + desc_lines as f32 * line_height
                } else {
                    0.0
                }
                + ENTRY_PADDING;

            if cursor_y + entry_h > entries_top && cursor_y < entries_bottom {
                rect_instances.push(RectInstance {
                    pos: [entry_x, cursor_y],
                    size: [content_width, entry_h],
                    color: COLOR_TOOL_BG,
                });
            }

            let inner_x = entry_x + ENTRY_PADDING * 2.0;
            let inner_width = content_width - ENTRY_PADDING * 4.0;
            let mut inner_y = cursor_y + ENTRY_PADDING;

            // Tool name.
            if inner_y >= entries_top && inner_y + line_height <= entries_bottom {
                self.render_text_run(
                    &entry.tool_name,
                    inner_x,
                    inner_y,
                    COLOR_TOOL_NAME_TEXT,
                    text_instances,
                );
            }
            inner_y += line_height;

            // Description (if any).
            if has_desc {
                inner_y += line_height * 0.3;
                inner_y = self.render_mcp_wrapped_text(
                    &entry.description,
                    inner_x,
                    inner_y,
                    inner_width,
                    entries_top,
                    entries_bottom,
                    COLOR_TOOL_DESC_TEXT,
                    text_instances,
                );
            }

            cursor_y = inner_y + ENTRY_PADDING + ENTRY_GAP;
        }

        hit_regions
    }

    /// Counts how many wrapped lines a text string will occupy at the given width.
    fn count_mcp_wrapped_lines(&self, text: &str, max_width: f32) -> usize {
        if text.is_empty() {
            return 1;
        }
        let chars_per_line = if self.cell_width > 0.0 {
            (max_width / self.cell_width).floor() as usize
        } else {
            1
        };
        if chars_per_line == 0 {
            return 1;
        }
        let mut lines = 1usize;
        let mut col = 0usize;
        for c in text.chars() {
            if c == '\n' {
                lines += 1;
                col = 0;
                continue;
            }
            col += 1;
            if col > chars_per_line {
                lines += 1;
                col = 1;
            }
        }
        lines
    }

    /// Renders text with character-level wrapping, returning the new y position.
    #[allow(clippy::too_many_arguments)]
    fn render_mcp_wrapped_text(
        &mut self,
        text: &str,
        x: f32,
        start_y: f32,
        max_width: f32,
        clip_top: f32,
        clip_bottom: f32,
        color: [f32; 4],
        text_instances: &mut Vec<TextInstance>,
    ) -> f32 {
        let line_height = self.cell_height;
        let mut cur_x = x;
        let mut cur_y = start_y;

        for c in text.chars() {
            if c == '\n' {
                cur_x = x;
                cur_y += line_height;
                continue;
            }
            if cur_x + self.cell_width > x + max_width {
                cur_x = x;
                cur_y += line_height;
            }
            if cur_y >= clip_top && cur_y + line_height <= clip_bottom {
                self.render_char_at(c, cur_x, cur_y, color, text_instances);
            }
            cur_x += self.cell_width;
        }

        cur_y + line_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_panel_entry_clone() {
        let entry = McpPanelEntry {
            server_name: "filesystem".to_string(),
            tool_name: "read_file".to_string(),
            description: "Read the contents of a file".to_string(),
        };
        let cloned = entry.clone();
        assert_eq!(cloned.server_name, "filesystem");
        assert_eq!(cloned.tool_name, "read_file");
    }

    #[test]
    fn mcp_panel_hit_region_debug() {
        let region = McpPanelHitRegion::CloseButton {
            rect: Viewport {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 24.0,
            },
        };
        let s = format!("{:?}", region);
        assert!(s.contains("CloseButton"));
    }
}
