use std::time::Instant;

use uuid::Uuid;

use crate::error::AppError;
use crate::protocol::common::{Operation, Protocol};
use crate::server::AppState;
use crate::store::NewRequestLog;

pub fn model_from_body_or_default(body: &str, default_model: Option<&str>) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("model")
                .and_then(|model| model.as_str())
                .map(ToString::to_string)
        })
        .or_else(|| default_model.map(ToString::to_string))
}

#[allow(clippy::too_many_arguments)]
pub fn record_llm_error(
    state: &AppState,
    request_id: Uuid,
    api_key_id: &str,
    protocol: Protocol,
    operation: Operation,
    model: &str,
    error: &AppError,
    start: Instant,
    request_body: Option<&[u8]>,
) {
    let error_msg = error.to_string();
    let provider = protocol.to_string();
    let elapsed_ms = start.elapsed().as_millis() as i64;

    if let Err(err) = record_llm_request(
        state,
        NewRequestLog {
            request_id: &request_id.to_string(),
            api_key_id,
            provider_name: "unrouted",
            provider: &provider,
            model,
            operation: &operation.to_string(),
            status_code: error.status_code().as_u16() as i64,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cached_tokens: 0,
            cache_write_tokens: 0,
            cost_cents: 0,
            latency_ms: elapsed_ms,
            first_byte_latency_ms: elapsed_ms,
            error_code: Some(error.error_code()),
            error: Some(&error_msg),
            request_body,
            response_body: None,
        },
    ) {
        tracing::error!(request_id = %request_id, error = %err, "Failed to persist LLM error log");
    }
}

pub fn record_llm_request(
    state: &AppState,
    entry: NewRequestLog<'_>,
) -> crate::store::StoreResult<crate::store::DbRequestLog> {
    state.store.insert_request_log_entry(entry)
}

pub fn extract_provider_error(body: &serde_json::Value) -> (Option<String>, Option<String>) {
    let error = body.get("error");
    let code = error
        .and_then(|value| value.get("code"))
        .or_else(|| body.get("code"))
        .and_then(value_to_string);
    let message = error
        .and_then(|value| value.get("message"))
        .or_else(|| body.get("message"))
        .and_then(value_to_string);

    (code, message)
}

fn value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}
