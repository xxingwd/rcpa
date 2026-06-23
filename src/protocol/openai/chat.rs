use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::Stream;
use std::{
    collections::HashSet,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

use crate::error::{AppError, AppResult};
use crate::middleware::auth;
use crate::protocol::audit;
use crate::protocol::common::{
    Operation, Protocol, ProxyContext, ProxyRequest, SessionAffinityMode, TokenUsage,
};
use crate::retry::policy::RetryPolicy;
use crate::server::AppState;
use crate::stats::cost::CostCalculator;
use crate::store::NewRequestLog;

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<impl IntoResponse, AppError> {
    crate::protocol::common::handle_llm_request(
        state,
        &headers,
        body,
        Protocol::Completions,
        Operation::ChatCompletions,
        SessionAffinityMode::Enabled,
    )
    .await
}

/// Build an SSE streaming response from a ProviderStreamResponse.
/// When `alias` is Some, rewrite the `model` field in each SSE data event
/// so the user never sees the real provider model name.
fn stream_response(
    stream_resp: crate::provider::ProviderStreamResponse,
    alias: Option<String>,
    audit: StreamAudit,
) -> Response {
    let status =
        StatusCode::from_u16(stream_resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let body = axum::body::Body::from_stream(AuditedSseStream {
        inner: stream_resp.stream,
        alias,
        audit,
        terminated: false,
    });

    Response::builder()
        .status(status)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
        .body(body)
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::empty())
                .unwrap()
        })
}

/// Rewrite the `"model"` field in SSE data lines within a byte chunk.
fn rewrite_sse_model(chunk: &[u8], alias: &str) -> Vec<u8> {
    let text = match std::str::from_utf8(chunk) {
        Ok(t) => t,
        Err(_) => return chunk.to_vec(),
    };

    let mut output = String::with_capacity(chunk.len());
    for line in text.split_inclusive('\n') {
        if let Some(json_str) = line.strip_prefix("data: ") {
            let json_str = json_str.trim_end_matches('\n');
            if json_str == "[DONE]" {
                output.push_str(line);
            } else if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(obj) = v.as_object_mut() {
                    obj.insert(
                        "model".to_string(),
                        serde_json::Value::String(alias.to_string()),
                    );
                }
                output.push_str("data: ");
                output.push_str(&v.to_string());
                if line.ends_with('\n') {
                    output.push('\n');
                }
            } else {
                output.push_str(line);
            }
        } else {
            output.push_str(line);
        }
    }
    output.into_bytes()
}

fn calculate_cost_cents(
    snapshot: &crate::config_service::ConfigSnapshot,
    resolved_model: &str,
    provider_name: &str,
    tokens: Option<&TokenUsage>,
) -> u64 {
    let Some(tokens) = tokens else {
        return 0;
    };

    let provider_pricing = snapshot.provider_model_pricing(provider_name, resolved_model);

    if let Some(rule) = provider_pricing {
        let input_cost = (tokens.prompt_tokens as f64 / 1000.0) * rule.input_per_1k;
        let output_cost = (tokens.completion_tokens as f64 / 1000.0) * rule.output_per_1k;
        let cents = (input_cost + output_cost) * 100.0;
        if cents > 0.0 && cents < 1.0 {
            1
        } else {
            cents.round() as u64
        }
    } else {
        CostCalculator::from_config(&snapshot.config.cost).calculate(
            resolved_model,
            tokens.prompt_tokens,
            tokens.completion_tokens,
        )
    }
}

fn token_log_fields(tokens: Option<&TokenUsage>) -> (i64, i64, i64, i64, i64) {
    tokens
        .map(|t| {
            (
                t.prompt_tokens as i64,
                t.completion_tokens as i64,
                t.total_tokens as i64,
                t.cached_tokens as i64,
                t.cache_write_tokens as i64,
            )
        })
        .unwrap_or((0, 0, 0, 0, 0))
}

fn next_retry_backoff_ms(
    retry_policy: &RetryPolicy,
    attempt: u32,
    max_attempts: u32,
) -> Option<u64> {
    (attempt + 1 < max_attempts).then(|| retry_policy.backoff_for(attempt).as_millis() as u64)
}

fn record_provider_failure(state: &AppState, provider_name: &str) {
    if state.router.record_provider_failure(provider_name) {
        state.sticky_sessions.invalidate_provider(provider_name);
    }
}

fn record_sticky_session_success(state: &AppState, ctx: &ProxyContext, provider_name: &str) {
    if !ctx.config_snapshot.config.routing.sticky.enabled {
        return;
    }
    let Some(session_affinity) = &ctx.session_affinity else {
        return;
    };
    let ttl = std::time::Duration::from_secs(ctx.config_snapshot.config.routing.sticky.ttl_secs);
    state.sticky_sessions.set_with_ttl(
        session_affinity.key.clone().into_string(),
        provider_name.to_string(),
        ttl,
    );
}

struct ErrorLogInput<'a> {
    request_id: &'a str,
    api_key_id: &'a str,
    session_hash: Option<&'a str>,
    provider_name: &'a str,
    protocol: &'a str,
    model: &'a str,
    operation: &'a str,
    status_code: i64,
    success: bool,
    metadata_json: &'a str,
    latency_ms: i64,
    request_body: Option<&'a [u8]>,
}

