//! Agent engine for autonomous task execution.
//!
//! Manages the agent lifecycle: task decomposition into typed actions,
//! plan parsing from AI responses, dangerous command detection,
//! and state machine transitions through the execution loop.

use serde::{Deserialize, Serialize};

use crate::error::AiError;
use crate::types::{AiContext, Message, Role};
use minal_config::ApprovalMode;

/// Actions the agent can perform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action")]
pub enum AgentAction {
    /// Execute a shell command.
    RunCommand {
        command: String,
        #[serde(default)]
        working_dir: Option<String>,
    },
    /// Write content to a file.
    EditFile {
        path: String,
        content: String,
        description: String,
    },
    /// Read a file's contents.
    ReadFile { path: String },
    /// Ask the user a question.
    AskUser { question: String },
    /// Mark the task as complete.
    Complete { summary: String },
}

impl AgentAction {
    /// Returns a human-readable description of this action.
    pub fn description(&self) -> String {
        match self {
            Self::RunCommand { command, .. } => format!("Run: {command}"),
            Self::EditFile {
                path, description, ..
            } => format!("Edit {path}: {description}"),
            Self::ReadFile { path } => format!("Read: {path}"),
            Self::AskUser { question } => format!("Ask: {question}"),
            Self::Complete { summary } => format!("Complete: {summary}"),
        }
    }

    /// Returns the action type as a string label.
    pub fn action_type(&self) -> &'static str {
        match self {
            Self::RunCommand { .. } => "RunCommand",
            Self::EditFile { .. } => "EditFile",
            Self::ReadFile { .. } => "ReadFile",
            Self::AskUser { .. } => "AskUser",
            Self::Complete { .. } => "Complete",
        }
    }
}

/// Status of a single agent step.
#[derive(Debug, Clone, PartialEq)]
pub enum StepStatus {
    /// Step has not yet been processed.
    Pending,
    /// Step is waiting for user approval.
    AwaitingApproval,
    /// Step has been approved and is ready to run.
    Approved,
    /// Step is currently executing.
    Running,
    /// Step completed successfully.
    Completed,
    /// Step failed.
    Failed,
    /// Step was skipped by the user.
    Skipped,
}

/// Result of executing a step.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Standard output from the step.
    pub output: String,
    /// Exit code, if applicable.
    pub exit_code: Option<i32>,
    /// Error message, if the step failed.
    pub error: Option<String>,
}

/// A single step in the agent's plan.
#[derive(Debug, Clone)]
pub struct AgentStep {
    /// Zero-based index of this step in the plan.
    pub index: usize,
    /// The action to perform.
    pub action: AgentAction,
    /// Current execution status.
    pub status: StepStatus,
    /// Result after execution, if available.
    pub result: Option<StepResult>,
}

/// The agent's execution plan.
#[derive(Debug, Clone)]
pub struct AgentPlan {
    /// Original task description.
    pub task: String,
    /// Ordered list of steps.
    pub steps: Vec<AgentStep>,
    /// Index of the step currently being processed.
    pub current_step: usize,
}

/// Overall agent state.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    /// Agent is idle, no active task.
    Idle,
    /// Agent is planning; waiting for AI response.
    Planning { task: String },
    /// Agent is waiting for the user to approve a step.
    AwaitingApproval { step_index: usize },
    /// Agent is executing a step.
    Executing { step_index: usize },
    /// Agent is waiting for a user answer to a question.
    WaitingForUser { step_index: usize, question: String },
    /// Agent completed the task successfully.
    Completed { summary: String },
    /// Agent encountered an unrecoverable error.
    Failed { error: String },
    /// Agent task was cancelled by the user.
    Cancelled,
}

/// Danger level of a command.
#[derive(Debug, Clone, PartialEq)]
pub enum DangerLevel {
    /// Command is safe to run.
    Safe,
    /// Command may have side effects; warn the user.
    Warning,
    /// Command is highly destructive.
    Dangerous,
}

/// Checks commands against dangerous patterns.
///
/// # Limitation
///
/// Detection is based on substring matching against a configurable list of
/// patterns. A determined attacker can trivially evade this check (e.g., by
/// splitting a command across multiple steps or using shell aliases). This
/// checker should be treated as a best-effort UX guardrail, **not** a security
/// boundary. Final authority over what executes must remain with the user.
pub struct DangerousCommandChecker {
    patterns: Vec<String>,
}

