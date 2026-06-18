use axum::{extract::State, response::IntoResponse};
use std::sync::Arc;

use crate::error::AppError;
use crate::middleware::auth;
use crate::protocol::audit;
use crate::protocol::common::{Operation, Protocol, ProxyContext, ProxyRequest};
use crate::server::AppState;

use super::chat::proxy_to_provider;

/// POST /v1/responses — OpenAI Responses API (Codex/agent format)
pub async fn responses(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<impl IntoResponse, AppError> {
    let start = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();
    let operation = Operation::Responses;
    let protocol = Protocol::Responses;
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

    let ctx = ProxyContext {
        request_id,
        auth_key: auth_result.key.clone(),
        config_snapshot,
        protocol,
        operation,
        model: model.clone(),
        resolved_model: resolved_model.clone(),
        stream,
        session_key: None,
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
