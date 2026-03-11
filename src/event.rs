//! Event types for inter-thread communication.
//!
//! Defines the messages passed between the main (winit) thread and the
//! I/O thread via crossbeam channels.

/// Actions sent from the main thread to the I/O thread.
#[derive(Debug)]
#[allow(dead_code)]
pub enum IoAction {
    /// Write bytes to the PTY (keyboard input).
    PtyWrite(Vec<u8>),
    /// Resize the PTY and terminal.
    Resize {
        /// New row count.
        rows: u16,
        /// New column count.
        cols: u16,
    },
    /// Shut down the I/O thread.
    Shutdown,
}

/// Events sent from the I/O thread to the main thread.
#[derive(Debug)]
#[allow(dead_code)]
pub enum MainEvent {
    /// Terminal content has changed, request redraw.
    Redraw,
    /// Child process has exited.
    ChildExited(Option<i32>),
    /// Terminal title has changed.
    TitleChanged(String),
}
