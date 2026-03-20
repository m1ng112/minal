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
    /// AI completion request with context.
    AiComplete {
        /// Text the user has typed so far on the current line.
        prefix: String,
        /// Recent terminal output lines for context.
        recent_output: Vec<String>,
    },
    /// Clean shutdown request.
    Shutdown,
}

/// Reasons for the I/O thread to wake the main thread via `EventLoopProxy`.
///
/// All pane-specific events carry a [`PaneId`] so the main thread can route
/// the event to the correct pane.
#[derive(Debug, Clone)]
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
}
