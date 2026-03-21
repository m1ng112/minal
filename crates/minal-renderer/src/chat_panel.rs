//! Chat panel overlay rendering.
//!
//! Builds rect and text instances for the AI chat panel that overlays
//! the bottom portion of the terminal window.

use crate::Viewport;
use crate::rect::RectInstance;
use crate::renderer::Renderer;
use crate::text::TextInstance;

// -- Layout constants --

/// Height of the header bar in pixels.
const HEADER_HEIGHT: f32 = 24.0;

/// Height of the input area in pixels.
const INPUT_HEIGHT: f32 = 32.0;

/// Horizontal padding inside the panel in pixels.
const PADDING_X: f32 = 8.0;

/// Vertical padding between messages in pixels.
const MESSAGE_GAP: f32 = 6.0;

// -- Colors --

/// Panel background color.
const COLOR_PANEL_BG: [f32; 4] = [0.1, 0.1, 0.15, 0.95];

/// Header bar background color.
const COLOR_HEADER_BG: [f32; 4] = [0.15, 0.15, 0.2, 1.0];

/// Header title text color.
const COLOR_HEADER_TEXT: [f32; 4] = [0.8, 0.8, 0.85, 1.0];

/// User message prefix color.
const COLOR_USER: [f32; 4] = [0.4, 0.7, 1.0, 1.0];

/// Assistant message prefix color.
const COLOR_ASSISTANT: [f32; 4] = [0.5, 0.9, 0.5, 1.0];

/// Error message color.
const COLOR_ERROR: [f32; 4] = [1.0, 0.4, 0.4, 1.0];

/// Normal message text color.
const COLOR_TEXT: [f32; 4] = [0.85, 0.85, 0.88, 1.0];

/// Bold text color (brighter).
const COLOR_TEXT_BOLD: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Code block background color.
const COLOR_CODE_BG: [f32; 4] = [0.06, 0.06, 0.09, 1.0];

/// Code block text color.
const COLOR_CODE_TEXT: [f32; 4] = [0.9, 0.8, 0.6, 1.0];

/// "Run" button background color.
const COLOR_RUN_BTN_BG: [f32; 4] = [0.2, 0.5, 0.3, 1.0];

/// "Run" button text color.
const COLOR_RUN_BTN_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Input area background color.
const COLOR_INPUT_BG: [f32; 4] = [0.08, 0.08, 0.12, 1.0];

/// Input placeholder text color.
const COLOR_PLACEHOLDER: [f32; 4] = [0.4, 0.4, 0.45, 1.0];

/// Input cursor color.
const COLOR_INPUT_CURSOR: [f32; 4] = [0.8, 0.8, 0.85, 0.9];

/// Streaming indicator suffix.
const STREAMING_SUFFIX: &str = "...";

/// Describes the role of a chat message for rendering purposes.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatRole {
    /// Message from the user.
    User,
    /// Response from the AI assistant.
    Assistant,
    /// Error message.
    Error,
}

/// A chat message to render.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Who sent this message.
    pub role: ChatRole,
    /// The message content (may contain markdown).
    pub content: String,
}

/// A clickable region in the chat panel.
#[derive(Debug, Clone)]
pub enum ChatHitRegion {
    /// "Run" button for a code block.
    ExecuteCodeBlock {
        /// Index of the code block in the message.
        index: usize,
        /// The code content to execute.
        code: String,
        /// Bounding rectangle.
        rect: Viewport,
    },
    /// Close button.
    CloseButton {
        /// Bounding rectangle.
        rect: Viewport,
    },
    /// Input text area.
    InputArea {
        /// Bounding rectangle.
        rect: Viewport,
    },
}

/// Parsed markdown span for rendering.
#[derive(Debug, Clone, PartialEq)]
enum ChatSpan {
    /// Normal or bold text.
    Text { text: String, bold: bool },
    /// A fenced code block.
    CodeBlock {
        code: String,
        language: Option<String>,
    },
    /// Line break.
    Newline,
}

