# Minal - AI特化ターミナルエミュレータ 詳細実装計画

## コンセプト

既存のターミナル（Wezterm, Ghostty, Alacritty）は高速・高機能だが、AI時代の開発ワークフローに最適化されていない。
Minal は AI をファーストクラスで統合したターミナルエミュレータを目指す。

## OSS 調査結果

### Alacritty (Rust, OpenGL)
- **構成**: 4 crate workspace (`alacritty`, `alacritty_terminal`, `alacritty_config`, `alacritty_config_derive`)
- **レンダリング**: glutin + OpenGL (GL ES 2.0)
- **フォント**: crossfont (FreeType/CoreText ラッパー)
- **ウィンドウ**: winit
- **VT パーサー**: 自作の状態マシンベース (Paul Williams の ANSI パーサーテーブル準拠)
- **特徴**: シンプル・高速・最小機能主義。タブ/ペイン機能なし

### Wezterm (Rust, OpenGL/Metal)
- **構成**: 20+ crate の大規模 workspace (`wezterm-gui`, `term`, `termwiz`, `mux`, `pty`, `wezterm-font` 等)
- **レンダリング**: 独自 GUI フレームワーク、OpenGL + Metal (macOS)
- **フォント**: 独自 (`wezterm-font`)、FreeType/CoreText/DirectWrite
- **VT パーサー**: `termwiz` crate (完全な xterm 互換、sixel 対応)
- **特徴**: 多機能 (タブ、ペイン、マルチプレクサ、Lua 設定)。コード量が非常に多い

### Rio (Rust, wgpu)
- **構成**: 8 crate workspace (`frontends/rioterm`, `rio-backend`, `sugarloaf`, `teletypewriter` 等)
- **レンダリング**: **wgpu** ベースの独自エンジン `sugarloaf`
- **フォント**: skrifa + font-kit + ttf-parser
- **VT パーサー**: Alacritty ベースのフォーク
- **特徴**: wgpu 採用の先進的アーキテクチャ。WASM 対応を視野

### Ghostty (Zig + Swift, Metal/OpenGL)
- **構成**: Zig モノリポ + macOS SwiftUI (`src/terminal/`, `src/renderer/`, `src/termio/`, `src/font/` 等)
- **レンダリング**: Metal (macOS) / OpenGL 4.3 (Linux) / WebGL (WASM)。Triple-buffered swap chain
- **フォント**: FreeType + HarfBuzz + Fontconfig。テクスチャアトラス方式
- **VT パーサー**: 自作 DEC ANSI 状態マシン (15状態、ルックアップテーブル駆動)
- **PTY**: POSIX `openpty()` 直接使用。libxev でイベント駆動
- **スレッドモデル**: **3スレッド** (メイン / I/O / レンダラー)、SPSC メールボックスで通信
- **特徴**: 高速・macOS ネイティブ統合が秀逸。レンダラー 120fps。libxev 独自イベントループ

### 設計判断への反映

| 判断ポイント | 選択 | 理由 | 参考 OSS |
|-------------|------|------|----------|
| レンダリング | **wgpu** | OpenGL は非推奨化傾向。Metal/Vulkan/DX12 を統一的に扱える | Rio (sugarloaf) |
| VT パーサー | **vte crate** + 自作拡張 | Alacritty 由来の `vte` は実績十分。独自拡張でAI連携用のフック追加 | Alacritty |
| フォント | **cosmic-text** | テキストレイアウト + シェーピング統合。skrifa/swash ベースで依存が軽量 | - |
| グリフアトラス | **guillotiere** | 2D ビンパッキング。Rio と同じ方式 | Rio (sugarloaf) |
| ウィンドウ | **winit** | macOS の NSWindow ラッパーとして安定 | Alacritty |
| PTY | **rustix** + 自作 | POSIX PTY を直接操作。Alacritty と同じ `rustix-openpty` も検討 | Alacritty, Ghostty |
| スレッドモデル | **3スレッド** | Ghostty と同じ。メイン/I/O/レンダラーを分離 | Ghostty |
| スレッド間通信 | **crossbeam channel** | Ghostty の SPSC メールボックスに相当。Rust エコシステムで標準的 | Ghostty |
| 設定 | **TOML** | Lua (Wezterm) は過剰 | Alacritty, Rio |

## 技術スタック（確定版）

