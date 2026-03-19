//! ANSI constants, color definitions, and terminal mode flags.

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

impl NamedColor {
    /// Maps a standard color (0-7) to its bright variant (8-15).
    ///
    /// Returns `None` if the color is already a bright variant.
    pub fn to_bright(self) -> Option<NamedColor> {
        match self {
            Self::Black => Some(Self::BrightBlack),
            Self::Red => Some(Self::BrightRed),
            Self::Green => Some(Self::BrightGreen),
            Self::Yellow => Some(Self::BrightYellow),
            Self::Blue => Some(Self::BrightBlue),
            Self::Magenta => Some(Self::BrightMagenta),
            Self::Cyan => Some(Self::BrightCyan),
            Self::White => Some(Self::BrightWhite),
            _ => None, // Already bright
        }
    }
}

/// Terminal color representation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Color {
    /// Use the terminal theme's default color.
    #[default]
    Default,
    /// One of the 16 named ANSI colors.
    Named(NamedColor),
    /// 256-color palette index.
    Indexed(u8),
    /// 24-bit RGB color.
    Rgb(u8, u8, u8),
}

/// Terminal DEC private modes and standard modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// DECCKM — Cursor key mode (application vs normal).
    CursorKeys,
    /// DECOM — Origin mode (relative to scroll region).
    Origin,
    /// DECAWM — Auto-wrap mode.
    AutoWrap,
    /// DECTCEM — Text cursor enable (show/hide).
    ShowCursor,
    /// Alternate screen buffer (DECSET 1049 / 47 / 1047).
    AlternateScreen,
    /// Bracketed paste mode (DECSET 2004).
    BracketedPaste,
    /// Focus reporting (DECSET 1004).
    FocusTracking,
    /// SGR mouse mode (DECSET 1006).
    SgrMouse,
    /// Button event mouse tracking (DECSET 1002).
    MouseMotion,
    /// Any event mouse tracking (DECSET 1003).
    MouseAll,
    /// Send mouse X & Y on button press (DECSET 1000).
    MouseReport,
    /// Line feed / new line mode (LNM).
    LineFeedNewLine,
    /// Insert/Replace mode (IRM).
    Insert,
}

/// Build the default 256-color palette.
///
/// Returns an array of 256 `(r, g, b)` tuples:
/// - 0..16: Standard and bright ANSI colors
/// - 16..232: 6×6×6 RGB color cube
/// - 232..256: 24-step grayscale ramp
pub fn build_256_palette() -> [(u8, u8, u8); 256] {
    let mut palette = [(0u8, 0u8, 0u8); 256];

    // Standard colors (0-7)
    palette[0] = (0, 0, 0); // Black
    palette[1] = (205, 0, 0); // Red
    palette[2] = (0, 205, 0); // Green
    palette[3] = (205, 205, 0); // Yellow
    palette[4] = (0, 0, 238); // Blue
    palette[5] = (205, 0, 205); // Magenta
    palette[6] = (0, 205, 205); // Cyan
    palette[7] = (229, 229, 229); // White

    // Bright colors (8-15)
    palette[8] = (127, 127, 127); // Bright Black
    palette[9] = (255, 0, 0); // Bright Red
    palette[10] = (0, 255, 0); // Bright Green
    palette[11] = (255, 255, 0); // Bright Yellow
    palette[12] = (92, 92, 255); // Bright Blue
    palette[13] = (255, 0, 255); // Bright Magenta
    palette[14] = (0, 255, 255); // Bright Cyan
    palette[15] = (255, 255, 255); // Bright White

    // 6x6x6 color cube (16-231)
    let levels: [u8; 6] = [0, 95, 135, 175, 215, 255];
    for r in 0..6u8 {
        for g in 0..6u8 {
            for b in 0..6u8 {
                let idx = 16 + (r as usize * 36) + (g as usize * 6) + b as usize;
                palette[idx] = (levels[r as usize], levels[g as usize], levels[b as usize]);
            }
        }
    }

    // Grayscale ramp (232-255)
    for i in 0..24u8 {
        let v = 8 + i * 10;
        palette[232 + i as usize] = (v, v, v);
    }

    palette
}

/// C0 control codes.
pub mod c0 {
    /// Null.
    pub const NUL: u8 = 0x00;
    /// Bell.
    pub const BEL: u8 = 0x07;
    /// Backspace.
    pub const BS: u8 = 0x08;
    /// Horizontal tab.
    pub const HT: u8 = 0x09;
    /// Line feed.
    pub const LF: u8 = 0x0A;
    /// Vertical tab.
    pub const VT: u8 = 0x0B;
    /// Form feed.
    pub const FF: u8 = 0x0C;
    /// Carriage return.
    pub const CR: u8 = 0x0D;
    /// Shift out (activate G1 charset).
    pub const SO: u8 = 0x0E;
    /// Shift in (activate G0 charset).
    pub const SI: u8 = 0x0F;
    /// Escape.
    pub const ESC: u8 = 0x1B;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_256_palette_length() {
        let palette = build_256_palette();
        assert_eq!(palette.len(), 256);
    }

    #[test]
    fn test_256_palette_standard_colors() {
        let palette = build_256_palette();
        assert_eq!(palette[0], (0, 0, 0)); // Black
        assert_eq!(palette[15], (255, 255, 255)); // Bright White
    }

    #[test]
    fn test_256_palette_color_cube() {
        let palette = build_256_palette();
        // Index 16 = (0, 0, 0), Index 231 = (255, 255, 255)
        assert_eq!(palette[16], (0, 0, 0));
        assert_eq!(palette[231], (255, 255, 255));
    }

    #[test]
    fn test_named_color_to_bright_standard() {
        assert_eq!(NamedColor::Black.to_bright(), Some(NamedColor::BrightBlack));
        assert_eq!(NamedColor::Red.to_bright(), Some(NamedColor::BrightRed));
        assert_eq!(NamedColor::Green.to_bright(), Some(NamedColor::BrightGreen));
        assert_eq!(
            NamedColor::Yellow.to_bright(),
            Some(NamedColor::BrightYellow)
        );
        assert_eq!(NamedColor::Blue.to_bright(), Some(NamedColor::BrightBlue));
        assert_eq!(
            NamedColor::Magenta.to_bright(),
            Some(NamedColor::BrightMagenta)
        );
        assert_eq!(NamedColor::Cyan.to_bright(), Some(NamedColor::BrightCyan));
        assert_eq!(NamedColor::White.to_bright(), Some(NamedColor::BrightWhite));
    }

    #[test]
    fn test_named_color_to_bright_already_bright() {
        assert_eq!(NamedColor::BrightBlack.to_bright(), None);
        assert_eq!(NamedColor::BrightRed.to_bright(), None);
        assert_eq!(NamedColor::BrightGreen.to_bright(), None);
        assert_eq!(NamedColor::BrightYellow.to_bright(), None);
        assert_eq!(NamedColor::BrightBlue.to_bright(), None);
        assert_eq!(NamedColor::BrightMagenta.to_bright(), None);
        assert_eq!(NamedColor::BrightCyan.to_bright(), None);
        assert_eq!(NamedColor::BrightWhite.to_bright(), None);
    }

    #[test]
    fn test_256_palette_grayscale() {
        let palette = build_256_palette();
        assert_eq!(palette[232], (8, 8, 8));
        assert_eq!(palette[255], (238, 238, 238));
    }
}
