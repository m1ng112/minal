//! Character set mapping for G0-G3 slots.
//!
//! Terminals can designate different character sets to four "slots" (G0-G3).
//! The active slot determines how bytes are mapped to display characters.

/// Character set slot identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharsetSlot {
    G0,
    G1,
    G2,
    G3,
}

/// Available character set encodings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Charset {
    /// Standard ASCII.
    #[default]
    Ascii,
    /// DEC Special Graphics (line drawing characters).
    DecSpecialGraphics,
    /// UK character set.
    Uk,
}

impl Charset {
    /// Map a byte to its display character in this charset.
    pub fn map(&self, c: char) -> char {
        match self {
            Charset::Ascii | Charset::Uk => c,
            Charset::DecSpecialGraphics => dec_special_graphics(c),
        }
    }
}

/// Map a character through the DEC Special Graphics charset.
///
/// Characters in the range 0x60-0x7E are mapped to line-drawing
/// and other special characters.
fn dec_special_graphics(c: char) -> char {
    match c {
        '`' => '\u{25C6}', // ◆ Diamond
        'a' => '\u{2592}', // ▒ Checkerboard
        'b' => '\u{2409}', // ␉ HT
        'c' => '\u{240C}', // ␌ FF
        'd' => '\u{240D}', // ␍ CR
        'e' => '\u{240A}', // ␊ LF
        'f' => '\u{00B0}', // ° Degree
        'g' => '\u{00B1}', // ± Plus/minus
        'h' => '\u{2424}', // ␤ NL
        'i' => '\u{240B}', // ␋ VT
        'j' => '\u{2518}', // ┘ Lower right corner
        'k' => '\u{2510}', // ┐ Upper right corner
        'l' => '\u{250C}', // ┌ Upper left corner
        'm' => '\u{2514}', // └ Lower left corner
        'n' => '\u{253C}', // ┼ Crossing
        'o' => '\u{23BA}', // ⎺ Scan line 1
        'p' => '\u{23BB}', // ⎻ Scan line 3
        'q' => '\u{2500}', // ─ Horizontal line
        'r' => '\u{23BC}', // ⎼ Scan line 7
        's' => '\u{23BD}', // ⎽ Scan line 9
        't' => '\u{251C}', // ├ Left tee
        'u' => '\u{2524}', // ┤ Right tee
        'v' => '\u{2534}', // ┴ Bottom tee
        'w' => '\u{252C}', // ┬ Top tee
        'x' => '\u{2502}', // │ Vertical line
        'y' => '\u{2264}', // ≤ Less than or equal
        'z' => '\u{2265}', // ≥ Greater than or equal
        '{' => '\u{03C0}', // π Pi
        '|' => '\u{2260}', // ≠ Not equal
        '}' => '\u{00A3}', // £ Pound sterling
        '~' => '\u{00B7}', // · Middle dot
        _ => c,
    }
}

/// Character set table managing the four G0-G3 slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharsetTable {
    /// The four charset slots.
    charsets: [Charset; 4],
    /// Currently active slot for GL (left half, 0x20-0x7F).
    pub active: CharsetSlot,
}

impl Default for CharsetTable {
    fn default() -> Self {
        Self {
            charsets: [Charset::Ascii; 4],
            active: CharsetSlot::G0,
        }
    }
}

impl CharsetTable {
    /// Get the charset for a specific slot.
    pub fn get(&self, slot: CharsetSlot) -> Charset {
        self.charsets[slot as usize]
    }

    /// Set the charset for a specific slot.
    pub fn set(&mut self, slot: CharsetSlot, charset: Charset) {
        self.charsets[slot as usize] = charset;
    }

    /// Get the currently active charset.
    pub fn active_charset(&self) -> Charset {
        self.charsets[self.active as usize]
    }

    /// Map a character through the currently active charset.
    pub fn map(&self, c: char) -> char {
        self.active_charset().map(c)
    }

    /// Set the active charset slot by index (0=G0, 1=G1, 2=G2, 3=G3).
    ///
    /// Out-of-range indices are silently ignored.
    pub fn set_active(&mut self, slot_index: usize) {
        let slot = match slot_index {
            0 => CharsetSlot::G0,
            1 => CharsetSlot::G1,
            2 => CharsetSlot::G2,
            3 => CharsetSlot::G3,
            _ => return,
        };
        self.active = slot;
    }

    /// Designate a charset for a slot by numeric indices.
    ///
    /// `slot_index`: 0=G0, 1=G1, 2=G2, 3=G3.
    /// `charset_id`: 0=ASCII, 1=DecSpecialGraphics.
    /// Out-of-range values are silently ignored.
    pub fn designate(&mut self, slot_index: usize, charset_id: usize) {
        let slot = match slot_index {
            0 => CharsetSlot::G0,
            1 => CharsetSlot::G1,
            2 => CharsetSlot::G2,
            3 => CharsetSlot::G3,
            _ => return,
        };
        let charset = match charset_id {
            0 => Charset::Ascii,
            1 => Charset::DecSpecialGraphics,
            _ => return,
        };
        self.set(slot, charset);
    }

    /// Reset all slots to ASCII.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_charset_is_ascii() {
        let table = CharsetTable::default();
        assert_eq!(table.active_charset(), Charset::Ascii);
        assert_eq!(table.map('A'), 'A');
    }

    #[test]
    fn test_dec_special_graphics_mapping() {
        let charset = Charset::DecSpecialGraphics;
        assert_eq!(charset.map('q'), '\u{2500}'); // ─
        assert_eq!(charset.map('x'), '\u{2502}'); // │
        assert_eq!(charset.map('l'), '\u{250C}'); // ┌
        assert_eq!(charset.map('A'), 'A'); // Not in special range
    }

    #[test]
    fn test_charset_slot_switching() {
        let mut table = CharsetTable::default();
        table.set(CharsetSlot::G1, Charset::DecSpecialGraphics);
        table.active = CharsetSlot::G1;
        assert_eq!(table.map('q'), '\u{2500}');

        table.active = CharsetSlot::G0;
        assert_eq!(table.map('q'), 'q');
    }

    #[test]
    fn test_charset_reset() {
        let mut table = CharsetTable::default();
        table.set(CharsetSlot::G0, Charset::DecSpecialGraphics);
        table.active = CharsetSlot::G1;
        table.reset();
        assert_eq!(table.active, CharsetSlot::G0);
        assert_eq!(table.active_charset(), Charset::Ascii);
    }
}
