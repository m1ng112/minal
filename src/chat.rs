//! Chat panel state for the inline AI chat overlay.
//!
//! Manages visibility, animation, input buffer, and conversation state.
//! The actual conversation logic is delegated to [`minal_ai::ChatEngine`]
//! while this module handles UI state and input routing.

use minal_ai::ChatEngine;
use minal_renderer::chat_panel::{ChatMessage, ChatRole};

/// State for the inline AI chat panel overlay.
pub struct ChatPanelState {
    /// Whether the panel should be visible (animation target).
    visible: bool,
    /// Chat engine managing conversation history and streaming.
    pub chat_engine: ChatEngine,
    /// Current text input buffer.
    pub input_buffer: String,
    /// Cursor position within `input_buffer` (byte offset).
    pub input_cursor: usize,
    /// Scroll offset in pixels for the message area.
    pub scroll_offset: f32,
    /// Current animation progress (0.0 = hidden, 1.0 = fully visible).
    pub animation_progress: f32,
    /// Animation target (0.0 or 1.0).
    animation_target: f32,
    /// Panel height as fraction of window height.
    pub panel_height_ratio: f32,
    /// Cached hit regions from the last render for mouse handling.
    pub hit_regions: Vec<minal_renderer::ChatHitRegion>,
    /// Extracted code blocks from assistant messages for execution.
    code_blocks: Vec<CodeBlock>,
    /// Error messages to display in the chat panel.
    error_messages: Vec<String>,
}

/// A code block extracted from an assistant message.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields read during code block execution via hit regions.
pub struct CodeBlock {
    /// The code content.
    pub code: String,
    /// Optional language identifier.
    pub language: Option<String>,
}

/// Animation interpolation speed (higher = faster).
const ANIMATION_SPEED: f32 = 8.0;

/// Threshold below which animation snaps to target.
const ANIMATION_EPSILON: f32 = 0.005;

impl ChatPanelState {
    /// Creates a new chat panel state from configuration.
    pub fn new(config: &minal_config::ChatConfig) -> Self {
        let system_prompt = config
            .system_prompt
            .clone()
            .unwrap_or_else(default_system_prompt);

        Self {
            visible: false,
            chat_engine: ChatEngine::new(config.max_history, system_prompt),
            input_buffer: String::new(),
            input_cursor: 0,
            scroll_offset: 0.0,
            animation_progress: 0.0,
            animation_target: 0.0,
            panel_height_ratio: config.panel_height_ratio,
            hit_regions: Vec::new(),
            code_blocks: Vec::new(),
            error_messages: Vec::new(),
        }
    }