| レイヤー | 技術 | バージョン目安 |
|---------|------|--------------|
| 言語 | Rust | edition 2024, MSRV 1.85+ |
| GPU レンダリング | wgpu | 28.x |
| ウィンドウ管理 | winit | 0.30.x |
| テキストレイアウト | cosmic-text | 0.12.x |
| グリフアトラス | guillotiere | 0.6.x |
| VT パーサー | vte | 0.13.x |
| PTY | rustix + rustix-openpty | 1.x |
| macOS 統合 | objc2 + objc2-app-kit | 0.3.x |
| 非同期 | tokio | 1.x |
| スレッド間通信 | crossbeam-channel | 0.5.x |
| HTTP (AI API) | reqwest | 0.12.x |
| 設定 | toml + serde | - |
| ログ | tracing + tracing-subscriber | 0.1.x |
| エラー | thiserror | 2.x |
| クリップボード | copypasta | 0.10.x |
| ファイル監視 | notify | 8.x |

## アーキテクチャ

### スレッドモデル (Ghostty 参考: 3スレッドアーキテクチャ)

```
┌─────────────────────────────────────────────────────────────────────┐
│                                                                     │
│  ┌─── Main Thread (winit EventLoop) ──────────────────────────┐    │
│  │  - ウィンドウイベント処理 (resize, focus, close)            │    │
│  │  - キーボード/マウス入力処理                                 │    │
│  │  - 設定変更の適用                                           │    │
│  │  - タブ/ペイン管理                                          │    │
│  └────────┬──────────────────────┬─────────────────────────────┘    │
│           │ crossbeam channel    │ crossbeam channel                │
│           ▼                      ▼                                  │
│  ┌─── I/O Thread ─────────┐  ┌─── Renderer Thread ────────────┐   │
│  │  tokio Runtime          │  │  wgpu render loop              │   │
│  │  - PTY read/write       │  │  - 120fps (VSync or timer)     │   │
│  │  - VT パース → Terminal │  │  - Terminal State → GPU 描画   │   │
│  │    State 更新           │  │  - グリフアトラス管理           │   │
│  │  - AI 非同期リクエスト  │  │  - UI オーバーレイ描画         │   │
│  │  - Shell Integration    │  │  - カーソル点滅 (600ms)        │   │
│  │    (OSC 133) 処理       │  │  - ダーティリージョン追跡      │   │
│  └────────┬────────────────┘  └────────────────────────────────┘   │
│           │                           ▲                             │
│           │   Arc<Mutex<TerminalState>>│                            │
│           └───────────────────────────┘                             │
│                                                                     │
│  共有状態: Terminal Grid + Cursor + Attributes                      │
│  (I/O スレッドが write、Renderer スレッドが read)                    │
│  Mutex は最小限のクリティカルセクション (grid snapshot コピー)         │
└─────────────────────────────────────────────────────────────────────┘
```

### クレート構成図

```
┌─────────────────────────────────────────────────────────────┐
│                        Minal App (src/)                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────────┐ │
│  │ main.rs  │→ │ app.rs   │→ │ event.rs │  │ window.rs   │ │
│  │ (entry)  │  │ (loop)   │  │ (dispatch│  │ (winit mgmt)│ │
│  └──────────┘  └────┬─────┘  └──────────┘  └─────────────┘ │
├─────────────────────┼───────────────────────────────────────┤
│  crates/            │                                        │
│                     ▼                                        │
│  ┌─── minal-core ──────────────────────────────────────┐    │
│  │  VT Parser (vte) ──→ Terminal State ──→ Grid/Cell   │    │
│  │       │                     │                │      │    │
│  │       ▼                     ▼                ▼      │    │
│  │  Escape Handler      Scrollback Buffer    Cursor    │    │
│  │       │                                             │    │
│  │       ▼                                             │    │
│  │  PTY Manager (rustix) ←──── Input Handler           │    │
│  └─────────────────────────────────────────────────────┘    │
│                     │                                        │
│                     ▼                                        │
│  ┌─── minal-renderer ─────────────────────────────────┐    │
│  │  wgpu Device/Surface                                │    │
│  │       │                                             │    │
│  │       ├──→ Text Pipeline (cosmic-text → glyph atlas)│    │
│  │       ├──→ Rect Pipeline (backgrounds, cursors)     │    │
│  │       └──→ UI Overlay Pipeline (AI panels)          │    │
│  └─────────────────────────────────────────────────────┘    │
│                     │                                        │
│                     ▼                                        │
│  ┌─── minal-ai ───────────────────────────────────────┐    │
│  │  Provider Trait ──→ Anthropic / OpenAI / Ollama     │    │
│  │       │                                             │    │
│  │       ├──→ Context Collector (CWD, git, history)    │    │
│  │       ├──→ Completion Engine (inline suggestions)   │    │
│  │       ├──→ Chat Engine (multi-turn conversation)    │    │
│  │       └──→ Session Analyzer (error detection)       │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
│  ┌─── minal-config ──────────────────────────────────┐     │
│  │  TOML Parser ──→ Theme / Keybind / AI Settings     │     │
│  └────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
```

## モジュール構成（詳細）

