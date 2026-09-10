//! Model providers.
//!
//! The agent is provider-neutral: [`LlmProvider`] is the only thing the planner
//! knows about, so a local model, a hosted API and a scripted test double are
//! interchangeable.

use std::sync::Mutex;

use crate::error::{AgentError, AgentResult};
use crate::model::{ChatRequest, ChatResponse, TokenUsage};

/// Something that can complete a chat request.
pub trait LlmProvider: Send + Sync {
    /// Human-readable provider name, used in the audit trace.
    fn name(&self) -> &str;

    /// Produce a completion.
    fn complete(&self, request: &ChatRequest) -> AgentResult<ChatResponse>;
}

/// A provider that replays a fixed list of responses.
///
/// This is how the planner and repair loop are tested without a network, and how
/// `rf-agent` can be demonstrated offline.
#[derive(Debug, Default)]
pub struct MockProvider {
    responses: Mutex<Vec<String>>,
    calls: Mutex<Vec<ChatRequest>>,
}

impl MockProvider {
    /// Build a provider that answers with `responses` in order.
    pub fn new<I, S>(responses: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            responses: Mutex::new(responses.into_iter().map(Into::into).collect()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Every request this provider has seen.
    pub fn calls(&self) -> Vec<ChatRequest> {
        self.calls.lock().expect("mock provider lock").clone()
    }
}

impl LlmProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn complete(&self, request: &ChatRequest) -> AgentResult<ChatResponse> {
        self.calls
            .lock()
            .expect("mock provider lock")
            .push(request.clone());
        let mut responses = self.responses.lock().expect("mock provider lock");
        if responses.is_empty() {
            return Err(AgentError::Provider(
                "mock provider ran out of responses".to_string(),
            ));
        }
        Ok(ChatResponse {
            content: responses.remove(0),
            model: "mock".to_string(),
            usage: TokenUsage {
                prompt_tokens: request
                    .messages
                    .iter()
                    .map(|message| message.content.split_whitespace().count() as u32)
                    .sum(),
                completion_tokens: 0,
            },
        })
    }
}

/// An OpenAI-compatible chat-completions provider.
///
/// Works with OpenAI, Azure OpenAI-compatible gateways, and local servers that
/// expose the same request shape (for example `llama.cpp` or vLLM), because the
/// wire format is the only thing it assumes.
#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    endpoint: String,
    api_key: Option<String>,
    model: String,
}

impl OpenAiProvider {
    /// Configure a provider.
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            api_key,
        }
    }

    /// Read configuration from the environment.
    ///
    /// `RF_LLM_ENDPOINT` and `RF_LLM_MODEL` select the server; `RF_LLM_API_KEY`
    /// (or `OPENAI_API_KEY`) supplies the credential.
    pub fn from_env() -> AgentResult<Self> {
        let endpoint = std::env::var("RF_LLM_ENDPOINT")
            .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
        let model = std::env::var("RF_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let api_key = std::env::var("RF_LLM_API_KEY")
            .ok()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok());
        if api_key.is_none() {
            return Err(AgentError::Provider(
                "no API key: set RF_LLM_API_KEY or OPENAI_API_KEY".to_string(),
            ));
        }
        Ok(Self::new(endpoint, model, api_key))
    }

    /// Model this provider will request.
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.model
    }

    fn complete(&self, request: &ChatRequest) -> AgentResult<ChatResponse> {
        let body = serde_json::json!({
            "model": self.model,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "messages": request.messages,
            "response_format": if request.json_mode {
                serde_json::json!({ "type": "json_object" })
            } else {
                serde_json::Value::Null
            },
        });

        let mut call = ureq::post(&self.endpoint).timeout(std::time::Duration::from_secs(120));
        if let Some(key) = &self.api_key {
            call = call.set("authorization", &format!("Bearer {key}"));
        }
        let response = call
            .set("content-type", "application/json")
            .send_json(body)
            .map_err(|error| AgentError::Provider(error.to_string()))?;

        let payload: serde_json::Value = response
            .into_json()
            .map_err(|error| AgentError::Provider(error.to_string()))?;

        let content = payload
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                AgentError::Provider("provider response had no message content".to_string())
            })?
            .to_string();

        let usage = payload.get("usage");
        Ok(ChatResponse {
            content,
            model: payload
                .get("model")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&self.model)
                .to_string(),
            usage: TokenUsage {
                prompt_tokens: usage
                    .and_then(|value| value.get("prompt_tokens"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as u32,
                completion_tokens: usage
                    .and_then(|value| value.get("completion_tokens"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as u32,
            },
        })
    }
}
