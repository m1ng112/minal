//! Cursor position, style, and movement management.

use crate::ansi::Color;
use crate::cell::CellAttributes;

/// Cursor visual style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CursorStyle {
    #[default]
    Block,
    Underline,
    Bar,
}

/// Saved cursor state for DECSC/DECRC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedCursor {
    pub col: usize,
    pub row: usize,
    pub attrs: CellAttributes,
    pub fg: Color,
    pub bg: Color,
    pub origin_mode: bool,
    pub auto_wrap: bool,
}

impl Default for SavedCursor {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            attrs: CellAttributes::default(),
            fg: Color::Default,
            bg: Color::Default,
            origin_mode: false,
            auto_wrap: true,
        }
    }
}

/// Terminal cursor state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// Column (0-indexed).
    pub col: usize,
    /// Row (0-indexed).
    pub row: usize,
    /// Visual style.
    pub style: CursorStyle,
    /// Whether the cursor is visible.
    pub visible: bool,
    /// Current cell attributes (applied when writing characters).
    pub attrs: CellAttributes,
    /// Current foreground color.
    pub fg: Color,
    /// Current background color.
    pub bg: Color,
    /// Whether the cursor is in the "pending wrap" state.
    ///
    /// When auto-wrap is enabled and the cursor is at the rightmost column,
    /// writing a character should first move to the next line.
    pub pending_wrap: bool,
    /// Saved cursor state (DECSC/DECRC).
    saved: SavedCursor,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            style: CursorStyle::default(),
            visible: true,
            attrs: CellAttributes::default(),
            fg: Color::Default,
            bg: Color::Default,
            pending_wrap: false,
            saved: SavedCursor::default(),
        }
    }
}

impl Cursor {
    /// Move cursor up by `n` rows, clamping at `top_margin`.
    pub fn move_up(&mut self, n: usize, top_margin: usize) {
        self.row = self.row.saturating_sub(n).max(top_margin);
        self.pending_wrap = false;
    }

    /// Move cursor down by `n` rows, clamping at `bottom_margin` (exclusive).
    pub fn move_down(&mut self, n: usize, bottom_margin: usize) {
        self.row = (self.row + n).min(bottom_margin.saturating_sub(1));
        self.pending_wrap = false;
    }

    /// Move cursor left by `n` columns, clamping at column 0.
    pub fn move_left(&mut self, n: usize) {
        self.col = self.col.saturating_sub(n);
        self.pending_wrap = false;
    }

    /// Move cursor right by `n` columns, clamping at `max_col` (exclusive).
    pub fn move_right(&mut self, n: usize, max_col: usize) {
        self.col = (self.col + n).min(max_col.saturating_sub(1));
        self.pending_wrap = false;
    }

    /// Move cursor to absolute position, clamping to grid bounds.
    pub fn goto(&mut self, row: usize, col: usize, max_rows: usize, max_cols: usize) {
        self.row = row.min(max_rows.saturating_sub(1));
        self.col = col.min(max_cols.saturating_sub(1));
        self.pending_wrap = false;
    }

    /// Move cursor to column, clamping to bounds.
    pub fn goto_col(&mut self, col: usize, max_cols: usize) {
        self.col = col.min(max_cols.saturating_sub(1));
        self.pending_wrap = false;
    }

    /// Move cursor to row, clamping to bounds.
    pub fn goto_row(&mut self, row: usize, max_rows: usize) {
        self.row = row.min(max_rows.saturating_sub(1));
        self.pending_wrap = false;
    }

    /// Save cursor state (DECSC).
    ///
    /// `origin_mode` and `auto_wrap` should be passed from the terminal's
    /// current mode set so they are restored correctly by DECRC.
    pub fn save(&mut self, origin_mode: bool, auto_wrap: bool) {
        self.saved = SavedCursor {
            col: self.col,
            row: self.row,
            attrs: self.attrs,
            fg: self.fg,
            bg: self.bg,
            origin_mode,
            auto_wrap,
        };
    }

    /// Restore cursor state (DECRC).
    pub fn restore(&mut self, max_rows: usize, max_cols: usize) {
        self.col = self.saved.col.min(max_cols.saturating_sub(1));
        self.row = self.saved.row.min(max_rows.saturating_sub(1));
        self.attrs = self.saved.attrs;
        self.fg = self.saved.fg;
        self.bg = self.saved.bg;
        self.pending_wrap = false;
    }

    /// Reset cursor to default state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_default() {
        let cursor = Cursor::default();
        assert_eq!(cursor.col, 0);
        assert_eq!(cursor.row, 0);
        assert!(cursor.visible);
        assert!(!cursor.pending_wrap);
    }

    #[test]
    fn test_move_up_clamped() {
        let mut cursor = Cursor::default();
        cursor.row = 5;
        cursor.move_up(10, 0);
        assert_eq!(cursor.row, 0);
    }

    #[test]
    fn test_move_up_with_margin() {
        let mut cursor = Cursor::default();
        cursor.row = 5;
        cursor.move_up(10, 3);
        assert_eq!(cursor.row, 3);
    }

    #[test]
    fn test_move_down_clamped() {
        let mut cursor = Cursor::default();
        cursor.move_down(100, 24);
        assert_eq!(cursor.row, 23);
    }

    #[test]
    fn test_move_left_clamped() {
        let mut cursor = Cursor::default();
        cursor.col = 2;
        cursor.move_left(5);
        assert_eq!(cursor.col, 0);
    }

    #[test]
    fn test_move_right_clamped() {
        let mut cursor = Cursor::default();
        cursor.move_right(100, 80);
        assert_eq!(cursor.col, 79);
    }

    #[test]
    fn test_goto() {
        let mut cursor = Cursor::default();
        cursor.goto(10, 40, 24, 80);
        assert_eq!(cursor.row, 10);
        assert_eq!(cursor.col, 40);
    }

    #[test]
    fn test_goto_clamp() {
        let mut cursor = Cursor::default();
        cursor.goto(100, 200, 24, 80);
        assert_eq!(cursor.row, 23);
        assert_eq!(cursor.col, 79);
    }

    #[test]
    fn test_save_restore() {
        let mut cursor = Cursor::default();
        cursor.goto(5, 10, 24, 80);
        cursor.fg = Color::Rgb(255, 0, 0);
        cursor.attrs.bold = true;
        cursor.save(false, true);

        cursor.goto(0, 0, 24, 80);
        cursor.fg = Color::Default;
        cursor.attrs.bold = false;

        cursor.restore(24, 80);
        assert_eq!(cursor.row, 5);
        assert_eq!(cursor.col, 10);
        assert_eq!(cursor.fg, Color::Rgb(255, 0, 0));
        assert!(cursor.attrs.bold);
    }

    #[test]
    fn test_pending_wrap_cleared_on_move() {
        let mut cursor = Cursor::default();
        cursor.pending_wrap = true;
        cursor.move_left(1);
        assert!(!cursor.pending_wrap);
    }
}
