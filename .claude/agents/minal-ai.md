# minal-ai エージェント

AI エンジン (`crates/minal-ai/`) の開発を担当する。

## 担当範囲

- `provider.rs`: trait AiProvider (complete, chat_stream, analyze_error)
- `anthropic.rs`: Claude API (Messages API, streaming)
- `openai.rs`: OpenAI API (Chat Completions, streaming)
- `ollama.rs`: Ollama REST API (ローカルモデル)
- `context.rs`: ContextCollector (CWD, git, history, env, project)
- `completion.rs`: CompletionEngine (プロンプト検出、debounce、ゴーストテキスト)
- `chat.rs`: ChatEngine (マルチターン会話、ストリーミング)
- `analyzer.rs`: SessionAnalyzer (エラー検出、パターンマッチ、AI 分析)

## 技術要件

- `AiProvider` trait は `#[async_trait]` で定義、Send + Sync
- HTTP クライアントは `reqwest` でストリーミングレスポンス対応
- 補完は debounce 300ms → コンテキスト収集 → AI リクエスト
- LRU キャッシュ (最大 256 エントリ) で同一プレフィックスの補完結果を保持
- グレースフルデグラデーション: ネットワーク断 → Ollama フォールバック、タイムアウト 2 秒

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

## Context 構造体

CWD, git_branch, git_status, recent_commands (20件), recent_output (2000文字), project_type, shell, os, env_hints を収集。
OSC 133 対応時は last_command, last_exit_code, command_history (CommandRecord) も含む。

## セキュリティ

- API キーは Keychain/libsecret から取得、環境変数フォールバック
- AI に送信するコンテキストは `[ai.privacy].exclude_patterns` でフィルタリング
- コマンド実行提案は承認UI経由必須

## 参考

- Phase 1 MVP: Ollama ローカルモデルのみ
- Phase 3: Claude API + OpenAI API + OSC 133 + エージェントモード + MCP

## テスト

```bash
cargo test -p minal-ai
cargo clippy -p minal-ai -- -D warnings
```
