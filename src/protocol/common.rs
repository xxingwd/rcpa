use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::config::AuthKey;
use crate::config::ProviderProtocol;
use crate::config_service::ConfigSnapshot;
use crate::error::AppError;
use crate::middleware::auth;
use crate::server::AppState;

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
    pub fn provider_protocol(self) -> ProviderProtocol {
        match self {
            Operation::ChatCompletions | Operation::Completions | Operation::Embeddings => {
                ProviderProtocol::Completions
            }
            Operation::Responses => ProviderProtocol::Responses,
            Operation::Messages => ProviderProtocol::Messages,
            Operation::ListModels => ProviderProtocol::Completions,
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
    pub session_affinity: Option<SessionAffinity>,
}

#[derive(Debug, Clone)]
pub struct SessionAffinity {
    pub id: String,
    pub source: String,
    pub hash: String,
    pub key: SessionAffinityKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionAffinityKey(String);

impl SessionAffinityKey {
    pub fn for_request(
        api_key_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        session_id: &str,
    ) -> Self {
        Self(format!(
            "{}::{}::{}::{}::{}",
            protocol, operation, api_key_id, model, session_id
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAffinityMode {
    Disabled,
    Enabled,
}

impl SessionAffinityMode {
    fn affinity_for_request(
        self,
        headers: &axum::http::HeaderMap,
        body: &serde_json::Value,
        api_key_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
    ) -> Option<SessionAffinity> {
        match self {
            SessionAffinityMode::Disabled => None,
            SessionAffinityMode::Enabled => {
                let session = extract_session_affinity_id(headers, body)?;
                let key = SessionAffinityKey::for_request(
                    api_key_id,
                    protocol,
                    operation,
                    model,
                    &session.id,
                );
                Some(SessionAffinity {
                    hash: stable_hash_hex(&format!("{}:{}", session.source, session.id)),
                    id: session.id,
                    source: session.source,
                    key,
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExtractedSessionID {
    id: String,
    source: String,
}

fn extract_session_affinity_id(
    headers: &axum::http::HeaderMap,
    body: &serde_json::Value,
) -> Option<ExtractedSessionID> {
    let metadata_user_id = body
        .get("metadata")
        .and_then(|metadata| metadata.get("user_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if let Some(session_id) = metadata_user_id.and_then(extract_claude_metadata_session_id) {
        return Some(session_id_with_source("metadata_session", session_id));
    }

    if let Some(session_id) = first_header_value(headers, &["x-session-id"]) {
        return Some(session_id_with_source("x_session_id", session_id));
    }

    if let Some(session_id) = first_header_value(headers, &["session-id", "session_id"]) {
        return Some(session_id_with_source("session_id_header", session_id));
    }

    if let Some(session_id) = first_header_value(headers, &["x-client-request-id"]) {
        return Some(session_id_with_source("x_client_request_id", session_id));
    }

    if let Some(user_id) = metadata_user_id {
        return Some(session_id_with_source("metadata_user", user_id));
    }

    if let Some(conversation_id) = body
        .get("conversation_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(session_id_with_source("conversation_id", conversation_id));
    }

    message_content_hash(body)
        .map(|hash| session_id_with_source("message_hash", format!("{hash:016x}")))
}

fn session_id_with_source(source: impl Into<String>, id: impl Into<String>) -> ExtractedSessionID {
    ExtractedSessionID {
        source: source.into(),
        id: id.into(),
    }
}

fn first_header_value(headers: &axum::http::HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn extract_claude_metadata_session_id(user_id: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(user_id) {
        if let Some(session_id) = value
            .get("session_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(session_id.to_string());
        }
    }

    let (_, session_id) = user_id.split_once("_session_")?;
    let session_id = session_id.trim();
    if session_id.is_empty() {
        None
    } else {
        Some(session_id.to_string())
    }
}

fn message_content_hash(body: &serde_json::Value) -> Option<u64> {
    let mut parts = Vec::new();
    if let Some(value) = body.get("instructions") {
        parts.push(format!("instructions={}", value));
    }
    if let Some(value) = body.get("system") {
        parts.push(format!("system={}", value));
    }
    if let Some(value) = body.get("input") {
        parts.push(format!("input={}", value));
    }
    if let Some(messages) = body.get("messages").and_then(|value| value.as_array()) {
        push_first_message_content(messages, "system", &mut parts);
        push_first_message_content(messages, "user", &mut parts);
        push_first_message_content(messages, "assistant", &mut parts);
    }
    if parts.is_empty() {
        return None;
    }

    Some(fnv1a_hash(
        parts
            .iter()
            .flat_map(|part| part.as_bytes().iter().copied()),
    ))
}

fn stable_hash_hex(value: &str) -> String {
    format!("{:016x}", fnv1a_hash(value.as_bytes().iter().copied()))
}

fn fnv1a_hash(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn push_first_message_content(messages: &[serde_json::Value], role: &str, parts: &mut Vec<String>) {
    if let Some(content) = messages
        .iter()
        .find(|message| message.get("role").and_then(|value| value.as_str()) == Some(role))
        .and_then(|message| message.get("content"))
    {
        parts.push(format!("message:{}={}", role, content));
    }
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

/// Shared pre-proxy logic for LLM API handlers.
///
/// Performs authentication, body parsing, model validation, and context
/// construction, then delegates to `proxy_to_provider`. This eliminates
/// the boilerplate that was previously duplicated across all five
/// protocol handler functions.
pub async fn handle_llm_request(
    state: Arc<AppState>,
    headers: &axum::http::HeaderMap,
    body: String,
    protocol: Protocol,
    operation: Operation,
    session_affinity: SessionAffinityMode,
) -> crate::error::AppResult<axum::response::Response> {
    use crate::protocol::audit;
    use crate::protocol::openai::chat::proxy_to_provider;

    let start = std::time::Instant::now();
    let request_id = Uuid::new_v4();
    let request_body = body.as_bytes();
    let config_snapshot = state.config_service.snapshot();
    let default_model = config_snapshot.config.routing.default_model.clone();
    let requested_model = audit::model_from_body_or_default(&body, default_model.as_deref());

    let auth_result = auth::authenticate(&state, headers)?;
    let api_key_id = auth::persisted_api_key_id(&auth_result.key);

    let body_value: serde_json::Value = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(e) => {
            let err = AppError::BadRequest(e.to_string());
            audit::record_llm_error(
                &state,
                request_id,
                api_key_id,
                protocol,
                operation,
                requested_model.as_deref().unwrap_or(""),
                &err,
                start,
                Some(request_body),
            )
            .await;
            return Err(err);
        }
    };

    let model = body_value
        .get("model")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .or(default_model)
        .ok_or_else(|| AppError::BadRequest("model is required".to_string()))?;

    let resolved_model = match state.validate_model_name_for_key(&model, &auth_result.key) {
        Ok(result) => result,
        Err(err) => {
            audit::record_llm_error(
                &state,
                request_id,
                api_key_id,
                protocol,
                operation,
                &model,
                &err,
                start,
                Some(request_body),
            )
            .await;
            return Err(err);
        }
    };

    if let Err(err) = auth::check_model_access_for_request(&auth_result.key, &model) {
        audit::record_llm_error(
            &state,
            request_id,
            api_key_id,
            protocol,
            operation,
            &resolved_model,
            &err,
            start,
            Some(request_body),
        )
        .await;
        return Err(err);
    }

    let stream = body_value
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let session_affinity = session_affinity.affinity_for_request(
        headers,
        &body_value,
        api_key_id,
        protocol,
        operation,
        &model,
    );

    let ctx = ProxyContext {
        request_id,
        auth_key: auth_result.key.clone(),
        config_snapshot,
        protocol,
        operation,
        model: model.clone(),
        resolved_model: resolved_model.clone(),
        stream,
        session_affinity,
    };

    let req = ProxyRequest {
        id: ctx.request_id,
        protocol,
        operation,
        model: resolved_model,
        body: body_value,
        stream,
    };

    proxy_to_provider(state, req, ctx).await
}

#[cfg(test)]
mod tests {
    use super::{
        extract_session_affinity_id, message_content_hash, Operation, Protocol, SessionAffinityKey,
        TokenUsage,
    };
    use axum::http::HeaderMap;

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
    fn session_affinity_key_uses_api_key_id_not_secret() {
        let key = SessionAffinityKey::for_request(
            "key-persisted-id",
            Protocol::Completions,
            Operation::ChatCompletions,
            "gpt-4o",
            "session-123",
        );

        assert_eq!(
            key.as_str(),
            "completions::chat_completions::key-persisted-id::gpt-4o::session-123"
        );
        assert!(!key.as_str().contains("secret"));
    }

    #[test]
    fn session_affinity_id_prefers_claude_metadata_session() {
        let mut headers = HeaderMap::new();
        headers.insert("x-session-id", "header-session".parse().unwrap());
        let body = serde_json::json!({
            "metadata": {
                "user_id": "user_hash_account__session_claude-session"
            },
            "conversation_id": "conversation-a"
        });

        let session = extract_session_affinity_id(&headers, &body).unwrap();
        assert_eq!(session.source, "metadata_session");
        assert_eq!(session.id, "claude-session");
    }

    #[test]
    fn session_affinity_id_reads_codex_session_header() {
        let mut headers = HeaderMap::new();
        headers.insert("session-id", "codex-session".parse().unwrap());
        let body = serde_json::json!({
            "conversation_id": "conversation-a"
        });

        let session = extract_session_affinity_id(&headers, &body).unwrap();
        assert_eq!(session.source, "session_id_header");
        assert_eq!(session.id, "codex-session");
    }

    #[test]
    fn session_affinity_id_uses_conversation_before_message_hash() {
        let headers = HeaderMap::new();
        let body = serde_json::json!({
            "conversation_id": "conversation-a",
            "messages": [{"role": "user", "content": "hello"}]
        });

        let session = extract_session_affinity_id(&headers, &body).unwrap();
        assert_eq!(session.source, "conversation_id");
        assert_eq!(session.id, "conversation-a");
    }

    #[test]
    fn session_affinity_id_falls_back_to_stable_message_hash() {
        let headers = HeaderMap::new();
        let body = serde_json::json!({
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": "hi"},
                {"role": "user", "content": "second turn"}
            ]
        });
        let first = extract_session_affinity_id(&headers, &body).unwrap();
        let second = extract_session_affinity_id(&headers, &body).unwrap();

        assert_eq!(first.source, "message_hash");
        assert!(!first.id.is_empty());
        assert_eq!(first, second);
        assert_eq!(message_content_hash(&body), message_content_hash(&body));
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