/// Build a NewRequestLog for an error response (zero tokens, zero cost).
fn error_log_entry(input: ErrorLogInput<'_>) -> NewRequestLog<'_> {
    NewRequestLog {
        request_id: input.request_id,
        api_key_id: input.api_key_id,
        session_hash: input.session_hash,
        provider_name: input.provider_name,
        protocol: input.protocol,
        model: input.model,
        operation: input.operation,
        status_code: input.status_code,
        success: input.success,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        cached_tokens: 0,
        cache_write_tokens: 0,
        cost_cents: 0,
        latency_ms: input.latency_ms,
        first_byte_latency_ms: input.latency_ms,
        metadata_json: input.metadata_json,
        request_body: input.request_body,
        response_body: None,
    }
}

struct LogMetadataInput<'a> {
    ctx: &'a ProxyContext,
    provider_name: &'a str,
    protocol: &'a str,
    provider_model: &'a str,
    status_code: i64,
    error_code: Option<&'a str>,
    error_message: Option<&'a str>,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    sticky_hit: Option<bool>,
    selected_provider_reason: Option<&'a str>,
    attempts: &'a [RetryAttemptLog],
    upstream_path: Option<&'a str>,
}

struct LogSessionMetadata<'a> {
    id: &'a str,
    source: Option<&'a str>,
    hash: Option<&'a str>,
    affinity_key: Option<&'a str>,
}

struct LogMetadata<'a> {
    session: Option<LogSessionMetadata<'a>>,
    requested_model: &'a str,
    resolved_model: &'a str,
    provider_model: &'a str,
    routing_strategy: &'a str,
    sticky_enabled: bool,
    provider_name: &'a str,
    protocol: &'a str,
    status_code: i64,
    error_code: Option<&'a str>,
    error_message: Option<&'a str>,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    sticky_hit: Option<bool>,
    selected_provider_reason: Option<&'a str>,
    attempts: &'a [RetryAttemptLog],
    upstream_path: Option<&'a str>,
    currency: &'a str,
}

#[derive(Debug, Clone)]
struct RetryAttemptLog {
    attempt: u32,
    provider_name: String,
    protocol: String,
    provider_model: String,
    status_code: i64,
    error_code: Option<String>,
    error_message: Option<String>,
    retryable: bool,
    backoff_ms_before_next: Option<u64>,
    selected_via: &'static str,
    sticky_hit: bool,
    provider_healthy_before_attempt: bool,
}

impl RetryAttemptLog {
    fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "attempt": self.attempt,
            "provider_name": self.provider_name,
            "protocol": self.protocol,
            "provider_model": self.provider_model,
            "status_code": self.status_code,
            "error_code": self.error_code,
            "error_message": self.error_message,
            "retryable": self.retryable,
            "backoff_ms_before_next": self.backoff_ms_before_next,
            "selected_via": self.selected_via,
            "sticky_hit": self.sticky_hit,
            "provider_healthy_before_attempt": self.provider_healthy_before_attempt
        })
    }
}

fn log_metadata_json(input: LogMetadataInput<'_>) -> String {
    let session = input
        .ctx
        .session_affinity
        .as_ref()
        .map(|affinity| LogSessionMetadata {
            id: affinity.id.as_str(),
            source: Some(affinity.source.as_str()),
            hash: Some(affinity.hash.as_str()),
            affinity_key: Some(affinity.key.as_str()),
        });

    build_log_metadata_json(LogMetadata {
        session,
        requested_model: &input.ctx.model,
        resolved_model: &input.ctx.resolved_model,
        provider_model: input.provider_model,
        routing_strategy: &input.ctx.config_snapshot.config.routing.strategy,
        sticky_enabled: input.ctx.config_snapshot.config.routing.sticky.enabled,
        provider_name: input.provider_name,
        protocol: input.protocol,
        status_code: input.status_code,
        error_code: input.error_code,
        error_message: input.error_message,
        attempt_count: input.attempt_count,
        retry_count: input.retry_count,
        total_backoff_ms: input.total_backoff_ms,
        sticky_hit: input.sticky_hit,
        selected_provider_reason: input.selected_provider_reason,
        attempts: input.attempts,
        upstream_path: input.upstream_path,
        currency: &input.ctx.config_snapshot.config.cost.currency,
    })
}

fn build_log_metadata_json(input: LogMetadata<'_>) -> String {
    let session = input.session.map(|session| {
        serde_json::json!({
            "id": session.id,
            "source": session.source,
            "hash": session.hash,
            "affinity_key": session.affinity_key
        })
    });
    let error = input.error_code.or(input.error_message).map(|_| {
        serde_json::json!({
            "code": input.error_code,
            "message": input.error_message,
            "retryable": false
        })
    });
    let attempts: Vec<serde_json::Value> = input
        .attempts
        .iter()
        .map(RetryAttemptLog::as_json)
        .collect();

    serde_json::json!({
        "session": session,
        "models": {
            "requested": input.requested_model,
            "resolved": input.resolved_model,
            "provider": input.provider_model
        },
        "routing": {
            "strategy": input.routing_strategy,
            "sticky_enabled": input.sticky_enabled,
            "sticky_hit": input.sticky_hit,
            "selected_provider_reason": input.selected_provider_reason,
            "candidates": null
        },
        "retry": {
            "attempt_count": input.attempt_count,
            "retry_count": input.retry_count,
            "total_backoff_ms": input.total_backoff_ms,
            "attempts": attempts
        },
        "upstream": {
            "path": input.upstream_path,
            "request_id": null,
            "status_code": input.status_code
        },
        "pricing": {
            "currency": input.currency
        },
        "error": error,
        "body": {
            "request_body": "upstream_request_body",
            "response_body": "upstream_response_body"
        },
        "provider": {
            "name": input.provider_name,
            "protocol": input.protocol
        }
    })
    .to_string()
}

struct PreparedAttempt {
    provider_name: String,
    provider: Arc<dyn crate::provider::ProviderAdapter>,
    protocol: String,
    actual_model: String,
    request_body_bytes: Option<Vec<u8>>,
    selected_via: &'static str,
    sticky_hit: bool,
    provider_healthy_before_attempt: bool,
}