impl DangerousCommandChecker {
    /// Creates a new checker with the given patterns.
    pub fn new(patterns: Vec<String>) -> Self {
        Self { patterns }
    }

    /// Checks if a command matches any dangerous pattern.
    pub fn is_dangerous(&self, command: &str) -> bool {
        let lower = command.to_lowercase();
        self.patterns
            .iter()
            .any(|p| lower.contains(&p.to_lowercase()))
    }

    /// Returns the danger level for a command.
    pub fn danger_level(&self, command: &str) -> DangerLevel {
        if !self.is_dangerous(command) {
            return DangerLevel::Safe;
        }
        let lower = command.to_lowercase();
        // Extra dangerous: rm -rf /, sudo rm, dd if=, mkfs
        if lower.contains("rm -rf /") || lower.contains("dd if=") || lower.contains("mkfs") {
            DangerLevel::Dangerous
        } else {
            DangerLevel::Warning
        }
    }
}

/// Wrapper for the AI response JSON containing steps.
#[derive(Deserialize)]
struct PlanResponse {
    steps: Vec<AgentAction>,
}

/// Parses an AI response into a list of agent actions.
///
/// Expected format: `{"steps": [{"action": "RunCommand", "command": "..."}, ...]}`
pub fn parse_agent_plan(response: &str) -> Result<Vec<AgentAction>, AiError> {
    // Try to find JSON in the response (may be wrapped in markdown)
    let json_str = match extract_json(response) {
        Some(s) => s,
        None => {
            return Err(AiError::Provider(
                "No valid JSON found in agent response".to_string(),
            ));
        }
    };

    let parsed: PlanResponse = serde_json::from_str(json_str)
        .map_err(|e| AiError::Provider(format!("Failed to parse agent plan: {e}")))?;

    if parsed.steps.is_empty() {
        return Err(AiError::Provider("Agent plan has no steps".to_string()));
    }

    Ok(parsed.steps)
}

/// Extracts JSON from a response that may contain markdown code blocks.
fn extract_json(response: &str) -> Option<&str> {
    // Look for ```json ... ``` blocks
    if let Some(start) = response.find("```json") {
        let json_start = start + 7; // Skip "```json"
        if let Some(end) = response[json_start..].find("```") {
            return Some(response[json_start..json_start + end].trim());
        }
    }
    // Look for ``` ... ``` blocks
    if let Some(start) = response.find("```\n{") {
        let json_start = start + 4; // Skip "```\n"
        if let Some(end) = response[json_start..].find("```") {
            return Some(response[json_start..json_start + end].trim());
        }
    }
    // Look for bare JSON object
    if let (Some(start), Some(end)) = (response.find('{'), response.rfind('}')) {
        if start < end {
            return Some(&response[start..=end]);
        }
    }
    None
}

/// The agent system prompt for planning.
const AGENT_SYSTEM_PROMPT: &str = r#"You are an AI agent that executes tasks autonomously in a terminal environment.

Given a task description, create a plan with concrete steps. Respond with a JSON object:

```json
{
  "steps": [
    {"action": "ReadFile", "path": "/path/to/file"},
    {"action": "RunCommand", "command": "cargo build", "working_dir": null},
    {"action": "EditFile", "path": "/path/to/file", "content": "new content", "description": "Update config"},
    {"action": "AskUser", "question": "Which database to use?"},
    {"action": "Complete", "summary": "Task completed successfully"}
  ]
}
```

Rules:
- Always end with a Complete action
- Use ReadFile before EditFile to understand current state
- Keep commands simple and safe
- Ask the user when requirements are ambiguous
- Limit plans to essential steps
"#;

/// Maximum number of replan attempts before the agent gives up.
const MAX_REPLAN_ATTEMPTS: usize = 3;

/// Manages the agent lifecycle and state transitions.
pub struct AgentEngine {
    state: AgentState,
    plan: Option<AgentPlan>,
    max_steps: usize,
    /// Number of replanning attempts triggered by step failures.
    replan_count: usize,
}

impl AgentEngine {
    /// Creates a new agent engine.
    pub fn new(max_steps: usize) -> Self {
        Self {
            state: AgentState::Idle,
            plan: None,
            max_steps,
            replan_count: 0,
        }
    }