/// Parses simple markdown into a sequence of [`ChatSpan`]s.
///
/// Supports:
/// - Fenced code blocks (```) with optional language tag
/// - Bold text (`**...**`)
/// - Line breaks
fn parse_simple_markdown(text: &str) -> Vec<ChatSpan> {
    let mut spans = Vec::new();
    let mut in_code_block = false;
    let mut code_buf = String::new();
    let mut code_lang: Option<String> = None;

    for line in text.split('\n') {
        if in_code_block {
            if line.trim_start().starts_with("```") {
                // End of code block.
                spans.push(ChatSpan::CodeBlock {
                    code: code_buf.clone(),
                    language: code_lang.take(),
                });
                code_buf.clear();
                in_code_block = false;
            } else {
                if !code_buf.is_empty() {
                    code_buf.push('\n');
                }
                code_buf.push_str(line);
            }
            continue;
        }

        if line.trim_start().starts_with("```") {
            // Start of code block.
            in_code_block = true;
            let after = line.trim_start().strip_prefix("```").unwrap_or("");
            let lang = after.trim();
            code_lang = if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            };
            code_buf.clear();
            continue;
        }

        // Parse inline bold markers (**...**) within the line.
        if !spans.is_empty() {
            spans.push(ChatSpan::Newline);
        }
        parse_inline_bold(line, &mut spans);
    }

    // Handle unclosed code block.
    if in_code_block && !code_buf.is_empty() {
        spans.push(ChatSpan::CodeBlock {
            code: code_buf,
            language: code_lang,
        });
    }

    spans
}

/// Parses bold markers (`**...**`) within a single line and appends spans.
fn parse_inline_bold(line: &str, spans: &mut Vec<ChatSpan>) {
    let mut remaining = line;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find("**") {
            // Text before the bold marker.
            if start > 0 {
                spans.push(ChatSpan::Text {
                    text: remaining[..start].to_string(),
                    bold: false,
                });
            }
            let after_open = &remaining[start + 2..];
            if let Some(end) = after_open.find("**") {
                spans.push(ChatSpan::Text {
                    text: after_open[..end].to_string(),
                    bold: true,
                });
                remaining = &after_open[end + 2..];
            } else {
                // No closing marker -- emit the rest as plain text.
                spans.push(ChatSpan::Text {
                    text: remaining[start..].to_string(),
                    bold: false,
                });
                return;
            }
        } else {
            spans.push(ChatSpan::Text {
                text: remaining.to_string(),
                bold: false,
            });
            return;
        }
    }
}

