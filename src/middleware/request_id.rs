use axum::{http::Request, middleware::Next, response::Response};
use uuid::Uuid;

/// Request ID injection middleware (axum from_fn compatible)
pub async fn request_id_middleware(mut request: Request<axum::body::Body>, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();

    request
        .headers_mut()
        .insert("X-Request-Id", request_id.parse().unwrap());

    let mut response = next.run(request).await;

    response
        .headers_mut()
        .insert("X-Request-Id", request_id.parse().unwrap());

    response
}
