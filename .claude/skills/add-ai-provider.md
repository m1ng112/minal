# AI プロバイダー追加スキル

新しい AI プロバイダーのサポートを追加する。

## 手順

1. `crates/minal-ai/src/<provider>.rs` を作成
2. `AiProvider` trait を実装 (complete, chat_stream, analyze_error)
3. `crates/minal-ai/src/lib.rs` にモジュール追加・エクスポート
4. `crates/minal-config/src/ai.rs` にプロバイダー設定を追加
5. テスト追加 (`cargo test -p minal-ai`)

## AiProvider trait

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn complete(&self, prompt: &str, context: &Context) -> Result<String>;
    async fn chat_stream(
        &self,
        messages: &[Message],
        context: &Context,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>>>>>;
    async fn analyze_error(&self, error: &ErrorContext) -> Result<Analysis>;
}
```

## 実装要件

- reqwest で HTTP リクエスト (ストリーミング対応)
- API キーは環境変数 or Keychain/libsecret から取得
- タイムアウト 2 秒
- レート制限のハンドリング (429 → リトライ with backoff)
- エラーは `thiserror` でカスタム型

## 既存プロバイダー

- `ollama.rs`: Ollama REST API (localhost:11434)
- `anthropic.rs`: Claude Messages API
- `openai.rs`: OpenAI Chat Completions API
