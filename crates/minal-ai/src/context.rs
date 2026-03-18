//! Terminal context gathering for AI completion.

use crate::provider::CompletionContext;
use minal_core::term::Terminal;

/// Gathers context from the terminal for AI completion.
pub struct ContextGatherer {
    /// Number of recent output lines to include.
    pub max_context_lines: usize,
}

impl Default for ContextGatherer {
    fn default() -> Self {
        Self {
            max_context_lines: 20,
        }
    }
}

impl ContextGatherer {
    /// Gather completion context from the terminal state.
    pub fn gather(&self, terminal: &Terminal) -> CompletionContext {
        let input_prefix = terminal.cursor_line_prefix();

        // Read recent output lines from the grid.
        let grid = terminal.grid();
        let cursor_row = terminal.cursor().row;
        let mut recent_output = Vec::new();

        let start_row = cursor_row.saturating_sub(self.max_context_lines);
        for row_idx in start_row..cursor_row {
            if let Some(row) = grid.row(row_idx) {
                let mut line = String::new();
                for col in 0..grid.cols() {
                    if let Some(cell) = row.get(col) {
                        line.push(cell.c);
                    }
                }
                let trimmed = line.trim_end().to_string();
                if !trimmed.is_empty() {
                    recent_output.push(trimmed);
                }
            }
        }

        CompletionContext {
            cwd: None, // CWD detection deferred to OSC 7 / /proc
            input_prefix,
            recent_output,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gather_empty_terminal() {
        let terminal = Terminal::new(24, 80);
        let gatherer = ContextGatherer::default();
        let ctx = gatherer.gather(&terminal);
        assert!(ctx.input_prefix.is_empty());
        assert!(ctx.recent_output.is_empty());
        assert!(ctx.cwd.is_none());
    }

    #[test]
    fn gather_with_input() {
        let mut terminal = Terminal::new(24, 80);
        for c in "git sta".chars() {
            terminal.input_char(c);
        }
        let gatherer = ContextGatherer::default();
        let ctx = gatherer.gather(&terminal);
        assert_eq!(ctx.input_prefix, "git sta");
    }

    #[test]
    fn gather_with_recent_output() {
        let mut terminal = Terminal::new(24, 80);
        // Write some output on row 0
        for c in "file.txt".chars() {
            terminal.input_char(c);
        }
        // Move to row 1
        terminal.linefeed();
        terminal.carriage_return();

        let gatherer = ContextGatherer::default();
        let ctx = gatherer.gather(&terminal);
        assert_eq!(ctx.recent_output.len(), 1);
        assert_eq!(ctx.recent_output[0], "file.txt");
    }
}