enum AttemptExecution {
    Response(Response),
    Retry(AppError),
    Fail(AppError),
}

struct RetryAttemptInput<'a> {
    attempt: u32,
    provider_name: &'a str,
    protocol: &'a str,
    provider_model: &'a str,
    status_code: i64,
    error_code: Option<String>,
    error_message: Option<String>,
    retryable: bool,
    backoff_ms_before_next: Option<u64>,
    selected_via: &'static str,
    sticky_hit: bool,
    provider_healthy_before_attempt: bool,
}

fn build_retry_attempt_log(input: RetryAttemptInput<'_>) -> RetryAttemptLog {
    RetryAttemptLog {
        attempt: input.attempt,
        provider_name: input.provider_name.to_string(),
        protocol: input.protocol.to_string(),
        provider_model: input.provider_model.to_string(),
        status_code: input.status_code,
        error_code: input.error_code,
        error_message: input.error_message,
        retryable: input.retryable,
        backoff_ms_before_next: input.backoff_ms_before_next,
        selected_via: input.selected_via,
        sticky_hit: input.sticky_hit,
        provider_healthy_before_attempt: input.provider_healthy_before_attempt,
    }
}

struct RequestOutcome<'a> {
    state: &'a AppState,
    provider_name: &'a str,
    actual_model: &'a str,
    latency: std::time::Duration,
    tokens: Option<&'a TokenUsage>,
    cost_cents: u64,
    api_key_id: &'a str,
    success: bool,
}

struct PersistErrorRequestLogInput<'a> {
    state: &'a AppState,
    ctx: &'a ProxyContext,
    api_key_id: &'a str,
    provider_name: &'a str,
    protocol: &'a str,
    provider_model: &'a str,
    error: &'a AppError,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    sticky_hit: Option<bool>,
    selected_provider_reason: Option<&'a str>,
    attempts: &'a [RetryAttemptLog],
    request_body: Option<&'a [u8]>,
    latency_ms: i64,
}

struct AttemptContext<'a> {
    state: &'a Arc<AppState>,
    req: &'a ProxyRequest,
    ctx: &'a ProxyContext,
    retry_policy: &'a RetryPolicy,
    api_key_id: &'a str,
    start: Instant,
    max_attempts: u32,
    total_backoff_ms: u64,
}

struct ProxyFailureInput<'a> {
    state: &'a AppState,
    ctx: &'a ProxyContext,
    api_key_id: &'a str,
    start: Instant,
    max_attempts: u32,
    total_backoff_ms: u64,
    last_error: Option<AppError>,
    last_actual_model: Option<&'a str>,
    retry_attempts: &'a [RetryAttemptLog],
    request_body: Option<&'a [u8]>,
}

struct StreamAuditInit<'a> {
    state: Arc<AppState>,
    ctx: &'a ProxyContext,
    provider_name: &'a str,
    protocol: &'a str,
    actual_model: &'a str,
    status_code: u16,
    start: Instant,
    first_byte_latency_ms: i64,
    request_body: Option<Vec<u8>>,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    routing_selected_reason: &'static str,
    routing_sticky_hit: bool,
    attempts: Vec<RetryAttemptLog>,
}

struct StreamAuditCompletion {
    success: bool,
    latency: std::time::Duration,
    latency_ms: i64,
    cost_cents: u64,
    error_code: Option<String>,
    error_message: Option<String>,
}

fn should_retry_provider_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::ProviderError { .. }
            | AppError::ProviderTimeout(_)
            | AppError::ServiceUnavailable(_)
    )
}

fn record_completed_request_outcome(input: RequestOutcome<'_>) {
    if input.success {
        input
            .state
            .router
            .record_provider_success(input.provider_name);
        input.state.stats.record_success(
            input.actual_model,
            input.provider_name,
            input.latency,
            input.tokens,
        );
        input.state.stats.record_cost(input.cost_cents);
        input
            .state
            .stats
            .record_key_usage(input.api_key_id, input.actual_model, input.cost_cents);
    } else {
        record_provider_failure(input.state, input.provider_name);
        input
            .state
            .stats
            .record_error(input.actual_model, input.provider_name);
    }
}

fn prepare_attempt(
    state: &AppState,
    req: &ProxyRequest,
    ctx: &ProxyContext,
    attempted_providers: &mut HashSet<String>,
) -> Result<PreparedAttempt, AppError> {
    let snapshot = ctx.config_snapshot.clone();

    let expected_protocol = req.operation.provider_protocol();
    let all_providers: Vec<String> = snapshot
        .providers_for_model(&req.model)
        .into_iter()
        .filter(|provider| snapshot.provider_supports_protocol(provider, expected_protocol))
        .collect();

    if !all_providers.is_empty()
        && all_providers
            .iter()
            .all(|p| attempted_providers.contains(p))
    {
        attempted_providers.clear();
    }

    let session_key = ctx
        .session_affinity
        .as_ref()
        .map(|affinity| affinity.key.as_str())
        .unwrap_or("");
    let route_decision = state.router.route_decision_with_exclusions(
        &req.model,
        &req.operation,
        state,
        &snapshot,
        session_key,
        Some(attempted_providers),
    )?;

    let provider_name = route_decision.provider_name.clone();
    attempted_providers.insert(provider_name.clone());
    let provider_healthy_before_attempt = state.router.is_provider_healthy(&provider_name);
    let provider = snapshot
        .registry
        .get(&provider_name)
        .ok_or_else(|| AppError::NoProviderAvailable(ctx.model.clone()))?;
    let protocol = ctx.operation.provider_protocol().to_string();
    let actual_model = provider.resolve_model(&req.model);
    let outbound_request_body = provider.serialize_request_body(req);
    let request_body_bytes = serde_json::to_vec(&outbound_request_body).ok();

    Ok(PreparedAttempt {
        provider_name,
        provider,
        protocol,
        actual_model,
        request_body_bytes,
        selected_via: route_decision.selection_reason.as_str(),
        sticky_hit: route_decision.sticky_hit,
        provider_healthy_before_attempt,
    })
}

