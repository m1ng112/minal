# Minal - AI特化ターミナルエミュレータ

## プロジェクト概要

Minal は AI をファーストクラスで統合した Rust 製ターミナルエミュレータ。
Alacritty/Ghostty/Wezterm/Rio を参考に、AI コマンド補完・インラインチャット・エラー分析・エージェント自律実行を差別化機能とする。

## 技術スタック

- **言語**: Rust (edition 2024, MSRV 1.85+)
- **GPU**: wgpu 28.x (Metal/Vulkan/DX12 統一)
- **ウィンドウ**: winit 0.30.x
- **テキストレイアウト**: cosmic-text 0.12.x
- **グリフアトラス**: guillotiere 0.6.x
- **VT パーサー**: vte 0.13.x
- **PTY**: rustix + rustix-openpty
- **非同期**: tokio 1.x
- **スレッド間通信**: crossbeam-channel 0.5.x
- **HTTP (AI API)**: reqwest 0.12.x
- **設定**: toml + serde
- **ログ**: tracing + tracing-subscriber
- **エラー**: thiserror 2.x

## アーキテクチャ

### Workspace 構成

```
minal/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── minal-core/         # ターミナルエミュレーション (VT パーサー, Grid, PTY)
│   ├── minal-renderer/     # GPU レンダリング (wgpu, テキスト/矩形パイプライン)
│   ├── minal-ai/           # AI エンジン (Provider, 補完, チャット, 分析)
│   └── minal-config/       # 設定管理 (TOML, テーマ, キーバインド)
├── shell-integration/      # シェル統合スクリプト (OSC 133)
└── src/                    # メインアプリケーション (main, app, event, window)
```

### 3スレッドモデル (Ghostty 参考)

1. **Main Thread** (winit EventLoop): ウィンドウ/入力イベント処理、タブ/ペイン管理
2. **I/O Thread** (tokio Runtime): PTY read/write、VT パース、AI 非同期リクエスト
3. **Renderer Thread** (wgpu): 120fps 描画、グリフアトラス管理、UI オーバーレイ

スレッド間通信は crossbeam-channel。共有状態は Arc<Mutex<TerminalState>> → 将来 double-buffering に移行。

## ビルド・テスト

```bash
# ビルド
cargo build
cargo build --release

# テスト
cargo test --workspace
cargo test -p minal-core
cargo test -p minal-renderer
cargo test -p minal-ai
cargo test -p minal-config

# Lint
cargo clippy --workspace -- -D warnings
cargo fmt --check

# 実行
cargo run
```

## コーディング規約

- `cargo fmt` (rustfmt.toml に従う) を常に適用
- `cargo clippy -- -D warnings` で警告ゼロを維持
- エラーハンドリングは `thiserror` でカスタムエラー型を定義、`unwrap()` は禁止 (テスト除く)
- ログは `tracing` マクロ (`tracing::info!`, `tracing::debug!` 等) を使用
- `unsafe` ブロックは最小限に。PTY/FFI 周りでのみ許可し、必ず `// SAFETY:` コメントを付ける
- プラットフォーム分岐コードは `cfg(target_os)` と trait abstraction で分離
- 公開 API には doc comment を付ける
- テストは各 crate の `tests/` or `#[cfg(test)] mod tests` に書く

## クレート間依存関係

```
minal (bin) → minal-core, minal-renderer, minal-ai, minal-config
minal-renderer → minal-core (Terminal State を読取)
minal-ai → minal-core (コンテキスト情報取得), minal-config (AI 設定)
minal-config → (外部依存のみ)
minal-core → (外部依存のみ)
```

## 重要な設計判断

- **wgpu 採用**: OpenGL 非推奨化に対応、Metal/Vulkan/DX12 を統一的に扱う (Rio 参考)
- **vte crate**: Alacritty 由来の実績あるVTパーサー。AI フック用に独自拡張
- **cosmic-text**: テキストレイアウト + シェーピング統合、skrifa/swash ベースで軽量
- **TOML 設定**: Lua (Wezterm) は過剰。シンプルな TOML で十分
- **macOS ファースト**: Phase 1-2 は macOS 主要ターゲット、Linux はビルド可能を維持