impl Renderer {
    /// Builds rect and text instances for the chat panel overlay.
    ///
    /// The panel is drawn within `panel_viewport` and contains a header bar,
    /// scrollable message area, and an input field at the bottom.
    ///
    /// Returns a list of [`ChatHitRegion`]s for click handling.
    #[allow(clippy::too_many_arguments)]
    pub fn build_chat_panel_instances(
        &mut self,
        panel_viewport: Viewport,
        messages: &[ChatMessage],
        streaming_text: &str,
        input_text: &str,
        input_cursor: usize,
        scroll_offset: f32,
        is_streaming: bool,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) -> Vec<ChatHitRegion> {
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

        // Header title text: "AI Chat"
        let title_x = panel_viewport.x + PADDING_X;
        let title_y = header_y + (HEADER_HEIGHT - self.cell_height) / 2.0;
        self.render_text_run(
            "AI Chat",
            title_x,
            title_y,
            COLOR_HEADER_TEXT,
            text_instances,
        );

        // Close button "x" at the right side of the header.
        let close_btn_width = self.cell_width * 2.0;
        let close_x = panel_viewport.x + panel_viewport.width - close_btn_width - PADDING_X;
        let close_y = title_y;
        self.render_text_run("x", close_x, close_y, COLOR_HEADER_TEXT, text_instances);

        hit_regions.push(ChatHitRegion::CloseButton {
            rect: Viewport {
                x: close_x,
                y: header_y,
                width: close_btn_width,
                height: HEADER_HEIGHT,
            },
        });

        // --- Input area (at the bottom) ---
        let input_y = panel_viewport.y + panel_viewport.height - INPUT_HEIGHT;
        rect_instances.push(RectInstance {
            pos: [panel_viewport.x, input_y],
            size: [panel_viewport.width, INPUT_HEIGHT],
            color: COLOR_INPUT_BG,
        });

        let input_text_y = input_y + (INPUT_HEIGHT - self.cell_height) / 2.0;
        let input_text_x = panel_viewport.x + PADDING_X;

        if input_text.is_empty() {
            self.render_text_run(
                "Type a message...",
                input_text_x,
                input_text_y,
                COLOR_PLACEHOLDER,
                text_instances,
            );
        } else {
            self.render_text_run(
                input_text,
                input_text_x,
                input_text_y,
                COLOR_TEXT,
                text_instances,
            );
        }

        // Input cursor (blinking bar).
        // Use character count (not byte offset) for correct multi-byte positioning.
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

        hit_regions.push(ChatHitRegion::InputArea {
            rect: Viewport {
                x: panel_viewport.x,
                y: input_y,
                width: panel_viewport.width,
                height: INPUT_HEIGHT,
            },
        });

        // --- Messages area ---
        let messages_top = panel_viewport.y + HEADER_HEIGHT;
        let messages_bottom = input_y;
        let content_width = panel_viewport.width - PADDING_X * 2.0;

        // Compute positions for all messages, then apply scroll offset.
        let mut cursor_y = messages_top + MESSAGE_GAP + scroll_offset;
        let msg_x = panel_viewport.x + PADDING_X;

        let mut code_block_index: usize = 0;

        for msg in messages {
            cursor_y = self.render_chat_message(
                msg,
                msg_x,
                cursor_y,
                content_width,
                messages_top,
                messages_bottom,
                &mut code_block_index,
                &mut hit_regions,
                rect_instances,
                text_instances,
            );
            cursor_y += MESSAGE_GAP;
        }

        // Streaming text as a partial assistant message.
        if is_streaming && !streaming_text.is_empty() {
            let streaming_content = format!("{streaming_text}{STREAMING_SUFFIX}");
            let streaming_msg = ChatMessage {
                role: ChatRole::Assistant,
                content: streaming_content,
            };
            self.render_chat_message(
                &streaming_msg,
                msg_x,
                cursor_y,
                content_width,
                messages_top,
                messages_bottom,
                &mut code_block_index,
                &mut hit_regions,
                rect_instances,
                text_instances,
            );
        }

        hit_regions
    }