```
minal/
├── Cargo.toml                    # Workspace root
├── PLAN.md
├── rustfmt.toml
├── .gitignore
│
├── crates/
│   ├── minal-core/               # ターミナルエミュレーション
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs            # pub API: Terminal struct
│   │       ├── term.rs           # Terminal 状態マシン (画面サイズ、モード、属性)
│   │       ├── grid.rs           # Row<Cell> のグリッド + リングバッファ
│   │       ├── cell.rs           # Cell: char + fg/bg + attributes (bold, italic, etc.)
│   │       ├── cursor.rs         # カーソル位置・スタイル
│   │       ├── scrollback.rs     # スクロールバック履歴バッファ
│   │       ├── handler.rs        # vte::Perform 実装 (エスケープシーケンス処理)
│   │       ├── ansi.rs           # ANSI 定数・型定義 (SGR, CSI, OSC, DCS)
│   │       ├── charset.rs        # 文字セットマッピング (G0-G3)
│   │       ├── pty.rs            # PTY 生成・読み書き (rustix forkpty)
│   │       └── selection.rs      # テキスト選択 (矩形/行)
│   │
│   ├── minal-renderer/           # GPU レンダリング (Rio の sugarloaf 参考)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs            # pub API: Renderer struct
│   │   │   ├── context.rs        # wgpu Device, Queue, Surface 管理
│   │   │   ├── atlas.rs          # グリフアトラス (LRU テクスチャキャッシュ)
│   │   │   │                     #   - cosmic-text でシェーピング → swash でラスタライズ
│   │   │   │                     #   - テクスチャアトラス (2048x2048) にパッキング
│   │   │   ├── text.rs           # テキストレンダリングパイプライン
│   │   │   │                     #   - 頂点: (x, y, u, v, fg_color, bg_color)
│   │   │   │                     #   - インスタンスレンダリングでセル単位描画
│   │   │   ├── rect.rs           # 矩形パイプライン (背景色、カーソル、選択範囲)
│   │   │   ├── overlay.rs        # UI オーバーレイ (AI パネル、補完ポップアップ)
│   │   │   └── shaders/
│   │   │       ├── text.wgsl     # テキスト描画シェーダー
│   │   │       └── rect.wgsl     # 矩形描画シェーダー
│   │   └── Cargo.toml
│   │
│   ├── minal-ai/                 # AI エンジン
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs            # pub API: AiEngine struct
│   │       ├── provider.rs       # trait AiProvider { async fn complete(), async fn chat() }
│   │       ├── anthropic.rs      # Claude API (Messages API, streaming)
│   │       ├── openai.rs         # OpenAI API (Chat Completions, streaming)
│   │       ├── ollama.rs         # Ollama REST API (ローカルモデル)
│   │       ├── context.rs        # ContextCollector: CWD, git status, shell history,
│   │       │                     #   最近のターミナル出力, 環境変数, プロジェクト構造
│   │       ├── completion.rs     # CompletionEngine:
│   │       │                     #   - シェルプロンプト検出 (PS1 パターンマッチ)
│   │       │                     #   - 入力バッファ監視
│   │       │                     #   - debounce (300ms) 後に AI リクエスト
│   │       │                     #   - ゴーストテキストとしてレンダラーに渡す
│   │       ├── chat.rs           # ChatEngine: マルチターン会話管理
│   │       │                     #   - メッセージ履歴
│   │       │                     #   - ストリーミングレスポンス
│   │       │                     #   - コマンド抽出 (```バッククォート内) → 実行ボタン
│   │       └── analyzer.rs       # SessionAnalyzer:
│   │                             #   - 出力パターンマッチ (exit code != 0, stack trace, etc.)
│   │                             #   - エラー分類 (build, test, runtime, permission)
│   │                             #   - バックグラウンドで AI に修正案を問い合わせ
│   │
│   └── minal-config/             # 設定管理
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs            # pub API: Config struct + hot-reload (notify)
│           ├── theme.rs          # カラーテーマ (16色 + 256色パレット + TrueColor)
│           ├── font.rs           # フォント設定 (family, size, line_height, etc.)
│           ├── keybind.rs        # キーバインド (デフォルト + カスタム)
│           └── ai.rs             # AI 設定 (プロバイダー、APIキー、モデル選択)
│
├── shell-integration/            # シェル統合スクリプト (OSC 133 対応)
│   ├── minal.zsh                 # Zsh: precmd/preexec フック
│   ├── minal.bash                # Bash: PROMPT_COMMAND フック
│   └── minal.fish                # Fish: fish_prompt/fish_preexec
│
└── src/                          # メインアプリケーション
    ├── main.rs                   # エントリーポイント: 設定読込 → App::run()
    ├── app.rs                    # メインイベントループ (winit EventLoop)
    │                             #   1. winit イベント受信
    │                             #   2. PTY 出力読取 → VT パーサー → Terminal 更新
    │                             #   3. Terminal 状態 → Renderer で描画
    │                             #   4. AI エンジンへの非同期イベント転送
    ├── event.rs                  # イベント型定義 + ディスパッチ
    │                             #   - WindowEvent (resize, focus, close)
    │                             #   - KeyEvent → PTY 書込 or AI トリガー
    │                             #   - PtyEvent (output ready)
    │                             #   - AiEvent (completion ready, chat response)
    └── window.rs                 # winit Window ラッパー + macOS ネイティブ統合
