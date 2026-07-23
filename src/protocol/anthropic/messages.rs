use axum::{extract::State, response::IntoResponse};
use std::sync::Arc;

use crate::error::AppError;
use crate::protocol::common::{Operation, Protocol, SessionAffinityMode};
use crate::server::AppState;

/// POST /v1/messages — Anthropic Messages API
pub async fn messages(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<impl IntoResponse, AppError> {
    crate::protocol::common::handle_llm_request(
        state,
        &headers,
        body,
        Protocol::Messages,
        Operation::Messages,
        SessionAffinityMode::Enabled,
    )
    .await
}
