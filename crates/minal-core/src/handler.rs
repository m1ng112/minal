//! VT escape sequence handler implementing `vte::Perform`.
//!
//! Processes escape sequences from the VT parser and translates them
//! into operations on the `Terminal` state machine.

use crate::ansi::{Color, Mode, NamedColor, c0};
use crate::charset::{Charset, CharsetSlot};
use crate::term::Terminal;

/// Handler for VT parser events.
///
/// Implements `vte::Perform` to process escape sequences and update
/// terminal state. Holds a mutable reference to the `Terminal`.
pub struct Handler<'a> {
    terminal: &'a mut Terminal,
}

impl<'a> Handler<'a> {
    /// Create a new handler wrapping the given terminal.
    pub fn new(terminal: &'a mut Terminal) -> Self {
        Self { terminal }
    }
}

/// Extract the first CSI parameter, defaulting to `default` if absent or zero.
fn param(params: &vte::Params, idx: usize, default: u16) -> u16 {
    params
        .iter()
        .nth(idx)
        .and_then(|sub| sub.first().copied())
        .map(|v| if v == 0 { default } else { v })
        .unwrap_or(default)
}

impl vte::Perform for Handler<'_> {
    fn print(&mut self, c: char) {
        self.terminal.input_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            c0::BS => self.terminal.backspace(),
            c0::HT => self.terminal.tab(),
            c0::LF | c0::VT | c0::FF => self.terminal.linefeed(),
            c0::CR => self.terminal.carriage_return(),
            c0::SI => self.terminal.charset_mut().active = CharsetSlot::G0,
            c0::SO => self.terminal.charset_mut().active = CharsetSlot::G1,
            c0::BEL => {
                // TODO: bell notification
                tracing::trace!("BEL");
            }
            _ => tracing::trace!("unhandled C0: {byte:#04x}"),
        }
    }

    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        tracing::trace!(
            "hook: params={params:?}, intermediates={intermediates:?}, \
             ignore={ignore}, action={action:?}"
        );
    }

    fn put(&mut self, byte: u8) {
        tracing::trace!("put: {byte:#04x}");
    }

    fn unhook(&mut self) {
        tracing::trace!("unhook");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let _ = bell_terminated;

        if params.is_empty() {
            return;
        }

        // Parse the command number from the first param
        let cmd = match std::str::from_utf8(params[0])
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
        {
            Some(n) => n,
            None => {
                tracing::trace!("OSC: non-numeric command");
                return;
            }
        };

        match cmd {
            // OSC 0 (icon name + title) and OSC 2 (title)
            0 | 2 => {
                if params.len() >= 2 {
                    if let Ok(title) = std::str::from_utf8(params[1]) {
                        self.terminal.set_title(title.to_string());
                    }
                }
            }
            _ => tracing::trace!("unhandled OSC {cmd}"),
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore {
            return;
        }

        let is_private = intermediates.first() == Some(&b'?');

        match action {
            // CUU — Cursor Up
            'A' => {
                let n = param(params, 0, 1) as usize;
                let (top, _) = self.terminal.scroll_region();
                self.terminal.cursor_mut().move_up(n, top);
            }
            // CUD — Cursor Down
            'B' => {
                let n = param(params, 0, 1) as usize;
                let (_, bottom) = self.terminal.scroll_region();
                self.terminal.cursor_mut().move_down(n, bottom);
            }
            // CUF — Cursor Forward
            'C' => {
                let n = param(params, 0, 1) as usize;
                let cols = self.terminal.cols();
                self.terminal.cursor_mut().move_right(n, cols);
            }
            // CUB — Cursor Back
            'D' => {
                let n = param(params, 0, 1) as usize;
                self.terminal.cursor_mut().move_left(n);
            }
            // CNL — Cursor Next Line
            'E' => {
                let n = param(params, 0, 1) as usize;
                let (_, bottom) = self.terminal.scroll_region();
                self.terminal.cursor_mut().move_down(n, bottom);
                // move_down already clears pending_wrap
                self.terminal.cursor_mut().col = 0;
            }
            // CPL — Cursor Previous Line
            'F' => {
                let n = param(params, 0, 1) as usize;
                let (top, _) = self.terminal.scroll_region();
                self.terminal.cursor_mut().move_up(n, top);
                // move_up already clears pending_wrap
                self.terminal.cursor_mut().col = 0;
            }
            // CHA — Cursor Horizontal Absolute
            'G' => {
                let col = param(params, 0, 1) as usize;
                let cols = self.terminal.cols();
                self.terminal
                    .cursor_mut()
                    .goto_col(col.saturating_sub(1), cols);
            }
            // CUP / HVP — Cursor Position
            'H' | 'f' => {
                let row = param(params, 0, 1) as usize;
                let col = param(params, 1, 1) as usize;
                let rows = self.terminal.rows();
                let cols = self.terminal.cols();
                self.terminal.cursor_mut().goto(
                    row.saturating_sub(1),
                    col.saturating_sub(1),
                    rows,
                    cols,
                );
            }
            // ED — Erase in Display
            'J' => {
                let mode = param(params, 0, 0);
                match mode {
                    0 => self.terminal.erase_display_below(),
                    1 => self.terminal.erase_display_above(),
                    2 | 3 => self.terminal.erase_display_all(),
                    _ => {}
                }
            }
            // EL — Erase in Line
            'K' => {
                let mode = param(params, 0, 0);
                match mode {
                    0 => self.terminal.erase_line_right(),
                    1 => self.terminal.erase_line_left(),
                    2 => self.terminal.erase_line_all(),
                    _ => {}
                }
            }
            // IL — Insert Lines (no-op if cursor outside scroll region)
            'L' => {
                let n = param(params, 0, 1) as usize;
                let (top, bottom) = self.terminal.scroll_region();
                let row = self.terminal.cursor().row;
                if row >= top && row < bottom {
                    self.terminal.insert_blank_lines(n);
                }
            }
            // DL — Delete Lines (no-op if cursor outside scroll region)
            'M' => {
                let n = param(params, 0, 1) as usize;
                let (top, bottom) = self.terminal.scroll_region();
                let row = self.terminal.cursor().row;
                if row >= top && row < bottom {
                    self.terminal.delete_lines(n);
                }
            }
            // DCH — Delete Characters
            'P' => {
                let n = param(params, 0, 1) as usize;
                self.terminal.delete_chars(n);
            }
            // SU — Scroll Up
            'S' if !is_private => {
                let n = param(params, 0, 1) as usize;
                self.terminal.scroll_up(n);
            }
            // SD — Scroll Down
            'T' if !is_private => {
                let n = param(params, 0, 1) as usize;
                self.terminal.scroll_down(n);
            }
            // ICH — Insert Blank Characters
            '@' => {
                let n = param(params, 0, 1) as usize;
                self.terminal.insert_blank_chars(n);
            }
            // ECH — Erase Characters
            'X' => {
                let n = param(params, 0, 1) as usize;
                let row = self.terminal.cursor().row;
                let col = self.terminal.cursor().col;
                let cols = self.terminal.cols();
                for c in col..(col + n).min(cols) {
                    if let Some(cell) = self.terminal.grid_mut().cell_mut(row, c) {
                        cell.reset();
                    }
                }
            }
            // VPA — Vertical Line Position Absolute
            'd' => {
                let row = param(params, 0, 1) as usize;
                let rows = self.terminal.rows();
                self.terminal
                    .cursor_mut()
                    .goto_row(row.saturating_sub(1), rows);
            }
            // SGR — Select Graphic Rendition
            'm' => {
                self.handle_sgr(params);
            }
            // DECSTBM — Set Top and Bottom Margins
            'r' => {
                let top = param(params, 0, 1) as usize;
                let bottom = param(params, 1, self.terminal.rows() as u16) as usize;
                self.terminal.set_scroll_region(top, bottom);
            }
            // DECSET / DECRST — set/reset DEC private modes
            'h' => {
                if is_private {
                    self.set_dec_modes(params, true);
                } else {
                    // Standard SM (Set Mode)
                    self.set_standard_modes(params, true);
                }
            }
            'l' => {
                if is_private {
                    self.set_dec_modes(params, false);
                } else {
                    // Standard RM (Reset Mode)
                    self.set_standard_modes(params, false);
                }
            }
            // TBC — Tab Clear
            'g' => {
                let mode = param(params, 0, 0);
                match mode {
                    0 => self.terminal.clear_tab_stop(),
                    3 => self.terminal.clear_all_tab_stops(),
                    _ => {}
                }
            }
            _ => {
                tracing::trace!(
                    "unhandled CSI: action={action:?}, params={params:?}, \
                     intermediates={intermediates:?}"
                );
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }

        match (intermediates, byte) {
            // DECSC — Save Cursor
            ([], b'7') => {
                self.terminal.save_cursor();
            }
            // DECRC — Restore Cursor
            ([], b'8') => {
                let rows = self.terminal.rows();
                let cols = self.terminal.cols();
                self.terminal.cursor_mut().restore(rows, cols);
            }
            // RI — Reverse Index
            ([], b'M') => {
                self.terminal.reverse_index();
            }
            // RIS — Full Reset
            ([], b'c') => {
                self.terminal.reset();
            }
            // HTS — Horizontal Tab Set
            ([], b'H') => {
                self.terminal.set_tab_stop();
            }
            // IND — Index (move cursor down, scroll if needed)
            ([], b'D') => {
                self.terminal.linefeed();
            }
            // NEL — Next Line
            ([], b'E') => {
                self.terminal.linefeed();
                self.terminal.carriage_return();
            }
            // Charset designation: G0
            ([b'('], c) => {
                let charset = charset_from_designator(c);
                self.terminal.charset_mut().set(CharsetSlot::G0, charset);
            }
            // Charset designation: G1
            ([b')'], c) => {
                let charset = charset_from_designator(c);
                self.terminal.charset_mut().set(CharsetSlot::G1, charset);
            }
            // Charset designation: G2
            ([b'*'], c) => {
                let charset = charset_from_designator(c);
                self.terminal.charset_mut().set(CharsetSlot::G2, charset);
            }
            // Charset designation: G3
            ([b'+'], c) => {
                let charset = charset_from_designator(c);
                self.terminal.charset_mut().set(CharsetSlot::G3, charset);
            }
            _ => {
                tracing::trace!("unhandled ESC: intermediates={intermediates:?}, byte={byte:#04x}");
            }
        }
    }
}

/// Map a charset designator byte to a `Charset`.
fn charset_from_designator(c: u8) -> Charset {
    match c {
        b'B' => Charset::Ascii,
        b'0' => Charset::DecSpecialGraphics,
        b'A' => Charset::Uk,
        _ => Charset::Ascii,
    }
}

impl Handler<'_> {
    /// Handle SGR (Select Graphic Rendition) parameters.
    fn handle_sgr(&mut self, params: &vte::Params) {
        let mut iter = params.iter().peekable();

        // If no parameters, treat as SGR 0 (reset)
        if iter.peek().is_none() {
            self.sgr_reset();
            return;
        }

        while let Some(sub) = iter.next() {
            let code = sub.first().copied().unwrap_or(0);
            match code {
                // Reset
                0 => self.sgr_reset(),
                // Bold
                1 => self.terminal.cursor_mut().attrs.bold = true,
                // Dim
                2 => self.terminal.cursor_mut().attrs.dim = true,
                // Italic
                3 => self.terminal.cursor_mut().attrs.italic = true,
                // Underline
                4 => self.terminal.cursor_mut().attrs.underline = true,
                // Blink
                5 | 6 => self.terminal.cursor_mut().attrs.blink = true,
                // Inverse
                7 => self.terminal.cursor_mut().attrs.inverse = true,
                // Hidden
                8 => self.terminal.cursor_mut().attrs.hidden = true,
                // Strikethrough
                9 => self.terminal.cursor_mut().attrs.strikethrough = true,
                // Normal intensity (not bold, not dim)
                22 => {
                    self.terminal.cursor_mut().attrs.bold = false;
                    self.terminal.cursor_mut().attrs.dim = false;
                }
                // Not italic
                23 => self.terminal.cursor_mut().attrs.italic = false,
                // Not underlined
                24 => self.terminal.cursor_mut().attrs.underline = false,
                // Not blinking
                25 => self.terminal.cursor_mut().attrs.blink = false,
                // Not inverse
                27 => self.terminal.cursor_mut().attrs.inverse = false,
                // Not hidden
                28 => self.terminal.cursor_mut().attrs.hidden = false,
                // Not strikethrough
                29 => self.terminal.cursor_mut().attrs.strikethrough = false,
                // Standard foreground colors (30-37)
                30..=37 => {
                    self.terminal.cursor_mut().fg = named_color(code - 30);
                }
                // Extended foreground color
                38 => {
                    if let Some(color) = parse_extended_color(&mut iter) {
                        self.terminal.cursor_mut().fg = color;
                    }
                }
                // Default foreground
                39 => self.terminal.cursor_mut().fg = Color::Default,
                // Standard background colors (40-47)
                40..=47 => {
                    self.terminal.cursor_mut().bg = named_color(code - 40);
                }
                // Extended background color
                48 => {
                    if let Some(color) = parse_extended_color(&mut iter) {
                        self.terminal.cursor_mut().bg = color;
                    }
                }
                // Default background
                49 => self.terminal.cursor_mut().bg = Color::Default,
                // Bright foreground colors (90-97)
                90..=97 => {
                    self.terminal.cursor_mut().fg = named_color(code - 90 + 8);
                }
                // Bright background colors (100-107)
                100..=107 => {
                    self.terminal.cursor_mut().bg = named_color(code - 100 + 8);
                }
                _ => tracing::trace!("unhandled SGR code: {code}"),
            }
        }
    }

    /// Reset all SGR attributes to defaults.
    fn sgr_reset(&mut self) {
        self.terminal.cursor_mut().attrs.reset();
        self.terminal.cursor_mut().fg = Color::Default;
        self.terminal.cursor_mut().bg = Color::Default;
    }

    /// Set or reset DEC private modes.
    fn set_dec_modes(&mut self, params: &vte::Params, enable: bool) {
        for sub in params.iter() {
            let code = sub.first().copied().unwrap_or(0);
            let mode = match code {
                1 => Some(Mode::CursorKeys),
                6 => Some(Mode::Origin),
                7 => Some(Mode::AutoWrap),
                25 => Some(Mode::ShowCursor),
                47 | 1047 => Some(Mode::AlternateScreen),
                1000 => Some(Mode::MouseReport),
                1002 => Some(Mode::MouseMotion),
                1003 => Some(Mode::MouseAll),
                1004 => Some(Mode::FocusTracking),
                1006 => Some(Mode::SgrMouse),
                1049 => {
                    // Enable: save cursor (DECSC) + switch to alt screen + clear
                    // Disable: switch back to primary + restore cursor (DECRC)
                    if enable {
                        self.terminal.save_cursor();
                        self.terminal.set_mode(Mode::AlternateScreen);
                    } else {
                        self.terminal.unset_mode(Mode::AlternateScreen);
                        let rows = self.terminal.rows();
                        let cols = self.terminal.cols();
                        self.terminal.cursor_mut().restore(rows, cols);
                    }
                    // Mode already set/unset above; skip the common path.
                    None
                }
                2004 => Some(Mode::BracketedPaste),
                _ => {
                    tracing::trace!("unhandled DECSET/DECRST mode: {code}");
                    None
                }
            };
            if let Some(m) = mode {
                if enable {
                    self.terminal.set_mode(m);
                } else {
                    self.terminal.unset_mode(m);
                }
            }
        }
    }

    /// Set or reset standard (non-private) modes.
    fn set_standard_modes(&mut self, params: &vte::Params, enable: bool) {
        for sub in params.iter() {
            let code = sub.first().copied().unwrap_or(0);
            let mode = match code {
                4 => Some(Mode::Insert),
                20 => Some(Mode::LineFeedNewLine),
                _ => {
                    tracing::trace!("unhandled SM/RM mode: {code}");
                    None
                }
            };
            if let Some(m) = mode {
                if enable {
                    self.terminal.set_mode(m);
                } else {
                    self.terminal.unset_mode(m);
                }
            }
        }
    }
}