```

## データフロー詳細

### 1. 入力 → PTY → 画面描画フロー

```
User Keypress
  │
  ▼
winit KeyEvent
  │
  ├──→ [通常キー] ──→ PTY write (UTF-8 バイト列)
  │                        │
  │                        ▼
  │                   Shell 処理 (bash/zsh/fish)
  │                        │
  │                        ▼
  │                   PTY read (非同期, tokio::io)
  │                        │
  │                        ▼
  │                   vte::Parser::advance() ──→ handler.rs (Perform trait)
  │                        │
  │                        ▼
  │                   Terminal State 更新 (grid, cursor, attributes)
  │                        │
  │                        ▼
  │                   Renderer::draw() ──→ wgpu submit
  │
  ├──→ [Ctrl+Shift+A] ──→ AI Chat パネル Toggle
  ├──→ [Ctrl+Shift+E] ──→ Error Summary パネル Toggle
  └──→ [Tab on ghost text] ──→ AI 補完を確定 → PTY write
```

### 2. AI 補完フロー

```
PTY Output (シェルプロンプト検出)
  │
  ▼
CompletionEngine::on_prompt_detected()
  │
  ▼
User Input (キーストローク監視)
  │
  ▼
Debounce Timer (300ms)
  │
  ▼
ContextCollector::gather()
  ├── current_directory
  ├── recent_commands (最新 20 件)
  ├── current_input_buffer
  ├── git_branch + git_status
  └── project_type (Cargo.toml → Rust, package.json → Node, etc.)
  │
  ▼
AiProvider::complete(context) ── async/streaming ──→
  │
  ▼
Ghost Text (灰色テキストでオーバーレイ描画)
  │
  ├──→ [Tab] ──→ 確定: PTY に書込
  ├──→ [Esc] ──→ 破棄
  └──→ [他のキー] ──→ 再 debounce
```

### 3. セッション分析フロー

```
PTY Output (全出力をバッファリング)
  │
  ▼
SessionAnalyzer::on_output(bytes)
  │
  ▼
Pattern Matcher
  ├── exit_code != 0 (プロンプト内の $? チェック)
  ├── "error" / "Error" / "ERROR" キーワード
  ├── stack trace パターン (Python traceback, Rust backtrace, etc.)
  ├── "command not found"
  └── コンパイルエラーパターン (rustc, gcc, tsc, etc.)
  │
  ▼ (エラー検出時)
  │
AiProvider::analyze(error_context)
  │
  ▼
Notification Badge (ターミナル右上にバッジ表示)
  │
  ▼ (Ctrl+Shift+E で展開)
  │
Error Summary Panel
  ├── エラー種別
  ├── 原因分析
  ├── 修正コマンド候補 (クリックで実行)
  └── 関連ドキュメントリンク
```

## セキュリティ設計

### コマンド実行の承認フロー
AI がターミナルにコマンドを送信する際、ユーザーの明示的な承認を必須とする:

```
AI がコマンドを提案
  │
  ▼
承認 UI (コマンド内容をハイライト表示)
  │
  ├──→ [Enter / クリック] ──→ PTY に送信・実行
  ├──→ [e] ──→ コマンドを編集してから実行
  └──→ [Esc] ──→ 破棄
