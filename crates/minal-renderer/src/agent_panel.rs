//! Agent panel overlay rendering.
//!
//! Builds rect and text instances for the autonomous AI agent panel that
//! overlays the bottom portion of the terminal window. Shows the current
//! task plan as a scrollable step list with approval controls.

use crate::Viewport;
use crate::rect::RectInstance;
use crate::renderer::Renderer;
use crate::text::TextInstance;

// -- Layout constants --

/// Height of the header bar in pixels.
const HEADER_HEIGHT: f32 = 24.0;

/// Height of the input area in pixels.
const INPUT_HEIGHT: f32 = 32.0;

/// Height of the approval button row in pixels.
const APPROVAL_HEIGHT: f32 = 32.0;

/// Horizontal padding inside the panel in pixels.
const PADDING_X: f32 = 8.0;

/// Vertical gap between step entries in pixels.
const STEP_GAP: f32 = 6.0;

/// Internal padding within each step entry in pixels.
const STEP_PADDING: f32 = 4.0;

// -- Colors --

/// Panel background color.
const COLOR_PANEL_BG: [f32; 4] = [0.08, 0.10, 0.14, 0.97];

/// Header bar background color.
const COLOR_HEADER_BG: [f32; 4] = [0.12, 0.15, 0.22, 1.0];

/// Header title text color.
const COLOR_HEADER_TEXT: [f32; 4] = [0.6, 0.8, 1.0, 1.0];

/// Header status indicator color (planning).
const COLOR_STATUS_PLANNING: [f32; 4] = [0.9, 0.7, 0.2, 1.0];

/// Header status indicator color (executing).
const COLOR_STATUS_EXECUTING: [f32; 4] = [0.3, 0.8, 0.4, 1.0];

/// Header status indicator color (awaiting approval).
const COLOR_STATUS_APPROVAL: [f32; 4] = [1.0, 0.6, 0.2, 1.0];

/// Header status indicator color (completed).
const COLOR_STATUS_COMPLETED: [f32; 4] = [0.3, 0.9, 0.5, 1.0];

/// Header status indicator color (failed/cancelled).
const COLOR_STATUS_FAILED: [f32; 4] = [1.0, 0.3, 0.3, 1.0];

/// Header status indicator color (idle).
const COLOR_STATUS_IDLE: [f32; 4] = [0.5, 0.5, 0.6, 1.0];

/// Step entry background color.
const COLOR_STEP_BG: [f32; 4] = [0.12, 0.13, 0.18, 1.0];

/// Step number text color.
const COLOR_STEP_NUMBER: [f32; 4] = [0.5, 0.5, 0.6, 1.0];

/// Action type badge background color.
const COLOR_BADGE_BG: [f32; 4] = [0.2, 0.25, 0.4, 1.0];

/// Action type badge text color.
const COLOR_BADGE_TEXT: [f32; 4] = [0.8, 0.9, 1.0, 1.0];

/// Step description text color.
const COLOR_DESC_TEXT: [f32; 4] = [0.85, 0.85, 0.88, 1.0];

/// Dangerous command warning badge background.
const COLOR_DANGER_BG: [f32; 4] = [0.8, 0.15, 0.15, 1.0];

/// Dangerous command warning badge text.
const COLOR_DANGER_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Status icon color for pending steps.
const COLOR_STATUS_PENDING: [f32; 4] = [0.4, 0.4, 0.5, 1.0];

/// Status icon color for running steps.
const COLOR_STATUS_RUNNING: [f32; 4] = [0.3, 0.7, 1.0, 1.0];

/// Status icon color for completed steps.
const COLOR_STATUS_DONE: [f32; 4] = [0.3, 0.9, 0.4, 1.0];

/// Status icon color for failed steps.
const COLOR_STATUS_FAIL: [f32; 4] = [1.0, 0.3, 0.3, 1.0];

/// Status icon color for skipped steps.
const COLOR_STATUS_SKIP: [f32; 4] = [0.6, 0.6, 0.3, 1.0];

/// Result output text color.
const COLOR_RESULT_TEXT: [f32; 4] = [0.6, 0.7, 0.6, 1.0];

/// Approve button background color.
const COLOR_APPROVE_BG: [f32; 4] = [0.15, 0.5, 0.25, 1.0];

/// Approve button text color.
const COLOR_APPROVE_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Skip button background color.
const COLOR_SKIP_BG: [f32; 4] = [0.35, 0.30, 0.12, 1.0];

