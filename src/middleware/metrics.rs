use axum::{extract::State, http::Request, middleware::Next, response::Response};
use std::sync::Arc;

use crate::server::AppState;

/// Metrics/logging middleware for API routes.
pub async fn metrics_middleware(
    State(state): State<Arc<AppState>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if request.uri().path().starts_with("/v1/admin/") {
        return next.run(request).await;
    }

    let path = request.uri().path().to_string();
    let method = request.method().as_str().to_string();
    let start = std::time::Instant::now();
    state.stats.record_request(&path, &method);
    let response = next.run(request).await;
    state
        .stats
        .record_response(&path, response.status().as_u16(), start.elapsed());
    response
}
