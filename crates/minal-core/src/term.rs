//! Terminal state machine: screen size, modes, attributes.

use std::collections::HashSet;

use crate::ansi::Mode;
use crate::charset::CharsetTable;
use crate::cursor::Cursor;
use crate::grid::Grid;
use crate::scrollback::Scrollback;
use crate::selection::Selection;

/// Default scrollback history size (number of rows).
const DEFAULT_SCROLLBACK_SIZE: usize = 10_000;

/// Default tab stop interval (every 8 columns).
const DEFAULT_TAB_INTERVAL: usize = 8;

/// The main terminal state.
#[derive(Debug)]
pub struct Terminal {
    /// The primary character grid.
    grid: Grid,
    /// The alternate screen buffer.
    alt_grid: Grid,
    /// Whether the alternate screen is active.
    alt_screen_active: bool,

    /// Cursor for primary screen.
    cursor: Cursor,
    /// Saved cursor for alternate screen.
    alt_cursor: Cursor,

    /// Active terminal modes.
    modes: HashSet<Mode>,

    /// Tab stop positions (column indices).
    tab_stops: Vec<bool>,

    /// Scroll region top (inclusive, 0-indexed).
    scroll_region_top: usize,
    /// Scroll region bottom (exclusive, 0-indexed).
    scroll_region_bottom: usize,

    /// Scrollback history buffer.
    scrollback: Scrollback,
    /// Current scroll display offset (0 = bottom, positive = scrolled up).
    scroll_offset: usize,

    /// Character set table.
    charset: CharsetTable,

    /// Current text selection.
    selection: Option<Selection>,

    /// Terminal title (set via OSC 0/2).
    title: String,

    /// Whether the terminal content has changed since last read.
    dirty: bool,
}

impl Terminal {
    /// Create a new terminal with the given dimensions.
    pub fn new(rows: usize, cols: usize) -> Self {
        let mut modes = HashSet::new();
        modes.insert(Mode::AutoWrap);
        modes.insert(Mode::ShowCursor);

        let mut tab_stops = vec![false; cols];
        for i in (0..cols).step_by(DEFAULT_TAB_INTERVAL) {
            tab_stops[i] = true;
        }

        Self {
            grid: Grid::new(rows, cols),
            alt_grid: Grid::new(rows, cols),
            alt_screen_active: false,
            cursor: Cursor::default(),
            alt_cursor: Cursor::default(),
            modes,
            tab_stops,
            scroll_region_top: 0,
            scroll_region_bottom: rows,
            scrollback: Scrollback::new(DEFAULT_SCROLLBACK_SIZE),
            scroll_offset: 0,
            charset: CharsetTable::default(),
            selection: None,
            title: String::new(),
            dirty: true,
        }
    }

    // ─── Dimensions ─────────────────────────────────────────────

    /// Number of columns.
    pub fn cols(&self) -> usize {
        self.grid.cols()
    }

    /// Number of rows.
    pub fn rows(&self) -> usize {
        self.grid.rows()
    }

    // ─── Grid access ────────────────────────────────────────────

    /// Immutable access to the active grid.
    pub fn grid(&self) -> &Grid {
        if self.alt_screen_active {
            &self.alt_grid
        } else {
            &self.grid
        }
    }

    /// Mutable access to the active grid.
    pub fn grid_mut(&mut self) -> &mut Grid {
        self.dirty = true;
        if self.alt_screen_active {
            &mut self.alt_grid
        } else {
            &mut self.grid
        }
    }

    // ─── Cursor access ─────────────────────────────────────────

    /// Immutable access to the cursor.
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// Mutable access to the cursor.
    pub fn cursor_mut(&mut self) -> &mut Cursor {
        &mut self.cursor
    }

    // ─── Mode management ────────────────────────────────────────

    /// Check if a mode is enabled.
    pub fn mode(&self, mode: Mode) -> bool {
        self.modes.contains(&mode)
    }

    /// Enable a mode.
    pub fn set_mode(&mut self, mode: Mode) {
        if mode == Mode::AlternateScreen {
            self.enter_alternate_screen();
        }
        self.modes.insert(mode);
        self.dirty = true;
    }

