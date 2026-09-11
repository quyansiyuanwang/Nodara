//! Provider-neutral chat types.

use serde::{Deserialize, Serialize};

/// Who produced a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Instructions from the host.
    System,
    /// The operator's request.
    User,
    /// A previous model reply.
    Assistant,
}

/// One chat message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Author.
    pub role: Role,
    /// Content.
    pub content: String,
}

impl ChatMessage {
    /// A system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    /// A user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    /// An assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

/// A completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Conversation so far.
    pub messages: Vec<ChatMessage>,
    /// Sampling temperature.
    pub temperature: f32,
    /// Upper bound on generated tokens.
    pub max_tokens: u32,
    /// Ask the provider for a JSON object rather than prose.
    pub json_mode: bool,
}

impl ChatRequest {
    /// A request with conservative defaults.
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        Self {
            messages,
            temperature: 0.0,
            max_tokens: 2048,
            json_mode: true,
        }
    }
}

/// Token accounting reported by a provider.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens in the prompt.
    pub prompt_tokens: u32,
    /// Tokens generated.
    pub completion_tokens: u32,
}

impl TokenUsage {
    /// Total tokens.
    pub fn total(&self) -> u32 {
        self.prompt_tokens + self.completion_tokens
    }
}

/// A completion response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Generated content.
    pub content: String,
    /// Model that answered.
    pub model: String,
    /// Token accounting.
    pub usage: TokenUsage,
}