```

- **自動実行モード**: ユーザーが明示的に有効化した場合のみ、信頼リスト (`~/.config/minal/trusted_commands.toml`) に一致するコマンドを自動実行
- **危険コマンド検出**: `rm -rf`, `sudo`, `dd`, `mkfs` 等のパターンを検出し、追加警告を表示
- **サンドボックス実行**: 将来的にコンテナ / namespace ベースのサンドボックスで AI コマンドを隔離実行するオプション

### API キー管理
- **macOS**: Keychain Services (`Security.framework`) に保存。`toml` にはキーを直接記載しない
- **Linux**: `libsecret` (GNOME Keyring) or `kwallet` (KDE) を利用
- **フォールバック**: `~/.config/minal/credentials` (mode 0600) に暗号化保存
- 環境変数 (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) からの読み込みも対応

### プライバシー
- AI に送信するコンテキストの範囲をユーザーが設定可能 (`[ai.privacy]` セクション)
- `exclude_patterns`: 特定のディレクトリやファイル内容を AI コンテキストから除外
- ローカルモデル (Ollama) の場合はデータが外部に出ないことを明示

## Phase 1: 基盤ターミナル (MVP) 詳細実装計画

### Step 1.1: プロジェクト初期化
- Cargo workspace セットアップ (4 crates)
- rustfmt.toml, .gitignore, .editorconfig
- CI 用 GitHub Actions (cargo check, clippy, test)

### Step 1.2: ウィンドウ + wgpu 初期化
**参考: Rio の `rio-window` + `sugarloaf`**
- winit の EventLoop + Window 作成
- wgpu の Instance → Adapter → Device → Queue → Surface 初期化
- 画面クリア (背景色で塗りつぶし) のレンダーパス
- リサイズ対応 (Surface の再設定)
- **macOS**: NSWindow の titlebar 透過、vibrancy 設定

### Step 1.3: テキストレンダリング
**参考: Rio の `sugarloaf/src/` グリフレンダリング**
- cosmic-text の FontSystem + SwashCache 初期化
- グリフアトラス実装:
  - 2048x2048 の RGBA テクスチャ
  - guillotiere (矩形パッキング) or 独自のシンプルアロケータ
  - LRU エビクション
- テキスト描画シェーダー (WGSL):
  - 頂点シェーダー: セル座標 → クリップ座標変換
  - フラグメントシェーダー: アトラスからサンプリング + 前景色適用
- 矩形描画シェーダー (背景色):
  - セル範囲の背景色をインスタンスレンダリング
- セルグリッド (80x24) をダミーデータで描画確認

### Step 1.4: ターミナルコア
**参考: Alacritty の `alacritty_terminal`**
- Cell 構造体: `char`, `fg: Color`, `bg: Color`, `attrs: CellAttributes`
- Grid 構造体: `Vec<Row>` のリングバッファ
  - `Row` = `Vec<Cell>` (固定幅、リサイズ時に再構築)
- Cursor: position (col, row) + style (block, underline, bar)
- Terminal 構造体:
  - grid, cursor, scroll_offset
  - mode flags (alt screen, origin mode, wrap mode, etc.)
  - saved_cursor (DECSC/DECRC)
  - tab_stops

### Step 1.5: VT パーサー
**参考: Alacritty の `alacritty_terminal/src/vt/` + `vte` crate**
- `vte::Parser` + `vte::Perform` trait 実装
- 最低限の対応シーケンス (Phase 1):
  - **Print**: 通常文字の書込
  - **C0 制御**: BS, HT, LF, CR, ESC
  - **CSI シーケンス**:
    - カーソル移動: CUU (A), CUD (B), CUF (C), CUB (D), CUP (H)
    - 消去: ED (J), EL (K)
    - SGR (m): bold, italic, underline, fg/bg color (8色, 256色, TrueColor)
    - スクロール: SU (S), SD (T)
    - モード設定: DECSET/DECRST (?25h/l カーソル表示, ?1049h/l alt screen)
  - **OSC**: ウィンドウタイトル設定 (OSC 0/2)

### Step 1.6: PTY 管理
**参考: Rio の `teletypewriter` crate**
- macOS: `forkpty()` で子プロセス生成
  - rustix の `openpt()` + `grantpt()` + `unlockpt()` + `ptsname()`
  - fork 後、子プロセスで `setsid()` + `ioctl(TIOCSCTTY)` + exec shell
- 非同期 I/O:
  - PTY master fd を `tokio::io::AsyncFd` でラップ
  - 読取: `poll_read_ready()` → バッファ読取 → VT パーサーに投入
  - 書込: キー入力を UTF-8 エンコードして write
- SIGWINCH: ウィンドウリサイズ時に `ioctl(TIOCSWINSZ)` で PTY サイズ更新

### Step 1.7: 3スレッドイベントループ統合 (Ghostty アーキテクチャ参考)

**メインスレッド (winit EventLoop)**:
- winit `EventLoop::run()` をメインスレッドで実行
- キーボード/マウスイベントを受信し I/O スレッドへ転送
- ウィンドウリサイズ → レンダラー + PTY (TIOCSWINSZ) に通知
- タブ/ペイン管理のオーケストレーション

**I/O スレッド (tokio Runtime)**:
- `std::thread::spawn` で起動、内部で `tokio::runtime::Runtime` を構築
- PTY master fd を `tokio::io::AsyncFd` で監視
- PTY 読取 → `vte::Parser::advance()` → Terminal State 更新 (Mutex 保護)
- メインスレッドからのキー入力 → PTY write
- AI 非同期リクエスト (reqwest) もこのスレッドで処理
- Shell Integration (OSC 133) のプロンプト/コマンド境界検出

**レンダラースレッド (wgpu)**:
- `std::thread::spawn` で起動
- 120fps タイマー (8.3ms 間隔) or VSync 駆動
- Terminal State の snapshot を取得 (Mutex の短期ロック)
- wgpu でフレーム描画 → Surface present
- カーソル点滅タイマー (600ms)
- ダーティフラグ: 状態変更がない場合はフレームスキップ

**スレッド間通信**:
```
Main Thread ──(KeyEvent)──→ crossbeam::channel ──→ I/O Thread
Main Thread ──(Resize)───→ crossbeam::channel ──→ I/O Thread
Main Thread ──(Resize)───→ crossbeam::channel ──→ Renderer Thread
I/O Thread  ──(Redraw)───→ crossbeam::channel ──→ Renderer Thread
I/O Thread  ──(AiResult)─→ crossbeam::channel ──→ Renderer Thread (overlay update)