    /// Toggles the panel open/closed.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        self.animation_target = if self.visible { 1.0 } else { 0.0 };
    }

    /// Whether the panel is visible or animating toward visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Whether the animation is still in progress.
    pub fn is_animating(&self) -> bool {
        (self.animation_progress - self.animation_target).abs() > ANIMATION_EPSILON
    }

    /// Updates animation progress. Returns `true` if a redraw is needed.
    pub fn update_animation(&mut self, dt: f32) -> bool {
        if !self.is_animating() {
            return false;
        }
        let diff = self.animation_target - self.animation_progress;
        self.animation_progress += diff * (ANIMATION_SPEED * dt).min(1.0);
        if (self.animation_progress - self.animation_target).abs() < ANIMATION_EPSILON {
            self.animation_progress = self.animation_target;
        }
        true
    }

    /// Computes the panel viewport in screen coordinates.
    pub fn panel_viewport(
        &self,
        screen_width: f32,
        screen_height: f32,
        top_offset: f32,
    ) -> minal_renderer::Viewport {
        let available_height = screen_height - top_offset;
        let panel_h = available_height * self.panel_height_ratio * self.animation_progress;
        let y = screen_height - panel_h;
        minal_renderer::Viewport {
            x: 0.0,
            y,
            width: screen_width,
            height: panel_h,
        }
    }

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, ch: char) {
        self.input_buffer.insert(self.input_cursor, ch);
        self.input_cursor += ch.len_utf8();
    }

    /// Insert a string at the cursor position.
    #[allow(dead_code)] // Will be used for paste support.
    pub fn insert_str(&mut self, s: &str) {
        self.input_buffer.insert_str(self.input_cursor, s);
        self.input_cursor += s.len();
    }

    /// Delete the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.input_cursor > 0 {
            // Find the previous character boundary.
            let prev = self.input_buffer[..self.input_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input_buffer.drain(prev..self.input_cursor);
            self.input_cursor = prev;
        }
    }

    /// Delete the character at the cursor (delete key).
    pub fn delete_char(&mut self) {
        if self.input_cursor < self.input_buffer.len() {
            let next = self.input_buffer[self.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.input_cursor + i)
                .unwrap_or(self.input_buffer.len());
            self.input_buffer.drain(self.input_cursor..next);
        }
    }

    /// Move cursor left one character.
    pub fn cursor_left(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor = self.input_buffer[..self.input_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right one character.
    pub fn cursor_right(&mut self) {
        if self.input_cursor < self.input_buffer.len() {
            self.input_cursor = self.input_buffer[self.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.input_cursor + i)
                .unwrap_or(self.input_buffer.len());
        }
    }

    /// Move cursor to the beginning of the input.
    pub fn cursor_home(&mut self) {
        self.input_cursor = 0;
    }

    /// Move cursor to the end of the input.
    pub fn cursor_end(&mut self) {
        self.input_cursor = self.input_buffer.len();
    }

    /// Take the input buffer content (clears it and resets cursor).
    ///
    /// Returns `None` if the input is empty or whitespace-only.
    pub fn take_input(&mut self) -> Option<String> {
        let text = self.input_buffer.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.input_buffer.clear();
        self.input_cursor = 0;
        Some(text)
    }

    /// Scroll up by the given number of pixels.
    pub fn scroll_up(&mut self, pixels: f32) {
        self.scroll_offset = (self.scroll_offset + pixels).max(0.0);
    }

    /// Scroll down by the given number of pixels.
    pub fn scroll_down(&mut self, pixels: f32) {
        self.scroll_offset = (self.scroll_offset - pixels).max(0.0);
    }

    /// Collect messages for rendering.
    ///
    /// Streaming content is rendered separately by the renderer via the
    /// `streaming_text` parameter to avoid double-rendering.
    pub fn render_messages(&self) -> Vec<ChatMessage> {
        let mut msgs: Vec<ChatMessage> = self
            .chat_engine
            .messages()
            .iter()
            .map(|m| ChatMessage {
                role: match m.role {
                    minal_ai::Role::User => ChatRole::User,
                    minal_ai::Role::Assistant | minal_ai::Role::System => ChatRole::Assistant,
                },
                content: m.content.clone(),
            })
            .collect();

        // Append any error messages.
        for err in &self.error_messages {
            msgs.push(ChatMessage {
                role: ChatRole::Error,
                content: err.clone(),
            });
        }
        // Clear errors after rendering so they show once.
        msgs
    }

    /// Extract code blocks from all assistant messages.
    pub fn extract_code_blocks(&mut self) {
        self.code_blocks.clear();
        for msg in self.chat_engine.messages() {
            if matches!(msg.role, minal_ai::Role::Assistant) {
                extract_code_blocks_from(&msg.content, &mut self.code_blocks);
            }
        }
    }

    /// Get a code block by its hit region index.
    #[allow(dead_code)] // Used for future code block inspection.
    pub fn get_code_block(&self, index: usize) -> Option<&CodeBlock> {
        self.code_blocks.get(index)
    }

    /// Add an error message to the conversation display.
    /// Add an error message and cancel any active stream.
    pub fn add_error_message(&mut self, error: &str) {
        self.chat_engine.cancel_stream();
        self.error_messages.push(error.to_string());
    }

    /// Whether the panel is fully hidden (animation complete at 0.0).
    pub fn is_fully_hidden(&self) -> bool {
        self.animation_progress < ANIMATION_EPSILON && self.animation_target < ANIMATION_EPSILON
    }
}

/// Extract fenced code blocks from markdown text.
fn extract_code_blocks_from(text: &str, out: &mut Vec<CodeBlock>) {
    let mut in_code = false;
    let mut code = String::new();
    let mut language = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code {
                // End of code block.
                out.push(CodeBlock {
                    code: code.trim_end().to_string(),
                    language: language.take(),
                });
                code.clear();
                in_code = false;
            } else {
                // Start of code block.
                let lang = trimmed.strip_prefix("```").unwrap_or("").trim();
                language = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
                in_code = true;
            }
        } else if in_code {
            if !code.is_empty() {
                code.push('\n');
            }
            code.push_str(line);
        }
    }
}

