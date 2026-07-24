use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::borrow::Cow;
use std::time::Duration;

/// Unified error type for the entire application
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Authentication failed: {0}")]
    Unauthorized(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("No provider available for model: {0}")]
    NoProviderAvailable(String),

    #[error("Provider error from '{provider_name}': {message}")]
    ProviderError {
        provider_name: String,
        status_code: Option<StatusCode>,
        error_code: Option<String>,
        message: String,
    },

    #[error("Provider timeout after {0:?}")]
    ProviderTimeout(Duration),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Stream error: {0}")]
    StreamError(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl AppError {
    /// HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::ModelNotFound(_) => StatusCode::NOT_FOUND,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::NoProviderAvailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::ProviderError { status_code, .. } => {
                status_code.unwrap_or(StatusCode::BAD_GATEWAY)
            }
            AppError::ProviderTimeout(_) => StatusCode::GATEWAY_TIMEOUT,
            AppError::ProtocolError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::ConfigError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::StreamError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// Error code string for metrics/analytics
    pub fn error_code(&self) -> Cow<'static, str> {
        match self {
            AppError::Unauthorized(_) => Cow::Borrowed("unauthorized"),
            AppError::ModelNotFound(_) => Cow::Borrowed("model_not_found"),
            AppError::NotFound(_) => Cow::Borrowed("not_found"),
            AppError::NoProviderAvailable(_) => Cow::Borrowed("no_provider"),
            AppError::ProviderError { error_code, .. } => error_code
                .as_ref()
                .map(|code| Cow::Owned(code.clone()))
                .unwrap_or_else(|| Cow::Borrowed("provider_error")),
            AppError::ProviderTimeout(_) => Cow::Borrowed("provider_timeout"),
            AppError::ProtocolError(_) => Cow::Borrowed("protocol_error"),
            AppError::BadRequest(_) => Cow::Borrowed("bad_request"),
            AppError::ConfigError(_) => Cow::Borrowed("config_error"),
            AppError::Internal(_) => Cow::Borrowed("internal_error"),
            AppError::StreamError(_) => Cow::Borrowed("stream_error"),
            AppError::ServiceUnavailable(_) => Cow::Borrowed("service_unavailable"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code();
        let message = self.to_string();

        tracing::warn!(
            error_code = code.as_ref(),
            status = status.as_u16(),
            "{}",
            message
        );

        let body = json!({
            "error": {
                "code": code,
                "message": message,
                "type": "rcpa_error"
            }
        });

        (status, Json(body)).into_response()
    }
}

/// Convenience type alias
pub type AppResult<T> = Result<T, AppError>;

impl From<crate::store::StoreError> for AppError {
    fn from(e: crate::store::StoreError) -> Self {
        match e {
            crate::store::StoreError::NotFound(msg) => AppError::NotFound(msg),
            crate::store::StoreError::InvalidData(msg) => AppError::BadRequest(msg),
            crate::store::StoreError::Maintenance(msg) => AppError::Internal(msg),
            crate::store::StoreError::Sql(err) => AppError::Internal(err.to_string()),
        }
    }
}