Terminal State: Double-Buffering 方式
  - I/O Thread: "back buffer" に VT パース結果を書込 (ロック不要)
  - swap: I/O Thread が更新完了時に AtomicPtr::swap で front/back を切替
  - Renderer Thread: "front buffer" を参照して描画 (ロック不要)
  - 高速出力時 (cat 大ファイル等) でもレンダラーをブロックしない
  - フォールバック: 初期実装は Arc<Mutex<Terminal>> + snapshot コピーで開始し、
    パフォーマンス問題が顕在化した段階で double-buffering に移行
```

### Step 1.8: 基本設定
- `~/.config/minal/minal.toml`:
  ```toml
  [font]
  family = "JetBrains Mono"
  size = 14.0

  [window]
  width = 80
  height = 24
  opacity = 1.0
  padding = 10

  [colors]
  background = "#1e1e2e"
  foreground = "#cdd6f4"
  # ... 16色パレット

  [shell]
  program = "/bin/zsh"
  args = ["-l"]
  ```

### Step 1.9: 最小 AI 補完 (MVP に含める)
AI 特化を最大の差別化とするため、Phase 1 の段階から最小限の AI 補完を組み込む:
- **Ollama ローカルモデルのみ** (ネットワーク不要、APIキー不要)
- シェルプロンプト検出は PS1 パターンマッチ (OSC 133 は Phase 3)
- 入力バッファ監視 + debounce (300ms) → Ollama に補完リクエスト
- ゴーストテキスト (灰色半透明) で候補表示
- Tab で確定、Esc で破棄
- AI 機能の ON/OFF トグル (`Ctrl+Shift+A`)
- **ゴール**: ターミナルとして最低限動く + AI 補完を体験できる状態を早期に達成し、フィードバックループを回す

## Phase 2: ターミナル機能充実 詳細

### Step 2.1: 色の完全対応
- 256色パレット (SGR 38;5;N / 48;5;N)
- TrueColor (SGR 38;2;R;G;B / 48;2;R;G;B)
- カラーテーマ切替 (Catppuccin, Tokyo Night, Dracula, etc.)

### Step 2.2: マウスイベント
**参考: Alacritty の mouse handling**
- X10 マウスプロトコル
- SGR マウスプロトコル (1006)
- テキスト選択 (ダブルクリックで単語、トリプルクリックで行)
- 選択範囲のクリップボードコピー

### Step 2.3: クリップボード
- macOS: `NSPasteboard` (objc2-app-kit)
- Cmd+C / Cmd+V

### Step 2.4: タブ / ペイン
**参考: Wezterm の `mux` crate**
- タブバー UI (wgpu で独自描画)
- Cmd+T: 新規タブ
- Cmd+D: 縦分割、Cmd+Shift+D: 横分割
- 各ペインが独立した Terminal + PTY を持つ
- ペイン間のフォーカス移動 (Cmd+[/])

### Step 2.5: macOS ネイティブ統合
- NSMenu: メニューバー
- NSWindow: titlebar integration (tabs in titlebar)
- NSAppearance: ダークモード連動
- Notification Center 統合 (AI 通知用)
- Handoff / Universal Clipboard 対応

## Phase 3: AI 統合 詳細

### Step 3.1: AI プロバイダー抽象化
```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Single-turn completion (for command suggestions)
    async fn complete(&self, prompt: &str, context: &Context) -> Result<String>;

    /// Streaming chat response
    async fn chat_stream(
        &self,
        messages: &[Message],
        context: &Context,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>>>>>;

    /// Analyze error output
    async fn analyze_error(&self, error: &ErrorContext) -> Result<Analysis>;
}
```

### Step 3.2: コンテキスト収集
```rust
pub struct Context {
    pub cwd: PathBuf,
    pub git_branch: Option<String>,
    pub git_status: Option<String>,
    pub recent_commands: Vec<String>,     // 最新 20 件
    pub recent_output: String,            // 最新 2000 文字
    pub project_type: Option<ProjectType>, // Rust, Node, Python, etc.
    pub shell: String,                    // zsh, bash, fish
    pub os: String,                       // macOS version
    pub env_hints: HashMap<String, String>, // 関連する環境変数
    // Shell Integration (OSC 133) から得られる構造化データ
    pub last_command: Option<String>,       // 最後に実行されたコマンド
    pub last_exit_code: Option<i32>,        // 最後の exit code
    pub command_history: Vec<CommandRecord>, // コマンド + 出力 + exit code のペア
}

