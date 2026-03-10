# VT シーケンス追加スキル

新しい VT エスケープシーケンスのサポートを追加する。

## 手順

1. `crates/minal-core/src/ansi.rs` にシーケンスの定数/型を定義
2. `crates/minal-core/src/handler.rs` の `vte::Perform` 実装にハンドラーを追加
3. `crates/minal-core/src/term.rs` の Terminal 状態マシンに処理を実装
4. 必要に応じて `grid.rs`, `cursor.rs` 等を更新
5. テストを追加 (`cargo test -p minal-core`)

## VT シーケンスカテゴリ

- **C0 制御**: BS(0x08), HT(0x09), LF(0x0A), CR(0x0D), ESC(0x1B)
- **CSI** (`ESC [`): カーソル移動, 消去, SGR, スクロール, モード設定
- **OSC** (`ESC ]`): タイトル設定, Shell Integration (133), カラー設定
- **DCS** (`ESC P`): Sixel, XTGETTCAP 等

## 参考リソース

- vte crate: `Perform` trait の `print`, `execute`, `csi_dispatch`, `osc_dispatch`, `esc_dispatch`
- Alacritty `alacritty_terminal/src/term/mod.rs`
- https://invisible-island.net/xterm/ctlseqs/ctlseqs.html

## テスト例

```rust
#[test]
fn test_cursor_up() {
    let mut term = Terminal::new(80, 24);
    term.set_cursor(5, 10);
    // CSI 3 A (カーソル3行上へ)
    term.process(b"\x1b[3A");
    assert_eq!(term.cursor().row, 7);
}
```