async fn persist_error_request_log(input: PersistErrorRequestLogInput<'_>) {
    let error_code = input.error.error_code();
    let error_message = input.error.to_string();
    let request_id = input.ctx.request_id.to_string();
    let operation = input.ctx.operation.to_string();
    let metadata = log_metadata_json(LogMetadataInput {
        ctx: input.ctx,
        provider_name: input.provider_name,
        protocol: input.protocol,
        provider_model: input.provider_model,
        status_code: input.error.status_code().as_u16() as i64,
        error_code: Some(error_code.as_ref()),
        error_message: Some(&error_message),
        attempt_count: input.attempt_count,
        retry_count: input.retry_count,
        total_backoff_ms: input.total_backoff_ms,
        sticky_hit: input.sticky_hit,
        selected_provider_reason: input.selected_provider_reason,
        attempts: input.attempts,
        upstream_path: None,
    });

    // Best-effort audit logging: request handling should not fail if persistence is unavailable.
    if let Err(err) = audit::record_llm_request(
        input.state,
        error_log_entry(ErrorLogInput {
            request_id: &request_id,
            api_key_id: input.api_key_id,
            session_hash: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.hash.as_str()),
            provider_name: input.provider_name,
            protocol: input.protocol,
            model: &input.ctx.resolved_model,
            operation: &operation,
            status_code: input.error.status_code().as_u16() as i64,
            success: false,
            metadata_json: &metadata,
            latency_ms: input.latency_ms,
            request_body: input.request_body,
        }),
    )
    .await
    {
        tracing::warn!(
            request_id = %input.ctx.request_id,
            error = %err,
            "Failed to persist error request audit"
        );
    }
}

async fn execute_stream_attempt(
    input: AttemptContext<'_>,
    prepared: PreparedAttempt,
    attempt: u32,
    retry_attempts: &mut Vec<RetryAttemptLog>,
) -> AttemptExecution {
    let attempt_number = attempt + 1;

    match prepared.provider.proxy_stream(input.req.clone()).await {
        Ok(stream_resp) => {
            let first_byte_latency_ms = stream_resp.first_byte_latency_ms as i64;
            let mut attempts = std::mem::take(retry_attempts);
            attempts.push(build_retry_attempt_log(RetryAttemptInput {
                attempt: attempt_number,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                status_code: stream_resp.status as i64,
                error_code: None,
                error_message: None,
                retryable: false,
                backoff_ms_before_next: None,
                selected_via: prepared.selected_via,
                sticky_hit: prepared.sticky_hit,
                provider_healthy_before_attempt: prepared.provider_healthy_before_attempt,
            }));
            let audit = StreamAudit::new(StreamAuditInit {
                state: input.state.clone(),
                ctx: input.ctx,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                actual_model: &prepared.actual_model,
                status_code: stream_resp.status,
                start: input.start,
                first_byte_latency_ms,
                request_body: prepared.request_body_bytes,
                attempt_count: attempt_number,
                retry_count: attempt,
                total_backoff_ms: input.total_backoff_ms,
                routing_selected_reason: prepared.selected_via,
                routing_sticky_hit: prepared.sticky_hit,
                attempts,
            });

            record_sticky_session_success(input.state, input.ctx, &prepared.provider_name);

            tracing::info!(
                request_id = %input.ctx.request_id,
                model = %prepared.actual_model,
                public_model = %input.ctx.model,
                provider = %prepared.provider_name,
                status = stream_resp.status,
                first_byte_latency_ms = first_byte_latency_ms,
                stream = true,
                "Streaming request started"
            );

            AttemptExecution::Response(stream_response(
                stream_resp,
                Some(input.ctx.model.clone()),
                audit,
            ))
        }
        Err(error) => {
            record_provider_failure(input.state, &prepared.provider_name);
            let should_retry = should_retry_provider_error(&error);
            let error_message = error.to_string();
            retry_attempts.push(build_retry_attempt_log(RetryAttemptInput {
                attempt: attempt_number,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                status_code: error.status_code().as_u16() as i64,
                error_code: Some(error.error_code().into_owned()),
                error_message: Some(error_message.clone()),
                retryable: should_retry,
                backoff_ms_before_next: should_retry
                    .then(|| next_retry_backoff_ms(input.retry_policy, attempt, input.max_attempts))
                    .flatten(),
                selected_via: prepared.selected_via,
                sticky_hit: prepared.sticky_hit,
                provider_healthy_before_attempt: prepared.provider_healthy_before_attempt,
            }));

            if should_retry {
                return AttemptExecution::Retry(error);
            }

            input
                .state
                .stats
                .record_error(&prepared.actual_model, &prepared.provider_name);
            let latency_ms = input.start.elapsed().as_millis() as i64;
            persist_error_request_log(PersistErrorRequestLogInput {
                state: input.state,
                ctx: input.ctx,
                api_key_id: input.api_key_id,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                error: &error,
                attempt_count: attempt_number,
                retry_count: attempt,
                total_backoff_ms: input.total_backoff_ms,
                sticky_hit: Some(prepared.sticky_hit),
                selected_provider_reason: Some(prepared.selected_via),
                attempts: retry_attempts,
                request_body: prepared.request_body_bytes.as_deref(),
                latency_ms,
            })
            .await;

            AttemptExecution::Fail(error)
        }
    }
}

