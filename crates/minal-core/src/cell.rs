//! Cell data structure: character + colors + attributes.

use crate::ansi::Color;

/// Visual attributes for a terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellAttributes {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub inverse: bool,
    pub hidden: bool,
    pub dim: bool,
    pub blink: bool,
}

impl CellAttributes {
    /// Returns true if no attributes are set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Reset all attributes to default.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// A single cell in the terminal grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The character displayed in this cell.
    pub c: char,
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Visual attributes.
    pub attrs: CellAttributes,
    /// Number of extra columns this cell occupies (0 for normal, 1 for wide chars).
    pub width: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttributes::default(),
            width: 1,
        }
    }
}

impl Cell {
    /// Returns true if this cell contains only the default space character.
    pub fn is_empty(&self) -> bool {
        self.c == ' '
            && self.fg == Color::Default
            && self.bg == Color::Default
            && self.attrs.is_empty()
    }

    /// Reset the cell to its default state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Create a cell that is a spacer for a wide character.
    pub fn wide_spacer() -> Self {
        Self {
            c: ' ',
            width: 0,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cell_is_empty() {
        let cell = Cell::default();
        assert!(cell.is_empty());
    }

    #[test]
    fn test_cell_with_char_not_empty() {
        let mut cell = Cell::default();
        cell.c = 'A';
        assert!(!cell.is_empty());
    }

    #[test]
    fn test_cell_reset() {
        let mut cell = Cell::default();
        cell.c = 'X';
        cell.fg = Color::Rgb(255, 0, 0);
        cell.attrs.bold = true;
        cell.reset();
        assert!(cell.is_empty());
    }

    #[test]
    fn test_wide_spacer() {
        let spacer = Cell::wide_spacer();
        assert_eq!(spacer.width, 0);
    }

    #[test]
    fn test_cell_attributes_reset() {
        let mut attrs = CellAttributes {
            bold: true,
            italic: true,
            underline: true,
            ..Default::default()
        };
        assert!(!attrs.is_empty());
        attrs.reset();
        assert!(attrs.is_empty());
    }
}
