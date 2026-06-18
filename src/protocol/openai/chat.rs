use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::Stream;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

use crate::error::{AppError, AppResult};
use crate::middleware::auth;
use crate::protocol::audit;
use crate::protocol::common::{Operation, Protocol, ProxyContext, ProxyRequest, TokenUsage};
use crate::server::AppState;
use crate::store::NewRequestLog;

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<impl IntoResponse, AppError> {
    let start = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();
    let operation = Operation::ChatCompletions;
    let protocol = Protocol::Completions;
    let request_body = body.as_bytes();
    let config_snapshot = state.config_service.snapshot();
    let default_model = config_snapshot.config.routing.default_model.clone();
    let requested_model = audit::model_from_body_or_default(&body, default_model.as_deref());

    let auth_result = auth::authenticate_llm_api_key(&state, &headers)?;
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
            );
            return Err(err);
        }
    };

    let model = body_value
        .get("model")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .or(default_model)
        .ok_or_else(|| AppError::BadRequest("model is required".to_string()))?;

    // Validate and resolve the user-facing model name.
    let (resolved_model, forced_provider) =
        match state.validate_model_name_for_key(&model, &auth_result.key) {
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
                );
                return Err(err);
            }
        };

    // Check model access against the resolved model name
    if let Err(err) =
        auth::check_model_access_for_request(&auth_result.key, &model, &resolved_model)
    {
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
        );
        return Err(err);
    }

    let stream = body_value
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let session_key = Some(format!("{}:{}", auth_result.key.key, model));

    let ctx = ProxyContext {
        request_id,
        auth_key: auth_result.key.clone(),
        config_snapshot,
        protocol,
        operation,
        model: model.clone(),
        resolved_model: resolved_model.clone(),
        stream,
        session_key,
        forced_provider,
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

#[allow(clippy::too_many_arguments)]
fn calculate_cost_cents(
    state: &AppState,
    snapshot: &crate::config_service::ConfigSnapshot,
    _user_model: &str,
    resolved_model: &str,
    provider_name: &str,
    _provider: &str,
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
        state.cost.calculate(
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

#[allow(clippy::too_many_arguments)]
struct StreamAudit {
    state: Arc<AppState>,
    config_snapshot: std::sync::Arc<crate::config_service::ConfigSnapshot>,
    request_id: String,
    api_key_id: String,
    auth_key: String,
    provider_name: String,
    provider: String,
    user_model: String,
    actual_model: String,
    operation: String,
    status_code: u16,
    start: Instant,
    first_byte_latency_ms: i64,
    first_chunk_seen: bool,
    request_body: Option<Vec<u8>>,
    tokens: Option<TokenUsage>,
    error_code: Option<String>,
    error_message: Option<String>,
    sse_buffer: String,
    finished: bool,
}

impl StreamAudit {
    #[allow(clippy::too_many_arguments)]
    fn new(
        state: Arc<AppState>,
        ctx: &ProxyContext,
        provider_name: &str,
        provider: &str,
        actual_model: &str,
        status_code: u16,
        start: Instant,
        first_byte_latency_ms: i64,
        request_body: Option<Vec<u8>>,
    ) -> Self {
        Self {
            state,
            config_snapshot: ctx.config_snapshot.clone(),
            request_id: ctx.request_id.to_string(),
            api_key_id: auth::persisted_api_key_id(&ctx.auth_key).to_string(),
            auth_key: ctx.auth_key.key.clone(),
            provider_name: provider_name.to_string(),
            provider: provider.to_string(),
            user_model: ctx.model.clone(),
            actual_model: actual_model.to_string(),
            operation: ctx.operation.to_string(),
            status_code,
            start,
            first_byte_latency_ms,
            first_chunk_seen: false,
            request_body,
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
        if self.provider == "messages" {
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

    fn finish(&mut self, stream_error: Option<(&str, String)>) {
        if self.finished {
            return;
        }
        self.finished = true;

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
        let latency_ms = latency.as_millis() as i64;
        let cost_cents = if success {
            calculate_cost_cents(
                &self.state,
                &self.config_snapshot,
                &self.user_model,
                &self.actual_model,
                &self.provider_name,
                &self.provider,
                self.tokens.as_ref(),
            )
        } else {
            0
        };

        if success {
            self.state
                .router
                .record_provider_success(&self.provider_name);
            self.state.stats.record_success(
                &self.actual_model,
                &self.provider_name,
                latency,
                self.tokens.as_ref(),
            );
            self.state.stats.record_cost(cost_cents);
            self.state
                .stats
                .record_key_usage(&self.auth_key, &self.actual_model, cost_cents);
        } else {
            self.state
                .router
                .record_provider_failure(&self.provider_name);
            self.state
                .stats
                .record_error(&self.actual_model, &self.provider_name);
        }

        let (input_tokens, output_tokens, total_tokens, cached_tokens, cache_write_tokens) =
            token_log_fields(self.tokens.as_ref());
        let _ = audit::record_llm_request(
            &self.state,
            NewRequestLog {
                request_id: &self.request_id,
                api_key_id: &self.api_key_id,
                provider_name: &self.provider_name,
                provider: &self.provider,
                model: &self.actual_model,
                operation: &self.operation,
                status_code: self.status_code as i64,
                input_tokens,
                output_tokens,
                total_tokens,
                cached_tokens,
                cache_write_tokens,
                cost_cents: cost_cents as i64,
                latency_ms,
                first_byte_latency_ms: self.first_byte_latency_ms,
                error_code: error_code.as_deref(),
                error: error_message.as_deref(),
                request_body: self.request_body.as_deref(),
                response_body: None,
            },
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

    // Route to provider (with retry support)
    let retry_policy = &state.retry_policy;
    let max_attempts = retry_policy.max_attempts();
    let mut last_error: Option<AppError> = None;
    let mut last_actual_model: Option<String> = None;

    for attempt in 0..max_attempts {
        if attempt > 0 {
            let backoff = retry_policy.backoff_for(attempt - 1);
            tracing::warn!(
                request_id = %ctx.request_id,
                attempt = attempt,
                backoff_ms = backoff.as_millis(),
                "Retrying request"
            );
            tokio::time::sleep(backoff).await;
        }

        let snapshot = ctx.config_snapshot.clone();

        // If a forced_provider is specified (from alias), use it directly;
        // otherwise route normally via the router.
        let provider_name = if let Some(ref forced) = ctx.forced_provider {
            state.router.ensure_provider_health_entry(forced);
            state.router.check_recovery();
            if snapshot.enabled_provider_can_serve_model(forced, &req.model)
                && snapshot.provider_protocol(forced) == Some(req.operation.provider_protocol())
                && state.router.is_provider_healthy(forced)
            {
                forced.clone()
            } else {
                last_error = Some(AppError::NoProviderAvailable(ctx.model.clone()));
                continue;
            }
        } else {
            let session_key = ctx.session_key.as_deref().unwrap_or("");
            match state
                .router
                .route(&req.model, &req.operation, &state, &snapshot, session_key)
            {
                Ok(name) => name,
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        };

        let provider = match snapshot.registry.get(&provider_name) {
            Some(b) => b,
            None => {
                last_error = Some(AppError::NoProviderAvailable(ctx.model.clone()));
                continue;
            }
        };
        let actual_model = provider.resolve_model(&req.model);
        last_actual_model = Some(actual_model.clone());
        let outbound_request_body = provider.serialize_request_body(&req);
        let request_body_bytes = serde_json::to_vec(&outbound_request_body).ok();

        // If streaming, use proxy_stream and return SSE response
        if ctx.stream {
            match provider.proxy_stream(req.clone()).await {
                Ok(stream_resp) => {
                    let first_byte_latency_ms = stream_resp.first_byte_latency_ms as i64;
                    let audit = StreamAudit::new(
                        state.clone(),
                        &ctx,
                        &provider_name,
                        provider.protocol(),
                        &actual_model,
                        stream_resp.status,
                        start,
                        first_byte_latency_ms,
                        request_body_bytes,
                    );

                    // Update sticky session
                    if snapshot.config.routing.sticky.enabled {
                        let session_key = format!("{}:{}", ctx.auth_key.key, ctx.model);
                        state
                            .sticky_sessions
                            .set(session_key, provider_name.clone());
                    }

                    tracing::info!(
                        request_id = %ctx.request_id,
                        model = %actual_model,
                        public_model = %ctx.model,
                        provider = %provider_name,
                        status = stream_resp.status,
                        first_byte_latency_ms = first_byte_latency_ms,
                        stream = true,
                        "Streaming request started"
                    );

                    return Ok(stream_response(stream_resp, Some(ctx.model.clone()), audit));
                }
                Err(e) => {
                    state.router.record_provider_failure(&provider_name);
                    let should_retry = matches!(
                        &e,
                        AppError::ProviderError { .. }
                            | AppError::ProviderTimeout(_)
                            | AppError::ServiceUnavailable(_)
                    );
                    if !should_retry {
                        state.stats.record_error(&actual_model, &provider_name);
                        let error_msg = e.to_string();
                        let latency_ms = start.elapsed().as_millis() as i64;
                        let _ = audit::record_llm_request(
                            &state,
                            NewRequestLog {
                                request_id: &ctx.request_id.to_string(),
                                api_key_id,
                                provider_name: &provider_name,
                                provider: provider.protocol(),
                                model: &actual_model,
                                operation: &ctx.operation.to_string(),
                                status_code: e.status_code().as_u16() as i64,
                                input_tokens: 0,
                                output_tokens: 0,
                                total_tokens: 0,
                                cached_tokens: 0,
                                cache_write_tokens: 0,
                                cost_cents: 0,
                                latency_ms,
                                first_byte_latency_ms: latency_ms,
                                error_code: Some(e.error_code()),
                                error: Some(&error_msg),
                                request_body: request_body_bytes.as_deref(),
                                response_body: None,
                            },
                        );
                        return Err(e);
                    }
                    last_error = Some(e);
                }
            }
        } else {
            // Non-streaming: execute proxy
            match provider.proxy(req.clone()).await {
                Ok(response) => {
                    let latency = start.elapsed();
                    let latency_ms = latency.as_millis() as u64;
                    let first_byte_latency_ms = response.first_byte_latency_ms as i64;
                    let response_body_bytes = serde_json::to_vec(&response.body).ok();

                    let cost_cents = calculate_cost_cents(
                        &state,
                        &ctx.config_snapshot,
                        &ctx.model,
                        &actual_model,
                        &provider_name,
                        provider.protocol(),
                        response.tokens.as_ref(),
                    );

                    if response.status < 400 {
                        state.router.record_provider_success(&provider_name);
                        state.stats.record_success(
                            &actual_model,
                            &provider_name,
                            latency,
                            response.tokens.as_ref(),
                        );
                        state.stats.record_cost(cost_cents);
                        state
                            .stats
                            .record_key_usage(&ctx.auth_key.key, &actual_model, cost_cents);
                    } else {
                        state.router.record_provider_failure(&provider_name);
                        state.stats.record_error(&actual_model, &provider_name);
                    }

                    let (provider_error_code, provider_error_message) = if response.status >= 400 {
                        audit::extract_provider_error(&response.body)
                    } else {
                        (None, None)
                    };
                    let (
                        input_tokens,
                        output_tokens,
                        total_tokens,
                        cached_tokens,
                        cache_write_tokens,
                    ) = token_log_fields(response.tokens.as_ref());
                    let _ = audit::record_llm_request(
                        &state,
                        NewRequestLog {
                            request_id: &ctx.request_id.to_string(),
                            api_key_id,
                            provider_name: &provider_name,
                            provider: provider.protocol(),
                            model: &actual_model,
                            operation: &ctx.operation.to_string(),
                            status_code: response.status as i64,
                            input_tokens,
                            output_tokens,
                            total_tokens,
                            cached_tokens,
                            cache_write_tokens,
                            cost_cents: cost_cents as i64,
                            latency_ms: latency_ms as i64,
                            first_byte_latency_ms,
                            error_code: provider_error_code.as_deref(),
                            error: provider_error_message.as_deref(),
                            request_body: request_body_bytes.as_deref(),
                            response_body: response_body_bytes.as_deref(),
                        },
                    );

                    // Update sticky session
                    if snapshot.config.routing.sticky.enabled {
                        let session_key = format!("{}:{}", ctx.auth_key.key, ctx.model);
                        state
                            .sticky_sessions
                            .set(session_key, provider_name.clone());
                    }

                    let status = StatusCode::from_u16(response.status)
                        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

                    tracing::info!(
                        request_id = %ctx.request_id,
                        model = %actual_model,
                        public_model = %ctx.resolved_model,
                        provider = %provider_name,
                        status = response.status,
                        latency_ms = latency_ms,
                        tokens = ?response.tokens,
                        cost_cents = cost_cents,
                        "Request completed"
                    );

                    let mut body = response.body;
                    if let Some(obj) = body.as_object_mut() {
                        obj.insert(
                            "model".to_string(),
                            serde_json::Value::String(ctx.model.clone()),
                        );
                    }

                    return Ok((status, Json(body)).into_response());
                }
                Err(e) => {
                    state.router.record_provider_failure(&provider_name);
                    let should_retry = matches!(
                        &e,
                        AppError::ProviderError { .. }
                            | AppError::ProviderTimeout(_)
                            | AppError::ServiceUnavailable(_)
                    );
                    if !should_retry {
                        state.stats.record_error(&actual_model, &provider_name);
                        let error_msg = e.to_string();
                        let latency_ms = start.elapsed().as_millis() as i64;
                        let _ = audit::record_llm_request(
                            &state,
                            NewRequestLog {
                                request_id: &ctx.request_id.to_string(),
                                api_key_id,
                                provider_name: &provider_name,
                                provider: provider.protocol(),
                                model: &actual_model,
                                operation: &ctx.operation.to_string(),
                                status_code: e.status_code().as_u16() as i64,
                                input_tokens: 0,
                                output_tokens: 0,
                                total_tokens: 0,
                                cached_tokens: 0,
                                cache_write_tokens: 0,
                                cost_cents: 0,
                                latency_ms,
                                first_byte_latency_ms: latency_ms,
                                error_code: Some(e.error_code()),
                                error: Some(&error_msg),
                                request_body: request_body_bytes.as_deref(),
                                response_body: None,
                            },
                        );
                        return Err(e);
                    }
                    last_error = Some(e);
                }
            }
        }
    }

    // All retries exhausted
    if let Some(err) = last_error {
        let log_model = last_actual_model.as_deref().unwrap_or(&ctx.resolved_model);
        state.stats.record_error(log_model, "retry_exhausted");
        let error_msg = err.to_string();
        let latency_ms = start.elapsed().as_millis() as i64;
        let _ = audit::record_llm_request(
            &state,
            NewRequestLog {
                request_id: &ctx.request_id.to_string(),
                api_key_id,
                provider_name: "unrouted",
                provider: &ctx.protocol.to_string(),
                model: log_model,
                operation: &ctx.operation.to_string(),
                status_code: err.status_code().as_u16() as i64,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                cached_tokens: 0,
                cache_write_tokens: 0,
                cost_cents: 0,
                latency_ms,
                first_byte_latency_ms: latency_ms,
                error_code: Some(err.error_code()),
                error: Some(&error_msg),
                request_body: original_request_body_bytes.as_deref(),
                response_body: None,
            },
        );
        Err(err)
    } else {
        let err = AppError::NoProviderAvailable(ctx.model.clone());
        let error_msg = err.to_string();
        let latency_ms = start.elapsed().as_millis() as i64;
        let _ = audit::record_llm_request(
            &state,
            NewRequestLog {
                request_id: &ctx.request_id.to_string(),
                api_key_id,
                provider_name: "unrouted",
                provider: &ctx.protocol.to_string(),
                model: &ctx.resolved_model,
                operation: &ctx.operation.to_string(),
                status_code: err.status_code().as_u16() as i64,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                cached_tokens: 0,
                cache_write_tokens: 0,
                cost_cents: 0,
                latency_ms,
                first_byte_latency_ms: latency_ms,
                error_code: Some(err.error_code()),
                error: Some(&error_msg),
                request_body: original_request_body_bytes.as_deref(),
                response_body: None,
            },
        );
        Err(err)
    }
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