async fn execute_non_stream_attempt(
    input: AttemptContext<'_>,
    prepared: PreparedAttempt,
    attempt: u32,
    retry_attempts: &mut Vec<RetryAttemptLog>,
) -> AttemptExecution {
    let attempt_number = attempt + 1;

    match prepared.provider.proxy(input.req.clone()).await {
        Ok(response) => {
            let latency = input.start.elapsed();
            let latency_ms = latency.as_millis() as i64;
            let first_byte_latency_ms = response.first_byte_latency_ms as i64;
            let response_body_bytes = serde_json::to_vec(&response.body).ok();
            let cost_cents = calculate_cost_cents(
                &input.ctx.config_snapshot,
                &prepared.actual_model,
                &prepared.provider_name,
                response.tokens.as_ref(),
            );
            let (provider_error_code, provider_error_message) = if response.status >= 400 {
                audit::extract_provider_error(&response.body)
            } else {
                (None, None)
            };
            let should_retry_response = input.retry_policy.should_retry(response.status)
                && attempt_number < input.max_attempts;
            retry_attempts.push(build_retry_attempt_log(RetryAttemptInput {
                attempt: attempt_number,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                status_code: response.status as i64,
                error_code: provider_error_code.clone(),
                error_message: provider_error_message.clone(),
                retryable: should_retry_response,
                backoff_ms_before_next: should_retry_response
                    .then(|| next_retry_backoff_ms(input.retry_policy, attempt, input.max_attempts))
                    .flatten(),
                selected_via: prepared.selected_via,
                sticky_hit: prepared.sticky_hit,
                provider_healthy_before_attempt: prepared.provider_healthy_before_attempt,
            }));

            if should_retry_response {
                record_provider_failure(input.state, &prepared.provider_name);
                return AttemptExecution::Retry(AppError::ProviderError {
                    provider_name: prepared.provider_name.clone(),
                    status_code: StatusCode::from_u16(response.status).ok(),
                    error_code: provider_error_code,
                    message: provider_error_message
                        .unwrap_or_else(|| format!("Provider returned {}", response.status)),
                });
            }

            record_completed_request_outcome(RequestOutcome {
                state: input.state.as_ref(),
                provider_name: &prepared.provider_name,
                actual_model: &prepared.actual_model,
                latency,
                tokens: response.tokens.as_ref(),
                cost_cents,
                api_key_id: input.api_key_id,
                success: response.status < 400,
            });

            let (input_tokens, output_tokens, total_tokens, cached_tokens, cache_write_tokens) =
                token_log_fields(response.tokens.as_ref());
            let metadata = log_metadata_json(LogMetadataInput {
                ctx: input.ctx,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                status_code: response.status as i64,
                error_code: provider_error_code.as_deref(),
                error_message: provider_error_message.as_deref(),
                attempt_count: attempt_number,
                retry_count: attempt,
                total_backoff_ms: input.total_backoff_ms,
                sticky_hit: Some(prepared.sticky_hit),
                selected_provider_reason: Some(prepared.selected_via),
                attempts: retry_attempts,
                upstream_path: None,
            });
            // Best-effort audit logging: a completed response should still be returned if persistence fails.
            if let Err(err) = audit::record_llm_request(
                input.state,
                NewRequestLog {
                    request_id: &input.ctx.request_id.to_string(),
                    api_key_id: input.api_key_id,
                    session_hash: input
                        .ctx
                        .session_affinity
                        .as_ref()
                        .map(|affinity| affinity.hash.as_str()),
                    provider_name: &prepared.provider_name,
                    protocol: &prepared.protocol,
                    model: &input.ctx.resolved_model,
                    operation: &input.ctx.operation.to_string(),
                    status_code: response.status as i64,
                    success: response.status < 400 && provider_error_message.is_none(),
                    input_tokens,
                    output_tokens,
                    total_tokens,
                    cached_tokens,
                    cache_write_tokens,
                    cost_cents: cost_cents as i64,
                    latency_ms,
                    first_byte_latency_ms,
                    metadata_json: &metadata,
                    request_body: prepared.request_body_bytes.as_deref(),
                    response_body: response_body_bytes.as_deref(),
                },
            )
            .await
            {
                tracing::warn!(
                    request_id = %input.ctx.request_id,
                    error = %err,
                    "Failed to persist completed request audit"
                );
            }

            record_sticky_session_success(input.state, input.ctx, &prepared.provider_name);

            tracing::info!(
                request_id = %input.ctx.request_id,
                model = %prepared.actual_model,
                public_model = %input.ctx.resolved_model,
                provider = %prepared.provider_name,
                status = response.status,
                latency_ms = latency_ms,
                tokens = ?response.tokens,
                cost_cents = cost_cents,
                "Request completed"
            );

            let status =
                StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut body = response.body;
            if let Some(obj) = body.as_object_mut() {
                obj.insert(
                    "model".to_string(),
                    serde_json::Value::String(input.ctx.model.clone()),
                );
            }

            AttemptExecution::Response((status, Json(body)).into_response())
        }
        Err(error) => {
            record_provider_failure(input.state, &prepared.provider_name);
            let should_retry = should_retry_provider_error(&error);
            let error_message = error.to_string();
            retry_attempts.push(build_retry_attempt_log(RetryAttemptInput {
                attempt: attempt_number,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                status_code: error.status_code().as_u16() as i64,
                error_code: Some(error.error_code().into_owned()),
                error_message: Some(error_message.clone()),
                retryable: should_retry,
                backoff_ms_before_next: should_retry
                    .then(|| next_retry_backoff_ms(input.retry_policy, attempt, input.max_attempts))
                    .flatten(),
                selected_via: prepared.selected_via,
                sticky_hit: prepared.sticky_hit,
                provider_healthy_before_attempt: prepared.provider_healthy_before_attempt,
            }));

            if should_retry {
                return AttemptExecution::Retry(error);
            }

            input
                .state
                .stats
                .record_error(&prepared.actual_model, &prepared.provider_name);
            let latency_ms = input.start.elapsed().as_millis() as i64;
            persist_error_request_log(PersistErrorRequestLogInput {
                state: input.state,
                ctx: input.ctx,
                api_key_id: input.api_key_id,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                error: &error,
                attempt_count: attempt_number,
                retry_count: attempt,
                total_backoff_ms: input.total_backoff_ms,
                sticky_hit: Some(prepared.sticky_hit),
                selected_provider_reason: Some(prepared.selected_via),
                attempts: retry_attempts,
                request_body: prepared.request_body_bytes.as_deref(),
                latency_ms,
            })
            .await;

            AttemptExecution::Fail(error)
        }
    }
}

