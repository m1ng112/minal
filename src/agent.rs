//! Agent panel state for the autonomous AI agent overlay.
//!
//! Manages visibility, animation, input buffer, and agent lifecycle state.
//! The actual planning and step execution logic is delegated to
//! [`minal_ai::AgentEngine`] while this module handles UI state and routing.

use std::collections::HashSet;

use minal_ai::{AgentAction, AgentEngine, AgentState, DangerousCommandChecker, StepResult};
use minal_config::AgentConfig;
use minal_renderer::Viewport;
use minal_renderer::agent_panel::{AgentPanelHitRegion, AgentPanelStep};

/// Animation interpolation speed (higher = faster).
const ANIMATION_SPEED: f32 = 8.0;

/// Threshold below which animation snaps to target.
const ANIMATION_EPSILON: f32 = 0.005;

/// State for the autonomous AI agent panel overlay.
pub struct AgentPanelState {
    /// Whether the panel should be visible (animation target).
    visible: bool,
    /// Current animation progress (0.0 = hidden, 1.0 = fully visible).
    pub animation_progress: f32,
    /// Animation target (0.0 or 1.0).
    animation_target: f32,
    /// Scroll offset in pixels for the step list.
    pub scroll_offset: f32,
    /// Panel height as fraction of window height.
    pub panel_height_ratio: f32,
    /// Cached hit regions from the last render for mouse handling.
    pub hit_regions: Vec<AgentPanelHitRegion>,
    /// Current text input buffer.
    pub input_buffer: String,
    /// Cursor position within `input_buffer` (byte offset).
    pub input_cursor: usize,
    /// The agent engine managing task lifecycle and state transitions.
    pub engine: AgentEngine,
    /// Dangerous command checker for auto-approval decisions.
    pub checker: DangerousCommandChecker,
    /// Approval mode configured by the user.
    pub approval_mode: minal_config::ApprovalMode,
    /// Timeout in seconds for command execution.
    pub step_timeout_secs: u64,
    /// Current user question pending answer (from AskUser action).
    pub user_question: Option<String>,
    /// Set of expanded step indices (for future toggle UI).
    #[allow(dead_code)]
    pub expanded_steps: HashSet<usize>,
}

impl AgentPanelState {
    /// Creates a new agent panel state from configuration.
    pub fn new(config: &AgentConfig) -> Self {
        Self {
            visible: false,
            animation_progress: 0.0,
            animation_target: 0.0,
            scroll_offset: 0.0,
            panel_height_ratio: config.panel_height_ratio,
            hit_regions: Vec::new(),
            input_buffer: String::new(),
            input_cursor: 0,
            engine: AgentEngine::new(config.max_steps),
            checker: DangerousCommandChecker::new(config.dangerous_commands.clone()),
            approval_mode: config.approval_mode.clone(),
            step_timeout_secs: config.step_timeout_secs,
            user_question: None,
            expanded_steps: HashSet::new(),
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
    ) -> Viewport {
        let available_height = screen_height - top_offset;
        let panel_h = available_height * self.panel_height_ratio * self.animation_progress;
        let y = screen_height - panel_h;
        Viewport {
            x: 0.0,
            y,
            width: screen_width,
            height: panel_h,
        }
    }

    /// Close the panel (sets animation target to hidden).
    pub fn close(&mut self) {
        self.visible = false;
        self.animation_target = 0.0;
    }

    /// Whether the panel is fully hidden (animation complete at 0.0).
    pub fn is_fully_hidden(&self) -> bool {
        !self.visible && self.animation_progress < ANIMATION_EPSILON
    }

    // -- Scroll --

    /// Scroll up by the given number of pixels.
    pub fn scroll_up(&mut self, pixels: f32) {
        self.scroll_offset = (self.scroll_offset + pixels).max(0.0);
    }

    /// Scroll down by the given number of pixels.
    pub fn scroll_down(&mut self, pixels: f32) {
        self.scroll_offset = (self.scroll_offset - pixels).max(0.0);
    }

    // -- Input buffer --

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, ch: char) {
        self.input_buffer.insert(self.input_cursor, ch);
        self.input_cursor += ch.len_utf8();
    }

