//! Cursor position and style management.

/// Cursor visual style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CursorStyle {
    #[default]
    Block,
    Underline,
    Bar,
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
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            style: CursorStyle::default(),
            visible: true,
        }
    }
}