## Phase 別スコープ

- **Phase 1 (MVP)**: ウィンドウ + wgpu + テキスト描画 + VTパーサー + PTY + 3スレッド + 基本設定 + 最小AI補完 (Ollama)
- **Phase 2**: 色完全対応 + マウス + クリップボード + タブ/ペイン + macOS ネイティブ統合
- **Phase 3**: AI プロバイダー抽象化 + OSC 133 + 補完拡張 + チャットパネル + セッション分析 + エージェントモード + MCP
- **Phase 4**: パフォーマンス最適化 + アクセシビリティ + プラグイン + 配布

## セキュリティ要件

- AI コマンド実行はユーザー承認必須 (承認UIフロー)
- API キーは Keychain/libsecret に保存、設定ファイルに直接記載しない
- `rm -rf`, `sudo` 等の危険コマンドには追加警告
- AI に送信するコンテキスト範囲はユーザー設定可能 (`[ai.privacy]`)

## 開発ワークフロー

### エージェントモデル選択

Agent ツールでエージェントを起動する際、タスクの性質に応じてモデルを使い分ける。

- **Opus** (思考・分析系): planner, code-reviewer, architect-reviewer, Plan agent など、計画・レビュー・設計判断が重要なエージェント
- **Sonnet** (実装・速度系): implementer, build-resolver, debugger, minal-* 系実装エージェントなど、コード生成・ビルド修正・速度が重要なエージェント

Agent ツールの `model` パラメータで `"opus"` / `"sonnet"` を明示的に指定すること。

### Issue 駆動開発

GitHub Issue に基づいて実装する場合、PR の説明文に `Closes #<issue番号>` を必ず記載する。
これにより PR がマージされた時点で対応する Issue が自動的にクローズされる。

```
## Summary
- 機能Xを実装

Closes #42
```

- 1つの PR で複数の Issue を閉じる場合は、それぞれ別の行に記載する (`Closes #42`, `Closes #43`)
- `Fixes #<番号>` や `Resolves #<番号>` も同様に使用可能
- PR タイトルではなく、必ず PR の **本文 (body)** に記載すること

各タスクは以下のフローに従って自動的に進める。実装とレビューを交互に繰り返すことで品質を高める。

```
Planning → Planning Review → Implement → Review → Implement → Review
```

### 1. Planning (設計)

- タスクの要件を分析し、影響範囲を特定する
- `planner` エージェントを使用して実装計画を作成する
- 変更対象のファイル、追加するモジュール、修正箇所を明確にする
- アーキテクチャ上の判断とトレードオフを記述する

### 2. Planning Review (設計レビュー)

- 計画の妥当性を検証する
- 既存アーキテクチャとの整合性を確認する
- クレート間依存関係に違反がないか確認する
- 抜け漏れや考慮不足がないか洗い出し、計画を修正する

### 3. Implement (実装 - 1st pass)

- `implementer` エージェントを使用してコードを実装する
- 計画に沿って変更を行う
- `cargo build` と `cargo test --workspace` でビルド・テストが通ることを確認する

### 4. Review (レビュー - 1st pass)

- `code-reviewer` エージェントを使用してコードレビューを行う
- コーディング規約 (fmt, clippy, unwrap 禁止等) の遵守を確認する
- セキュリティ要件の遵守を確認する
- 改善点・問題点をリストアップする

### 5. Implement (実装 - 2nd pass)

- レビューで指摘された問題を修正する
- リファクタリングや品質改善を行う
- 再度 `cargo build` と `cargo test --workspace` で確認する

### 6. Review (最終レビュー)

- 修正後のコードを最終レビューする
- すべての指摘が解消されていることを確認する
- `cargo clippy --workspace -- -D warnings` と `cargo fmt --check` をパスすることを確認する
- 問題がなければ完了、残課題があれば Implement に戻る
