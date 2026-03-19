//! Mouse protocol types and encoding for terminal mouse reporting.
//!
//! Supports X10 (basic) and SGR (extended, DECSET 1006) mouse protocols.

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

/// Mouse event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventKind {
    Press,
    Release,
    Motion,
}

/// Modifier key state during a mouse event.
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseModifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// A mouse event in terminal grid coordinates (0-indexed).
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub button: MouseButton,
    /// 0-indexed column.
    pub col: usize,
    /// 0-indexed row.
    pub row: usize,
    pub modifiers: MouseModifiers,
}

impl MouseEvent {
    /// Compute the X10 button byte (before adding 32 for the wire format).
    fn button_code(&self) -> u8 {
        let base = match self.button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::WheelUp => 64,
            MouseButton::WheelDown => 65,
        };

        // Release uses button code 3 (except for wheel events).
        let base = if self.kind == MouseEventKind::Release
            && !matches!(self.button, MouseButton::WheelUp | MouseButton::WheelDown)
        {
            3
        } else {
            base
        };

        let mut code = base;
        if self.modifiers.shift {
            code += 4;
        }
        if self.modifiers.alt {
            code += 8;
        }
        if self.modifiers.ctrl {
            code += 16;
        }
        if self.kind == MouseEventKind::Motion {
            code += 32;
        }
        code
    }
}

/// Encode a mouse event using the X10 protocol.
///
/// Format: `ESC [ M Cb Cx Cy`
/// - Cb = button_code + 32
/// - Cx = col + 32 + 1  (1-indexed, offset by 32)
/// - Cy = row + 32 + 1  (1-indexed, offset by 32)
///
/// Coordinates are capped at 223 (255 - 32).
pub fn encode_x10(event: &MouseEvent) -> Vec<u8> {
    let cb = event.button_code() + 32;
    // Cap coordinates at 222 since X10 uses a single byte with +32+1 offset.
    let cx = (event.col.min(222) as u8) + 32 + 1;
    let cy = (event.row.min(222) as u8) + 32 + 1;
    vec![0x1b, b'[', b'M', cb, cx, cy]
}

