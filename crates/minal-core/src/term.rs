//! Terminal state machine: screen size, modes, attributes.

use crate::cursor::Cursor;
use crate::grid::Grid;

/// The main terminal state.
#[derive(Debug)]
pub struct Terminal {
    /// The character grid.
    pub grid: Grid,
    /// Cursor position and style.
    pub cursor: Cursor,
    /// Number of columns.
    cols: usize,
    /// Number of rows.
    rows: usize,
}

impl Terminal {
    /// Create a new terminal with the given dimensions.
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            grid: Grid::new(rows, cols),
            cursor: Cursor::default(),
            cols,
            rows,
        }
    }

    /// Number of columns.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Number of rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Resize the terminal.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.rows = rows;
        self.cols = cols;
        self.grid.resize(rows, cols);

        // Clamp cursor to new bounds
        if self.cursor.col >= cols {
            self.cursor.col = cols.saturating_sub(1);
        }
        if self.cursor.row >= rows {
            self.cursor.row = rows.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_creation() {
        let term = Terminal::new(24, 80);
        assert_eq!(term.rows(), 24);
        assert_eq!(term.cols(), 80);
        assert_eq!(term.cursor.col, 0);
        assert_eq!(term.cursor.row, 0);
    }

    #[test]
    fn test_terminal_resize() {
        let mut term = Terminal::new(24, 80);
        term.resize(30, 120);
        assert_eq!(term.rows(), 30);
        assert_eq!(term.cols(), 120);
    }
}
