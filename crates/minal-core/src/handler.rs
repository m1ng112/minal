//! VT escape sequence handler implementing `vte::Perform`.

use tracing::{debug, trace};

use crate::ansi::{Color, Mode, NamedColor, c0};
use crate::term::Terminal;

/// Handler for VT parser events.
///
/// Implements `vte::Perform` to process escape sequences and update
/// terminal state.
pub struct Handler<'a> {
    terminal: &'a mut Terminal,
}

impl<'a> Handler<'a> {
    /// Create a new handler wrapping a terminal.
    pub fn new(terminal: &'a mut Terminal) -> Self {
        Self { terminal }
    }
}

/// Extract the first parameter from vte::Params, defaulting to `default` if missing/zero.
fn param(params: &vte::Params, idx: usize, default: u16) -> u16 {
    params
        .iter()
        .nth(idx)
        .and_then(|p| p.first().copied())
        .map(|v| if v == 0 { default } else { v })
        .unwrap_or(default)
}

impl vte::Perform for Handler<'_> {
    fn print(&mut self, c: char) {
        trace!("print: {:?}", c);
        self.terminal.input_char(c);
    }

    fn execute(&mut self, byte: u8) {
        trace!("execute: {:#04x}", byte);
        match byte {
            c0::BS => self.terminal.backspace(),
            c0::HT => self.terminal.tab(),
            c0::LF | c0::VT | c0::FF => self.terminal.linefeed(),
            c0::CR => self.terminal.carriage_return(),
            c0::BEL => { /* bell - ignore for now */ }
            c0::SO => self.terminal.charset_mut().set_active(1),
            c0::SI => self.terminal.charset_mut().set_active(0),
            _ => trace!("unhandled C0: {:#04x}", byte),
        }
    }

    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        trace!(
            "hook: params={:?}, intermediates={:?}, ignore={}, action={:?}",
            params, intermediates, ignore, action
        );
    }

    fn put(&mut self, byte: u8) {
        trace!("put: {:#04x}", byte);
    }

    fn unhook(&mut self) {
        trace!("unhook");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        trace!(
            "osc_dispatch: params_count={}, bell={}",
            params.len(),
            bell_terminated
        );

        if params.is_empty() {
            return;
        }

        // Parse OSC command number
        let cmd = match std::str::from_utf8(params[0]) {
            Ok(s) => s,
            Err(_) => return,
        };

        match cmd {
            // OSC 0: Set icon name and window title
            // OSC 2: Set window title
            "0" | "2" => {
                if params.len() >= 2 {
                    if let Ok(title) = std::str::from_utf8(params[1]) {
                        self.terminal.set_title(title.to_string());
                        debug!("Set terminal title: {:?}", title);
                    }
                }
            }
            _ => trace!("unhandled OSC: {}", cmd),
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

        trace!(
            "csi_dispatch: action={:?}, params={:?}, intermediates={:?}",
            action, params, intermediates
        );

        let is_private = intermediates.first() == Some(&b'?');

        match (action, is_private) {
            // CUU - Cursor Up
            ('A', false) => {
                let n = param(params, 0, 1) as usize;
                let (top, _) = self.terminal.scroll_region();
                self.terminal.cursor_mut().move_up(n, top);
            }
            // CUD - Cursor Down
            ('B', false) => {
                let n = param(params, 0, 1) as usize;
                let (_, bottom) = self.terminal.scroll_region();
                self.terminal.cursor_mut().move_down(n, bottom);
            }
            // CUF - Cursor Forward
            ('C', false) => {
                let n = param(params, 0, 1) as usize;
                let cols = self.terminal.cols();
                self.terminal.cursor_mut().move_right(n, cols);
            }
            // CUB - Cursor Backward
            ('D', false) => {
                let n = param(params, 0, 1) as usize;
                self.terminal.cursor_mut().move_left(n);
            }
            // CNL - Cursor Next Line
            ('E', false) => {
                let n = param(params, 0, 1) as usize;
                let (_, bottom) = self.terminal.scroll_region();
                self.terminal.cursor_mut().move_down(n, bottom);
                self.terminal.carriage_return();
            }
            // CPL - Cursor Previous Line
            ('F', false) => {
                let n = param(params, 0, 1) as usize;
                let (top, _) = self.terminal.scroll_region();
                self.terminal.cursor_mut().move_up(n, top);
                self.terminal.carriage_return();
            }
            // CHA - Cursor Horizontal Absolute
            ('G', false) => {
                let col = param(params, 0, 1) as usize;
                let cols = self.terminal.cols();
                self.terminal
                    .cursor_mut()
                    .goto_col(col.saturating_sub(1), cols);
            }
            // CUP - Cursor Position / HVP
            ('H' | 'f', false) => {
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
            // ED - Erase in Display
            ('J', false) => {
                let mode = param(params, 0, 0);
                match mode {
                    0 => self.terminal.erase_display_below(),
                    1 => self.terminal.erase_display_above(),
                    2 | 3 => self.terminal.erase_display_all(),
                    _ => trace!("unhandled ED mode: {}", mode),
                }
            }
            // EL - Erase in Line
            ('K', false) => {
                let mode = param(params, 0, 0);
                match mode {
                    0 => self.terminal.erase_line_right(),
                    1 => self.terminal.erase_line_left(),
                    2 => self.terminal.erase_line_all(),
                    _ => trace!("unhandled EL mode: {}", mode),
                }
            }
            // IL - Insert Lines
            ('L', false) => {
                let n = param(params, 0, 1) as usize;
                self.terminal.insert_blank_lines(n);
            }
            // DL - Delete Lines
            ('M', false) => {
                let n = param(params, 0, 1) as usize;
                self.terminal.delete_lines(n);
            }
            // DCH - Delete Characters
            ('P', false) => {
                let n = param(params, 0, 1) as usize;
                self.terminal.delete_chars(n);
            }
            // SU - Scroll Up
            ('S', false) => {
                let n = param(params, 0, 1) as usize;
                self.terminal.scroll_up_lines(n);
            }
            // SD - Scroll Down
            ('T', false) => {
                let n = param(params, 0, 1) as usize;
                self.terminal.scroll_down_lines(n);
            }
            // ECH - Erase Characters
            ('X', false) => {
                let n = param(params, 0, 1) as usize;
                self.terminal.erase_chars(n);
            }
            // ICH - Insert Blank Characters
            ('@', false) => {
                let n = param(params, 0, 1) as usize;
                self.terminal.insert_blank_chars(n);
            }
            // VPA - Vertical Position Absolute
            ('d', false) => {
                let row = param(params, 0, 1) as usize;
                let rows = self.terminal.rows();
                self.terminal
                    .cursor_mut()
                    .goto_row(row.saturating_sub(1), rows);
            }
            // TBC - Tab Clear
            ('g', false) => {
                let mode = param(params, 0, 0);
                match mode {
                    0 => self.terminal.clear_tab_stop(),
                    3 => self.terminal.clear_all_tab_stops(),
                    _ => trace!("unhandled TBC mode: {}", mode),
                }
            }
            // DECSET - DEC Private Mode Set
            ('h', true) => {
                for p in params.iter() {
                    if let Some(&mode_num) = p.first() {
                        if let Some(mode) = dec_mode(mode_num) {
                            self.terminal.set_mode(mode);
                        }
                    }
                }
            }
            // SM - Set Mode (standard)
            ('h', false) => {
                for p in params.iter() {
                    if let Some(&mode_num) = p.first() {
                        match mode_num {
                            4 => self.terminal.set_mode(Mode::Insert),
                            20 => self.terminal.set_mode(Mode::LineFeedNewLine),
                            _ => trace!("unhandled SM mode: {}", mode_num),
                        }
                    }
                }
            }
            // DECRST - DEC Private Mode Reset
            ('l', true) => {
                for p in params.iter() {
                    if let Some(&mode_num) = p.first() {
                        if let Some(mode) = dec_mode(mode_num) {
                            self.terminal.unset_mode(mode);
                        }
                    }
                }
            }
            // RM - Reset Mode (standard)
            ('l', false) => {
                for p in params.iter() {
                    if let Some(&mode_num) = p.first() {
                        match mode_num {
                            4 => self.terminal.unset_mode(Mode::Insert),
                            20 => self.terminal.unset_mode(Mode::LineFeedNewLine),
                            _ => trace!("unhandled RM mode: {}", mode_num),
                        }
                    }
                }
            }
            // SGR - Select Graphic Rendition
            ('m', false) => {
                self.handle_sgr(params);
            }
            // DSR - Device Status Report (just log)
            ('n', false) => {
                let mode = param(params, 0, 0);
                debug!("DSR request: mode={}", mode);
            }
            // DECSTBM - Set Scrolling Region
            ('r', false) => {
                let top = param(params, 0, 1) as usize;
                let rows = self.terminal.rows();
                let bottom = param(params, 1, rows as u16) as usize;
                self.terminal.set_scroll_region(top, bottom);
            }
            // DECSC - Save Cursor (via CSI)
            ('s', false) => {
                self.terminal.cursor_mut().save();
            }
            // DECRC - Restore Cursor (via CSI)
            ('u', false) => {
                let rows = self.terminal.rows();
                let cols = self.terminal.cols();
                self.terminal.cursor_mut().restore(rows, cols);
            }
            _ => {
                trace!("unhandled CSI: action={:?}, private={}", action, is_private);
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }

        trace!(
            "esc_dispatch: byte={:#04x}, intermediates={:?}",
            byte, intermediates
        );

        match (byte, intermediates) {
            // DECSC - Save Cursor
            (b'7', []) => {
                self.terminal.cursor_mut().save();
            }
            // DECRC - Restore Cursor
            (b'8', []) => {
                let rows = self.terminal.rows();
                let cols = self.terminal.cols();
                self.terminal.cursor_mut().restore(rows, cols);
            }
            // RIS - Full Reset
            (b'c', []) => {
                self.terminal.reset();
            }
            // IND - Index (move cursor down, scroll if at bottom)
            (b'D', []) => {
                self.terminal.linefeed();
            }
            // NEL - Next Line (move to start of next line)
            (b'E', []) => {
                self.terminal.linefeed();
                self.terminal.carriage_return();
            }
            // HTS - Horizontal Tab Set
            (b'H', []) => {
                self.terminal.set_tab_stop();
            }
            // RI - Reverse Index
            (b'M', []) => {
                self.terminal.reverse_index();
            }
            // Charset designation G0-G3
            (b'B', [b'(']) | (b'0', [b'(']) => {
                // G0 charset: B=ASCII, 0=DEC Special Graphics
                let charset = if byte == b'0' { 1 } else { 0 };
                self.terminal.charset_mut().designate(0, charset);
            }
            (b'B', [b')']) | (b'0', [b')']) => {
                let charset = if byte == b'0' { 1 } else { 0 };
                self.terminal.charset_mut().designate(1, charset);
            }
            (b'B', [b'*']) | (b'0', [b'*']) => {
                let charset = if byte == b'0' { 1 } else { 0 };
                self.terminal.charset_mut().designate(2, charset);
            }
            (b'B', [b'+']) | (b'0', [b'+']) => {
                let charset = if byte == b'0' { 1 } else { 0 };
                self.terminal.charset_mut().designate(3, charset);
            }
            _ => {
                trace!(
                    "unhandled ESC: byte={:#04x}, intermediates={:?}",
                    byte, intermediates
                );
            }
        }
    }
}

impl Handler<'_> {
    /// Handle SGR (Select Graphic Rendition) parameters.
    fn handle_sgr(&mut self, params: &vte::Params) {
        let mut iter = params.iter();

        // If no params, reset
        if params.iter().next().is_none() {
            self.terminal.cursor_mut().attrs.reset();
            self.terminal.cursor_mut().fg = Color::Default;
            self.terminal.cursor_mut().bg = Color::Default;
            return;
        }

        while let Some(sub) = iter.next() {
            let code = sub.first().copied().unwrap_or(0);

            match code {
                0 => {
                    self.terminal.cursor_mut().attrs.reset();
                    self.terminal.cursor_mut().fg = Color::Default;
                    self.terminal.cursor_mut().bg = Color::Default;
                }
                1 => self.terminal.cursor_mut().attrs.bold = true,
                2 => self.terminal.cursor_mut().attrs.dim = true,
                3 => self.terminal.cursor_mut().attrs.italic = true,
                4 => self.terminal.cursor_mut().attrs.underline = true,
                5 => self.terminal.cursor_mut().attrs.blink = true,
                7 => self.terminal.cursor_mut().attrs.inverse = true,
                8 => self.terminal.cursor_mut().attrs.hidden = true,
                9 => self.terminal.cursor_mut().attrs.strikethrough = true,
                21 => self.terminal.cursor_mut().attrs.bold = false,
                22 => {
                    self.terminal.cursor_mut().attrs.bold = false;
                    self.terminal.cursor_mut().attrs.dim = false;
                }
                23 => self.terminal.cursor_mut().attrs.italic = false,
                24 => self.terminal.cursor_mut().attrs.underline = false,
                25 => self.terminal.cursor_mut().attrs.blink = false,
                27 => self.terminal.cursor_mut().attrs.inverse = false,
                28 => self.terminal.cursor_mut().attrs.hidden = false,
                29 => self.terminal.cursor_mut().attrs.strikethrough = false,
                // Foreground colors (30-37)
                30..=37 => {
                    self.terminal.cursor_mut().fg = named_color(code - 30);
                }
                38 => {
                    // Extended foreground color
                    if let Some(color) = parse_extended_color(&mut iter) {
                        self.terminal.cursor_mut().fg = color;
                    }
                }
                39 => self.terminal.cursor_mut().fg = Color::Default,
                // Background colors (40-47)
                40..=47 => {
                    self.terminal.cursor_mut().bg = named_color(code - 40);
                }
                48 => {
                    // Extended background color
                    if let Some(color) = parse_extended_color(&mut iter) {
                        self.terminal.cursor_mut().bg = color;
                    }
                }
                49 => self.terminal.cursor_mut().bg = Color::Default,
                // Bright foreground (90-97)
                90..=97 => {
                    self.terminal.cursor_mut().fg = named_color(code - 90 + 8);
                }
                // Bright background (100-107)
                100..=107 => {
                    self.terminal.cursor_mut().bg = named_color(code - 100 + 8);
                }
                _ => trace!("unhandled SGR code: {}", code),
            }
        }
    }
}

/// Map a color index (0-15) to a named Color.
fn named_color(index: u16) -> Color {
    match index {
        0 => Color::Named(NamedColor::Black),
        1 => Color::Named(NamedColor::Red),
        2 => Color::Named(NamedColor::Green),
        3 => Color::Named(NamedColor::Yellow),
        4 => Color::Named(NamedColor::Blue),
        5 => Color::Named(NamedColor::Magenta),
        6 => Color::Named(NamedColor::Cyan),
        7 => Color::Named(NamedColor::White),
        8 => Color::Named(NamedColor::BrightBlack),
        9 => Color::Named(NamedColor::BrightRed),
        10 => Color::Named(NamedColor::BrightGreen),
        11 => Color::Named(NamedColor::BrightYellow),
        12 => Color::Named(NamedColor::BrightBlue),
        13 => Color::Named(NamedColor::BrightMagenta),
        14 => Color::Named(NamedColor::BrightCyan),
        15 => Color::Named(NamedColor::BrightWhite),
        _ => Color::Default,
    }
}

/// Parse extended color sequences (38;5;N or 38;2;R;G;B).
fn parse_extended_color<'b>(iter: &mut impl Iterator<Item = &'b [u16]>) -> Option<Color> {
    let sub = iter.next()?;
    let kind = sub.first().copied()?;
    match kind {
        // 256-color
        5 => {
            let idx_sub = iter.next()?;
            let idx = idx_sub.first().copied()?;
            Some(Color::Indexed(idx as u8))
        }
        // TrueColor
        2 => {
            let r_sub = iter.next()?;
            let r = r_sub.first().copied()? as u8;
            let g_sub = iter.next()?;
            let g = g_sub.first().copied()? as u8;
            let b_sub = iter.next()?;
            let b = b_sub.first().copied()? as u8;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

/// Map DEC private mode number to Mode enum.
fn dec_mode(num: u16) -> Option<Mode> {
    match num {
        1 => Some(Mode::CursorKeys),
        6 => Some(Mode::Origin),
        7 => Some(Mode::AutoWrap),
        25 => Some(Mode::ShowCursor),
        47 | 1047 | 1049 => Some(Mode::AlternateScreen),
        1000 => Some(Mode::MouseReport),
        1002 => Some(Mode::MouseMotion),
        1003 => Some(Mode::MouseAll),
        1004 => Some(Mode::FocusTracking),
        1006 => Some(Mode::SgrMouse),
        2004 => Some(Mode::BracketedPaste),
        _ => {
            trace!("unhandled DEC private mode: {}", num);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::{Color, NamedColor};
    use crate::term::Terminal;

    fn parse(terminal: &mut Terminal, data: &[u8]) {
        let mut parser = vte::Parser::new();
        for &byte in data {
            let mut handler = Handler::new(terminal);
            parser.advance(&mut handler, byte);
        }
    }

    #[test]
    fn test_print() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"Hello");
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'H');
        assert_eq!(term.grid().cell(0, 4).unwrap().c, 'o');
        assert_eq!(term.cursor().col, 5);
    }

    #[test]
    fn test_crlf() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"AB\r\nCD");
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'A');
        assert_eq!(term.grid().cell(0, 1).unwrap().c, 'B');
        assert_eq!(term.grid().cell(1, 0).unwrap().c, 'C');
        assert_eq!(term.grid().cell(1, 1).unwrap().c, 'D');
    }

    #[test]
    fn test_cursor_up() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().row = 5;
        parse(&mut term, b"\x1b[3A");
        assert_eq!(term.cursor().row, 2);
    }

    #[test]
    fn test_cursor_down() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[5B");
        assert_eq!(term.cursor().row, 5);
    }

    #[test]
    fn test_cursor_forward() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[10C");
        assert_eq!(term.cursor().col, 10);
    }

    #[test]
    fn test_cursor_backward() {
        let mut term = Terminal::new(24, 80);
        term.cursor_mut().col = 20;
        parse(&mut term, b"\x1b[5D");
        assert_eq!(term.cursor().col, 15);
    }

    #[test]
    fn test_cursor_position() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[10;20H");
        assert_eq!(term.cursor().row, 9);
        assert_eq!(term.cursor().col, 19);
    }

    #[test]
    fn test_erase_display() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"Hello");
        parse(&mut term, b"\x1b[2J");
        assert_eq!(term.grid().cell(0, 0).unwrap().c, ' ');
    }

    #[test]
    fn test_erase_line() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"Hello World");
        parse(&mut term, b"\x1b[5G"); // move to col 4
        parse(&mut term, b"\x1b[K"); // erase to end of line
        assert_eq!(term.grid().cell(0, 3).unwrap().c, 'l');
        assert_eq!(term.grid().cell(0, 4).unwrap().c, ' ');
    }

    #[test]
    fn test_sgr_bold() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[1mA");
        assert!(term.grid().cell(0, 0).unwrap().attrs.bold);
    }

    #[test]
    fn test_sgr_fg_color() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[31mA");
        assert_eq!(
            term.grid().cell(0, 0).unwrap().fg,
            Color::Named(NamedColor::Red)
        );
    }

    #[test]
    fn test_sgr_256_color() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[38;5;196mA");
        assert_eq!(term.grid().cell(0, 0).unwrap().fg, Color::Indexed(196));
    }

    #[test]
    fn test_sgr_truecolor() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[38;2;255;128;64mA");
        assert_eq!(term.grid().cell(0, 0).unwrap().fg, Color::Rgb(255, 128, 64));
    }

    #[test]
    fn test_sgr_reset() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[1;31mA\x1b[0mB");
        assert!(term.grid().cell(0, 0).unwrap().attrs.bold);
        assert_eq!(
            term.grid().cell(0, 0).unwrap().fg,
            Color::Named(NamedColor::Red)
        );
        assert!(!term.grid().cell(0, 1).unwrap().attrs.bold);
        assert_eq!(term.grid().cell(0, 1).unwrap().fg, Color::Default);
    }

    #[test]
    fn test_decset_alternate_screen() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"A"); // write 'A' to primary
        parse(&mut term, b"\x1b[?1049h"); // enter alt screen
        assert!(term.alt_screen_active());
        parse(&mut term, b"\x1b[?1049l"); // leave alt screen
        assert!(!term.alt_screen_active());
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'A');
    }

    #[test]
    fn test_scroll_region() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[5;20r");
        assert_eq!(term.scroll_region(), (4, 20));
    }

    #[test]
    fn test_osc_title() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b]0;My Terminal\x07");
        assert_eq!(term.title(), "My Terminal");
    }

    #[test]
    fn test_esc_save_restore_cursor() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"\x1b[10;20H"); // move to row 9, col 19
        parse(&mut term, b"\x1b7"); // save cursor
        parse(&mut term, b"\x1b[1;1H"); // move to 0,0
        parse(&mut term, b"\x1b8"); // restore cursor
        assert_eq!(term.cursor().row, 9);
        assert_eq!(term.cursor().col, 19);
    }

    #[test]
    fn test_esc_ris() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"Hello");
        parse(&mut term, b"\x1bc"); // RIS
        assert_eq!(term.grid().cell(0, 0).unwrap().c, ' ');
        assert_eq!(term.cursor().col, 0);
        assert_eq!(term.cursor().row, 0);
    }

    #[test]
    fn test_insert_blank_chars() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"ABCDE");
        parse(&mut term, b"\x1b[3G"); // col 2
        parse(&mut term, b"\x1b[2@"); // insert 2 blanks
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'A');
        assert_eq!(term.grid().cell(0, 1).unwrap().c, 'B');
        assert_eq!(term.grid().cell(0, 2).unwrap().c, ' ');
        assert_eq!(term.grid().cell(0, 3).unwrap().c, ' ');
        assert_eq!(term.grid().cell(0, 4).unwrap().c, 'C');
    }

    #[test]
    fn test_backspace() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"AB\x08C");
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'A');
        assert_eq!(term.grid().cell(0, 1).unwrap().c, 'C');
    }

    #[test]
    fn test_tab() {
        let mut term = Terminal::new(24, 80);
        parse(&mut term, b"A\tB");
        assert_eq!(term.grid().cell(0, 0).unwrap().c, 'A');
        assert_eq!(term.cursor().col, 9); // 'B' at col 8, cursor at 9
    }
}