    /// Disable a mode.
    pub fn unset_mode(&mut self, mode: Mode) {
        if mode == Mode::AlternateScreen {
            self.leave_alternate_screen();
        }
        self.modes.remove(&mode);
        self.dirty = true;
    }

    // ─── Alternate screen buffer ────────────────────────────────

    /// Whether the alternate screen is active.
    pub fn alt_screen_active(&self) -> bool {
        self.alt_screen_active
    }

    /// Enter the alternate screen buffer.
    fn enter_alternate_screen(&mut self) {
        if self.alt_screen_active {
            return;
        }
        self.cursor.save(); // save first, so the clone captures the saved state
        self.alt_cursor = self.cursor.clone();
        self.alt_screen_active = true;
        self.alt_grid.clear();
    }

    /// Leave the alternate screen buffer.
    fn leave_alternate_screen(&mut self) {
        if !self.alt_screen_active {
            return;
        }
        self.alt_screen_active = false;
        std::mem::swap(&mut self.cursor, &mut self.alt_cursor);
        let rows = self.grid.rows();
        let cols = self.grid.cols();
        self.cursor.restore(rows, cols);
    }

    // ─── Scroll region ──────────────────────────────────────────

    /// Set the scroll region (1-indexed top and bottom, inclusive).
    /// Resets cursor to home position.
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let max_rows = self.grid().rows();
        let top = top.saturating_sub(1); // convert to 0-indexed
        let bottom = bottom.min(max_rows); // exclusive

