//! Error panel overlay rendering.
//!
//! Builds rect and text instances for the error analysis panel that overlays
//! the terminal window, showing detected errors with AI explanations and
//! suggested fix commands.

use crate::Viewport;
use crate::rect::RectInstance;
use crate::renderer::Renderer;
use crate::text::TextInstance;

// -- Layout constants --

/// Height of the header bar in pixels.
const HEADER_HEIGHT: f32 = 24.0;

/// Horizontal padding inside the panel in pixels.
const PADDING_X: f32 = 8.0;

/// Vertical gap between error entries in pixels.
const ENTRY_GAP: f32 = 8.0;

/// Internal padding within each error entry in pixels.
const ENTRY_PADDING: f32 = 6.0;

// -- Colors --

/// Panel background color.
const COLOR_PANEL_BG: [f32; 4] = [0.1, 0.1, 0.12, 0.95];

/// Header bar background color.
const COLOR_HEADER_BG: [f32; 4] = [0.15, 0.12, 0.12, 1.0];

/// Header title text color.
const COLOR_HEADER_TEXT: [f32; 4] = [1.0, 0.6, 0.6, 1.0];

/// Error entry background color.
const COLOR_ENTRY_BG: [f32; 4] = [0.12, 0.12, 0.15, 1.0];

/// Command text color.
const COLOR_COMMAND_TEXT: [f32; 4] = [0.7, 0.8, 1.0, 1.0];

/// Summary text color.
const COLOR_SUMMARY_TEXT: [f32; 4] = [0.85, 0.85, 0.88, 1.0];

/// AI explanation text color.
const COLOR_EXPLANATION_TEXT: [f32; 4] = [0.7, 0.9, 0.7, 1.0];

/// Suggestion button background color.
const COLOR_SUGGESTION_BTN_BG: [f32; 4] = [0.2, 0.4, 0.5, 1.0];

/// Suggestion button text color.
const COLOR_SUGGESTION_BTN_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Close button text color.
const COLOR_CLOSE_TEXT: [f32; 4] = [0.6, 0.6, 0.65, 1.0];

/// Dismiss button text color.
const COLOR_DISMISS_TEXT: [f32; 4] = [0.5, 0.5, 0.55, 1.0];

/// Error count badge background color.
const COLOR_BADGE_BG: [f32; 4] = [0.85, 0.2, 0.2, 0.95];

/// Error count badge text color.
const COLOR_BADGE_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

// -- Category badge colors --

/// Build error category color.
const COLOR_CAT_BUILD: [f32; 4] = [0.9, 0.3, 0.3, 1.0];

/// Test error category color.
const COLOR_CAT_TEST: [f32; 4] = [0.9, 0.6, 0.2, 1.0];

/// Runtime error category color.
const COLOR_CAT_RUNTIME: [f32; 4] = [0.9, 0.4, 0.4, 1.0];

/// Permission error category color.
const COLOR_CAT_PERMISSION: [f32; 4] = [0.9, 0.8, 0.2, 1.0];

/// Not-found error category color.
const COLOR_CAT_NOT_FOUND: [f32; 4] = [0.6, 0.6, 0.9, 1.0];

/// Network error category color.
const COLOR_CAT_NETWORK: [f32; 4] = [0.4, 0.7, 0.9, 1.0];

/// Unknown error category color.
const COLOR_CAT_UNKNOWN: [f32; 4] = [0.6, 0.6, 0.6, 1.0];

/// A clickable region in the error panel.
#[derive(Debug, Clone)]
pub enum ErrorPanelHitRegion {
    /// Execute a fix command.
    ExecuteFixCommand {
        /// Index of the error this suggestion belongs to.
        error_index: usize,
        /// The command to execute.
        command: String,
        /// Bounding rectangle.
        rect: Viewport,
    },
    /// Close the panel.
    CloseButton {
        /// Bounding rectangle.
        rect: Viewport,
    },
    /// Dismiss a specific error.
    DismissError {
        /// Index of the error to dismiss.
        index: usize,
        /// Bounding rectangle.
        rect: Viewport,
    },
}

/// An error entry to render in the panel.
#[derive(Debug, Clone)]
pub struct ErrorPanelEntry {
    /// Error category label (e.g. "Build", "Runtime").
    pub category: String,
    /// The command that failed.
    pub command: String,
    /// Brief summary of the error.
    pub summary: String,
    /// AI analysis explanation, if available.
    pub explanation: Option<String>,
    /// AI-suggested fix commands, if available.
    pub suggestions: Vec<String>,
}

