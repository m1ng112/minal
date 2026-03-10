# ビルド・テストスキル

プロジェクト全体のビルド、テスト、lint を実行する。

## ビルド

```bash
cargo build --workspace
```

## リリースビルド

```bash
cargo build --workspace --release
```

## テスト (全体)

```bash
cargo test --workspace
```

## 個別クレートテスト

```bash
cargo test -p minal-core
cargo test -p minal-renderer
cargo test -p minal-ai
cargo test -p minal-config
```

## Lint

```bash
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

## フォーマット修正

```bash
cargo fmt --all
```

## 全チェック (CI 相当)

```bash
cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace
```