/// Skip button text color.
const COLOR_SKIP_TEXT: [f32; 4] = [1.0, 0.9, 0.5, 1.0];

/// Cancel button background color.
const COLOR_CANCEL_BG: [f32; 4] = [0.45, 0.12, 0.12, 1.0];

/// Cancel button text color.
const COLOR_CANCEL_TEXT: [f32; 4] = [1.0, 0.6, 0.6, 1.0];

/// Input area background color.
const COLOR_INPUT_BG: [f32; 4] = [0.08, 0.08, 0.12, 1.0];

/// Input placeholder text color.
const COLOR_PLACEHOLDER: [f32; 4] = [0.4, 0.4, 0.45, 1.0];

/// Input cursor color.
const COLOR_INPUT_CURSOR: [f32; 4] = [0.7, 0.85, 1.0, 0.9];

/// User question text color.
const COLOR_QUESTION_TEXT: [f32; 4] = [0.9, 0.8, 0.5, 1.0];

/// User question area background.
const COLOR_QUESTION_BG: [f32; 4] = [0.14, 0.12, 0.06, 0.95];

/// Close button text color.
const COLOR_CLOSE_TEXT: [f32; 4] = [0.6, 0.6, 0.65, 1.0];

/// Input text color.
const COLOR_INPUT_TEXT: [f32; 4] = [0.85, 0.85, 0.88, 1.0];

/// Render data for a single agent step passed from the main thread.
#[derive(Debug, Clone)]
pub struct AgentPanelStep {
    /// Zero-based index of this step.
    pub index: usize,
    /// Human-readable action type label (e.g. "RunCommand", "ReadFile").
    pub action_type: String,
    /// Human-readable description of the action.
    pub description: String,
    /// Step status string (e.g. "Pending", "Running", "Completed").
    pub status: String,
    /// Whether this action was flagged as dangerous.
    pub is_dangerous: bool,
    /// Result output after execution, if available.
    pub result_output: Option<String>,
    /// Danger warning message, if applicable.
    pub danger_warning: Option<String>,
}

/// A clickable region in the agent panel.
#[derive(Debug, Clone)]
pub enum AgentPanelHitRegion {
    /// [Approve] button for the current step.
    ApproveButton {
        /// Bounding rectangle.
        rect: Viewport,
    },
    /// [Skip] button for the current step.
    SkipButton {
        /// Bounding rectangle.
        rect: Viewport,
    },
    /// [Cancel] button to cancel the task.
    CancelButton {
        /// Bounding rectangle.
        rect: Viewport,
    },
    /// Close button in the header.
    CloseButton {
        /// Bounding rectangle.
        rect: Viewport,
    },
    /// Submit button for task input.
    SubmitButton {
        /// Bounding rectangle.
        rect: Viewport,
    },
    /// Text input area.
    InputArea {
        /// Bounding rectangle.
        rect: Viewport,
    },
}

/// Returns the status icon character and color for a step status string.
fn status_icon(status: &str) -> (char, [f32; 4]) {
    match status {
        "Pending" | "AwaitingApproval" => ('~', COLOR_STATUS_PENDING),
        "Running" | "Executing..." => ('>', COLOR_STATUS_RUNNING),
        "Completed" => ('v', COLOR_STATUS_DONE),
        "Failed" => ('x', COLOR_STATUS_FAIL),
        "Skipped" => ('-', COLOR_STATUS_SKIP),
        _ => ('?', COLOR_STATUS_PENDING),
    }
}

/// Returns the panel-header status color for the given status text.
fn header_status_color(status: &str) -> [f32; 4] {
    match status {
        "Planning..." => COLOR_STATUS_PLANNING,
        "Executing..." => COLOR_STATUS_EXECUTING,
        "Awaiting Approval" => COLOR_STATUS_APPROVAL,
        "Completed" => COLOR_STATUS_COMPLETED,
        "Failed" | "Cancelled" => COLOR_STATUS_FAILED,
        _ => COLOR_STATUS_IDLE,
    }
}

