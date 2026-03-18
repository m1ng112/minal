---
name: add-vt-sequence
description: "Add support for a new VT escape sequence to the terminal emulator core. Use when implementing CSI, OSC, DCS, or C0 control sequences."
argument-hint: "[sequence-name]"
---

Add support for the VT escape sequence `$ARGUMENTS` to `crates/minal-core/`.

## Steps

1. Define constants/types in `crates/minal-core/src/ansi.rs`
2. Add handler in `crates/minal-core/src/handler.rs` (`vte::Perform` implementation)
3. Implement processing in `crates/minal-core/src/term.rs` Terminal state machine
4. Update `grid.rs`, `cursor.rs`, or other modules as needed
5. Add tests and run `cargo test -p minal-core`

## VT Sequence Categories

- **C0 control**: BS(0x08), HT(0x09), LF(0x0A), CR(0x0D), ESC(0x1B)
- **CSI** (`ESC [`): Cursor movement, erase, SGR, scroll, mode set
- **OSC** (`ESC ]`): Title, Shell Integration (133), color settings
- **DCS** (`ESC P`): Sixel, XTGETTCAP, etc.

## vte Perform Trait Methods

- `print(char)`: Printable character
- `execute(byte)`: C0 control character
- `csi_dispatch(params, intermediates, ignore, action)`: CSI sequence
- `osc_dispatch(params, bell_terminated)`: OSC sequence
- `esc_dispatch(intermediates, ignore, byte)`: ESC sequence

## Reference

- Alacritty: `alacritty_terminal/src/term/mod.rs`
- XTerm control sequences: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html

## Test Template

```rust
#[test]
fn test_sequence_name() {
    let mut term = Terminal::new(80, 24);
    term.set_cursor(5, 10);
    // Describe what the sequence does
    term.process(b"\x1b[..."); // Replace with actual sequence
    assert_eq!(term.cursor().row, expected_row);
}
```
