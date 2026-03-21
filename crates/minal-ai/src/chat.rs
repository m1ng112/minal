//! Chat engine for multi-turn AI conversations.
//!
//! Manages conversation history, streaming token accumulation,
//! and message formatting for the AI chat panel.

use std::collections::VecDeque;

use crate::types::{Message, Role};

/// Manages multi-turn conversation state for the chat panel.
pub struct ChatEngine {
    messages: VecDeque<Message>,
    max_history: usize,
    system_prompt: String,
    streaming_buffer: String,
    is_streaming: bool,
}

impl ChatEngine {
    /// Creates a new chat engine.
    pub fn new(max_history: usize, system_prompt: String) -> Self {
        Self {
            messages: VecDeque::new(),
            max_history,
            system_prompt,
            streaming_buffer: String::new(),
            is_streaming: false,
        }
    }

    /// Adds a user message and returns the full message list for the API.
    ///
    /// The returned list includes the system prompt as the first message.
    pub fn add_user_message(&mut self, text: &str) -> Vec<Message> {
        self.messages.push_back(Message {
            role: Role::User,
            content: text.to_string(),
        });
        self.evict_old_messages();
        self.is_streaming = true;
        tracing::debug!(
            msg_count = self.messages.len(),
            "User message added to chat"
        );
        self.api_messages()
    }

    /// Returns the full message list including system prompt for API calls.
    fn api_messages(&self) -> Vec<Message> {
        let mut msgs = Vec::with_capacity(self.messages.len() + 1);
        if !self.system_prompt.is_empty() {
            msgs.push(Message {
                role: Role::System,
                content: self.system_prompt.clone(),
            });
        }
        msgs.extend(self.messages.iter().cloned());
        msgs
    }

    /// Appends a streaming token chunk to the buffer.
    pub fn append_streaming_chunk(&mut self, text: &str) {
        self.streaming_buffer.push_str(text);
    }

    /// Finalizes the stream, creating an assistant message from the buffer.
    ///
    /// Returns the complete assistant response.
    pub fn finalize_stream(&mut self) -> String {
        let response = std::mem::take(&mut self.streaming_buffer);
        self.messages.push_back(Message {
            role: Role::Assistant,
            content: response.clone(),
        });
        self.is_streaming = false;
        self.evict_old_messages();
        tracing::debug!(
            response_len = response.len(),
            msg_count = self.messages.len(),
            "Chat stream finalized"
        );
        response
    }

    /// Cancels the current stream without saving the partial response.
    pub fn cancel_stream(&mut self) {
        self.streaming_buffer.clear();
        self.is_streaming = false;
        tracing::debug!("Chat stream cancelled");
    }

    /// Returns the conversation messages (excluding system prompt).
    pub fn messages(&self) -> &VecDeque<Message> {
        &self.messages
    }

    /// Returns the current streaming buffer content.
    pub fn streaming_buffer(&self) -> &str {
        &self.streaming_buffer
    }