    /// Renders a single chat message, returning the new y cursor position.
    #[allow(clippy::too_many_arguments)]
    fn render_chat_message(
        &mut self,
        msg: &ChatMessage,
        x: f32,
        start_y: f32,
        content_width: f32,
        clip_top: f32,
        clip_bottom: f32,
        code_block_index: &mut usize,
        hit_regions: &mut Vec<ChatHitRegion>,
        rect_instances: &mut Vec<RectInstance>,
        text_instances: &mut Vec<TextInstance>,
    ) -> f32 {
        let line_height = self.cell_height;
        let mut cur_y = start_y;

        // Render role prefix.
        let (prefix, prefix_color) = match msg.role {
            ChatRole::User => ("You: ", COLOR_USER),
            ChatRole::Assistant => ("AI: ", COLOR_ASSISTANT),
            ChatRole::Error => ("Error: ", COLOR_ERROR),
        };

        let mut cur_x = x;
        if cur_y >= clip_top && cur_y + line_height <= clip_bottom {
            self.render_text_run(prefix, cur_x, cur_y, prefix_color, text_instances);
        }
        cur_x += prefix.len() as f32 * self.cell_width;

        // Parse message content as simple markdown.
        let spans = parse_simple_markdown(&msg.content);

        let text_color = match msg.role {
            ChatRole::Error => COLOR_ERROR,
            _ => COLOR_TEXT,
        };

        for span in &spans {
            match span {
                ChatSpan::Text { text, bold } => {
                    let color = if *bold { COLOR_TEXT_BOLD } else { text_color };
                    // Character-level wrapping.
                    for c in text.chars() {
                        if cur_x + self.cell_width > x + content_width {
                            cur_x = x;
                            cur_y += line_height;
                        }
                        if cur_y >= clip_top && cur_y + line_height <= clip_bottom {
                            self.render_char_at(c, cur_x, cur_y, color, text_instances);
                        }
                        cur_x += self.cell_width;
                    }
                }
                ChatSpan::Newline => {
                    cur_x = x;
                    cur_y += line_height;
                }
                ChatSpan::CodeBlock { code, language: _ } => {
                    // Start code block on a new line.
                    cur_y += line_height;

                    let code_lines: Vec<&str> = code.split('\n').collect();
                    let code_height = code_lines.len() as f32 * line_height + 4.0;

                    // Code block background.
                    if cur_y >= clip_top && cur_y + code_height <= clip_bottom {
                        rect_instances.push(RectInstance {
                            pos: [x, cur_y],
                            size: [content_width, code_height],
                            color: COLOR_CODE_BG,
                        });
                    }

                    let code_y_start = cur_y + 2.0;
                    for code_line in &code_lines {
                        let mut lx = x + 4.0;
                        if cur_y >= clip_top && cur_y + line_height <= clip_bottom {
                            for c in code_line.chars() {
                                if lx + self.cell_width > x + content_width - 4.0 {
                                    lx = x + 4.0;
                                    cur_y += line_height;
                                }
                                self.render_char_at(
                                    c,
                                    lx,
                                    cur_y + 2.0,
                                    COLOR_CODE_TEXT,
                                    text_instances,
                                );
                                lx += self.cell_width;
                            }
                        }
                        cur_y += line_height;
                    }

                    // "Run" button.
                    let btn_text = " Run ";
                    let btn_width = btn_text.len() as f32 * self.cell_width + 4.0;
                    let btn_height = line_height + 2.0;
                    let btn_x = x + content_width - btn_width - 4.0;
                    let btn_y = code_y_start;

                    if btn_y >= clip_top && btn_y + btn_height <= clip_bottom {
                        rect_instances.push(RectInstance {
                            pos: [btn_x, btn_y],
                            size: [btn_width, btn_height],
                            color: COLOR_RUN_BTN_BG,
                        });
                        self.render_text_run(
                            btn_text,
                            btn_x + 2.0,
                            btn_y + 1.0,
                            COLOR_RUN_BTN_TEXT,
                            text_instances,
                        );
                    }

                    hit_regions.push(ChatHitRegion::ExecuteCodeBlock {
                        index: *code_block_index,
                        code: code.clone(),
                        rect: Viewport {
                            x: btn_x,
                            y: btn_y,
                            width: btn_width,
                            height: btn_height,
                        },
                    });
                    *code_block_index += 1;

                    cur_x = x;
                }
            }
        }

        cur_y + line_height
    }

    /// Renders a string of text at the given position without wrapping.
    pub(crate) fn render_text_run(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        text_instances: &mut Vec<TextInstance>,
    ) {
        for (i, c) in text.chars().enumerate() {
            let cx = x + i as f32 * self.cell_width;
            self.render_char_at(c, cx, y, color, text_instances);
        }
    }

