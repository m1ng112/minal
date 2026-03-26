//! Lock-free terminal state snapshot for the renderer.
//!
//! A [`TerminalSnapshot`] captures the subset of terminal state needed for
//! rendering. The I/O thread creates a snapshot after each batch parse and
//! publishes it via an atomic swap (e.g. `arc_swap::ArcSwap`). The render
//! path reads the latest snapshot without locking.

use crate::cursor::Cursor;
use crate::grid::Grid;
use crate::selection::Selection;
use crate::term::{GhostText, Terminal};

/// An immutable snapshot of the terminal state needed for rendering.
#[derive(Debug, Clone)]
pub struct TerminalSnapshot {
    /// The terminal character grid.
    pub grid: Grid,
    /// Cursor state.
    pub cursor: Cursor,
    /// AI ghost text overlay, if any.
    pub ghost_text: Option<GhostText>,
    /// Current text selection, if any.
    pub selection: Option<Selection>,
    /// Generation counter from the terminal.
    pub generation: u64,
}

impl TerminalSnapshot {
    /// Creates a snapshot from the current terminal state.
    pub fn from_terminal(term: &Terminal) -> Self {
        Self {
            grid: term.grid().clone(),
            cursor: term.cursor().clone(),
            ghost_text: term.ghost_text().cloned(),
            selection: term.selection().cloned(),
            generation: term.generation(),
        }
    }
}
