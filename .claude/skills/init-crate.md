# クレート初期化スキル

新しい workspace メンバーのクレートを初期化する。

## 手順

1. `crates/<crate-name>/` ディレクトリを作成
2. `Cargo.toml` を作成 (workspace メンバーの規約に従う)
3. `src/lib.rs` を作成
4. ルートの `Cargo.toml` の `[workspace].members` に追加
5. `cargo check -p <crate-name>` で確認

## Cargo.toml テンプレート

```toml
[package]
name = "<crate-name>"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"

[dependencies]
thiserror = "2"
tracing = "0.1"
```

## 規約

- クレート名は `minal-` プレフィックス
- edition 2024, rust-version 1.85
- エラーは `thiserror` でカスタム型定義
- ログは `tracing` を使用
- 公開 API には doc comment を付ける