    /// Renders a single character at the given position using the glyph atlas.
    pub(crate) fn render_char_at(
        &mut self,
        c: char,
        x: f32,
        y: f32,
        color: [f32; 4],
        text_instances: &mut Vec<TextInstance>,
    ) {
        if c == ' ' || c == '\0' {
            return;
        }

        let font_size = self.font_size;
        let font_size_px = font_size as u32;
        let baseline_y = self.baseline_y;
        let atlas_w = self.glyph_atlas.size().0 as f32;
        let atlas_h = self.glyph_atlas.size().1 as f32;

        let glyph_key = match self.char_glyph_cache.get(&c) {
            Some(cached) => *cached,
            None => {
                let key = crate::renderer::resolve_glyph_key(
                    &mut self.font_system,
                    c,
                    font_size,
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
                    fg_color: color,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_text() {
        let spans = parse_simple_markdown("Hello world");
        assert_eq!(
            spans,
            vec![ChatSpan::Text {
                text: "Hello world".to_string(),
                bold: false,
            }]
        );
    }

    #[test]
    fn parse_bold_text() {
        let spans = parse_simple_markdown("Hello **world**!");
        assert_eq!(
            spans,
            vec![
                ChatSpan::Text {
                    text: "Hello ".to_string(),
                    bold: false,
                },
                ChatSpan::Text {
                    text: "world".to_string(),
                    bold: true,
                },
                ChatSpan::Text {
                    text: "!".to_string(),
                    bold: false,
                },
            ]
        );
    }

    #[test]
    fn parse_code_block() {
        let input = "Before\n```rust\nfn main() {}\n```\nAfter";
        let spans = parse_simple_markdown(input);
        assert_eq!(
            spans,
            vec![
                ChatSpan::Text {
                    text: "Before".to_string(),
                    bold: false,
                },
                ChatSpan::CodeBlock {
                    code: "fn main() {}".to_string(),
                    language: Some("rust".to_string()),
                },
                ChatSpan::Newline,
                ChatSpan::Text {
                    text: "After".to_string(),
                    bold: false,
                },
            ]
        );
    }

    #[test]
    fn parse_code_block_no_language() {
        let input = "```\nsome code\n```";
        let spans = parse_simple_markdown(input);
        assert_eq!(
            spans,
            vec![ChatSpan::CodeBlock {
                code: "some code".to_string(),
                language: None,
            }]
        );
    }

    #[test]
    fn parse_multiline_code_block() {
        let input = "```\nline1\nline2\nline3\n```";
        let spans = parse_simple_markdown(input);
        assert_eq!(
            spans,
            vec![ChatSpan::CodeBlock {
                code: "line1\nline2\nline3".to_string(),
                language: None,
            }]
        );
    }

    #[test]
    fn parse_multiple_lines() {
        let spans = parse_simple_markdown("line1\nline2");
        assert_eq!(
            spans,
            vec![
                ChatSpan::Text {
                    text: "line1".to_string(),
                    bold: false,
                },
                ChatSpan::Newline,
                ChatSpan::Text {
                    text: "line2".to_string(),
                    bold: false,
                },
            ]
        );
    }

    #[test]
    fn parse_unclosed_bold() {
        let spans = parse_simple_markdown("Hello **world");
        assert_eq!(
            spans,
            vec![
                ChatSpan::Text {
                    text: "Hello ".to_string(),
                    bold: false,
                },
                ChatSpan::Text {
                    text: "**world".to_string(),
                    bold: false,
                },
            ]
        );
    }

    #[test]
    fn parse_unclosed_code_block() {
        let input = "```rust\nfn main() {}";
        let spans = parse_simple_markdown(input);
        assert_eq!(
            spans,
            vec![ChatSpan::CodeBlock {
                code: "fn main() {}".to_string(),
                language: Some("rust".to_string()),
            }]
        );
    }

    #[test]
    fn parse_empty_string() {
        let spans = parse_simple_markdown("");
        // An empty string splits into one empty line, but parse_inline_bold
        // produces a single empty Text span. However, the first line does not
        // get a preceding Newline, and parse_inline_bold on "" returns immediately
        // without pushing anything. So the result is empty.
        assert!(spans.is_empty());
    }

    #[test]
    fn parse_bold_multiple_segments() {
        let spans = parse_simple_markdown("a **b** c **d** e");
        assert_eq!(
            spans,
            vec![
                ChatSpan::Text {
                    text: "a ".to_string(),
                    bold: false,
                },
                ChatSpan::Text {
                    text: "b".to_string(),
                    bold: true,
                },
                ChatSpan::Text {
                    text: " c ".to_string(),
                    bold: false,
                },
                ChatSpan::Text {
                    text: "d".to_string(),
                    bold: true,
                },
                ChatSpan::Text {
                    text: " e".to_string(),
                    bold: false,
                },
            ]
        );
    }

    #[test]
    fn chat_message_clone() {
        let msg = ChatMessage {
            role: ChatRole::User,
            content: "Hello".to_string(),
        };
        let cloned = msg.clone();
        assert_eq!(cloned.role, ChatRole::User);
        assert_eq!(cloned.content, "Hello");
    }

    #[test]
    fn chat_role_equality() {
        assert_eq!(ChatRole::User, ChatRole::User);
        assert_eq!(ChatRole::Assistant, ChatRole::Assistant);
        assert_eq!(ChatRole::Error, ChatRole::Error);
        assert_ne!(ChatRole::User, ChatRole::Assistant);
    }
}