    /// Starts a new task. Returns messages to send to the AI for planning.
    pub fn start_task(&mut self, task: &str, context: &AiContext) -> Vec<Message> {
        self.state = AgentState::Planning {
            task: task.to_string(),
        };
        self.plan = None;
        self.replan_count = 0;

        let system = Message {
            role: Role::System,
            content: AGENT_SYSTEM_PROMPT.to_string(),
        };

        let user = Message {
            role: Role::User,
            content: format!(
                "Task: {task}\n\nContext:\n{}",
                context.format_completion_prompt()
            ),
        };

        tracing::info!(task, "Agent task started");
        vec![system, user]
    }

    /// Receives and parses the AI's plan response.
    ///
    /// # Errors
    /// Returns `AiError` if the response cannot be parsed, exceeds `max_steps`,
    /// or is called when the engine is not in the `Planning` state.
    pub fn receive_plan(&mut self, response: &str) -> Result<(), AiError> {
        if !matches!(self.state, AgentState::Planning { .. }) {
            return Err(AiError::Provider(
                "Cannot receive plan: not in Planning state".to_string(),
            ));
        }

        let actions = parse_agent_plan(response)?;

        if actions.len() > self.max_steps {
            return Err(AiError::Provider(format!(
                "Plan has {} steps, maximum is {}",
                actions.len(),
                self.max_steps
            )));
        }

        let steps: Vec<AgentStep> = actions
            .into_iter()
            .enumerate()
            .map(|(i, action)| AgentStep {
                index: i,
                action,
                status: StepStatus::Pending,
                result: None,
            })
            .collect();

        let task = match &self.state {
            AgentState::Planning { task } => task.clone(),
            _ => String::new(),
        };

        self.plan = Some(AgentPlan {
            task,
            steps,
            current_step: 0,
        });

        // Transition to awaiting approval for the first step
        self.advance_to_next_step();

        tracing::info!(
            step_count = self.plan.as_ref().map_or(0, |p| p.steps.len()),
            "Agent plan received"
        );

        Ok(())
    }

    /// Returns the current agent state.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    /// Returns the current plan, if any.
    pub fn plan(&self) -> Option<&AgentPlan> {
        self.plan.as_ref()
    }

    /// Returns the current step, if any.
    pub fn current_step(&self) -> Option<&AgentStep> {
        let plan = self.plan.as_ref()?;
        plan.steps.get(plan.current_step)
    }

    /// Approves the current step and returns the action to execute.
    pub fn approve_step(&mut self) -> Option<AgentAction> {
        let plan = self.plan.as_mut()?;
        let step = plan.steps.get_mut(plan.current_step)?;

        if step.status != StepStatus::AwaitingApproval {
            return None;
        }

        step.status = StepStatus::Approved;
        let action = step.action.clone();

        // For Complete and AskUser, we handle differently
        match &action {
            AgentAction::Complete { summary } => {
                step.status = StepStatus::Completed;
                self.state = AgentState::Completed {
                    summary: summary.clone(),
                };
            }
            AgentAction::AskUser { question } => {
                step.status = StepStatus::Running;
                self.state = AgentState::WaitingForUser {
                    step_index: plan.current_step,
                    question: question.clone(),
                };
            }
            _ => {
                step.status = StepStatus::Running;
                self.state = AgentState::Executing {
                    step_index: plan.current_step,
                };
            }
        }

        tracing::debug!(step = plan.current_step, "Agent step approved");
        Some(action)
    }

    /// Skips the current step.
    pub fn skip_step(&mut self) {
        if let Some(plan) = self.plan.as_mut() {
            if let Some(step) = plan.steps.get_mut(plan.current_step) {
                step.status = StepStatus::Skipped;
                tracing::debug!(step = plan.current_step, "Agent step skipped");
            }
            plan.current_step += 1;
            self.advance_to_next_step();
        }
    }

