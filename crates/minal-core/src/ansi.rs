//! ANSI constants and type definitions (SGR, CSI, OSC, DCS).

/// Standard ANSI color names (0-7 normal, 8-15 bright).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedColor {
    Black = 0,
    Red = 1,
    Green = 2,
    Yellow = 3,
    Blue = 4,
    Magenta = 5,
    Cyan = 6,
    White = 7,
    BrightBlack = 8,
    BrightRed = 9,
    BrightGreen = 10,
    BrightYellow = 11,
    BrightBlue = 12,
    BrightMagenta = 13,
    BrightCyan = 14,
    BrightWhite = 15,
}

/// Terminal color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// One of the 16 named ANSI colors.
    Named(NamedColor),
    /// 256-color palette index.
    Indexed(u8),
    /// 24-bit RGB color.
    Rgb(u8, u8, u8),
}

impl Default for Color {
    fn default() -> Self {
        Self::Named(NamedColor::White)
    }
}
