//! Model providers.
//!
//! The agent is provider-neutral: [`LlmProvider`] is the only thing the planner
//! knows about, so a local model, a hosted API and a scripted test double are
//! interchangeable.

use std::io::{BufRead, BufReader};
use std::sync::Mutex;

use crate::error::{AgentError, AgentResult};
use crate::model::{ChatMessage, ChatRequest, ChatResponse, TokenUsage};

/// Something that can complete a chat request.
pub trait LlmProvider: Send + Sync {
    /// Human-readable provider name, used in the audit trace.
    fn name(&self) -> &str;

    /// Produce a completion.
    fn complete(&self, request: &ChatRequest) -> AgentResult<ChatResponse>;

    /// Produce a completion while reporting text deltas when the provider
    /// supports SSE streaming. Providers without native streaming fall back to
    /// one complete delta so callers have a single code path.
    fn complete_streaming(
        &self,
        request: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
    ) -> AgentResult<ChatResponse> {
        let response = self.complete(request)?;
        on_delta(&response.content);
        Ok(response)
    }
}

/// A provider that replays a fixed list of responses.
///
/// This is how the planner and repair loop are tested without a network, and how
/// `nodara-agent` can be demonstrated offline.
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

    fn complete_streaming(
        &self,
        request: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
    ) -> AgentResult<ChatResponse> {
        let response = self.complete(request)?;
        for chunk in response.content.as_bytes().chunks(12) {
            if let Ok(text) = std::str::from_utf8(chunk) {
                on_delta(text);
            }
        }
        Ok(response)
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
    /// `NODARA_LLM_ENDPOINT` and `NODARA_LLM_MODEL` select the server; `NODARA_LLM_API_KEY`
    /// (or `OPENAI_API_KEY`) supplies the credential.
    pub fn from_env() -> AgentResult<Self> {
        let endpoint = std::env::var("NODARA_LLM_ENDPOINT")
            .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
        let model = std::env::var("NODARA_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let api_key = std::env::var("NODARA_LLM_API_KEY")
            .ok()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok());
        if api_key.is_none() {
            return Err(AgentError::Provider(
                "no API key: set NODARA_LLM_API_KEY or OPENAI_API_KEY".to_string(),
            ));
        }
        Ok(Self::new(endpoint, model, api_key))
    }

    /// Model this provider will request.
    pub fn model(&self) -> &str {
        &self.model
    }
}

/// Convert provider-neutral messages to OpenAI-compatible content parts.
///
/// Messages without images remain ordinary strings, preserving compatibility
/// with text-only and older OpenAI-compatible servers. Messages with artifacts
/// use the documented `text` + `image_url` content-part shape.
fn wire_messages(messages: &[ChatMessage]) -> serde_json::Value {
    serde_json::Value::Array(
        messages
            .iter()
            .map(|message| {
                let role = serde_json::to_value(message.role)
                    .unwrap_or_else(|_| serde_json::Value::String("user".to_string()));
                if message.images.is_empty() {
                    return serde_json::json!({
                        "role": role,
                        "content": message.content,
                    });
                }
                let mut parts = vec![serde_json::json!({
                    "type": "text",
                    "text": message.content,
                })];
                parts.extend(message.images.iter().map(|image| {
                    serde_json::json!({
                        "type": "image_url",
                        "image_url": {
                            "url": format!(
                                "data:{};base64,{}",
                                image.media_type,
                                image.data_base64,
                            )
                        }
                    })
                }));
                serde_json::json!({
                    "role": role,
                    "content": parts,
                })
            })
            .collect(),
    )
}

impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.model
    }

    fn complete_streaming(
        &self,
        request: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
    ) -> AgentResult<ChatResponse> {
        let body = serde_json::json!({
            "model": self.model,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "messages": wire_messages(&request.messages),
            "response_format": if request.json_mode {
                serde_json::json!({ "type": "json_object" })
            } else {
                serde_json::Value::Null
            },
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        let mut call = ureq::post(&self.endpoint).timeout(std::time::Duration::from_secs(120));
        if let Some(key) = &self.api_key {
            call = call.set("authorization", &format!("Bearer {key}"));
        }
        let response = match call.set("content-type", "application/json").send_json(body) {
            Ok(response) => response,
            Err(ureq::Error::Status(status, response)) => {
                let detail = response.into_string().unwrap_or_default();
                let detail = detail.trim();
                if matches!(status, 400 | 404 | 405 | 415 | 422) {
                    return self.complete(request);
                }
                return Err(AgentError::Provider(if detail.is_empty() {
                    format!("provider returned HTTP {status}")
                } else {
                    format!("provider returned HTTP {status}: {detail}")
                }));
            }
            Err(error) => return Err(AgentError::Provider(error.to_string())),
        };

        let reader = BufReader::new(response.into_reader());
        let mut content = String::new();
        let mut model = self.model.clone();
        let mut usage = crate::model::TokenUsage::default();
        for line in reader.lines() {
            let line = line.map_err(|error| AgentError::Provider(error.to_string()))?;
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let payload: serde_json::Value = match serde_json::from_str(data) {
                Ok(payload) => payload,
                Err(_) => continue,
            };
            if let Some(value) = payload.get("model").and_then(serde_json::Value::as_str) {
                model = value.to_string();
            }
            if let Some(delta) = payload
                .get("choices")
                .and_then(|choices| choices.get(0))
                .and_then(|choice| choice.get("delta"))
                .and_then(|delta| delta.get("content"))
                .and_then(serde_json::Value::as_str)
            {
                content.push_str(delta);
                on_delta(delta);
            }
            if let Some(value) = payload.get("usage") {
                usage.prompt_tokens = value
                    .get("prompt_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(usage.prompt_tokens as u64)
                    as u32;
                usage.completion_tokens = value
                    .get("completion_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(usage.completion_tokens as u64)
                    as u32;
            }
        }
        if content.is_empty() {
            return self.complete(request);
        }
        Ok(ChatResponse {
            content,
            model,
            usage,
        })
    }

    fn complete(&self, request: &ChatRequest) -> AgentResult<ChatResponse> {
        let body = serde_json::json!({
            "model": self.model,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "messages": wire_messages(&request.messages),
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
            .map_err(|error| match error {
                // A status error carries the provider's diagnostic body (rate
                // limits, quota, model errors); surface it instead of the bare
                // status line.
                ureq::Error::Status(status, response) => {
                    let detail = response.into_string().unwrap_or_default();
                    let detail = detail.trim();
                    if detail.is_empty() {
                        AgentError::Provider(format!("provider returned HTTP {status}"))
                    } else {
                        AgentError::Provider(format!("provider returned HTTP {status}: {detail}"))
                    }
                }
                other => AgentError::Provider(other.to_string()),
            })?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChatImage;

    #[test]
    fn image_messages_use_openai_content_parts() {
        let message = ChatMessage::user_with_images(
            "inspect the screenshot",
            vec![ChatImage {
                name: "desktop.png".to_string(),
                media_type: "image/png".to_string(),
                data_base64: "AQID".to_string(),
                artifact_id: Some("artifact-1".to_string()),
            }],
        );

        let wire = wire_messages(&[message]);
        assert_eq!(wire[0]["role"], "user");
        assert_eq!(wire[0]["content"][0]["type"], "text");
        assert_eq!(wire[0]["content"][1]["type"], "image_url");
        assert_eq!(
            wire[0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,AQID"
        );
    }

    #[test]
    fn text_only_messages_remain_plain_strings() {
        let wire = wire_messages(&[ChatMessage::user("hello")]);
        assert_eq!(wire[0]["content"], "hello");
    }
}