    /// Reports the result of executing a step. Returns messages for replanning if needed.
    pub fn report_step_result(&mut self, result: StepResult) -> Option<Vec<Message>> {
        // Collect the data we need before taking the mutable borrow, including the
        // task name so we can transition back to Planning state after the borrow ends.
        let (current_step_idx, step_index, step_description, failed, replan_content, task) = {
            let plan = self.plan.as_mut()?;
            let task = plan.task.clone();
            let step = plan.steps.get_mut(plan.current_step)?;

            let failed = result.error.is_some() || result.exit_code.is_some_and(|c| c != 0);

            let replan_content = if failed {
                Some(format!(
                    "Step {} ({}) failed.\nOutput: {}\nError: {}\n\nPlease provide an updated plan to complete the original task. Respond with the same JSON format.",
                    step.index,
                    step.action.description(),
                    result.output,
                    result.error.as_deref().unwrap_or("unknown error")
                ))
            } else {
                None
            };

            let current_step_idx = plan.current_step;
            let step_index = step.index;
            let step_description = step.action.description();

            if failed {
                step.status = StepStatus::Failed;
                tracing::warn!(
                    step = current_step_idx,
                    error = ?result.error,
                    "Agent step failed"
                );
            } else {
                step.status = StepStatus::Completed;
            }
            step.result = Some(result);

            (
                current_step_idx,
                step_index,
                step_description,
                failed,
                replan_content,
                task,
            )
        };

        if failed {
            self.replan_count += 1;
            if self.replan_count >= MAX_REPLAN_ATTEMPTS {
                tracing::error!(
                    replan_count = self.replan_count,
                    "Agent exceeded maximum replan attempts; giving up"
                );
                self.state = AgentState::Failed {
                    error: format!(
                        "Step failed and maximum replan attempts ({MAX_REPLAN_ATTEMPTS}) reached"
                    ),
                };
                return None;
            }

            // Transition back to Planning so the engine is in the correct state
            // when the next `receive_plan` call arrives.
            self.state = AgentState::Planning { task };

            let replan_msg = Message {
                role: Role::User,
                content: replan_content.unwrap_or_default(),
            };
            return Some(vec![
                Message {
                    role: Role::System,
                    content: AGENT_SYSTEM_PROMPT.to_string(),
                },
                replan_msg,
            ]);
        }

        // Advance past the completed step.
        if let Some(plan) = self.plan.as_mut() {
            plan.current_step += 1;
        }
        self.advance_to_next_step();

        tracing::debug!(
            step = current_step_idx,
            index = step_index,
            description = step_description,
            "Agent step completed"
        );
        None
    }

    /// Cancels the agent task.
    pub fn cancel(&mut self) {
        self.state = AgentState::Cancelled;
        tracing::info!("Agent task cancelled");
    }

    /// Checks if the current step can be auto-approved given the approval mode.
    pub fn is_step_auto_approvable(
        &self,
        approval_mode: &ApprovalMode,
        checker: &DangerousCommandChecker,
    ) -> bool {
        match approval_mode {
            ApprovalMode::Step => false,
            ApprovalMode::AutoAll => true,
            ApprovalMode::AutoSafe => {
                if let Some(step) = self.current_step() {
                    match &step.action {
                        AgentAction::RunCommand { command, .. } => !checker.is_dangerous(command),
                        AgentAction::EditFile { .. } => false, // Writing files requires approval
                        AgentAction::ReadFile { .. } => true,
                        AgentAction::AskUser { .. } => false,
                        AgentAction::Complete { .. } => true,
                    }
                } else {
                    false
                }
            }
        }
    }

