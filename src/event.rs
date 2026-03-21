//! Inter-thread event types for the 3-thread architecture.
//!
//! Defines the message types flowing between threads:
//! - [`IoEvent`]: Main thread -> I/O thread (via crossbeam channel, per-pane)
//! - [`WakeupReason`]: I/O thread -> Main thread (via winit EventLoopProxy)

use minal_core::pty::PtySize;

use crate::pane::PaneId;

/// Events sent from the Main thread to the I/O thread via crossbeam channel.
///
/// Each pane has its own channel, so events are implicitly scoped to a pane.
#[derive(Debug)]
pub enum IoEvent {
    /// Keyboard input bytes to write to PTY.
    Input(Vec<u8>),
    /// Terminal resize notification.
    Resize(PtySize),
    /// AI completion request with full context.
    AiComplete {
        /// Full AI context including CWD, git, shell, OS, and command history.
        context: minal_ai::AiContext,
    },
    /// AI chat request with conversation messages.
    AiChat {
        /// Conversation messages.
        messages: Vec<minal_ai::Message>,
        /// Terminal context.
        context: minal_ai::AiContext,
    },
    /// AI error analysis request.
    // Phase 3 UI: not yet wired to a key binding but handled by the I/O loop.
    #[expect(dead_code)]
    AiAnalyze {
        /// Error context to analyze.
        error: minal_ai::ErrorContext,
    },
    /// Clean shutdown request.
    Shutdown,
}

/// Actions that can be triggered from the macOS native menu bar.
///
/// These are sent as [`WakeupReason::MenuAction`] to allow future integration
/// where AppKit menu actions wake the winit event loop.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Variants are reserved for future menu-bar integration.
pub enum MenuAction {
    /// User chose "New Tab" from the menu.
    NewTab,
    /// User chose "Close Tab" from the menu.
    CloseTab,
    /// User chose "About Minal" from the menu.
    About,
}

/// Reasons for the I/O thread to wake the main thread via `EventLoopProxy`.
///
/// All pane-specific events carry a [`PaneId`] so the main thread can route
/// the event to the correct pane.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some variants are reserved for future integration.
pub enum WakeupReason {
    /// Terminal state was updated in the given pane; request a redraw.
    PaneUpdated(PaneId),
    /// Child process in the given pane exited with the given code.
    PaneExited(PaneId, i32),
    /// AI completion result is ready for the given pane.
    PaneCompletionReady(PaneId, String),
    /// AI completion request failed for the given pane.
    PaneCompletionFailed(PaneId),
    /// Theme configuration was changed (hot-reload, global).
    ThemeChanged(Box<minal_config::ThemeConfig>),
    /// An escape sequence requested a clipboard write (OSC 52) in the given pane.
    PaneClipboardSet(PaneId, String),
    /// An escape sequence requested a clipboard read (OSC 52) in the given pane.
    PaneClipboardGet(PaneId),
    /// A streaming chat token arrived.
    PaneChatChunk(PaneId, String),
    /// Chat stream completed.
    PaneChatDone(PaneId),
    /// Chat stream error.
    PaneChatError(PaneId, String),
    /// Error analysis result ready.
    PaneAnalysisReady(PaneId, minal_ai::ErrorAnalysis),
    /// A shell command completed (OSC 133;D) with structured record.
    PaneCommandCompleted(PaneId, minal_core::shell_integration::ShellCommandRecord),
    /// A new prompt started (OSC 133;A) — triggers context prefetch.
    PanePromptStarted(PaneId),
    /// AI provider status notification (for status bar display).
    AiProviderStatus(PaneId, String),
    /// A macOS menu bar action was triggered.
    MenuAction(MenuAction),
}