/// Parse an extended color (256-color or TrueColor) from SGR sub-parameters.
///
/// Expected formats:
/// - `38;5;N` or `48;5;N` for 256-color
/// - `38;2;R;G;B` or `48;2;R;G;B` for TrueColor
fn parse_extended_color<'a>(iter: &mut impl Iterator<Item = &'a [u16]>) -> Option<Color> {
    let kind = iter.next()?.first().copied()?;
    match kind {
        // 256-color: 5;N
        5 => {
            let idx = iter.next()?.first().copied()?;
            if idx > 255 {
                return None;
            }
            Some(Color::Indexed(idx as u8))
        }
        // TrueColor: 2;R;G;B
        2 => {
            let r = iter.next()?.first().copied()?;
            let g = iter.next()?.first().copied()?;
            let b = iter.next()?.first().copied()?;
            if r > 255 || g > 255 || b > 255 {
                return None;
            }
            Some(Color::Rgb(r as u8, g as u8, b as u8))
        }
        _ => None,
    }
}

/// Map a color index (0-15) to a `Color::Named`.
fn named_color(idx: u16) -> Color {
    Color::Named(named_color_from_index(idx))
}

/// Map a 0-15 color index to a `NamedColor`.
fn named_color_from_index(idx: u16) -> NamedColor {
    match idx {
        0 => NamedColor::Black,
        1 => NamedColor::Red,
        2 => NamedColor::Green,
        3 => NamedColor::Yellow,
        4 => NamedColor::Blue,
        5 => NamedColor::Magenta,
        6 => NamedColor::Cyan,
        7 => NamedColor::White,
        8 => NamedColor::BrightBlack,
        9 => NamedColor::BrightRed,
        10 => NamedColor::BrightGreen,
        11 => NamedColor::BrightYellow,
        12 => NamedColor::BrightBlue,
        13 => NamedColor::BrightMagenta,
        14 => NamedColor::BrightCyan,
        15 => NamedColor::BrightWhite,
        _ => NamedColor::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vte::Parser;

    /// Helper: feed raw bytes through a VT parser into our handler.
    fn process(terminal: &mut Terminal, data: &[u8]) {
        let mut parser = Parser::new();
        for &byte in data {
            let mut handler = Handler::new(terminal);
            parser.advance(&mut handler, byte);
        }
    }

    // ─── Print ───────────────────────────────────────────────────

    #[test]
    fn test_print_ascii() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"Hello");
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some('H'));
        assert_eq!(term.grid().cell(0, 4).map(|c| c.c), Some('o'));
        assert_eq!(term.cursor().col, 5);
    }

    #[test]
    fn test_print_utf8() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, "日本語".as_bytes());
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some('日'));
        assert_eq!(term.grid().cell(0, 1).map(|c| c.c), Some('本'));
        assert_eq!(term.grid().cell(0, 2).map(|c| c.c), Some('語'));
    }

    // ─── C0 Controls ────────────────────────────────────────────

    #[test]
    fn test_c0_backspace() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"AB\x08");
        assert_eq!(term.cursor().col, 1);
    }

    #[test]
    fn test_c0_tab() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\t");
        assert_eq!(term.cursor().col, 8);
    }

    #[test]
    fn test_c0_linefeed() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"A\n");
        assert_eq!(term.cursor().row, 1);
    }

    #[test]
    fn test_c0_carriage_return() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"Hello\r");
        assert_eq!(term.cursor().col, 0);
    }

    #[test]
    fn test_c0_shift_out_in() {
        let mut term = Terminal::new(24, 80);
        term.charset_mut()
            .set(CharsetSlot::G1, Charset::DecSpecialGraphics);

        // SO activates G1
        process(&mut term, b"\x0E");
        assert_eq!(term.charset().active, CharsetSlot::G1);

        // SI activates G0
        process(&mut term, b"\x0F");
        assert_eq!(term.charset().active, CharsetSlot::G0);
    }

    // ─── CSI: Cursor Movement ────────────────────────────────────

    #[test]
    fn test_csi_cursor_up() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().row = 10;
        process(&mut term, b"\x1b[3A");
        assert_eq!(term.cursor().row, 7);
    }

    #[test]
    fn test_csi_cursor_down() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[5B");
        assert_eq!(term.cursor().row, 5);
    }

    #[test]
    fn test_csi_cursor_forward() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[10C");
        assert_eq!(term.cursor().col, 10);
    }

    #[test]
    fn test_csi_cursor_back() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().col = 20;
        process(&mut term, b"\x1b[5D");
        assert_eq!(term.cursor().col, 15);
    }

    #[test]
    fn test_csi_cursor_position() {
        let mut term = Terminal::new(24, 80);
        // CSI 10;20H = move to row 10, col 20 (1-indexed)
        process(&mut term, b"\x1b[10;20H");
        assert_eq!(term.cursor().row, 9);
        assert_eq!(term.cursor().col, 19);
    }

    #[test]
    fn test_csi_cursor_position_default() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().row = 10;
        term.cursor_mut().col = 20;
        // CSI H with no params = home (1,1)
        process(&mut term, b"\x1b[H");
        assert_eq!(term.cursor().row, 0);
        assert_eq!(term.cursor().col, 0);
    }

    #[test]
    fn test_csi_cursor_horizontal_absolute() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[15G");
        assert_eq!(term.cursor().col, 14);
    }

    #[test]
    fn test_csi_vertical_position_absolute() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[12d");
        assert_eq!(term.cursor().row, 11);
    }

    #[test]
    fn test_csi_cursor_next_line() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().col = 30;
        process(&mut term, b"\x1b[2E");
        assert_eq!(term.cursor().row, 2);
        assert_eq!(term.cursor().col, 0);
    }

    #[test]
    fn test_csi_cursor_previous_line() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().row = 10;
        term.cursor_mut().col = 30;
        process(&mut term, b"\x1b[3F");
        assert_eq!(term.cursor().row, 7);
        assert_eq!(term.cursor().col, 0);
    }

    // ─── CSI: Erase ─────────────────────────────────────────────

    #[test]
    fn test_csi_erase_display_below() {
        let mut term = Terminal::new(5, 10);
        process(&mut term, b"ABCDEFGHIJ");
        term.cursor_mut().row = 0;
        term.cursor_mut().col = 5;
        process(&mut term, b"\x1b[J");
        // Cols 0-4 should still have A-E
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some('A'));
        assert_eq!(term.grid().cell(0, 4).map(|c| c.c), Some('E'));
        // Cols 5+ should be blank
        assert_eq!(term.grid().cell(0, 5).map(|c| c.c), Some(' '));
    }

    #[test]
    fn test_csi_erase_display_all() {
        let mut term = Terminal::new(5, 10);
        process(&mut term, b"Hello");
        process(&mut term, b"\x1b[2J");
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some(' '));
    }

    #[test]
    fn test_csi_erase_line_right() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"Hello World");
        term.cursor_mut().col = 5;
        process(&mut term, b"\x1b[K");
        assert_eq!(term.grid().cell(0, 4).map(|c| c.c), Some('o'));
        assert_eq!(term.grid().cell(0, 5).map(|c| c.c), Some(' '));
    }

    #[test]
    fn test_csi_erase_line_left() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"Hello World");
        term.cursor_mut().col = 5;
        process(&mut term, b"\x1b[1K");
        assert_eq!(term.grid().cell(0, 5).map(|c| c.c), Some(' '));
        assert_eq!(term.grid().cell(0, 6).map(|c| c.c), Some('W'));
    }

    // ─── CSI: Insert / Delete ────────────────────────────────────

    #[test]
    fn test_csi_insert_blank_chars() {
        let mut term = Terminal::new(24, 10);
        process(&mut term, b"ABCDE");
        term.cursor_mut().col = 2;
        process(&mut term, b"\x1b[2@");
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some('A'));
        assert_eq!(term.grid().cell(0, 1).map(|c| c.c), Some('B'));
        assert_eq!(term.grid().cell(0, 2).map(|c| c.c), Some(' '));
        assert_eq!(term.grid().cell(0, 3).map(|c| c.c), Some(' '));
        assert_eq!(term.grid().cell(0, 4).map(|c| c.c), Some('C'));
    }

    #[test]
    fn test_csi_delete_chars() {
        let mut term = Terminal::new(24, 10);
        process(&mut term, b"ABCDE");
        term.cursor_mut().col = 1;
        process(&mut term, b"\x1b[2P");
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some('A'));
        assert_eq!(term.grid().cell(0, 1).map(|c| c.c), Some('D'));
    }

    #[test]
    fn test_csi_insert_lines() {
        let mut term = Terminal::new(5, 10);
        process(&mut term, b"Line0\r\nLine1\r\nLine2");
        term.cursor_mut().row = 1;
        term.cursor_mut().col = 0;
        process(&mut term, b"\x1b[1L");
        // Row 1 should now be blank
        assert_eq!(term.grid().cell(1, 0).map(|c| c.c), Some(' '));
        // Old row 1 pushed to row 2
        assert_eq!(term.grid().cell(2, 0).map(|c| c.c), Some('L'));
    }

    #[test]
    fn test_csi_delete_lines() {
        let mut term = Terminal::new(5, 10);
        process(&mut term, b"Line0\r\nLine1\r\nLine2");
        term.cursor_mut().row = 1;
        term.cursor_mut().col = 0;
        process(&mut term, b"\x1b[1M");
        // Row 1 should now have Line2's content
        assert_eq!(term.grid().cell(1, 0).map(|c| c.c), Some('L'));
    }

    // ─── CSI: Scroll ────────────────────────────────────────────

    #[test]
    fn test_csi_scroll_up() {
        let mut term = Terminal::new(5, 10);
        process(&mut term, b"Row0\r\nRow1\r\nRow2");
        process(&mut term, b"\x1b[1S");
        // Row 0 should now have Row1's content
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some('R'));
        assert_eq!(term.grid().cell(0, 3).map(|c| c.c), Some('1'));
    }

    #[test]
    fn test_csi_scroll_down() {
        let mut term = Terminal::new(5, 10);
        process(&mut term, b"Row0\r\nRow1\r\nRow2");
        process(&mut term, b"\x1b[1T");
        // Row 0 should now be blank
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some(' '));
        // Row 1 should have Row0's content
        assert_eq!(term.grid().cell(1, 0).map(|c| c.c), Some('R'));
        assert_eq!(term.grid().cell(1, 3).map(|c| c.c), Some('0'));
    }

    // ─── CSI: Erase Characters ──────────────────────────────────

    #[test]
    fn test_csi_erase_characters() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"ABCDE");
        term.cursor_mut().col = 1;
        process(&mut term, b"\x1b[3X");
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some('A'));
        assert_eq!(term.grid().cell(0, 1).map(|c| c.c), Some(' '));
        assert_eq!(term.grid().cell(0, 2).map(|c| c.c), Some(' '));
        assert_eq!(term.grid().cell(0, 3).map(|c| c.c), Some(' '));
        assert_eq!(term.grid().cell(0, 4).map(|c| c.c), Some('E'));
    }

    // ─── CSI: SGR ───────────────────────────────────────────────

    #[test]
    fn test_sgr_reset() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[1;3;4m"); // bold, italic, underline
        assert!(term.cursor().attrs.bold);
        assert!(term.cursor().attrs.italic);
        assert!(term.cursor().attrs.underline);

        process(&mut term, b"\x1b[0m"); // reset
        assert!(!term.cursor().attrs.bold);
        assert!(!term.cursor().attrs.italic);
        assert!(!term.cursor().attrs.underline);
    }

    #[test]
    fn test_sgr_bold_italic_underline() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[1m");
        assert!(term.cursor().attrs.bold);
        process(&mut term, b"\x1b[3m");
        assert!(term.cursor().attrs.italic);
        process(&mut term, b"\x1b[4m");
        assert!(term.cursor().attrs.underline);
    }

    #[test]
    fn test_sgr_dim_blink_inverse_hidden_strikethrough() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[2m");
        assert!(term.cursor().attrs.dim);
        process(&mut term, b"\x1b[5m");
        assert!(term.cursor().attrs.blink);
        process(&mut term, b"\x1b[7m");
        assert!(term.cursor().attrs.inverse);
        process(&mut term, b"\x1b[8m");
        assert!(term.cursor().attrs.hidden);
        process(&mut term, b"\x1b[9m");
        assert!(term.cursor().attrs.strikethrough);
    }

    #[test]
    fn test_sgr_disable_attributes() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[1;2;3;4;5;7;8;9m");
        process(&mut term, b"\x1b[22m"); // not bold/dim
        assert!(!term.cursor().attrs.bold);
        assert!(!term.cursor().attrs.dim);
        process(&mut term, b"\x1b[23m"); // not italic
        assert!(!term.cursor().attrs.italic);
        process(&mut term, b"\x1b[24m"); // not underline
        assert!(!term.cursor().attrs.underline);
        process(&mut term, b"\x1b[25m"); // not blink
        assert!(!term.cursor().attrs.blink);
        process(&mut term, b"\x1b[27m"); // not inverse
        assert!(!term.cursor().attrs.inverse);
        process(&mut term, b"\x1b[28m"); // not hidden
        assert!(!term.cursor().attrs.hidden);
        process(&mut term, b"\x1b[29m"); // not strikethrough
        assert!(!term.cursor().attrs.strikethrough);
    }

    #[test]
    fn test_sgr_standard_fg_colors() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[31m"); // red foreground
        assert_eq!(term.cursor().fg, Color::Named(NamedColor::Red));
    }

    #[test]
    fn test_sgr_standard_bg_colors() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[42m"); // green background
        assert_eq!(term.cursor().bg, Color::Named(NamedColor::Green));
    }

    #[test]
    fn test_sgr_bright_fg_colors() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[91m"); // bright red foreground
        assert_eq!(term.cursor().fg, Color::Named(NamedColor::BrightRed));
    }

    #[test]
    fn test_sgr_bright_bg_colors() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[102m"); // bright green background
        assert_eq!(term.cursor().bg, Color::Named(NamedColor::BrightGreen));
    }

    #[test]
    fn test_sgr_256_color_fg() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[38;5;196m"); // 256-color red
        assert_eq!(term.cursor().fg, Color::Indexed(196));
    }

    #[test]
    fn test_sgr_256_color_bg() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[48;5;82m"); // 256-color green
        assert_eq!(term.cursor().bg, Color::Indexed(82));
    }

    #[test]
    fn test_sgr_truecolor_fg() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[38;2;255;128;0m"); // orange
        assert_eq!(term.cursor().fg, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn test_sgr_truecolor_bg() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[48;2;0;100;200m");
        assert_eq!(term.cursor().bg, Color::Rgb(0, 100, 200));
    }

    #[test]
    fn test_sgr_default_colors() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[31;42m"); // red fg, green bg
        process(&mut term, b"\x1b[39m"); // default fg
        assert_eq!(term.cursor().fg, Color::Default);
        assert_eq!(term.cursor().bg, Color::Named(NamedColor::Green));
        process(&mut term, b"\x1b[49m"); // default bg
        assert_eq!(term.cursor().bg, Color::Default);
    }

    // ─── CSI: Modes ─────────────────────────────────────────────

    #[test]
    fn test_csi_show_hide_cursor() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[?25l"); // hide cursor
        assert!(!term.mode(Mode::ShowCursor));
        process(&mut term, b"\x1b[?25h"); // show cursor
        assert!(term.mode(Mode::ShowCursor));
    }

    #[test]
    fn test_csi_alternate_screen() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"A"); // write 'A' in primary
        process(&mut term, b"\x1b[?1049h"); // enter alt screen
        assert!(term.alt_screen_active());
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some(' ')); // alt is clear

        process(&mut term, b"\x1b[?1049l"); // leave alt screen
        assert!(!term.alt_screen_active());
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some('A')); // primary restored
    }

    #[test]
    fn test_csi_bracketed_paste() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[?2004h");
        assert!(term.mode(Mode::BracketedPaste));
        process(&mut term, b"\x1b[?2004l");
        assert!(!term.mode(Mode::BracketedPaste));
    }

    #[test]
    fn test_csi_insert_mode() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[4h"); // enable insert mode
        assert!(term.mode(Mode::Insert));
        process(&mut term, b"\x1b[4l"); // disable insert mode
        assert!(!term.mode(Mode::Insert));
    }

    // ─── CSI: Scroll Region ─────────────────────────────────────

    #[test]
    fn test_csi_set_scroll_region() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[5;20r"); // rows 5-20
        let (top, bottom) = term.scroll_region();
        assert_eq!(top, 4); // 0-indexed
        assert_eq!(bottom, 20); // exclusive
    }

    // ─── CSI: Tab Clear ─────────────────────────────────────────

    #[test]
    fn test_csi_tab_clear() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().col = 8;
        process(&mut term, b"\x1b[0g"); // clear tab at current position
        term.cursor_mut().col = 0;
        term.tab();
        // Should skip over cleared tab at 8, go to 16
        assert_eq!(term.cursor().col, 16);
    }

    #[test]
    fn test_csi_tab_clear_all() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b[3g"); // clear all tabs
        term.tab();
        // With no tab stops, should go to last column
        assert_eq!(term.cursor().col, 79);
    }

    // ─── OSC ────────────────────────────────────────────────────

    #[test]
    fn test_osc_set_title() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b]0;My Terminal\x07");
        assert_eq!(term.title(), "My Terminal");
    }

    #[test]
    fn test_osc_set_title_osc2() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1b]2;Window Title\x07");
        assert_eq!(term.title(), "Window Title");
    }

    // ─── ESC ────────────────────────────────────────────────────

    #[test]
    fn test_esc_save_restore_cursor() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().row = 5;
        term.cursor_mut().col = 10;
        process(&mut term, b"\x1b7"); // DECSC
        term.cursor_mut().row = 0;
        term.cursor_mut().col = 0;
        process(&mut term, b"\x1b8"); // DECRC
        assert_eq!(term.cursor().row, 5);
        assert_eq!(term.cursor().col, 10);
    }

    #[test]
    fn test_esc_reverse_index() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().row = 5;
        process(&mut term, b"\x1bM"); // RI
        assert_eq!(term.cursor().row, 4);
    }

    #[test]
    fn test_esc_full_reset() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"Hello");
        term.cursor_mut().attrs.bold = true;
        process(&mut term, b"\x1bc"); // RIS
        assert_eq!(term.cursor().col, 0);
        assert_eq!(term.cursor().row, 0);
        assert!(!term.cursor().attrs.bold);
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some(' '));
    }

    #[test]
    fn test_esc_tab_set() {
        let mut term = Terminal::new(24, 80);
        // Clear all tabs first
        term.clear_all_tab_stops();
        // Set tab at column 5
        term.cursor_mut().col = 5;
        process(&mut term, b"\x1bH"); // HTS
        // Go to col 0 and tab
        term.cursor_mut().col = 0;
        term.tab();
        assert_eq!(term.cursor().col, 5);
    }

    #[test]
    fn test_esc_index() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"\x1bD"); // IND
        assert_eq!(term.cursor().row, 1);
    }

    #[test]
    fn test_esc_next_line() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().col = 10;
        process(&mut term, b"\x1bE"); // NEL
        assert_eq!(term.cursor().row, 1);
        assert_eq!(term.cursor().col, 0);
    }

    #[test]
    fn test_esc_charset_designation() {
        let mut term = Terminal::new(24, 80);
        // Designate DEC Special Graphics to G0
        process(&mut term, b"\x1b(0");
        assert_eq!(
            term.charset().get(CharsetSlot::G0),
            Charset::DecSpecialGraphics
        );
        // Designate ASCII back to G0
        process(&mut term, b"\x1b(B");
        assert_eq!(term.charset().get(CharsetSlot::G0), Charset::Ascii);
    }

    // ─── Integration: Combined sequences ─────────────────────────

    #[test]
    fn test_colored_text() {
        let mut term = Terminal::new(24, 80);
        // Red foreground, write 'X', reset
        process(&mut term, b"\x1b[31mX\x1b[0m");
        let cell = term.grid().cell(0, 0);
        assert_eq!(cell.map(|c| c.c), Some('X'));
        assert_eq!(cell.map(|c| c.fg), Some(Color::Named(NamedColor::Red)));
    }

    #[test]
    fn test_cursor_movement_and_write() {
        let mut term = Terminal::new(24, 80);
        // Move to row 3, col 5 (1-indexed: 4, 6) then write
        process(&mut term, b"\x1b[4;6HA");
        assert_eq!(term.grid().cell(3, 5).map(|c| c.c), Some('A'));
    }

    #[test]
    fn test_erase_and_rewrite() {
        let mut term = Terminal::new(24, 80);
        process(&mut term, b"ABCDE");
        // Clear entire line
        process(&mut term, b"\x1b[2K");
        // Move to start and write new content
        process(&mut term, b"\x1b[1GFGH");
        assert_eq!(term.grid().cell(0, 0).map(|c| c.c), Some('F'));
        assert_eq!(term.grid().cell(0, 1).map(|c| c.c), Some('G'));
        assert_eq!(term.grid().cell(0, 2).map(|c| c.c), Some('H'));
    }
}