impl Renderer {
    /// Builds rect and text instances for the agent panel overlay.
    ///
    /// The panel is drawn within `panel_viewport` and shows:
    /// - A header bar with title, status indicator, and close button.
    /// - A scrollable step list.
    /// - Approval buttons when awaiting approval.
    /// - An optional user question area.
    /// - An input field at the bottom.
    ///
    /// Returns a list of [`AgentPanelHitRegion`]s for click handling.
    #[allow(clippy::too_many_arguments)]
    pub fn build_agent_panel_instances(
        &mut self,
        panel_viewport: Viewport,
        steps: &[AgentPanelStep],
        status_text: &str,
        input_text: &str,
        input_cursor: usize,
        scroll_offset: f32,
        user_question: Option<&str>,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) -> Vec<AgentPanelHitRegion> {
        let mut hit_regions = Vec::new();

        // --- Panel background ---
        rect_instances.push(RectInstance {
            pos: [panel_viewport.x, panel_viewport.y],
            size: [panel_viewport.width, panel_viewport.height],
            color: COLOR_PANEL_BG,
        });

        // --- Header bar ---
        let header_y = panel_viewport.y;
        rect_instances.push(RectInstance {
            pos: [panel_viewport.x, header_y],
            size: [panel_viewport.width, HEADER_HEIGHT],
            color: COLOR_HEADER_BG,
        });

        // Header title "Agent Mode"
        let title_x = panel_viewport.x + PADDING_X;
        let title_y = header_y + (HEADER_HEIGHT - self.cell_height) / 2.0;
        self.render_text_run(
            "Agent Mode",
            title_x,
            title_y,
            COLOR_HEADER_TEXT,
            text_instances,
        );

        // Status indicator after the title.
        let title_width = "Agent Mode".len() as f32 * self.cell_width;
        let status_x = title_x + title_width + self.cell_width;
        let status_color = header_status_color(status_text);
        self.render_text_run(status_text, status_x, title_y, status_color, text_instances);

        // Close button "[x]" at the right side of the header.
        let close_label = "[x]";
        let close_btn_width = close_label.len() as f32 * self.cell_width;
        let close_x = panel_viewport.x + panel_viewport.width - close_btn_width - PADDING_X;
        let close_y = title_y;
        self.render_text_run(
            close_label,
            close_x,
            close_y,
            COLOR_CLOSE_TEXT,
            text_instances,
        );
        hit_regions.push(AgentPanelHitRegion::CloseButton {
            rect: Viewport {
                x: close_x,
                y: header_y,
                width: close_btn_width,
                height: HEADER_HEIGHT,
            },
        });

        // --- Determine which sections to show ---
        let awaiting_approval = status_text == "Awaiting Approval";
        let is_idle = status_text == "Idle";
        let waiting_for_user = status_text == "Waiting for Input";

        // Layout from bottom up:
        // 1. Input area (always shown)
        // 2. User question (if waiting for user or any question present)
        // 3. Approval buttons (when awaiting approval)
        let mut bottom_y = panel_viewport.y + panel_viewport.height;

        // --- Input area (at the bottom) ---
        bottom_y -= INPUT_HEIGHT;
        let input_y = bottom_y;
        rect_instances.push(RectInstance {
            pos: [panel_viewport.x, input_y],
            size: [panel_viewport.width, INPUT_HEIGHT],
            color: COLOR_INPUT_BG,
        });

        let input_text_y = input_y + (INPUT_HEIGHT - self.cell_height) / 2.0;
        let input_text_x = panel_viewport.x + PADDING_X;

        if input_text.is_empty() {
            let placeholder = if is_idle {
                "Enter task..."
            } else if waiting_for_user {
                "Type your answer..."
            } else {
                ""
            };
            if !placeholder.is_empty() {
                self.render_text_run(
                    placeholder,
                    input_text_x,
                    input_text_y,
                    COLOR_PLACEHOLDER,
                    text_instances,
                );
            }
        } else {
            self.render_text_run(
                input_text,
                input_text_x,
                input_text_y,
                COLOR_INPUT_TEXT,
                text_instances,
            );
        }

        // Input cursor.
        if is_idle || waiting_for_user {
            let cursor_char_count = if input_cursor <= input_text.len() {
                input_text[..input_cursor].chars().count()
            } else {
                input_text.chars().count()
            };
            let cursor_x = input_text_x + cursor_char_count as f32 * self.cell_width;
            rect_instances.push(RectInstance {
                pos: [cursor_x, input_text_y],
                size: [2.0, self.cell_height],
                color: COLOR_INPUT_CURSOR,
            });
        }

        // Submit button when idle.
        if is_idle {
            let submit_label = "[>]";
            let submit_width = submit_label.len() as f32 * self.cell_width + 4.0;
            let submit_x = panel_viewport.x + panel_viewport.width - submit_width - PADDING_X;
            let submit_y = input_text_y;
            self.render_text_run(
                submit_label,
                submit_x,
                submit_y,
                COLOR_APPROVE_TEXT,
                text_instances,
            );
            hit_regions.push(AgentPanelHitRegion::SubmitButton {
                rect: Viewport {
                    x: submit_x,
                    y: input_y,
                    width: submit_width,
                    height: INPUT_HEIGHT,
                },
            });
        }

        hit_regions.push(AgentPanelHitRegion::InputArea {
            rect: Viewport {
                x: panel_viewport.x,
                y: input_y,
                width: panel_viewport.width,
                height: INPUT_HEIGHT,
            },
        });

        // --- User question area (if applicable) ---
        if let Some(question) = user_question {
            let question_lines =
                self.count_wrapped_lines_agent(question, panel_viewport.width - PADDING_X * 2.0);
            let question_height = question_lines as f32 * self.cell_height + STEP_PADDING * 2.0;
            bottom_y -= question_height;

            rect_instances.push(RectInstance {
                pos: [panel_viewport.x, bottom_y],
                size: [panel_viewport.width, question_height],
                color: COLOR_QUESTION_BG,
            });

            let q_label = "Q: ";
            self.render_text_run(
                q_label,
                panel_viewport.x + PADDING_X,
                bottom_y + STEP_PADDING,
                COLOR_QUESTION_TEXT,
                text_instances,
            );
            self.render_wrapped_text_agent(
                question,
                panel_viewport.x + PADDING_X + q_label.len() as f32 * self.cell_width,
                bottom_y + STEP_PADDING,
                panel_viewport.width - PADDING_X * 2.0 - q_label.len() as f32 * self.cell_width,
                panel_viewport.y,
                panel_viewport.y + panel_viewport.height,
                COLOR_QUESTION_TEXT,
                text_instances,
            );
        }

        // --- Approval buttons (shown when awaiting approval) ---
        if awaiting_approval {
            bottom_y -= APPROVAL_HEIGHT;
            let btn_y = bottom_y;
            let btn_area_x = panel_viewport.x + PADDING_X;

            // [Approve] button.
            let approve_label = " Approve ";
            let approve_width = approve_label.len() as f32 * self.cell_width + 4.0;
            let approve_height = self.cell_height + 4.0;
            let approve_btn_y = btn_y + (APPROVAL_HEIGHT - approve_height) / 2.0;

            rect_instances.push(RectInstance {
                pos: [btn_area_x, approve_btn_y],
                size: [approve_width, approve_height],
                color: COLOR_APPROVE_BG,
            });
            self.render_text_run(
                approve_label,
                btn_area_x + 2.0,
                approve_btn_y + 2.0,
                COLOR_APPROVE_TEXT,
                text_instances,
            );
            hit_regions.push(AgentPanelHitRegion::ApproveButton {
                rect: Viewport {
                    x: btn_area_x,
                    y: approve_btn_y,
                    width: approve_width,
                    height: approve_height,
                },
            });

            // [Skip] button.
            let skip_label = " Skip ";
            let skip_width = skip_label.len() as f32 * self.cell_width + 4.0;
            let skip_x = btn_area_x + approve_width + PADDING_X;

            rect_instances.push(RectInstance {
                pos: [skip_x, approve_btn_y],
                size: [skip_width, approve_height],
                color: COLOR_SKIP_BG,
            });
            self.render_text_run(
                skip_label,
                skip_x + 2.0,
                approve_btn_y + 2.0,
                COLOR_SKIP_TEXT,
                text_instances,
            );
            hit_regions.push(AgentPanelHitRegion::SkipButton {
                rect: Viewport {
                    x: skip_x,
                    y: approve_btn_y,
                    width: skip_width,
                    height: approve_height,
                },
            });

            // [Cancel] button at the right.
            let cancel_label = " Cancel ";
            let cancel_width = cancel_label.len() as f32 * self.cell_width + 4.0;
            let cancel_x = panel_viewport.x + panel_viewport.width - cancel_width - PADDING_X;

            rect_instances.push(RectInstance {
                pos: [cancel_x, approve_btn_y],
                size: [cancel_width, approve_height],
                color: COLOR_CANCEL_BG,
            });
            self.render_text_run(
                cancel_label,
                cancel_x + 2.0,
                approve_btn_y + 2.0,
                COLOR_CANCEL_TEXT,
                text_instances,
            );
            hit_regions.push(AgentPanelHitRegion::CancelButton {
                rect: Viewport {
                    x: cancel_x,
                    y: approve_btn_y,
                    width: cancel_width,
                    height: approve_height,
                },
            });
        }

        // --- Step list area ---
        let steps_top = panel_viewport.y + HEADER_HEIGHT;
        let steps_bottom = bottom_y;
        let content_width = panel_viewport.width - PADDING_X * 2.0;

        let mut cursor_y = steps_top + STEP_GAP + scroll_offset;
        let step_x = panel_viewport.x + PADDING_X;

        for step in steps {
            cursor_y = self.render_agent_step(
                step,
                step_x,
                cursor_y,
                content_width,
                steps_top,
                steps_bottom,
                rect_instances,
                text_instances,
            );
            cursor_y += STEP_GAP;
        }

        hit_regions
    }

