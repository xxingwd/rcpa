use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AuthKey;
use crate::config_service::ConfigSnapshot;
use std::sync::Arc;

/// Supported AI API protocols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Protocol {
    Completions,
    Responses,
    Messages,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Completions => write!(f, "completions"),
            Protocol::Responses => write!(f, "responses"),
            Protocol::Messages => write!(f, "messages"),
        }
    }
}

/// Operation type for routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operation {
    ChatCompletions,
    Completions,
    Responses,
    Embeddings,
    Messages,
    ListModels,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operation::ChatCompletions => write!(f, "chat_completions"),
            Operation::Completions => write!(f, "completions"),
            Operation::Responses => write!(f, "responses"),
            Operation::Embeddings => write!(f, "embeddings"),
            Operation::Messages => write!(f, "messages"),
            Operation::ListModels => write!(f, "list_models"),
        }
    }
}

impl Operation {
    pub fn provider_protocol(self) -> &'static str {
        match self {
            Operation::ChatCompletions | Operation::Completions | Operation::Embeddings => {
                "completions"
            }
            Operation::Responses => "responses",
            Operation::Messages => "messages",
            Operation::ListModels => "completions",
        }
    }
}

/// Unified request representation for internal routing
#[derive(Debug, Clone)]
pub struct ProxyRequest {
    pub id: Uuid,
    pub protocol: Protocol,
    pub operation: Operation,
    pub model: String,
    pub body: serde_json::Value,
    pub stream: bool,
}

/// Context carried through the proxy pipeline
#[derive(Clone)]
pub struct ProxyContext {
    pub request_id: Uuid,
    pub auth_key: AuthKey,
    pub config_snapshot: Arc<ConfigSnapshot>,
    pub protocol: Protocol,
    pub operation: Operation,
    pub model: String,
    pub resolved_model: String,
    pub stream: bool,
    pub session_key: Option<String>,
    /// If set, bypass routing and send to this specific provider (from model alias)
    pub forced_provider: Option<String>,
}

/// Token usage information extracted from responses
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cached_tokens: u64,
    pub cache_write_tokens: u64,
}

impl TokenUsage {
    pub fn from_openai_body(body: &serde_json::Value) -> Option<Self> {
        Self::from_openai_usage(body.get("usage")?)
    }

    pub fn from_openai_usage(usage: &serde_json::Value) -> Option<Self> {
        if usage.is_null() {
            return None;
        }

        let prompt_tokens = first_u64(usage, &["prompt_tokens", "input_tokens"]).unwrap_or(0);
        let completion_tokens =
            first_u64(usage, &["completion_tokens", "output_tokens"]).unwrap_or(0);
        let total_tokens = usage
            .get("total_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(prompt_tokens + completion_tokens);

        let cached_tokens = nested_u64(
            usage,
            &[
                ("prompt_tokens_details", "cached_tokens"),
                ("input_tokens_details", "cached_tokens"),
            ],
        )
        .or_else(|| first_u64(usage, &["cache_read_input_tokens", "cached_tokens"]))
        .unwrap_or(0);

        let cache_write_tokens = nested_u64(
            usage,
            &[
                ("prompt_tokens_details", "cache_write_tokens"),
                ("input_tokens_details", "cache_write_tokens"),
            ],
        )
        .or_else(|| {
            first_u64(
                usage,
                &["cache_creation_input_tokens", "cache_write_tokens"],
            )
        })
        .unwrap_or(0);

        if prompt_tokens == 0
            && completion_tokens == 0
            && total_tokens == 0
            && cached_tokens == 0
            && cache_write_tokens == 0
        {
            return None;
        }

        Some(Self {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens,
            cache_write_tokens,
        })
    }

    pub fn from_anthropic_body(body: &serde_json::Value) -> Option<Self> {
        Self::from_anthropic_usage(body.get("usage")?)
    }

    pub fn from_anthropic_usage(usage: &serde_json::Value) -> Option<Self> {
        if usage.is_null() {
            return None;
        }

        let uncached_input = usage.get("input_tokens").and_then(|v| v.as_u64());
        let cached_tokens = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_write_tokens = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let completion_tokens = usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let Some(uncached_input) = uncached_input else {
            return if completion_tokens > 0 {
                Some(Self {
                    completion_tokens,
                    total_tokens: completion_tokens,
                    ..Default::default()
                })
            } else {
                None
            };
        };

        let prompt_tokens = uncached_input + cached_tokens + cache_write_tokens;
        Some(Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            cached_tokens,
            cache_write_tokens,
        })
    }