async fn finalize_proxy_failure(input: ProxyFailureInput<'_>) -> AppError {
    let provider_model = input.last_actual_model.unwrap_or(&input.ctx.resolved_model);
    let had_last_error = input.last_error.is_some();
    let err = input
        .last_error
        .unwrap_or_else(|| AppError::NoProviderAvailable(input.ctx.model.clone()));

    if had_last_error {
        input
            .state
            .stats
            .record_error(provider_model, "retry_exhausted");
    }

    let latency_ms = input.start.elapsed().as_millis() as i64;
    let protocol = input.ctx.protocol.to_string();
    persist_error_request_log(PersistErrorRequestLogInput {
        state: input.state,
        ctx: input.ctx,
        api_key_id: input.api_key_id,
        provider_name: "unrouted",
        protocol: &protocol,
        provider_model,
        error: &err,
        attempt_count: input.max_attempts,
        retry_count: input.max_attempts.saturating_sub(1),
        total_backoff_ms: input.total_backoff_ms,
        sticky_hit: None,
        selected_provider_reason: None,
        attempts: input.retry_attempts,
        request_body: input.request_body,
        latency_ms,
    })
    .await;

    err
}

struct StreamAudit {
    state: Arc<AppState>,
    config_snapshot: std::sync::Arc<crate::config_service::ConfigSnapshot>,
    request_id: String,
    api_key_id: String,
    provider_name: String,
    protocol: String,
    requested_model: String,
    resolved_model: String,
    session_id: Option<String>,
    session_source: Option<String>,
    session_hash: Option<String>,
    session_affinity_key: Option<String>,
    routing_strategy: String,
    sticky_enabled: bool,
    actual_model: String,
    operation: String,
    status_code: u16,
    start: Instant,
    first_byte_latency_ms: i64,
    first_chunk_seen: bool,
    request_body: Option<Vec<u8>>,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    routing_selected_reason: &'static str,
    routing_sticky_hit: bool,
    attempts: Vec<RetryAttemptLog>,
    tokens: Option<TokenUsage>,
    error_code: Option<String>,
    error_message: Option<String>,
    sse_buffer: String,
    finished: bool,
}