    /// Renders a single agent step entry, returning the new y cursor position.
    #[allow(clippy::too_many_arguments)]
    fn render_agent_step(
        &mut self,
        step: &AgentPanelStep,
        x: f32,
        start_y: f32,
        content_width: f32,
        clip_top: f32,
        clip_bottom: f32,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) -> f32 {
        let line_height = self.cell_height;

        // Compute the entry height.
        let mut entry_height = STEP_PADDING * 2.0 + line_height; // header line
        if step.is_dangerous {
            entry_height += line_height;
        }
        if let Some(ref output) = step.result_output {
            let output_preview: String = output
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect();
            if !output_preview.is_empty() {
                entry_height += line_height;
            }
        }

        // Entry background.
        if start_y + entry_height > clip_top && start_y < clip_bottom {
            rect_instances.push(RectInstance {
                pos: [x, start_y],
                size: [content_width, entry_height],
                color: COLOR_STEP_BG,
            });
        }

        let inner_x = x + STEP_PADDING;
        let inner_width = content_width - STEP_PADDING * 2.0;
        let mut cur_y = start_y + STEP_PADDING;

        // Status icon + step number.
        let (icon_char, icon_color) = status_icon(&step.status);
        if cur_y >= clip_top && cur_y + line_height <= clip_bottom {
            self.render_char_at(icon_char, inner_x, cur_y, icon_color, text_instances);
        }
        let num_str = format!("{:2}.", step.index + 1);
        let num_x = inner_x + self.cell_width + 2.0;
        if cur_y >= clip_top && cur_y + line_height <= clip_bottom {
            self.render_text_run(&num_str, num_x, cur_y, COLOR_STEP_NUMBER, text_instances);
        }

        // Action type badge.
        let badge_text = format!(" {} ", step.action_type);
        let badge_width = badge_text.len() as f32 * self.cell_width + 4.0;
        let badge_height = line_height;
        let badge_x = num_x + num_str.len() as f32 * self.cell_width + self.cell_width;
        if cur_y >= clip_top && cur_y + badge_height <= clip_bottom {
            rect_instances.push(RectInstance {
                pos: [badge_x, cur_y],
                size: [badge_width, badge_height],
                color: COLOR_BADGE_BG,
            });
            self.render_text_run(
                &badge_text,
                badge_x + 2.0,
                cur_y,
                COLOR_BADGE_TEXT,
                text_instances,
            );
        }

        // Description (clipped to remaining width).
        let desc_x = badge_x + badge_width + self.cell_width;
        let desc_max_width = inner_width - (desc_x - inner_x);
        if cur_y >= clip_top && cur_y + line_height <= clip_bottom && desc_max_width > 0.0 {
            let chars_fit = (desc_max_width / self.cell_width).floor() as usize;
            let desc_display: String = step.description.chars().take(chars_fit).collect();
            self.render_text_run(
                &desc_display,
                desc_x,
                cur_y,
                COLOR_DESC_TEXT,
                text_instances,
            );
        }
        cur_y += line_height;

        // Dangerous command warning badge.
        if step.is_dangerous {
            let warning = step
                .danger_warning
                .as_deref()
                .unwrap_or("Dangerous command!");
            let warn_text = format!(" DANGER: {} ", warning);
            let warn_width = (warn_text.len() as f32 * self.cell_width + 4.0).min(inner_width);
            let warn_height = line_height;
            if cur_y >= clip_top && cur_y + warn_height <= clip_bottom {
                rect_instances.push(RectInstance {
                    pos: [inner_x, cur_y],
                    size: [warn_width, warn_height],
                    color: COLOR_DANGER_BG,
                });
                let chars_fit = ((warn_width - 4.0) / self.cell_width).floor() as usize;
                let warn_display: String = warn_text.chars().take(chars_fit).collect();
                self.render_text_run(
                    &warn_display,
                    inner_x + 2.0,
                    cur_y,
                    COLOR_DANGER_TEXT,
                    text_instances,
                );
            }
            cur_y += line_height;
        }

        // Result output (first line preview).
        if let Some(ref output) = step.result_output {
            let first_line: String = output
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect();
            if !first_line.is_empty() && cur_y >= clip_top && cur_y + line_height <= clip_bottom {
                let result_text = format!("> {first_line}");
                let chars_fit = (inner_width / self.cell_width).floor() as usize;
                let result_display: String = result_text.chars().take(chars_fit).collect();
                self.render_text_run(
                    &result_display,
                    inner_x,
                    cur_y,
                    COLOR_RESULT_TEXT,
                    text_instances,
                );
                cur_y += line_height;
            }
        }

        cur_y + STEP_PADDING
    }