    pub fn apply_anthropic_stream_usage(&mut self, usage: &serde_json::Value) {
        if let Some(next) = Self::from_anthropic_usage(usage) {
            if usage.get("input_tokens").is_some()
                || usage.get("cache_read_input_tokens").is_some()
                || usage.get("cache_creation_input_tokens").is_some()
            {
                self.prompt_tokens = next.prompt_tokens;
                self.cached_tokens = next.cached_tokens;
                self.cache_write_tokens = next.cache_write_tokens;
            }
            if usage.get("output_tokens").is_some() {
                self.completion_tokens = next.completion_tokens;
            }
            self.total_tokens = self.prompt_tokens + self.completion_tokens;
        }
    }
}

fn first_u64(value: &serde_json::Value, fields: &[&str]) -> Option<u64> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(|v| v.as_u64()))
}

fn nested_u64(value: &serde_json::Value, fields: &[(&str, &str)]) -> Option<u64> {
    fields.iter().find_map(|(outer, inner)| {
        value
            .get(*outer)
            .and_then(|v| v.get(*inner))
            .and_then(|v| v.as_u64())
    })
}

/// Result of proxying a request
#[derive(Debug, Clone)]
pub struct ProxyOutcome {
    pub request_id: Uuid,
    pub provider: String,
    pub model: String,
    pub status: u16,
    pub latency_ms: u64,
    pub tokens: Option<TokenUsage>,
    pub error: Option<String>,
    pub cost_cents: u64,
}

#[cfg(test)]
mod tests {
    use super::TokenUsage;

    #[test]
    fn openai_usage_reads_chat_and_responses_shapes() {
        let chat = serde_json::json!({
            "prompt_tokens": 2006,
            "completion_tokens": 300,
            "total_tokens": 2306,
            "prompt_tokens_details": { "cached_tokens": 1920 }
        });
        let tokens = TokenUsage::from_openai_usage(&chat).unwrap();
        assert_eq!(tokens.prompt_tokens, 2006);
        assert_eq!(tokens.completion_tokens, 300);
        assert_eq!(tokens.total_tokens, 2306);
        assert_eq!(tokens.cached_tokens, 1920);

        let responses = serde_json::json!({
            "input_tokens": 36,
            "input_tokens_details": { "cached_tokens": 12 },
            "output_tokens": 87,
            "output_tokens_details": { "reasoning_tokens": 0 },
            "total_tokens": 123
        });
        let tokens = TokenUsage::from_openai_usage(&responses).unwrap();
        assert_eq!(tokens.prompt_tokens, 36);
        assert_eq!(tokens.completion_tokens, 87);
        assert_eq!(tokens.total_tokens, 123);
        assert_eq!(tokens.cached_tokens, 12);
    }

    #[test]
    fn anthropic_usage_counts_cache_tokens_as_input_detail() {
        let usage = serde_json::json!({
            "input_tokens": 20,
            "output_tokens": 8,
            "cache_read_input_tokens": 9,
            "cache_creation_input_tokens": 4
        });

        let tokens = TokenUsage::from_anthropic_usage(&usage).unwrap();
        assert_eq!(tokens.prompt_tokens, 33);
        assert_eq!(tokens.completion_tokens, 8);
        assert_eq!(tokens.total_tokens, 41);
        assert_eq!(tokens.cached_tokens, 9);
        assert_eq!(tokens.cache_write_tokens, 4);
    }

    #[test]
    fn anthropic_stream_delta_updates_output_without_losing_input() {
        let mut tokens = TokenUsage::from_anthropic_usage(&serde_json::json!({
            "input_tokens": 20,
            "output_tokens": 1,
            "cache_read_input_tokens": 9,
            "cache_creation_input_tokens": 4
        }))
        .unwrap();

        tokens.apply_anthropic_stream_usage(&serde_json::json!({ "output_tokens": 15 }));
        assert_eq!(tokens.prompt_tokens, 33);
        assert_eq!(tokens.completion_tokens, 15);
        assert_eq!(tokens.total_tokens, 48);
        assert_eq!(tokens.cached_tokens, 9);
        assert_eq!(tokens.cache_write_tokens, 4);
    }
}
