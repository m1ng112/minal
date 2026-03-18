---
name: add-ai-provider
description: "Add a new AI provider implementation to crates/minal-ai/. Use when integrating a new LLM API (e.g., Anthropic, OpenAI, Ollama, or custom providers)."
argument-hint: "[provider-name]"
---

Add a new AI provider `$ARGUMENTS` to `crates/minal-ai/`.

## Steps

1. Create `crates/minal-ai/src/$ARGUMENTS.rs`
2. Implement the `AiProvider` trait (complete, chat_stream, analyze_error)
3. Add module and re-export in `crates/minal-ai/src/lib.rs`
4. Add provider config variant in `crates/minal-config/src/ai.rs`
5. Add tests: `cargo test -p minal-ai`

## AiProvider Trait

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

## Implementation Requirements

- Use `reqwest` for HTTP requests with streaming response support
- API keys from environment variables or Keychain/libsecret
- 2-second timeout
- Rate limit handling (429 -> retry with exponential backoff)
- Errors use `thiserror` custom types
- Never log API keys or sensitive data

## Existing Providers

- `ollama.rs`: Ollama REST API (localhost:11434)
- `anthropic.rs`: Claude Messages API
- `openai.rs`: OpenAI Chat Completions API