    /// Whether a stream is currently in progress.
    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    /// Clears all conversation history.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.streaming_buffer.clear();
        self.is_streaming = false;
    }

    /// Evicts oldest messages when history exceeds `max_history`.
    ///
    /// Always keeps at least one user-assistant pair.
    fn evict_old_messages(&mut self) {
        while self.messages.len() > self.max_history && self.messages.len() > 2 {
            self.messages.pop_front();
        }
        if self.messages.len() > self.max_history {
            tracing::debug!(
                evicted_to = self.messages.len(),
                max = self.max_history,
                "Chat history evicted"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_empty_engine() {
        let engine = ChatEngine::new(100, "You are helpful.".to_string());
        assert!(engine.messages().is_empty());
        assert!(!engine.is_streaming());
        assert!(engine.streaming_buffer().is_empty());
    }

    #[test]
    fn test_add_user_message_returns_list_with_system_prompt() {
        let mut engine = ChatEngine::new(100, "System prompt.".to_string());
        let msgs = engine.add_user_message("Hello");

        // Should have system prompt + user message
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::System);
        assert_eq!(msgs[0].content, "System prompt.");
        assert_eq!(msgs[1].role, Role::User);
        assert_eq!(msgs[1].content, "Hello");
    }

    #[test]
    fn test_add_user_message_without_system_prompt() {
        let mut engine = ChatEngine::new(100, String::new());
        let msgs = engine.add_user_message("Hello");

        // No system prompt, just the user message
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].content, "Hello");
    }

    #[test]
    fn test_add_user_message_sets_streaming() {
        let mut engine = ChatEngine::new(100, String::new());
        assert!(!engine.is_streaming());
        engine.add_user_message("Hello");
        assert!(engine.is_streaming());
    }

    #[test]
    fn test_append_streaming_chunk_accumulates_text() {
        let mut engine = ChatEngine::new(100, String::new());
        engine.add_user_message("Hello");

        engine.append_streaming_chunk("Hello");
        assert_eq!(engine.streaming_buffer(), "Hello");

        engine.append_streaming_chunk(", world!");
        assert_eq!(engine.streaming_buffer(), "Hello, world!");
    }

    #[test]
    fn test_finalize_stream_creates_assistant_message() {
        let mut engine = ChatEngine::new(100, String::new());
        engine.add_user_message("Hello");
        engine.append_streaming_chunk("Hi there!");

        let response = engine.finalize_stream();
        assert_eq!(response, "Hi there!");
        assert!(!engine.is_streaming());
        assert!(engine.streaming_buffer().is_empty());

        // Should now have user + assistant messages
        let msgs = engine.messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].content, "Hello");
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].content, "Hi there!");
    }

    #[test]
    fn test_cancel_stream_clears_buffer_without_adding_message() {
        let mut engine = ChatEngine::new(100, String::new());
        engine.add_user_message("Hello");
        engine.append_streaming_chunk("partial response...");

        engine.cancel_stream();
        assert!(!engine.is_streaming());
        assert!(engine.streaming_buffer().is_empty());

        // Only the user message should remain
        let msgs = engine.messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::User);
    }

    #[test]
    fn test_history_eviction() {
        // max_history = 4 means at most 4 messages
        let mut engine = ChatEngine::new(4, String::new());

        // Add 3 user-assistant pairs (6 messages total, exceeds max_history)
        engine.add_user_message("msg1");
        engine.append_streaming_chunk("reply1");
        engine.finalize_stream();

        engine.add_user_message("msg2");
        engine.append_streaming_chunk("reply2");
        engine.finalize_stream();

        engine.add_user_message("msg3");
        engine.append_streaming_chunk("reply3");
        engine.finalize_stream();

        // Should have been evicted to at most 4 messages
        let msgs = engine.messages();
        assert!(msgs.len() <= 4);
        // Oldest messages should have been removed
        // The last message should be the most recent assistant reply
        let last = &msgs[msgs.len() - 1];
        assert_eq!(last.role, Role::Assistant);
        assert_eq!(last.content, "reply3");
    }

    #[test]
    fn test_eviction_keeps_at_least_two_messages() {
        // max_history = 1, but we should always keep at least 2
        let mut engine = ChatEngine::new(1, String::new());

        engine.add_user_message("hello");
        engine.append_streaming_chunk("hi");
        engine.finalize_stream();

        // Even with max_history=1, should keep the pair
        assert_eq!(engine.messages().len(), 2);
    }

    #[test]
    fn test_clear_resets_everything() {
        let mut engine = ChatEngine::new(100, "system".to_string());
        engine.add_user_message("hello");
        engine.append_streaming_chunk("partial");

        engine.clear();
        assert!(engine.messages().is_empty());
        assert!(engine.streaming_buffer().is_empty());
        assert!(!engine.is_streaming());
    }

    #[test]
    fn test_multi_turn_conversation() {
        let mut engine = ChatEngine::new(100, "You are helpful.".to_string());

        // Turn 1
        let msgs1 = engine.add_user_message("What is Rust?");
        assert_eq!(msgs1.len(), 2); // system + user
        engine.append_streaming_chunk("Rust is a systems programming language.");
        engine.finalize_stream();

        // Turn 2
        let msgs2 = engine.add_user_message("What about its ownership model?");
        assert_eq!(msgs2.len(), 4); // system + user1 + assistant1 + user2
        assert_eq!(msgs2[0].role, Role::System);
        assert_eq!(msgs2[1].role, Role::User);
        assert_eq!(msgs2[1].content, "What is Rust?");
        assert_eq!(msgs2[2].role, Role::Assistant);
        assert_eq!(msgs2[2].content, "Rust is a systems programming language.");
        assert_eq!(msgs2[3].role, Role::User);
        assert_eq!(msgs2[3].content, "What about its ownership model?");
    }

    #[test]
    fn test_streaming_buffer_returns_accumulated_text() {
        let mut engine = ChatEngine::new(100, String::new());
        assert_eq!(engine.streaming_buffer(), "");

        engine.add_user_message("test");
        engine.append_streaming_chunk("chunk1");
        assert_eq!(engine.streaming_buffer(), "chunk1");

        engine.append_streaming_chunk(" chunk2");
        assert_eq!(engine.streaming_buffer(), "chunk1 chunk2");

        engine.append_streaming_chunk(" chunk3");
        assert_eq!(engine.streaming_buffer(), "chunk1 chunk2 chunk3");
    }

    #[test]
    fn test_is_streaming_state_transitions() {
        let mut engine = ChatEngine::new(100, String::new());

        // Initially not streaming
        assert!(!engine.is_streaming());

        // After adding user message, streaming starts
        engine.add_user_message("hello");
        assert!(engine.is_streaming());

        // After finalizing, streaming stops
        engine.append_streaming_chunk("response");
        engine.finalize_stream();
        assert!(!engine.is_streaming());

        // After another user message, streaming starts again
        engine.add_user_message("follow up");
        assert!(engine.is_streaming());

        // Cancel also stops streaming
        engine.cancel_stream();
        assert!(!engine.is_streaming());
    }
}