impl StreamAudit {
    fn new(input: StreamAuditInit<'_>) -> Self {
        Self {
            state: input.state,
            config_snapshot: input.ctx.config_snapshot.clone(),
            request_id: input.ctx.request_id.to_string(),
            api_key_id: auth::persisted_api_key_id(&input.ctx.auth_key).to_string(),
            provider_name: input.provider_name.to_string(),
            protocol: input.protocol.to_string(),
            requested_model: input.ctx.model.clone(),
            resolved_model: input.ctx.resolved_model.clone(),
            session_id: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.id.clone()),
            session_source: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.source.clone()),
            session_hash: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.hash.clone()),
            session_affinity_key: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.key.as_str().to_string()),
            routing_strategy: input.ctx.config_snapshot.config.routing.strategy.clone(),
            sticky_enabled: input.ctx.config_snapshot.config.routing.sticky.enabled,
            actual_model: input.actual_model.to_string(),
            operation: input.ctx.operation.to_string(),
            status_code: input.status_code,
            start: input.start,
            first_byte_latency_ms: input.first_byte_latency_ms,
            first_chunk_seen: false,
            request_body: input.request_body,
            attempt_count: input.attempt_count,
            retry_count: input.retry_count,
            total_backoff_ms: input.total_backoff_ms,
            routing_selected_reason: input.routing_selected_reason,
            routing_sticky_hit: input.routing_sticky_hit,
            attempts: input.attempts,
            tokens: None,
            error_code: None,
            error_message: None,
            sse_buffer: String::new(),
            finished: false,
        }
    }

    fn observe_chunk(&mut self, chunk: &[u8]) {
        if !self.first_chunk_seen {
            self.first_chunk_seen = true;
            self.first_byte_latency_ms = self.start.elapsed().as_millis() as i64;
        }

        let text = String::from_utf8_lossy(chunk);
        self.sse_buffer.push_str(&text);

        while let Some(pos) = self.sse_buffer.find('\n') {
            let line = self.sse_buffer[..pos].trim_end_matches('\r').to_string();
            self.sse_buffer.drain(..=pos);
            self.observe_sse_line(&line);
        }

        if self.sse_buffer.len() > 256 * 1024 {
            self.sse_buffer.clear();
        }
    }

    fn observe_sse_line(&mut self, line: &str) {
        let Some(data) = line.strip_prefix("data:") else {
            return;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
            return;
        };
        self.observe_json_event(&value);
    }

    fn observe_json_event(&mut self, value: &serde_json::Value) {
        if self.protocol == "messages" {
            if let Some(usage) = value
                .get("message")
                .and_then(|message| message.get("usage"))
            {
                let mut tokens = self.tokens.take().unwrap_or_default();
                tokens.apply_anthropic_stream_usage(usage);
                self.tokens = Some(tokens);
            }
            if let Some(usage) = value.get("usage") {
                let mut tokens = self.tokens.take().unwrap_or_default();
                tokens.apply_anthropic_stream_usage(usage);
                self.tokens = Some(tokens);
            }
        } else {
            let usage = value.get("usage").or_else(|| {
                value
                    .get("response")
                    .and_then(|response| response.get("usage"))
            });
            if let Some(usage) = usage.and_then(TokenUsage::from_openai_usage) {
                self.tokens = Some(usage);
            }
        }

        let (error_code, error_message) = audit::extract_provider_error(value);
        if error_code.is_some() || error_message.is_some() {
            self.error_code = error_code;
            self.error_message = error_message;
        }
    }

    fn completion_info(&self, stream_error: Option<(&str, String)>) -> StreamAuditCompletion {
        let (forced_code, forced_message) = stream_error
            .map(|(code, message)| (Some(code.to_string()), Some(message)))
            .unwrap_or((None, None));
        let error_code = forced_code.or_else(|| {
            self.error_code
                .clone()
                .or_else(|| (self.status_code >= 400).then(|| "stream_response_error".to_string()))
        });
        let error_message = forced_message.or_else(|| {
            self.error_message.clone().or_else(|| {
                (self.status_code >= 400)
                    .then(|| format!("Streaming response returned {}", self.status_code))
            })
        });
        let success = self.status_code < 400 && error_message.is_none();
        let latency = self.start.elapsed();
        let cost_cents = if success {
            calculate_cost_cents(
                &self.config_snapshot,
                &self.actual_model,
                &self.provider_name,
                self.tokens.as_ref(),
            )
        } else {
            0
        };

        StreamAuditCompletion {
            success,
            latency,
            latency_ms: latency.as_millis() as i64,
            cost_cents,
            error_code,
            error_message,
        }
    }

    fn record_completion_stats(&self, completion: &StreamAuditCompletion) {
        record_completed_request_outcome(RequestOutcome {
            state: &self.state,
            provider_name: &self.provider_name,
            actual_model: &self.actual_model,
            latency: completion.latency,
            tokens: self.tokens.as_ref(),
            cost_cents: completion.cost_cents,
            api_key_id: &self.api_key_id,
            success: completion.success,
        });
    }

    fn spawn_persist_completion_log(
        &self,
        completion: StreamAuditCompletion,
        input_tokens: i64,
        output_tokens: i64,
        total_tokens: i64,
        cached_tokens: i64,
        cache_write_tokens: i64,
    ) {
        let state = self.state.clone();
        let request_id = self.request_id.clone();
        let api_key_id = self.api_key_id.clone();
        let provider_name = self.provider_name.clone();
        let protocol = self.protocol.clone();
        let actual_model = self.actual_model.clone();
        let operation = self.operation.clone();
        let status_code = self.status_code;
        let first_byte_latency_ms = self.first_byte_latency_ms;
        let request_body = self.request_body.clone();
        let session_hash = self.session_hash.clone();
        let error_code = completion.error_code.clone();
        let error_message = completion.error_message.clone();
        let session_id = self.session_id.clone();
        let session_source = self.session_source.clone();
        let session_affinity_key = self.session_affinity_key.clone();
        let requested_model = self.requested_model.clone();
        let resolved_model = self.resolved_model.clone();
        let routing_strategy = self.routing_strategy.clone();
        let sticky_enabled = self.sticky_enabled;
        let currency = self.config_snapshot.config.cost.currency.clone();
        let metadata = build_log_metadata_json(LogMetadata {
            session: session_id.as_deref().map(|id| LogSessionMetadata {
                id,
                source: session_source.as_deref(),
                hash: session_hash.as_deref(),
                affinity_key: session_affinity_key.as_deref(),
            }),
            requested_model: &requested_model,
            resolved_model: &resolved_model,
            provider_model: &actual_model,
            routing_strategy: &routing_strategy,
            sticky_enabled,
            provider_name: &provider_name,
            protocol: &protocol,
            status_code: status_code as i64,
            error_code: error_code.as_deref(),
            error_message: error_message.as_deref(),
            attempt_count: self.attempt_count,
            retry_count: self.retry_count,
            total_backoff_ms: self.total_backoff_ms,
            sticky_hit: Some(self.routing_sticky_hit),
            selected_provider_reason: Some(self.routing_selected_reason),
            attempts: &self.attempts,
            upstream_path: None,
            currency: &currency,
        });

        tokio::spawn(async move {
            // Best-effort audit logging: streaming completion should not block socket teardown.
            if let Err(err) = audit::record_llm_request(
                &state,
                NewRequestLog {
                    request_id: &request_id,
                    api_key_id: &api_key_id,
                    session_hash: session_hash.as_deref(),
                    provider_name: &provider_name,
                    protocol: &protocol,
                    model: &resolved_model,
                    operation: &operation,
                    status_code: status_code as i64,
                    success: completion.success,
                    input_tokens,
                    output_tokens,
                    total_tokens,
                    cached_tokens,
                    cache_write_tokens,
                    cost_cents: completion.cost_cents as i64,
                    latency_ms: completion.latency_ms,
                    first_byte_latency_ms,
                    metadata_json: &metadata,
                    request_body: request_body.as_deref(),
                    response_body: None,
                },
            )
            .await
            {
                tracing::warn!(
                    request_id = %request_id,
                    error = %err,
                    "Failed to persist streaming completion audit"
                );
            }
        });
    }

    fn finish(&mut self, stream_error: Option<(&str, String)>) {
        if self.finished {
            return;
        }
        self.finished = true;

        let completion = self.completion_info(stream_error);
        self.record_completion_stats(&completion);

        let (input_tokens, output_tokens, total_tokens, cached_tokens, cache_write_tokens) =
            token_log_fields(self.tokens.as_ref());
        self.spawn_persist_completion_log(
            completion,
            input_tokens,
            output_tokens,
            total_tokens,
            cached_tokens,
            cache_write_tokens,
        );
    }
}