    /// Renders text with character-level wrapping for the agent panel,
    /// returning the new y position.
    #[allow(clippy::too_many_arguments)]
    fn render_wrapped_text_agent(
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
    fn count_wrapped_lines_agent(&self, text: &str, max_width: f32) -> usize {
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
    fn agent_panel_step_clone() {
        let step = AgentPanelStep {
            index: 0,
            action_type: "RunCommand".to_string(),
            description: "ls -la".to_string(),
            status: "Pending".to_string(),
            is_dangerous: false,
            result_output: None,
            danger_warning: None,
        };
        let cloned = step.clone();
        assert_eq!(cloned.index, 0);
        assert_eq!(cloned.action_type, "RunCommand");
        assert_eq!(cloned.status, "Pending");
        assert!(!cloned.is_dangerous);
    }

    #[test]
    fn agent_panel_step_with_result() {
        let step = AgentPanelStep {
            index: 1,
            action_type: "ReadFile".to_string(),
            description: "Read: /etc/hosts".to_string(),
            status: "Completed".to_string(),
            is_dangerous: false,
            result_output: Some("127.0.0.1 localhost".to_string()),
            danger_warning: None,
        };
        assert!(step.result_output.is_some());
    }

    #[test]
    fn agent_panel_step_dangerous() {
        let step = AgentPanelStep {
            index: 2,
            action_type: "RunCommand".to_string(),
            description: "Run: rm -rf /tmp".to_string(),
            status: "AwaitingApproval".to_string(),
            is_dangerous: true,
            result_output: None,
            danger_warning: Some("Destructive command detected".to_string()),
        };
        assert!(step.is_dangerous);
        assert!(step.danger_warning.is_some());
    }

    #[test]
    fn agent_panel_hit_region_debug() {
        let region = AgentPanelHitRegion::ApproveButton {
            rect: Viewport {
                x: 10.0,
                y: 20.0,
                width: 80.0,
                height: 24.0,
            },
        };
        let s = format!("{:?}", region);
        assert!(s.contains("ApproveButton"));
    }

    #[test]
    fn agent_panel_hit_region_all_variants() {
        let vp = Viewport {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 20.0,
        };
        let regions = vec![
            AgentPanelHitRegion::ApproveButton { rect: vp },
            AgentPanelHitRegion::SkipButton { rect: vp },
            AgentPanelHitRegion::CancelButton { rect: vp },
            AgentPanelHitRegion::CloseButton { rect: vp },
            AgentPanelHitRegion::SubmitButton { rect: vp },
            AgentPanelHitRegion::InputArea { rect: vp },
        ];
        assert_eq!(regions.len(), 6);
    }

    #[test]
    fn status_icon_pending() {
        let (icon, _) = status_icon("Pending");
        assert_eq!(icon, '~');
    }

    #[test]
    fn status_icon_completed() {
        let (icon, _) = status_icon("Completed");
        assert_eq!(icon, 'v');
    }

    #[test]
    fn status_icon_failed() {
        let (icon, _) = status_icon("Failed");
        assert_eq!(icon, 'x');
    }

    #[test]
    fn header_status_color_planning() {
        let color = header_status_color("Planning...");
        assert_eq!(color, COLOR_STATUS_PLANNING);
    }

    #[test]
    fn header_status_color_idle() {
        let color = header_status_color("Idle");
        assert_eq!(color, COLOR_STATUS_IDLE);
    }
}
