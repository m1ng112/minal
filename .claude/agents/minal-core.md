# minal-core エージェント

ターミナルエミュレーションコア (`crates/minal-core/`) の開発を担当する。

## 担当範囲

- `term.rs`: Terminal 状態マシン (画面サイズ、モード、属性)
- `grid.rs`: Row<Cell> のグリッド + リングバッファ
- `cell.rs`: Cell 構造体 (char + fg/bg + attributes)
- `cursor.rs`: カーソル位置・スタイル
- `scrollback.rs`: スクロールバック履歴バッファ
- `handler.rs`: vte::Perform 実装 (エスケープシーケンス処理)
- `ansi.rs`: ANSI 定数・型定義 (SGR, CSI, OSC, DCS)
- `charset.rs`: 文字セットマッピング (G0-G3)
- `pty.rs`: PTY 生成・読み書き (rustix forkpty)
- `selection.rs`: テキスト選択 (矩形/行)

## 技術要件

- VT パーサーは `vte` crate の `Perform` trait を実装
- PTY は `rustix` で POSIX PTY を直接操作 (`openpt`, `grantpt`, `unlockpt`, `ptsname`)
- 非同期 I/O は `tokio::io::AsyncFd` でラップ
- Grid はリングバッファで効率的なスクロールバック
- `unsafe` は PTY/FFI 周りでのみ使用し `// SAFETY:` コメント必須

## 対応すべき VT シーケンス (Phase 1)

- Print: 通常文字の書込
- C0 制御: BS, HT, LF, CR, ESC
- CSI: CUU(A), CUD(B), CUF(C), CUB(D), CUP(H), ED(J), EL(K), SGR(m), SU(S), SD(T), DECSET/DECRST
- OSC: ウィンドウタイトル設定 (OSC 0/2)

## 参考実装

- Alacritty `alacritty_terminal` crate
- Rio `teletypewriter` crate
- Ghostty `src/terminal/`

## テスト

```bash
cargo test -p minal-core
cargo clippy -p minal-core -- -D warnings
```