/// Returns the badge color for an error category string.
fn category_color(category: &str) -> [f32; 4] {
    match category.to_lowercase().as_str() {
        "build" => COLOR_CAT_BUILD,
        "test" => COLOR_CAT_TEST,
        "runtime" => COLOR_CAT_RUNTIME,
        "permission" => COLOR_CAT_PERMISSION,
        "not found" | "notfound" | "not_found" => COLOR_CAT_NOT_FOUND,
        "network" => COLOR_CAT_NETWORK,
        _ => COLOR_CAT_UNKNOWN,
    }
}

impl Renderer {
    /// Builds rect and text instances for the error panel overlay.
    ///
    /// The panel is drawn within `viewport` and contains a header bar with
    /// an error count, followed by scrollable error entries. Each entry shows
    /// a category badge, the failed command, a summary, an optional AI
    /// explanation, and "Run" buttons for suggested fix commands.
    ///
    /// Returns a list of [`ErrorPanelHitRegion`]s for click handling.
    pub fn build_error_panel_instances(
        &mut self,
        viewport: Viewport,
        entries: &[ErrorPanelEntry],
        scroll_offset: f32,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) -> Vec<ErrorPanelHitRegion> {
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

        // Header title text: "Errors (N)"
        let title = format!("Errors ({})", entries.len());
        let title_x = viewport.x + PADDING_X;
        let title_y = header_y + (HEADER_HEIGHT - self.cell_height) / 2.0;
        self.render_text_run(&title, title_x, title_y, COLOR_HEADER_TEXT, text_instances);

        // Close button "[X]" at the right side of the header.
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

        hit_regions.push(ErrorPanelHitRegion::CloseButton {
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

        for (entry_idx, entry) in entries.iter().enumerate() {
            let entry_start_y = cursor_y;

            // Compute entry height first for background rect.
            // We render content and track height, then draw background underneath.
            let line_height = self.cell_height;

            // Count lines for this entry to compute background height.
            let mut entry_height = ENTRY_PADDING; // top padding

            // Category badge line.
            entry_height += line_height;
            // Command line.
            entry_height += line_height;
            // Summary line (may wrap).
            let summary_lines =
                self.count_wrapped_lines(&entry.summary, content_width - ENTRY_PADDING * 2.0);
            entry_height += summary_lines as f32 * line_height;
            // Explanation lines (may wrap).
            if let Some(ref explanation) = entry.explanation {
                entry_height += line_height * 0.5; // small gap before explanation
                let explanation_lines =
                    self.count_wrapped_lines(explanation, content_width - ENTRY_PADDING * 2.0);
                entry_height += explanation_lines as f32 * line_height;
            }
            // Suggestion buttons.
            if !entry.suggestions.is_empty() {
                entry_height += line_height * 0.5; // gap
                entry_height += line_height + 2.0; // button row
            }
            entry_height += ENTRY_PADDING; // bottom padding

            // Entry background.
            if cursor_y + entry_height > entries_top && cursor_y < entries_bottom {
                rect_instances.push(RectInstance {
                    pos: [entry_x, cursor_y],
                    size: [content_width, entry_height],
                    color: COLOR_ENTRY_BG,
                });
            }

            let inner_x = entry_x + ENTRY_PADDING;
            let inner_width = content_width - ENTRY_PADDING * 2.0;
            cursor_y += ENTRY_PADDING;

            // --- Category badge ---
            let badge_color = category_color(&entry.category);
            let badge_text = format!(" {} ", entry.category);
            let badge_width = badge_text.len() as f32 * self.cell_width + 4.0;
            let badge_height = line_height;

            if cursor_y >= entries_top && cursor_y + badge_height <= entries_bottom {
                rect_instances.push(RectInstance {
                    pos: [inner_x, cursor_y],
                    size: [badge_width, badge_height],
                    color: badge_color,
                });
                self.render_text_run(
                    &badge_text,
                    inner_x + 2.0,
                    cursor_y,
                    COLOR_BADGE_TEXT,
                    text_instances,
                );

                // Dismiss button "[x]" at the right side of the entry.
                let dismiss_label = "[x]";
                let dismiss_width = dismiss_label.len() as f32 * self.cell_width;
                let dismiss_x = entry_x + content_width - ENTRY_PADDING - dismiss_width;
                self.render_text_run(
                    dismiss_label,
                    dismiss_x,
                    cursor_y,
                    COLOR_DISMISS_TEXT,
                    text_instances,
                );

                hit_regions.push(ErrorPanelHitRegion::DismissError {
                    index: entry_idx,
                    rect: Viewport {
                        x: dismiss_x,
                        y: cursor_y,
                        width: dismiss_width,
                        height: badge_height,
                    },
                });
            }
            cursor_y += line_height;

            // --- Command ---
            let cmd_prefix = "$ ";
            let cmd_display = format!("{cmd_prefix}{}", entry.command);
            cursor_y = self.render_wrapped_text(
                &cmd_display,
                inner_x,
                cursor_y,
                inner_width,
                entries_top,
                entries_bottom,
                COLOR_COMMAND_TEXT,
                text_instances,
            );

            // --- Summary ---
            cursor_y = self.render_wrapped_text(
                &entry.summary,
                inner_x,
                cursor_y,
                inner_width,
                entries_top,
                entries_bottom,
                COLOR_SUMMARY_TEXT,
                text_instances,
            );

            // --- Explanation (if available) ---
            if let Some(ref explanation) = entry.explanation {
                cursor_y += self.cell_height * 0.5; // small gap
                let explanation_label = format!("AI: {explanation}");
                cursor_y = self.render_wrapped_text(
                    &explanation_label,
                    inner_x,
                    cursor_y,
                    inner_width,
                    entries_top,
                    entries_bottom,
                    COLOR_EXPLANATION_TEXT,
                    text_instances,
                );
            }

            // --- Suggestion buttons ---
            if !entry.suggestions.is_empty() {
                cursor_y += self.cell_height * 0.5; // gap
                let mut btn_x = inner_x;

                for suggestion in &entry.suggestions {
                    let btn_label = format!(" Run: {} ", suggestion);
                    let btn_width = btn_label.len() as f32 * self.cell_width + 4.0;
                    let btn_height = self.cell_height + 2.0;

                    // Wrap to next line if button doesn't fit.
                    if btn_x + btn_width > inner_x + inner_width && btn_x > inner_x {
                        btn_x = inner_x;
                        cursor_y += btn_height + 2.0;
                    }

                    if cursor_y >= entries_top && cursor_y + btn_height <= entries_bottom {
                        rect_instances.push(RectInstance {
                            pos: [btn_x, cursor_y],
                            size: [btn_width, btn_height],
                            color: COLOR_SUGGESTION_BTN_BG,
                        });
                        self.render_text_run(
                            &btn_label,
                            btn_x + 2.0,
                            cursor_y + 1.0,
                            COLOR_SUGGESTION_BTN_TEXT,
                            text_instances,
                        );
                    }

                    hit_regions.push(ErrorPanelHitRegion::ExecuteFixCommand {
                        error_index: entry_idx,
                        command: suggestion.clone(),
                        rect: Viewport {
                            x: btn_x,
                            y: cursor_y,
                            width: btn_width,
                            height: btn_height,
                        },
                    });

                    btn_x += btn_width + 4.0; // gap between buttons
                }

                cursor_y += self.cell_height + 2.0;
            }

            cursor_y += ENTRY_PADDING; // bottom padding

            // Use the actual rendered height for the entry gap.
            let _actual_height = cursor_y - entry_start_y;
            cursor_y += ENTRY_GAP;
        }

        hit_regions
    }

    /// Builds rect and text instances for a small error count badge.
    ///
    /// Renders a small red rounded-rect badge at the bottom-right of the
    /// terminal area showing the number of errors. Does nothing when
    /// `error_count` is zero.
    pub fn build_error_badge_instances(
        &mut self,
        screen_width: f32,
        screen_height: f32,
        error_count: usize,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) {
        if error_count == 0 {
            return;
        }

        let count_text = format!(" {} ", error_count);
        let badge_width = count_text.len() as f32 * self.cell_width + 4.0;
        let badge_height = self.cell_height + 4.0;

        // Position at bottom-right with some margin.
        let margin = 8.0;
        let badge_x = screen_width - badge_width - margin;
        let badge_y = screen_height - badge_height - margin;

        // Badge background.
        rect_instances.push(RectInstance {
            pos: [badge_x, badge_y],
            size: [badge_width, badge_height],
            color: COLOR_BADGE_BG,
        });

        // Badge text centered vertically.
        let text_y = badge_y + (badge_height - self.cell_height) / 2.0;
        self.render_text_run(
            &count_text,
            badge_x + 2.0,
            text_y,
            COLOR_BADGE_TEXT,
            text_instances,
        );
    }

    /// Renders text with character-level wrapping, returning the new y position.
    ///
    /// Only renders lines that fall within the clip region `[clip_top, clip_bottom]`.
    #[allow(clippy::too_many_arguments)]
    fn render_wrapped_text(
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

    /// Counts how many wrapped lines a text string will occupy at the given width.
    fn count_wrapped_lines(&self, text: &str, max_width: f32) -> usize {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_color_build() {
        assert_eq!(category_color("Build"), COLOR_CAT_BUILD);
        assert_eq!(category_color("build"), COLOR_CAT_BUILD);
        assert_eq!(category_color("BUILD"), COLOR_CAT_BUILD);
    }

    #[test]
    fn category_color_test() {
        assert_eq!(category_color("Test"), COLOR_CAT_TEST);
        assert_eq!(category_color("test"), COLOR_CAT_TEST);
    }

    #[test]
    fn category_color_runtime() {
        assert_eq!(category_color("Runtime"), COLOR_CAT_RUNTIME);
    }

    #[test]
    fn category_color_permission() {
        assert_eq!(category_color("Permission"), COLOR_CAT_PERMISSION);
    }

    #[test]
    fn category_color_not_found() {
        assert_eq!(category_color("Not Found"), COLOR_CAT_NOT_FOUND);
        assert_eq!(category_color("notfound"), COLOR_CAT_NOT_FOUND);
        assert_eq!(category_color("not_found"), COLOR_CAT_NOT_FOUND);
    }

    #[test]
    fn category_color_network() {
        assert_eq!(category_color("Network"), COLOR_CAT_NETWORK);
    }

    #[test]
    fn category_color_unknown_fallback() {
        assert_eq!(category_color("SomethingElse"), COLOR_CAT_UNKNOWN);
        assert_eq!(category_color(""), COLOR_CAT_UNKNOWN);
    }

    #[test]
    fn error_panel_entry_clone() {
        let entry = ErrorPanelEntry {
            category: "Build".to_string(),
            command: "cargo build".to_string(),
            summary: "compilation failed".to_string(),
            explanation: Some("Missing import".to_string()),
            suggestions: vec!["cargo fix".to_string()],
        };
        let cloned = entry.clone();
        assert_eq!(cloned.category, "Build");
        assert_eq!(cloned.command, "cargo build");
        assert_eq!(cloned.summary, "compilation failed");
        assert_eq!(cloned.explanation, Some("Missing import".to_string()));
        assert_eq!(cloned.suggestions, vec!["cargo fix".to_string()]);
    }

    #[test]
    fn error_panel_entry_no_explanation() {
        let entry = ErrorPanelEntry {
            category: "Runtime".to_string(),
            command: "./app".to_string(),
            summary: "segfault".to_string(),
            explanation: None,
            suggestions: Vec::new(),
        };
        assert!(entry.explanation.is_none());
        assert!(entry.suggestions.is_empty());
    }

    #[test]
    fn error_panel_hit_region_debug() {
        let region = ErrorPanelHitRegion::CloseButton {
            rect: Viewport {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        };
        // Ensure Debug is implemented.
        let debug_str = format!("{:?}", region);
        assert!(debug_str.contains("CloseButton"));
    }

    #[test]
    fn error_panel_hit_region_execute_fix() {
        let region = ErrorPanelHitRegion::ExecuteFixCommand {
            error_index: 0,
            command: "cargo fix".to_string(),
            rect: Viewport {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 20.0,
            },
        };
        let debug_str = format!("{:?}", region);
        assert!(debug_str.contains("ExecuteFixCommand"));
        assert!(debug_str.contains("cargo fix"));
    }

    #[test]
    fn error_panel_hit_region_dismiss() {
        let region = ErrorPanelHitRegion::DismissError {
            index: 3,
            rect: Viewport {
                x: 0.0,
                y: 0.0,
                width: 50.0,
                height: 20.0,
            },
        };
        let debug_str = format!("{:?}", region);
        assert!(debug_str.contains("DismissError"));
    }
}
