//! Inter-thread event types for the 3-thread architecture.
//!
//! Defines the message types flowing between threads:
//! - [`IoEvent`]: Main thread -> I/O thread (via crossbeam channel)
//! - [`WakeupReason`]: I/O thread -> Main thread (via winit EventLoopProxy)

use minal_core::pty::PtySize;

/// Events sent from the Main thread to the I/O thread via crossbeam channel.
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
#[derive(Debug, Clone)]
pub enum WakeupReason {
    /// Terminal state was updated; request a redraw.
    TerminalUpdated,
    /// Child process exited with the given code.
    ChildExited(i32),
    /// AI completion result is ready.
    CompletionReady(String),
    /// AI completion request failed.
    CompletionFailed,
    /// Theme configuration was changed (hot-reload).
    ThemeChanged(Box<minal_config::ThemeConfig>),
}