        if top < bottom {
            self.scroll_region_top = top;
            self.scroll_region_bottom = bottom;
            self.cursor.goto(0, 0, max_rows, self.grid().cols());
        }
    }

    /// Get the scroll region as `(top_inclusive, bottom_exclusive)`.
    pub fn scroll_region(&self) -> (usize, usize) {
        (self.scroll_region_top, self.scroll_region_bottom)
    }

    // ─── Tab stops ──────────────────────────────────────────────

    /// Set a tab stop at the current cursor column.
    pub fn set_tab_stop(&mut self) {
        let col = self.cursor.col;
        if col < self.tab_stops.len() {
            self.tab_stops[col] = true;
        }
    }

    /// Clear the tab stop at the current cursor column.
    pub fn clear_tab_stop(&mut self) {
        let col = self.cursor.col;
        if col < self.tab_stops.len() {
            self.tab_stops[col] = false;
        }
    }

    /// Clear all tab stops.
    pub fn clear_all_tab_stops(&mut self) {
        self.tab_stops.fill(false);
    }

    /// Move cursor to the next tab stop.
    pub fn tab(&mut self) {
        let cols = self.grid().cols();
        let mut col = self.cursor.col + 1;
        while col < cols {
            if col < self.tab_stops.len() && self.tab_stops[col] {
                break;
            }
            col += 1;
        }
        self.cursor.col = col.min(cols.saturating_sub(1));
        self.cursor.pending_wrap = false;
    }

    /// Move cursor to the previous tab stop.
    pub fn reverse_tab(&mut self) {
        if self.cursor.col == 0 {
            return;
        }
        let mut col = self.cursor.col - 1;
        loop {
            if col < self.tab_stops.len() && self.tab_stops[col] {
                break;
            }
            if col == 0 {
                break;
            }
            col -= 1;
        }
        self.cursor.col = col;
        self.cursor.pending_wrap = false;
    }

    // ─── Scrollback ─────────────────────────────────────────────

    /// Immutable access to the scrollback buffer.
    pub fn scrollback(&self) -> &Scrollback {
        &self.scrollback
    }

    /// Current scroll display offset.
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Scroll up (view older content).
    pub fn scroll_display_up(&mut self, count: usize) {
        let max = self.scrollback.len();
        self.scroll_offset = (self.scroll_offset + count).min(max);
    }

    /// Scroll down (view newer content).
    pub fn scroll_display_down(&mut self, count: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(count);
    }

    /// Reset scroll to bottom.
    pub fn scroll_display_reset(&mut self) {
        self.scroll_offset = 0;
    }

    // ─── Character set ──────────────────────────────────────────

    /// Access the charset table.
    pub fn charset(&self) -> &CharsetTable {
        &self.charset
    }

    /// Mutable access to the charset table.
    pub fn charset_mut(&mut self) -> &mut CharsetTable {
        &mut self.charset
    }

    // ─── Selection ──────────────────────────────────────────────

    /// Get the current selection.
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Set the selection.
    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    // ─── Title ──────────────────────────────────────────────────

    /// Get the terminal title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Set the terminal title.
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    // ─── Dirty flag ─────────────────────────────────────────────

    /// Whether the terminal has been modified.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear the dirty flag.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    // ─── Text input operations ──────────────────────────────────

    /// Write a character at the current cursor position.
    pub fn input_char(&mut self, c: char) {
        let cols = self.grid().cols();

        // Handle pending wrap
        if self.cursor.pending_wrap && self.mode(Mode::AutoWrap) {
            self.cursor.col = 0;
            if self.cursor.row + 1 >= self.scroll_region_bottom {
                self.scroll_up(1);
            } else {
                self.cursor.row += 1;
            }
            self.cursor.pending_wrap = false;
        }

        // Map through active charset
        let mapped = self.charset.map(c);

        // Write character to grid
        let cursor_row = self.cursor.row;
        let cursor_col = self.cursor.col;
        let fg = self.cursor.fg;
        let bg = self.cursor.bg;
        let attrs = self.cursor.attrs;
        if let Some(cell) = self.grid_mut().cell_mut(cursor_row, cursor_col) {
            cell.c = mapped;
            cell.fg = fg;
            cell.bg = bg;
            cell.attrs = attrs;
        }

        // Advance cursor
        if self.cursor.col + 1 >= cols {
            if self.mode(Mode::AutoWrap) {
                self.cursor.pending_wrap = true;
            }
            // Cursor stays at last column if no auto-wrap
        } else {
            self.cursor.col += 1;
        }

        self.dirty = true;
    }

    /// Carriage return: move cursor to column 0.
    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
        self.cursor.pending_wrap = false;
    }

    /// Line feed: move cursor down, scrolling if needed.
    pub fn linefeed(&mut self) {
        if self.cursor.row + 1 >= self.scroll_region_bottom {
            self.scroll_up(1);
        } else {
            self.cursor.row += 1;
        }
        self.cursor.pending_wrap = false;

        // In LineFeedNewLine mode, also do carriage return
        if self.mode(Mode::LineFeedNewLine) {
            self.carriage_return();
        }
        self.dirty = true;
    }

    /// Reverse index: move cursor up, scrolling down if at top of region.
    pub fn reverse_index(&mut self) {
        if self.cursor.row == self.scroll_region_top {
            self.scroll_down(1);
        } else {
            self.cursor.row = self.cursor.row.saturating_sub(1);
        }
        self.cursor.pending_wrap = false;
        self.dirty = true;
    }

    /// Backspace: move cursor left by one column.
    pub fn backspace(&mut self) {
        self.cursor.move_left(1);
    }

    // ─── Scroll operations ──────────────────────────────────────

    /// Scroll the scroll region up by `count` lines.
    fn scroll_up(&mut self, count: usize) {
        let top = self.scroll_region_top;
        let bottom = self.scroll_region_bottom;

        // Only save to scrollback when scrolling the entire screen from top
        if !self.alt_screen_active && top == 0 {
            let grid = &mut self.grid;
            let scrolled = grid.scroll_up(top, bottom, count);
            self.scrollback.push_rows(scrolled);
        } else {
            self.grid_mut().scroll_up(top, bottom, count);
        }
        self.dirty = true;
    }

    /// Scroll the scroll region down by `count` lines.
    fn scroll_down(&mut self, count: usize) {
        let top = self.scroll_region_top;
        let bottom = self.scroll_region_bottom;
        self.grid_mut().scroll_down(top, bottom, count);
        self.dirty = true;
    }

    // ─── Erase operations ───────────────────────────────────────

    /// Erase from cursor to end of line.
    pub fn erase_line_right(&mut self) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        if let Some(r) = self.grid_mut().row_mut(row) {
            r.clear_from(col);
        }
    }

    /// Erase from start of line to cursor.
    pub fn erase_line_left(&mut self) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        if let Some(r) = self.grid_mut().row_mut(row) {
            r.clear_to(col);
        }
    }

    /// Erase entire current line.
    pub fn erase_line_all(&mut self) {
        let row = self.cursor.row;
        if let Some(r) = self.grid_mut().row_mut(row) {
            r.clear();
        }
    }

    /// Erase from cursor to end of screen.
    pub fn erase_display_below(&mut self) {
        self.erase_line_right();
        let rows = self.grid().rows();
        let cursor_row = self.cursor.row;
        for r in (cursor_row + 1)..rows {
            if let Some(row) = self.grid_mut().row_mut(r) {
                row.clear();
            }
        }
    }

    /// Erase from start of screen to cursor.
    pub fn erase_display_above(&mut self) {
        self.erase_line_left();
        let cursor_row = self.cursor.row;
        for r in 0..cursor_row {
            if let Some(row) = self.grid_mut().row_mut(r) {
                row.clear();
            }
        }
    }

    /// Erase entire screen.
    pub fn erase_display_all(&mut self) {
        self.grid_mut().clear();
        self.dirty = true;
    }

    // ─── Insert/Delete ──────────────────────────────────────────

    /// Insert blank characters at cursor position.
    pub fn insert_blank_chars(&mut self, count: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        if let Some(r) = self.grid_mut().row_mut(row) {
            r.insert_cells(col, count);
        }
    }

    /// Delete characters at cursor position.
    pub fn delete_chars(&mut self, count: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        if let Some(r) = self.grid_mut().row_mut(row) {
            r.delete_cells(col, count);
        }
    }

    /// Insert blank lines at cursor row.
    pub fn insert_blank_lines(&mut self, count: usize) {
        let row = self.cursor.row;
        let bottom = self.scroll_region_bottom;
        self.grid_mut().insert_lines(row, bottom, count);
    }

    /// Delete lines at cursor row.
    pub fn delete_lines(&mut self, count: usize) {
        let row = self.cursor.row;
        let bottom = self.scroll_region_bottom;
        self.grid_mut().delete_lines(row, bottom, count);
    }

    // ─── Resize ─────────────────────────────────────────────────

    /// Resize the terminal.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let old_rows = self.grid.rows();
        self.grid.resize(rows, cols);
        self.alt_grid.resize(rows, cols);

        // Update scroll region: if it covered the full screen, keep it full;
        // otherwise clamp to not exceed new row count
        let is_full_screen = self.scroll_region_top == 0 && self.scroll_region_bottom >= old_rows;
        if is_full_screen || self.scroll_region_bottom > rows {
            self.scroll_region_bottom = rows;
        }
        if self.scroll_region_top >= rows {
            self.scroll_region_top = 0;
        }

        // Update tab stops
        self.tab_stops.resize(cols, false);
        for i in (0..cols).step_by(DEFAULT_TAB_INTERVAL) {
            if !self.tab_stops[i] {
                self.tab_stops[i] = true;
            }
        }

        // Clamp cursor to new bounds
        if self.cursor.col >= cols {
            self.cursor.col = cols.saturating_sub(1);
        }
        if self.cursor.row >= rows {
            self.cursor.row = rows.saturating_sub(1);
        }

        // Clear selection on resize
        self.selection = None;
        self.dirty = true;
    }

    // ─── Reset ──────────────────────────────────────────────────

    /// Full reset of terminal state (RIS).
    pub fn reset(&mut self) {
        let rows = self.grid.rows();
        let cols = self.grid.cols();

        self.grid.clear();
        self.alt_grid.clear();
        self.alt_screen_active = false;
        self.cursor.reset();
        self.alt_cursor.reset();
        self.modes.clear();
        self.modes.insert(Mode::AutoWrap);
        self.modes.insert(Mode::ShowCursor);

        self.tab_stops.fill(false);
        for i in (0..cols).step_by(DEFAULT_TAB_INTERVAL) {
            self.tab_stops[i] = true;
        }

        self.scroll_region_top = 0;
        self.scroll_region_bottom = rows;
        self.scrollback.clear();
        self.scroll_offset = 0;
        self.charset.reset();
        self.selection = None;
        self.title.clear();
        self.dirty = true;
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
        assert_eq!(term.cursor().col, 0);
        assert_eq!(term.cursor().row, 0);
    }

    #[test]
    fn test_terminal_resize() {
        let mut term = Terminal::new(24, 80);
        term.resize(30, 120);
        assert_eq!(term.rows(), 30);
        assert_eq!(term.cols(), 120);
    }

    #[test]
    fn test_input_char() {
        let mut term = Terminal::new(24, 80);
        term.input_char('A');
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'A');
        assert_eq!(term.cursor().col, 1);
    }

    #[test]
    fn test_input_chars_line() {
        let mut term = Terminal::new(24, 80);
        for c in "Hello".chars() {
            term.input_char(c);
        }
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'H');
        assert_eq!(term.grid().cell(0, 4).unwrap().c, 'o');
        assert_eq!(term.cursor().col, 5);
    }

    #[test]
    fn test_auto_wrap() {
        let mut term = Terminal::new(24, 5);
        for c in "ABCDE".chars() {
            term.input_char(c);
        }
        // Cursor should be at col 4 with pending_wrap
        assert!(term.cursor().pending_wrap);
        assert_eq!(term.cursor().col, 4);

        // Next char wraps to next line
        term.input_char('F');
        assert_eq!(term.cursor().row, 1);
        assert_eq!(term.cursor().col, 1);
        assert_eq!(term.grid().cell(1, 0).unwrap().c, 'F');
    }

    #[test]
    fn test_linefeed() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().row = 5;
        term.linefeed();
        assert_eq!(term.cursor().row, 6);
    }

    #[test]
    fn test_linefeed_scroll() {
        let mut term = Terminal::new(3, 10);
        term.input_char('A');
        term.cursor_mut().row = 2;
        term.linefeed();
        // Should have scrolled, cursor stays at row 2
        assert_eq!(term.cursor().row, 2);
        // 'A' should have scrolled into scrollback
        assert_eq!(term.scrollback().len(), 1);
    }

    #[test]
    fn test_carriage_return() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().col = 40;
        term.carriage_return();
        assert_eq!(term.cursor().col, 0);
    }

    #[test]
    fn test_backspace() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().col = 5;
        term.backspace();
        assert_eq!(term.cursor().col, 4);
    }

    #[test]
    fn test_backspace_at_zero() {
        let mut term = Terminal::new(24, 80);
        term.backspace();
        assert_eq!(term.cursor().col, 0);
    }

    #[test]
    fn test_tab() {
        let mut term = Terminal::new(24, 80);
        term.tab();
        assert_eq!(term.cursor().col, 8);
        term.tab();
        assert_eq!(term.cursor().col, 16);
    }

    #[test]
    fn test_reverse_tab() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().col = 20;
        term.reverse_tab();
        assert_eq!(term.cursor().col, 16);
    }

    #[test]
    fn test_alternate_screen() {
        let mut term = Terminal::new(24, 80);
        term.input_char('A');
        // cursor is now at col 1

        term.set_mode(Mode::AlternateScreen);
        assert!(term.alt_screen_active());
        // Alt screen should be clear
        assert_eq!(term.grid().cell(0, 0).unwrap().c, ' ');

        // Cursor carries over; write 'B' at current col (1)
        term.input_char('B');
        assert_eq!(term.grid().cell(0, 1).unwrap().c, 'B');

        term.unset_mode(Mode::AlternateScreen);
        assert!(!term.alt_screen_active());
        // Primary screen should still have 'A'
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'A');
        // Cursor should be restored to where it was before entering alt screen
        assert_eq!(term.cursor().col, 1);
        assert_eq!(term.cursor().row, 0);
    }

    #[test]
    fn test_scroll_region() {
        let mut term = Terminal::new(10, 80);
        term.set_scroll_region(3, 7); // rows 3-7 (1-indexed)
        assert_eq!(term.scroll_region(), (2, 7)); // 0-indexed: top=2, bottom=7
    }

    #[test]
    fn test_erase_line_right() {
        let mut term = Terminal::new(24, 80);
        for c in "Hello, World!".chars() {
            term.input_char(c);
        }
        term.cursor_mut().col = 5;
        term.erase_line_right();
        assert_eq!(term.grid().cell(0, 4).unwrap().c, 'o');
        assert_eq!(term.grid().cell(0, 5).unwrap().c, ' ');
        assert_eq!(term.grid().cell(0, 6).unwrap().c, ' ');
    }

    #[test]
    fn test_erase_line_left() {
        let mut term = Terminal::new(24, 80);
        for c in "Hello, World!".chars() {
            term.input_char(c);
        }
        // "Hello, World!" -> H(0) e(1) l(2) l(3) o(4) ,(5) ' '(6) W(7) ...
        term.cursor_mut().col = 5;
        term.erase_line_left();
        // Cols 0-5 should be cleared
        assert_eq!(term.grid().cell(0, 5).unwrap().c, ' ');
        assert_eq!(term.grid().cell(0, 4).unwrap().c, ' ');
        // Col 6 (' ') and col 7 ('W') should be untouched
        assert_eq!(term.grid().cell(0, 7).unwrap().c, 'W');
    }

    #[test]
    fn test_erase_display_all() {
        let mut term = Terminal::new(24, 80);
        term.input_char('X');
        term.erase_display_all();
        assert_eq!(term.grid().cell(0, 0).unwrap().c, ' ');
    }

    #[test]
    fn test_modes() {
        let mut term = Terminal::new(24, 80);
        assert!(term.mode(Mode::AutoWrap));
        assert!(term.mode(Mode::ShowCursor));
        assert!(!term.mode(Mode::Origin));

        term.set_mode(Mode::Origin);
        assert!(term.mode(Mode::Origin));

        term.unset_mode(Mode::Origin);
        assert!(!term.mode(Mode::Origin));
    }

    #[test]
    fn test_scroll_display() {
        let mut term = Terminal::new(3, 10);
        // Fill scrollback with some rows
        for _ in 0..5 {
            term.input_char('X');
            term.linefeed();
        }

        let sb_len = term.scrollback().len();
        assert!(sb_len > 0);

        term.scroll_display_up(2);
        assert_eq!(term.scroll_offset(), 2);

        term.scroll_display_down(1);
        assert_eq!(term.scroll_offset(), 1);

        term.scroll_display_reset();
        assert_eq!(term.scroll_offset(), 0);
    }

    #[test]
    fn test_insert_blank_chars() {
        let mut term = Terminal::new(24, 10);
        for c in "ABCDE".chars() {
            term.input_char(c);
        }
        term.cursor_mut().col = 2;
        term.insert_blank_chars(2);
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'A');
        assert_eq!(term.grid().cell(0, 1).unwrap().c, 'B');
        assert_eq!(term.grid().cell(0, 2).unwrap().c, ' ');
        assert_eq!(term.grid().cell(0, 3).unwrap().c, ' ');
        assert_eq!(term.grid().cell(0, 4).unwrap().c, 'C');
    }

    #[test]
    fn test_delete_chars() {
        let mut term = Terminal::new(24, 10);
        for c in "ABCDE".chars() {
            term.input_char(c);
        }
        term.cursor_mut().col = 1;
        term.delete_chars(2);
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'A');
        assert_eq!(term.grid().cell(0, 1).unwrap().c, 'D');
        assert_eq!(term.grid().cell(0, 2).unwrap().c, 'E');
    }

    #[test]
    fn test_terminal_reset() {
        let mut term = Terminal::new(24, 80);
        term.input_char('X');
        term.set_mode(Mode::Origin);
        term.set_title("test".to_string());

        term.reset();

        assert_eq!(term.grid().cell(0, 0).unwrap().c, ' ');
        assert!(!term.mode(Mode::Origin));
        assert!(term.mode(Mode::AutoWrap));
        assert_eq!(term.cursor().col, 0);
        assert_eq!(term.cursor().row, 0);
        assert!(term.title().is_empty());
    }

    #[test]
    fn test_reverse_index() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().row = 5;
        term.reverse_index();
        assert_eq!(term.cursor().row, 4);
    }

    #[test]
    fn test_reverse_index_at_top_scrolls() {
        let mut term = Terminal::new(5, 10);
        term.set_scroll_region(1, 5);
        term.cursor_mut().row = 0;
        // Write something to row 0 first
        term.input_char('Z');
        term.cursor_mut().col = 0;
        term.cursor_mut().row = 0;
        term.reverse_index();
        // Row 0 should be blank (new row inserted at top)
        assert_eq!(term.grid().cell(0, 0).unwrap().c, ' ');
    }

    #[test]
    fn test_dirty_flag() {
        let mut term = Terminal::new(24, 80);
        term.clear_dirty();
        assert!(!term.is_dirty());
        term.input_char('A');
        assert!(term.is_dirty());
    }
}