    /// Delete the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.input_cursor > 0 {
            let prev = self.input_buffer[..self.input_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input_buffer.drain(prev..self.input_cursor);
            self.input_cursor = prev;
        }
    }

    /// Move cursor left one character.
    pub fn move_cursor_left(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor = self.input_buffer[..self.input_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right one character.
    pub fn move_cursor_right(&mut self) {
        if self.input_cursor < self.input_buffer.len() {
            self.input_cursor = self.input_buffer[self.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.input_cursor + i)
                .unwrap_or(self.input_buffer.len());
        }
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

    // -- Agent lifecycle --

    /// Starts a new task. Returns messages to send to the AI for planning.
    pub fn start_task(
        &mut self,
        task: &str,
        context: &minal_ai::AiContext,
    ) -> Vec<minal_ai::Message> {
        self.user_question = None;
        self.scroll_offset = 0.0;
        self.engine.start_task(task, context)
    }

    /// Receives and parses the AI's plan response.
    ///
    /// # Errors
    /// Returns an error string if the response cannot be parsed.
    pub fn receive_plan(&mut self, response: &str) -> Result<(), String> {
        self.engine
            .receive_plan(response)
            .map_err(|e| e.to_string())
    }

    /// Approves the current step and returns the action to execute.
    pub fn approve_current(&mut self) -> Option<AgentAction> {
        let action = self.engine.approve_step()?;
        // If the action is AskUser, capture the question.
        if let AgentAction::AskUser { ref question } = action {
            self.user_question = Some(question.clone());
        } else {
            self.user_question = None;
        }
        Some(action)
    }

    /// Skips the current step.
    pub fn skip_current(&mut self) {
        self.engine.skip_step();
        self.user_question = None;
    }

    /// Cancels the agent task.
    pub fn cancel(&mut self) {
        self.engine.cancel();
        self.user_question = None;
    }

    /// Reports the result of executing a step.
    ///
    /// Returns messages for replanning if the step failed.
    pub fn report_result(&mut self, result: StepResult) -> Option<Vec<minal_ai::Message>> {
        self.user_question = None;
        self.engine.report_step_result(result)
    }

    /// Checks if the current step can be auto-approved given the approval mode.
    pub fn is_auto_approvable(&self) -> bool {
        self.engine
            .is_step_auto_approvable(&self.approval_mode, &self.checker)
    }

    /// Converts the engine's current plan into render-ready steps.
    pub fn render_steps(&self) -> Vec<AgentPanelStep> {
        let plan = match self.engine.plan() {
            Some(p) => p,
            None => return Vec::new(),
        };

        plan.steps
            .iter()
            .map(|step| {
                let is_dangerous = match &step.action {
                    AgentAction::RunCommand { command, .. } => self.checker.is_dangerous(command),
                    _ => false,
                };

                let danger_warning = if is_dangerous {
                    match &step.action {
                        AgentAction::RunCommand { command, .. } => {
                            match self.checker.danger_level(command) {
                                minal_ai::DangerLevel::Dangerous => {
                                    Some("Highly destructive command".to_string())
                                }
                                minal_ai::DangerLevel::Warning => {
                                    Some("Potentially dangerous command".to_string())
                                }
                                minal_ai::DangerLevel::Safe => None,
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                let result_output = step.result.as_ref().map(|r| {
                    if r.output.is_empty() {
                        r.error.clone().unwrap_or_default()
                    } else {
                        r.output.clone()
                    }
                });

                let status = match step.status {
                    minal_ai::StepStatus::Pending => "Pending",
                    minal_ai::StepStatus::AwaitingApproval => "AwaitingApproval",
                    minal_ai::StepStatus::Approved => "Approved",
                    minal_ai::StepStatus::Running => "Running",
                    minal_ai::StepStatus::Completed => "Completed",
                    minal_ai::StepStatus::Failed => "Failed",
                    minal_ai::StepStatus::Skipped => "Skipped",
                }
                .to_string();

                AgentPanelStep {
                    index: step.index,
                    action_type: step.action.action_type().to_string(),
                    description: step.action.description(),
                    status,
                    is_dangerous,
                    result_output,
                    danger_warning,
                }
            })
            .collect()
    }

    /// Returns the human-readable status text for the current engine state.
    pub fn status_text(&self) -> &str {
        match self.engine.state() {
            AgentState::Idle => "Idle",
            AgentState::Planning { .. } => "Planning...",
            AgentState::AwaitingApproval { .. } => "Awaiting Approval",
            AgentState::Executing { .. } => "Executing...",
            AgentState::WaitingForUser { .. } => "Waiting for Input",
            AgentState::Completed { .. } => "Completed",
            AgentState::Failed { .. } => "Failed",
            AgentState::Cancelled => "Cancelled",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AgentConfig {
        AgentConfig::default()
    }

    #[test]
    fn new_panel_is_hidden() {
        let panel = AgentPanelState::new(&test_config());
        assert!(!panel.is_visible());
        assert!(panel.is_fully_hidden());
        assert_eq!(panel.animation_progress, 0.0);
    }

    #[test]
    fn toggle_makes_visible() {
        let mut panel = AgentPanelState::new(&test_config());
        panel.toggle();
        assert!(panel.is_visible());
        assert!(panel.is_animating());
    }

    #[test]
    fn double_toggle_hides() {
        let mut panel = AgentPanelState::new(&test_config());
        panel.toggle();
        panel.toggle();
        assert!(!panel.is_visible());
    }

    #[test]
    fn animation_progresses() {
        let mut panel = AgentPanelState::new(&test_config());
        panel.toggle();
        for _ in 0..100 {
            panel.update_animation(0.016);
        }
        assert!(!panel.is_animating());
        assert!((panel.animation_progress - 1.0).abs() < 0.01);
    }

    #[test]
    fn input_buffer_operations() {
        let mut panel = AgentPanelState::new(&test_config());
        panel.insert_char('h');
        panel.insert_char('i');
        assert_eq!(panel.input_buffer, "hi");
        assert_eq!(panel.input_cursor, 2);

        panel.backspace();
        assert_eq!(panel.input_buffer, "h");
        assert_eq!(panel.input_cursor, 1);

        panel.move_cursor_left();
        assert_eq!(panel.input_cursor, 0);

        panel.insert_char('a');
        assert_eq!(panel.input_buffer, "ah");
    }

    #[test]
    fn take_input_clears() {
        let mut panel = AgentPanelState::new(&test_config());
        panel.insert_char('t');
        panel.insert_char('e');
        panel.insert_char('s');
        panel.insert_char('t');
        let text = panel.take_input();
        assert_eq!(text, Some("test".to_string()));
        assert!(panel.input_buffer.is_empty());
        assert_eq!(panel.input_cursor, 0);
    }

    #[test]
    fn take_input_empty_returns_none() {
        let mut panel = AgentPanelState::new(&test_config());
        assert!(panel.take_input().is_none());
    }

    #[test]
    fn status_text_idle() {
        let panel = AgentPanelState::new(&test_config());
        assert_eq!(panel.status_text(), "Idle");
    }

    #[test]
    fn status_text_planning() {
        let mut panel = AgentPanelState::new(&test_config());
        let context = minal_ai::AiContext::default();
        panel.start_task("do something", &context);
        assert_eq!(panel.status_text(), "Planning...");
    }

    #[test]
    fn render_steps_empty_when_no_plan() {
        let panel = AgentPanelState::new(&test_config());
        assert!(panel.render_steps().is_empty());
    }

    #[test]
    fn render_steps_after_plan() {
        let mut panel = AgentPanelState::new(&test_config());
        let context = minal_ai::AiContext::default();
        panel.start_task("test task", &context);

        let plan_json = r#"{"steps": [
            {"action": "RunCommand", "command": "ls -la"},
            {"action": "Complete", "summary": "Done"}
        ]}"#;
        panel.receive_plan(plan_json).unwrap();

        let steps = panel.render_steps();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].action_type, "RunCommand");
        assert_eq!(steps[1].action_type, "Complete");
    }

    #[test]
    fn panel_viewport_calculation() {
        let mut panel = AgentPanelState::new(&test_config());
        panel.animation_progress = 1.0;
        let vp = panel.panel_viewport(800.0, 600.0, 28.0);
        let config = test_config();
        let expected_height = (600.0 - 28.0) * config.panel_height_ratio;
        assert!((vp.height - expected_height).abs() < 0.1);
        assert_eq!(vp.width, 800.0);
    }

    #[test]
    fn scroll_operations() {
        let mut panel = AgentPanelState::new(&test_config());
        panel.scroll_up(50.0);
        assert_eq!(panel.scroll_offset, 50.0);
        panel.scroll_down(30.0);
        assert_eq!(panel.scroll_offset, 20.0);
        panel.scroll_down(100.0);
        assert_eq!(panel.scroll_offset, 0.0); // Clamp at 0.
    }

    #[test]
    fn auto_approvable_step_mode_returns_false() {
        let mut config = test_config();
        config.approval_mode = minal_config::ApprovalMode::Step;
        let panel = AgentPanelState::new(&config);
        assert!(!panel.is_auto_approvable());
    }

    #[test]
    fn close_hides_panel() {
        let mut panel = AgentPanelState::new(&test_config());
        panel.toggle();
        assert!(panel.is_visible());
        panel.close();
        assert!(!panel.is_visible());
    }
}