    /// Advances to the next pending step, or marks complete if all done.
    fn advance_to_next_step(&mut self) {
        let plan = match self.plan.as_ref() {
            Some(p) => p,
            None => return,
        };

        if plan.current_step >= plan.steps.len() {
            self.state = AgentState::Completed {
                summary: "All steps completed".to_string(),
            };
            return;
        }

        let step_index = plan.current_step;
        // Set the step to awaiting approval
        if let Some(plan) = self.plan.as_mut() {
            if let Some(step) = plan.steps.get_mut(step_index) {
                step.status = StepStatus::AwaitingApproval;
            }
        }
        self.state = AgentState::AwaitingApproval { step_index };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dangerous_command_checker_default_patterns() {
        let checker = DangerousCommandChecker::new(vec![
            "rm -rf".to_string(),
            "sudo".to_string(),
            "dd ".to_string(),
            "mkfs".to_string(),
        ]);

        assert!(checker.is_dangerous("rm -rf /tmp/test"));
        assert!(checker.is_dangerous("sudo apt install"));
        assert!(checker.is_dangerous("dd if=/dev/zero"));
        assert!(checker.is_dangerous("mkfs.ext4 /dev/sda1"));
        assert!(!checker.is_dangerous("ls -la"));
        assert!(!checker.is_dangerous("cargo build"));
    }

    #[test]
    fn test_danger_level() {
        let checker = DangerousCommandChecker::new(vec![
            "rm -rf".to_string(),
            "sudo".to_string(),
            "dd ".to_string(),
            "mkfs".to_string(),
        ]);

        assert_eq!(checker.danger_level("ls"), DangerLevel::Safe);
        assert_eq!(
            checker.danger_level("sudo apt install"),
            DangerLevel::Warning
        );
        assert_eq!(checker.danger_level("rm -rf /"), DangerLevel::Dangerous);
        assert_eq!(
            checker.danger_level("dd if=/dev/zero"),
            DangerLevel::Dangerous
        );
    }

    #[test]
    fn test_parse_agent_plan_valid() {
        let response = r#"{"steps": [
            {"action": "RunCommand", "command": "ls -la"},
            {"action": "ReadFile", "path": "/tmp/test"},
            {"action": "Complete", "summary": "Done"}
        ]}"#;
        let actions = parse_agent_plan(response).unwrap();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].action_type(), "RunCommand");
        assert_eq!(actions[1].action_type(), "ReadFile");
        assert_eq!(actions[2].action_type(), "Complete");
    }

    #[test]
    fn test_parse_agent_plan_with_markdown() {
        let response = "Here's my plan:\n```json\n{\"steps\": [{\"action\": \"RunCommand\", \"command\": \"echo hello\"}, {\"action\": \"Complete\", \"summary\": \"Done\"}]}\n```";
        let actions = parse_agent_plan(response).unwrap();
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn test_parse_agent_plan_empty_steps() {
        let response = r#"{"steps": []}"#;
        assert!(parse_agent_plan(response).is_err());
    }

    #[test]
    fn test_parse_agent_plan_invalid_json() {
        let response = "This is not JSON";
        assert!(parse_agent_plan(response).is_err());
    }

    #[test]
    fn test_agent_engine_state_transitions() {
        let mut engine = AgentEngine::new(20);
        assert_eq!(*engine.state(), AgentState::Idle);

        // Start task
        let context = AiContext::default();
        let msgs = engine.start_task("test task", &context);
        assert!(!msgs.is_empty());
        assert!(matches!(engine.state(), AgentState::Planning { .. }));

        // Receive plan
        let plan_response = r#"{"steps": [
            {"action": "RunCommand", "command": "echo hello"},
            {"action": "Complete", "summary": "Done"}
        ]}"#;
        engine.receive_plan(plan_response).unwrap();
        assert!(matches!(
            engine.state(),
            AgentState::AwaitingApproval { step_index: 0 }
        ));

        // Approve first step
        let action = engine.approve_step().unwrap();
        assert!(matches!(action, AgentAction::RunCommand { .. }));
        assert!(matches!(
            engine.state(),
            AgentState::Executing { step_index: 0 }
        ));

        // Report success
        let result = StepResult {
            output: "hello\n".to_string(),
            exit_code: Some(0),
            error: None,
        };
        let replan = engine.report_step_result(result);
        assert!(replan.is_none());

        // Should now be awaiting approval for step 1 (Complete)
        assert!(matches!(
            engine.state(),
            AgentState::AwaitingApproval { step_index: 1 }
        ));

        // Approve Complete step
        let action = engine.approve_step().unwrap();
        assert!(matches!(action, AgentAction::Complete { .. }));
        assert!(matches!(engine.state(), AgentState::Completed { .. }));
    }

    #[test]
    fn test_agent_engine_skip_step() {
        let mut engine = AgentEngine::new(20);
        let context = AiContext::default();
        engine.start_task("test", &context);

        let plan_response = r#"{"steps": [
            {"action": "RunCommand", "command": "echo hello"},
            {"action": "Complete", "summary": "Done"}
        ]}"#;
        engine.receive_plan(plan_response).unwrap();

        engine.skip_step();
        assert!(matches!(
            engine.state(),
            AgentState::AwaitingApproval { step_index: 1 }
        ));
    }

    #[test]
    fn test_agent_engine_cancel() {
        let mut engine = AgentEngine::new(20);
        let context = AiContext::default();
        engine.start_task("test", &context);
        engine.cancel();
        assert_eq!(*engine.state(), AgentState::Cancelled);
    }

    #[test]
    fn test_agent_engine_failed_step_triggers_replan() {
        let mut engine = AgentEngine::new(20);
        let context = AiContext::default();
        engine.start_task("test", &context);

        let plan_response = r#"{"steps": [
            {"action": "RunCommand", "command": "cargo build"},
            {"action": "Complete", "summary": "Done"}
        ]}"#;
        engine.receive_plan(plan_response).unwrap();
        engine.approve_step();

        let result = StepResult {
            output: String::new(),
            exit_code: Some(1),
            error: Some("build failed".to_string()),
        };
        let replan = engine.report_step_result(result);
        assert!(replan.is_some()); // Should request replanning
    }

    #[test]
    fn test_auto_approval_step_mode() {
        let engine = AgentEngine::new(20);
        let checker = DangerousCommandChecker::new(vec!["rm -rf".to_string()]);
        assert!(!engine.is_step_auto_approvable(&ApprovalMode::Step, &checker));
    }

    #[test]
    fn test_auto_approval_auto_safe() {
        let mut engine = AgentEngine::new(20);
        let checker = DangerousCommandChecker::new(vec!["rm -rf".to_string()]);
        let context = AiContext::default();
        engine.start_task("test", &context);

        // Safe command
        let plan_response = r#"{"steps": [{"action": "RunCommand", "command": "ls"}, {"action": "Complete", "summary": "Done"}]}"#;
        engine.receive_plan(plan_response).unwrap();
        assert!(engine.is_step_auto_approvable(&ApprovalMode::AutoSafe, &checker));
    }

    #[test]
    fn test_auto_approval_auto_safe_blocks_dangerous() {
        let mut engine = AgentEngine::new(20);
        let checker = DangerousCommandChecker::new(vec!["rm -rf".to_string()]);
        let context = AiContext::default();
        engine.start_task("test", &context);

        let plan_response = r#"{"steps": [{"action": "RunCommand", "command": "rm -rf /tmp"}, {"action": "Complete", "summary": "Done"}]}"#;
        engine.receive_plan(plan_response).unwrap();
        assert!(!engine.is_step_auto_approvable(&ApprovalMode::AutoSafe, &checker));
    }

    #[test]
    fn test_auto_approval_auto_all() {
        let mut engine = AgentEngine::new(20);
        let checker = DangerousCommandChecker::new(vec!["rm -rf".to_string()]);
        let context = AiContext::default();
        engine.start_task("test", &context);

        let plan_response = r#"{"steps": [{"action": "RunCommand", "command": "rm -rf /tmp"}, {"action": "Complete", "summary": "Done"}]}"#;
        engine.receive_plan(plan_response).unwrap();
        assert!(engine.is_step_auto_approvable(&ApprovalMode::AutoAll, &checker));
    }

    #[test]
    fn test_agent_action_description() {
        let action = AgentAction::RunCommand {
            command: "ls -la".to_string(),
            working_dir: None,
        };
        assert_eq!(action.description(), "Run: ls -la");
        assert_eq!(action.action_type(), "RunCommand");
    }

    #[test]
    fn test_plan_too_many_steps() {
        let mut engine = AgentEngine::new(2);
        let context = AiContext::default();
        engine.start_task("test", &context);

        let plan_response = r#"{"steps": [
            {"action": "RunCommand", "command": "a"},
            {"action": "RunCommand", "command": "b"},
            {"action": "RunCommand", "command": "c"},
            {"action": "Complete", "summary": "Done"}
        ]}"#;
        assert!(engine.receive_plan(plan_response).is_err());
    }

    #[test]
    fn test_agent_action_serde() {
        let action = AgentAction::RunCommand {
            command: "ls".to_string(),
            working_dir: Some("/tmp".to_string()),
        };
        let json = serde_json::to_string(&action).unwrap();
        let parsed: AgentAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, parsed);
    }

    #[test]
    fn test_ask_user_action() {
        let mut engine = AgentEngine::new(20);
        let context = AiContext::default();
        engine.start_task("test", &context);

        let plan_response = r#"{"steps": [
            {"action": "AskUser", "question": "Which port?"},
            {"action": "Complete", "summary": "Done"}
        ]}"#;
        engine.receive_plan(plan_response).unwrap();
        let action = engine.approve_step().unwrap();
        assert!(matches!(action, AgentAction::AskUser { .. }));
        assert!(matches!(engine.state(), AgentState::WaitingForUser { .. }));
    }
}
