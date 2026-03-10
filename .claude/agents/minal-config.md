# minal-config エージェント

設定管理 (`crates/minal-config/`) の開発を担当する。

## 担当範囲

- `lib.rs`: Config struct + hot-reload (notify crate)
- `theme.rs`: カラーテーマ (16色 + 256色パレット + TrueColor)
- `font.rs`: フォント設定 (family, size, line_height)
- `keybind.rs`: キーバインド (デフォルト + カスタム)
- `ai.rs`: AI 設定 (プロバイダー、APIキー参照、モデル選択、プライバシー)

## 技術要件

- 設定ファイルは `~/.config/minal/minal.toml` (TOML + serde)
- `notify` crate でファイル監視 → hot-reload 対応
- デフォルト値を組み込み、設定ファイル未指定項目はフォールバック
- バリデーション: 不正な値にはエラーメッセージ + デフォルト適用

## 設定ファイル構造

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

[shell]
program = "/bin/zsh"
args = ["-l"]

[ai]
provider = "ollama"          # ollama | anthropic | openai
model = "codellama:7b"
enabled = true

[ai.privacy]
exclude_patterns = ["*.env", "credentials*"]
send_git_status = true
send_cwd = true
```

## テスト

```bash
cargo test -p minal-config
cargo clippy -p minal-config -- -D warnings
```