/// Encode a mouse event using the SGR protocol (DECSET 1006).
///
/// Format: `ESC [ < Cb ; Cx ; Cy M` (press/motion) or `ESC [ < Cb ; Cx ; Cy m` (release)
/// - Cb = button code (no +32 offset)
/// - Cx = col + 1 (1-indexed)
/// - Cy = row + 1 (1-indexed)
///
/// No coordinate limit (uses decimal numbers).
pub fn encode_sgr(event: &MouseEvent) -> Vec<u8> {
    let cb = event.button_code();
    let cx = event.col + 1;
    let cy = event.row + 1;
    let suffix = if event.kind == MouseEventKind::Release {
        'm'
    } else {
        'M'
    };
    format!("\x1b[<{cb};{cx};{cy}{suffix}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(kind: MouseEventKind, button: MouseButton, col: usize, row: usize) -> MouseEvent {
        MouseEvent {
            kind,
            button,
            col,
            row,
            modifiers: MouseModifiers::default(),
        }
    }

    #[test]
    fn test_x10_left_press() {
        let event = make_event(MouseEventKind::Press, MouseButton::Left, 0, 0);
        let encoded = encode_x10(&event);
        // button_code = 0, +32 = 32; col = 0+32+1=33; row = 0+32+1=33
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 32, 33, 33]);
    }

    #[test]
    fn test_x10_right_press() {
        let event = make_event(MouseEventKind::Press, MouseButton::Right, 5, 10);
        let encoded = encode_x10(&event);
        // button_code = 2, +32 = 34; col = 5+33=38; row = 10+33=43
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 34, 38, 43]);
    }

    #[test]
    fn test_x10_release() {
        let event = make_event(MouseEventKind::Release, MouseButton::Left, 0, 0);
        let encoded = encode_x10(&event);
        // Release uses button code 3, +32 = 35
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 35, 33, 33]);
    }

    #[test]
    fn test_x10_wheel_up() {
        let event = make_event(MouseEventKind::Press, MouseButton::WheelUp, 0, 0);
        let encoded = encode_x10(&event);
        // WheelUp = 64, +32 = 96
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 96, 33, 33]);
    }

    #[test]
    fn test_x10_wheel_down() {
        let event = make_event(MouseEventKind::Press, MouseButton::WheelDown, 0, 0);
        let encoded = encode_x10(&event);
        // WheelDown = 65, +32 = 97
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 97, 33, 33]);
    }

    #[test]
    fn test_x10_coordinate_capping() {
        let event = make_event(MouseEventKind::Press, MouseButton::Left, 300, 300);
        let encoded = encode_x10(&event);
        // Coordinates capped at 222, so +32+1 = 255
        assert_eq!(encoded[4], 255);
        assert_eq!(encoded[5], 255);
    }

    #[test]
    fn test_x10_with_shift() {
        let event = MouseEvent {
            kind: MouseEventKind::Press,
            button: MouseButton::Left,
            col: 0,
            row: 0,
            modifiers: MouseModifiers {
                shift: true,
                alt: false,
                ctrl: false,
            },
        };
        let encoded = encode_x10(&event);
        // Left + shift = 0 + 4 = 4, +32 = 36
        assert_eq!(encoded[3], 36);
    }

    #[test]
    fn test_x10_with_ctrl() {
        let event = MouseEvent {
            kind: MouseEventKind::Press,
            button: MouseButton::Left,
            col: 0,
            row: 0,
            modifiers: MouseModifiers {
                shift: false,
                alt: false,
                ctrl: true,
            },
        };
        let encoded = encode_x10(&event);
        // Left + ctrl = 0 + 16 = 16, +32 = 48
        assert_eq!(encoded[3], 48);
    }

    #[test]
    fn test_x10_motion() {
        let event = MouseEvent {
            kind: MouseEventKind::Motion,
            button: MouseButton::Left,
            col: 5,
            row: 10,
            modifiers: MouseModifiers::default(),
        };
        let encoded = encode_x10(&event);
        // Left + motion = 0 + 32 = 32, +32 = 64
        assert_eq!(encoded[3], 64);
    }

    #[test]
    fn test_sgr_left_press() {
        let event = make_event(MouseEventKind::Press, MouseButton::Left, 0, 0);
        let encoded = encode_sgr(&event);
        assert_eq!(encoded, b"\x1b[<0;1;1M");
    }

    #[test]
    fn test_sgr_right_press() {
        let event = make_event(MouseEventKind::Press, MouseButton::Right, 5, 10);
        let encoded = encode_sgr(&event);
        assert_eq!(encoded, b"\x1b[<2;6;11M");
    }

    #[test]
    fn test_sgr_release() {
        let event = make_event(MouseEventKind::Release, MouseButton::Left, 0, 0);
        let encoded = encode_sgr(&event);
        // Release uses lowercase 'm'
        assert_eq!(encoded, b"\x1b[<3;1;1m");
    }

    #[test]
    fn test_sgr_wheel_up() {
        let event = make_event(MouseEventKind::Press, MouseButton::WheelUp, 10, 20);
        let encoded = encode_sgr(&event);
        assert_eq!(encoded, b"\x1b[<64;11;21M");
    }

    #[test]
    fn test_sgr_large_coordinates() {
        let event = make_event(MouseEventKind::Press, MouseButton::Left, 500, 300);
        let encoded = encode_sgr(&event);
        assert_eq!(encoded, b"\x1b[<0;501;301M");
    }

    #[test]
    fn test_sgr_motion() {
        let event = MouseEvent {
            kind: MouseEventKind::Motion,
            button: MouseButton::Left,
            col: 5,
            row: 10,
            modifiers: MouseModifiers::default(),
        };
        let encoded = encode_sgr(&event);
        // motion adds 32 to button code: 0 + 32 = 32
        assert_eq!(encoded, b"\x1b[<32;6;11M");
    }

    #[test]
    fn test_sgr_with_modifiers() {
        let event = MouseEvent {
            kind: MouseEventKind::Press,
            button: MouseButton::Left,
            col: 0,
            row: 0,
            modifiers: MouseModifiers {
                shift: true,
                alt: true,
                ctrl: true,
            },
        };
        let encoded = encode_sgr(&event);
        // shift(4) + alt(8) + ctrl(16) = 28
        assert_eq!(encoded, b"\x1b[<28;1;1M");
    }
}