struct AuditedSseStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    alias: Option<String>,
    audit: StreamAudit,
    terminated: bool,
}

impl Stream for AuditedSseStream {
    type Item = Result<Bytes, reqwest::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.terminated {
            return Poll::Ready(None);
        }

        match self.inner.as_mut().poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Ok(chunk))) => {
                let alias = self.alias.clone();
                self.audit.observe_chunk(&chunk);
                let output = if let Some(alias) = alias {
                    Bytes::from(rewrite_sse_model(&chunk, &alias))
                } else {
                    chunk
                };
                Poll::Ready(Some(Ok(output)))
            }
            Poll::Ready(Some(Err(err))) => {
                let message = err.to_string();
                self.terminated = true;
                self.audit.finish(Some(("stream_error", message)));
                Poll::Ready(Some(Err(err)))
            }
            Poll::Ready(None) => {
                self.terminated = true;
                self.audit.finish(None);
                Poll::Ready(None)
            }
        }
    }
}

impl Drop for AuditedSseStream {
    fn drop(&mut self) {
        if !self.audit.finished {
            self.audit.finish(Some((
                "stream_closed",
                "Streaming response closed before completion".to_string(),
            )));
        }
    }
}

/// Shared proxy logic used by all protocol handlers
pub async fn proxy_to_provider(
    state: Arc<AppState>,
    req: ProxyRequest,
    ctx: ProxyContext,
) -> AppResult<Response> {
    let start = std::time::Instant::now();
    let original_request_body_bytes = serde_json::to_vec(&req.body).ok();
    let api_key_id = auth::persisted_api_key_id(&ctx.auth_key);
    let retry_policy = RetryPolicy::from_config(&ctx.config_snapshot.config.retry);
    let max_attempts = retry_policy.max_attempts();
    let mut last_error: Option<AppError> = None;
    let mut last_actual_model: Option<String> = None;
    let mut total_backoff_ms = 0u64;
    let mut attempted_providers = HashSet::new();
    let mut retry_attempts = Vec::new();

    for attempt in 0..max_attempts {
        if attempt > 0 {
            let backoff = retry_policy.backoff_for(attempt - 1);
            let backoff_ms = backoff.as_millis() as u64;
            total_backoff_ms = total_backoff_ms.saturating_add(backoff_ms);
            tracing::warn!(
                request_id = %ctx.request_id,
                attempt = attempt,
                backoff_ms,
                "Retrying request"
            );
            tokio::time::sleep(backoff).await;
            // Force check recovery before retrying
            state.router.check_recovery();
        }

        let prepared = match prepare_attempt(&state, &req, &ctx, &mut attempted_providers) {
            Ok(prepared) => prepared,
            Err(err) => {
                last_error = Some(err);
                continue;
            }
        };
        last_actual_model = Some(prepared.actual_model.clone());

        let execution = if ctx.stream {
            execute_stream_attempt(
                AttemptContext {
                    state: &state,
                    req: &req,
                    ctx: &ctx,
                    retry_policy: &retry_policy,
                    api_key_id,
                    start,
                    max_attempts,
                    total_backoff_ms,
                },
                prepared,
                attempt,
                &mut retry_attempts,
            )
            .await
        } else {
            execute_non_stream_attempt(
                AttemptContext {
                    state: &state,
                    req: &req,
                    ctx: &ctx,
                    retry_policy: &retry_policy,
                    api_key_id,
                    start,
                    max_attempts,
                    total_backoff_ms,
                },
                prepared,
                attempt,
                &mut retry_attempts,
            )
            .await
        };

        match execution {
            AttemptExecution::Response(response) => return Ok(response),
            AttemptExecution::Retry(err) => last_error = Some(err),
            AttemptExecution::Fail(err) => return Err(err),
        }
    }

    Err(finalize_proxy_failure(ProxyFailureInput {
        state: &state,
        ctx: &ctx,
        api_key_id,
        start,
        max_attempts,
        total_backoff_ms,
        last_error,
        last_actual_model: last_actual_model.as_deref(),
        retry_attempts: &retry_attempts,
        request_body: original_request_body_bytes.as_deref(),
    })
    .await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_sse_model_single_event() {
        let chunk = "data: {\"id\":\"chatcmpl-123\",\"model\":\"gpt-4o\",\"choices\":[]}\n\n";
        let result = rewrite_sse_model(chunk.as_bytes(), "my-gpt");
        let text = String::from_utf8(result).unwrap();
        assert!(text.contains("\"model\":\"my-gpt\""));
        assert!(!text.contains("\"model\":\"gpt-4o\""));
    }

    #[test]
    fn test_rewrite_sse_model_done() {
        let chunk = "data: [DONE]\n\n";
        let result = rewrite_sse_model(chunk.as_bytes(), "my-gpt");
        assert_eq!(String::from_utf8(result).unwrap(), "data: [DONE]\n\n");
    }

    #[test]
    fn test_rewrite_sse_model_non_data_line() {
        let chunk = "event: message\n";
        let result = rewrite_sse_model(chunk.as_bytes(), "my-gpt");
        assert_eq!(String::from_utf8(result).unwrap(), "event: message\n");
    }

    #[test]
    fn test_rewrite_sse_model_multiple_events() {
        let chunk = "data: {\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: {\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"!\"}}]}\n\ndata: [DONE]\n\n";
        let result = rewrite_sse_model(chunk.as_bytes(), "my-alias");
        let text = String::from_utf8(result).unwrap();
        assert!(!text.contains("gpt-4o"));
        assert_eq!(text.matches("my-alias").count(), 2);
        assert!(text.contains("[DONE]"));
    }
}