/// Default system prompt for the chat engine.
fn default_system_prompt() -> String {
    "You are Minal AI, a helpful terminal assistant. \
     You help users with command-line tasks, explain errors, \
     and suggest solutions. Keep responses concise and actionable. \
     When suggesting commands, wrap them in code blocks."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> minal_config::ChatConfig {
        minal_config::ChatConfig {
            panel_height_ratio: 0.3,
            max_history: 50,
            system_prompt: None,
        }
    }

    #[test]
    fn new_panel_is_hidden() {
        let panel = ChatPanelState::new(&test_config());
        assert!(!panel.is_visible());
        assert!(panel.is_fully_hidden());
        assert_eq!(panel.animation_progress, 0.0);
    }

    #[test]
    fn toggle_makes_visible() {
        let mut panel = ChatPanelState::new(&test_config());
        panel.toggle();
        assert!(panel.is_visible());
        assert!(panel.is_animating());
    }

    #[test]
    fn double_toggle_hides() {
        let mut panel = ChatPanelState::new(&test_config());
        panel.toggle();
        panel.toggle();
        assert!(!panel.is_visible());
    }

    #[test]
    fn animation_progresses() {
        let mut panel = ChatPanelState::new(&test_config());
        panel.toggle();
        for _ in 0..100 {
            panel.update_animation(0.016);
        }
        assert!(!panel.is_animating());
        assert!((panel.animation_progress - 1.0).abs() < 0.01);
    }

    #[test]
    fn input_buffer_operations() {
        let mut panel = ChatPanelState::new(&test_config());
        panel.insert_char('h');
        panel.insert_char('i');
        assert_eq!(panel.input_buffer, "hi");
        assert_eq!(panel.input_cursor, 2);

        panel.backspace();
        assert_eq!(panel.input_buffer, "h");
        assert_eq!(panel.input_cursor, 1);

        panel.cursor_left();
        assert_eq!(panel.input_cursor, 0);

        panel.insert_char('a');
        assert_eq!(panel.input_buffer, "ah");
    }

    #[test]
    fn take_input_clears() {
        let mut panel = ChatPanelState::new(&test_config());
        panel.insert_str("hello world");
        let text = panel.take_input();
        assert_eq!(text, Some("hello world".to_string()));
        assert!(panel.input_buffer.is_empty());
        assert_eq!(panel.input_cursor, 0);
    }

    #[test]
    fn take_input_empty_returns_none() {
        let mut panel = ChatPanelState::new(&test_config());
        assert!(panel.take_input().is_none());
        panel.insert_str("   ");
        assert!(panel.take_input().is_none());
    }

    #[test]
    fn extract_code_blocks_basic() {
        let mut blocks = Vec::new();
        extract_code_blocks_from("hello\n```bash\nls -la\necho hi\n```\nbye", &mut blocks);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "ls -la\necho hi");
        assert_eq!(blocks[0].language, Some("bash".to_string()));
    }

    #[test]
    fn extract_multiple_code_blocks() {
        let mut blocks = Vec::new();
        extract_code_blocks_from("```\nfoo\n```\ntext\n```python\nbar\n```", &mut blocks);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].code, "foo");
        assert!(blocks[0].language.is_none());
        assert_eq!(blocks[1].code, "bar");
        assert_eq!(blocks[1].language, Some("python".to_string()));
    }

    #[test]
    fn panel_viewport_calculation() {
        let mut panel = ChatPanelState::new(&test_config());
        panel.animation_progress = 1.0;
        let vp = panel.panel_viewport(800.0, 600.0, 28.0);
        // Available height = 600 - 28 = 572
        // Panel height = 572 * 0.3 * 1.0 = 171.6
        // y = 600 - 171.6 = 428.4
        assert!((vp.height - 171.6).abs() < 0.1);
        assert!((vp.y - 428.4).abs() < 0.1);
        assert_eq!(vp.width, 800.0);
    }

    #[test]
    fn scroll_operations() {
        let mut panel = ChatPanelState::new(&test_config());
        panel.scroll_up(50.0);
        assert_eq!(panel.scroll_offset, 50.0);
        panel.scroll_down(30.0);
        assert_eq!(panel.scroll_offset, 20.0);
        panel.scroll_down(100.0);
        assert_eq!(panel.scroll_offset, 0.0); // Clamp at 0
    }
}
