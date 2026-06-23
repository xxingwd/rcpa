use axum::{extract::State, response::IntoResponse};
use std::sync::Arc;

use crate::error::AppError;
use crate::protocol::common::{Operation, Protocol, SessionAffinityMode};
use crate::server::AppState;

/// POST /v1/completions — Traditional text completion
pub async fn completions(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<impl IntoResponse, AppError> {
    crate::protocol::common::handle_llm_request(
        state,
        &headers,
        body,
        Protocol::Completions,
        Operation::Completions,
        SessionAffinityMode::Disabled,
    )
    .await
}
