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

### 設計判断への反映

| 判断ポイント | 選択 | 理由 |
|-------------|------|------|
| レンダリング | **wgpu** | Rio と同じ。OpenGL は非推奨化傾向。Metal/Vulkan/DX12 を統一的に扱える |
| VT パーサー | **vte crate** + 自作拡張 | Alacritty 由来の `vte` は実績十分。独自拡張でAI連携用のフック追加 |
| フォント | **cosmic-text** | テキストレイアウト + シェーピング統合。skrifa/swash ベースで依存が軽量 |
| ウィンドウ | **winit** | Alacritty/Rio と同様。macOS の NSWindow ラッパーとして安定 |
| PTY | **rustix** + 自作 | portable-pty は Wezterm 依存が大きい。rustix で POSIX PTY を直接操作（macOS ならシンプル） |
| 設定 | **TOML** | Alacritty と同じ。Lua (Wezterm) は過剰 |

## 技術スタック（確定版）

| レイヤー | 技術 | バージョン目安 |
|---------|------|--------------|
| 言語 | Rust | edition 2024, MSRV 1.85+ |
| GPU レンダリング | wgpu | 28.x |
| ウィンドウ管理 | winit | 0.30.x |
| テキストレイアウト | cosmic-text | 0.12.x |
| VT パーサー | vte | 0.13.x |
| PTY | rustix (macOS POSIX API) | 1.x |
| macOS 統合 | objc2 + objc2-app-kit | 0.3.x |
| 非同期 | tokio | 1.x |
| HTTP (AI API) | reqwest | 0.12.x |
| 設定 | toml + serde | - |
| ログ | tracing | 0.1.x |
| エラー | thiserror | 2.x |

## アーキテクチャ

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

### Step 1.7: イベントループ統合
- winit EventLoop をメインスレッドで実行
- tokio Runtime を別スレッドで起動 (PTY I/O + AI 非同期処理用)
- チャネルベース通信:
  ```
  winit thread ──(KeyEvent)──→ channel ──→ tokio task (PTY write)
  tokio task (PTY read) ──(PtyOutput)──→ channel ──→ winit thread (redraw)
  ```
- フレームレート制御: VSync or 60fps cap
- Dirty flag: Terminal 状態変更時のみ再描画

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
}
```

### Step 3.3: AI 補完エンジン
- シェルプロンプト検出: PS1 パターン + カーソル位置追跡
- キーストロークごとに debounce タイマーリセット
- Ollama (ローカル) を第一選択、フォールバックで Claude API
- ゴーストテキスト描画: 通常テキストと同じパイプラインで灰色半透明

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

## 差別化ポイント

| 機能 | Wezterm | Ghostty | Alacritty | **Minal** |
|------|---------|---------|-----------|-----------|
| GPU レンダリング | OpenGL | Metal | OpenGL | **wgpu** (Metal/Vulkan) |
| AI コマンド補完 | - | - | - | **Ghost text** |
| インライン AI チャット | - | - | - | **Slide-in panel** |
| エラー自動分析 | - | - | - | **Background analyzer** |
| コンテキスト認識 | - | - | - | **Git/project/env aware** |
| マルチ AI プロバイダー | - | - | - | **Claude/OpenAI/Ollama** |
| 設定 | Lua | TOML-like | TOML | **TOML** |
| macOS ネイティブ | 部分的 | 完全 | 部分的 | **完全** |