pub struct CommandRecord {
    pub command: String,
    pub output: String,          // 最大 4096 文字にトランケート
    pub exit_code: i32,
    pub timestamp: SystemTime,
    pub cwd: PathBuf,
}
```

### Step 3.2.5: Shell Integration (OSC 133) 実装
**参考: Ghostty `src/shell-integration/`, iTerm2, Wezterm**

ターミナルとシェル間の構造化通信プロトコル:
```
[プロンプト表示] → OSC 133;A (プロンプト開始)
[ユーザー入力]  → OSC 133;B (コマンド入力開始)
[Enter 押下]    → OSC 133;C (コマンド実行開始)
[コマンド完了]  → OSC 133;D;{exit_code} (コマンド終了)
```

実装:
- `handler.rs` の OSC ハンドラーで OSC 133 を解析
- `ShellIntegration` 構造体でプロンプト/コマンド/出力の状態を追跡
- コマンド完了時に `CommandRecord` を生成 → AI コンテキストに自動追加
- シェル設定スクリプト (`shell-integration/minal.{zsh,bash,fish}`) を提供

### Step 3.3: AI 補完エンジン (Phase 1 の最小実装を拡張)
- シェルプロンプト検出: **OSC 133;A** (Shell Integration) を第一選択、フォールバックで PS1 パターンマッチ
- キーストロークごとに debounce タイマーリセット
- Ollama (ローカル) を第一選択、フォールバックで Claude API
- ゴーストテキスト描画: 通常テキストと同じパイプラインで灰色半透明

**レイテンシ・キャッシュ戦略**:
- **補完キャッシュ**: 同一プレフィックスの補完結果を LRU キャッシュ (最大 256 エントリ) に保持。キャッシュヒット時は AI リクエスト不要
- **プリフェッチ**: コマンド入力開始時にプロジェクトコンテキストを事前収集し、AI リクエスト時のレイテンシを削減
- **Ollama ウォームアップ**: アプリ起動時にダミーリクエストでモデルをメモリにロード。初回補完のコールドスタートを回避
- **メモリ制限**: Ollama 使用時のメモリ使用量を監視し、システムメモリ圧迫時は補完を一時停止
- **グレースフルデグラデーション**:
  - ネットワーク断時: クラウド API → Ollama にフォールバック
  - Ollama 未起動時: AI 補完を無効化し、ステータスバーに通知
  - タイムアウト (2秒): レスポンスが遅い場合はリクエストをキャンセルし、次の入力を待つ

### Step 3.4: インラインチャットパネル
- 画面下部 30% にスライドインするパネル
- パネル内テキスト入力 (独自テキストエディタ、1行 or 複数行)
- マークダウン簡易レンダリング (コードブロック、太字)
- ストリーミング表示 (トークンごとに追記)
- コードブロック内に「実行」ボタン → PTY に送信

### Step 3.5: セッション分析
- 出力パイプラインにタップ (全出力をリングバッファにコピー)
- パターンマッチャー:
  - regex ベースの高速マッチ
  - exit code 監視 (PROMPT_COMMAND / precmd フック)
- エラー検出時:
  - 非同期で AI 分析リクエスト
  - ステータスバーにバッジ表示 (赤丸 + 件数)
  - パネル展開でエラー詳細 + 修正案一覧

### Step 3.6: エージェントモード (自律実行)
Claude Code のような「タスクを渡して自律的に実行」するモードを提供:

```
ユーザー: 「このプロジェクトのテストを全部通るようにして」
  │
  ▼
Agent Loop:
  1. コンテキスト収集 (プロジェクト構造、テスト結果、エラー内容)
  2. AI が次のアクションを決定 (コマンド実行 / ファイル編集 / 質問)
  3. 承認 UI 表示 (Step-by-step or Auto-approve モード)
  4. アクション実行 → 結果を AI にフィードバック
  5. 完了条件を満たすまで 1-4 を繰り返し
```

**実装要素**:
- `AgentEngine` 構造体: タスク → プラン → 実行のループ管理
- アクション型:
  - `RunCommand(String)` → PTY 経由で実行
  - `EditFile { path, diff }` → ファイル編集 (diff 表示 + 承認)
  - `ReadFile(PathBuf)` → ファイル読取 (コンテキスト追加)
  - `AskUser(String)` → ユーザーへの質問
  - `Complete(String)` → タスク完了報告
- **承認モード**:
  - `step`: 各アクションごとにユーザー承認 (デフォルト)
  - `auto-safe`: 読取系は自動承認、書込/実行は承認要求
  - `auto-all`: 全アクションを自動承認 (信頼環境向け)
- ターミナル下部にエージェント進捗パネル表示 (実行中タスク、完了ステップ数)

### Step 3.7: MCP (Model Context Protocol) クライアント
外部ツールとの標準化された連携プロトコル:

```
Minal (MCP Client) ←→ MCP Server (ファイル操作, DB, API, etc.)
                   ←→ MCP Server (GitHub, Jira, etc.)
                   ←→ MCP Server (カスタムツール)
```

**実装要素**:
- MCP クライアントライブラリの統合 (JSON-RPC over stdio/SSE)
- `~/.config/minal/mcp_servers.toml` でサーバー定義:
  ```toml
  [[mcp_servers]]
  name = "filesystem"
  command = "npx"
  args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/project"]

  [[mcp_servers]]
  name = "github"
  command = "npx"
  args = ["-y", "@modelcontextprotocol/server-github"]
  env = { GITHUB_TOKEN = "from_keychain" }
  ```
- AI がツール呼び出し可能: ファイル読み書き、検索、外部API、DB クエリ等
- エージェントモードとの統合: MCP ツールをアクション型として追加

## Phase 4: 磨き込み

### Step 4.1: パフォーマンス最適化
- ダーティリージョン追跡 (変更セルのみ再描画)
- グリフキャッシュの事前ウォームアップ (ASCII 0x20-0x7E)
- PTY 読取のバッチ処理 (複数回の read をまとめて VT パース)
- フレームスキップ (高速出力時に中間フレームを間引く)

### Step 4.2: アクセシビリティ
- macOS VoiceOver 対応 (NSAccessibility)
- 高コントラストテーマ
- フォントサイズ動的変更 (Cmd+/-)

### Step 4.3: プラグインシステム
- WASI ベースのプラグインランタイム
- イベントフック API (on_command, on_output, on_error)
- カスタム AI プロバイダープラグイン

### Step 4.4: 配布
- Homebrew formula
- GitHub Releases (.dmg, .app)
- 自動アップデート (Sparkle framework)

## マルチプラットフォーム対応ロードマップ

基本方針: **macOS ファースト → Linux → (将来) Windows**

| Phase | macOS | Linux | Windows |
|-------|-------|-------|---------|
| Phase 1 (MVP) | **主要ターゲット** | ビルド可能を維持 (CI) | - |
| Phase 2 | 完全対応 | **基本動作確認** | - |
| Phase 3 | 完全対応 | 完全対応 | - |
| Phase 4 | 完全対応 | 完全対応 | 検討 |

### プラットフォーム分岐ポイント

| 機能 | macOS | Linux |
|------|-------|-------|
| PTY | `forkpty()` via rustix | 同左 (POSIX 共通) |
| GPU | wgpu → Metal backend | wgpu → Vulkan backend |
| ウィンドウ | winit (Cocoa) | winit (X11/Wayland) |
| クリップボード | `NSPasteboard` (objc2) | `wl-copy`/`xclip` or `smithay-clipboard` |
| フォント検出 | CoreText | fontconfig |
| キーチェーン | Security.framework | libsecret / kwallet |
| 通知 | NSUserNotification | libnotify / D-Bus |
| ネイティブ統合 | NSMenu, NSAppearance | GTK/Qt テーマ連携 (best-effort) |

### 方針
- `cfg(target_os)` で分岐するコードは `platform/` モジュールに集約
- trait abstraction (`trait Clipboard`, `trait KeychainStore` 等) でプラットフォーム差を吸収
- CI で macOS + Linux のクロスビルド・テストを常時実行

## 差別化ポイント

| 機能 | Wezterm | Ghostty | Alacritty | **Minal** |
|------|---------|---------|-----------|-----------|
| GPU レンダリング | OpenGL | Metal | OpenGL | **wgpu** (Metal/Vulkan) |
| AI コマンド補完 | - | - | - | **Ghost text** |
| インライン AI チャット | - | - | - | **Slide-in panel** |
| エラー自動分析 | - | - | - | **Background analyzer** |
| エージェント自律実行 | - | - | - | **Agent mode (承認UI付き)** |
| MCP ツール連携 | - | - | - | **MCP client** |
| コンテキスト認識 | - | - | - | **Git/project/env aware** |
| マルチ AI プロバイダー | - | - | - | **Claude/OpenAI/Ollama** |
| セキュリティ | - | - | - | **コマンド承認 + Keychain** |
| 設定 | Lua | TOML-like | TOML | **TOML** |
| macOS ネイティブ | 部分的 | 完全 | 部分的 | **完全** |
| Linux 対応 | 完全 | 完全 | 完全 | **Phase 2〜** |
