use std::sync::Arc;

use axum::{
    body::Body,
    extract::{FromRequest, Multipart, State},
    http::{header::CONTENT_TYPE, Request},
    response::{IntoResponse, Response},
};

use crate::error::AppError;
use crate::protocol::common::{
    MultipartFormData, MultipartPart, MultipartPartValue, Operation, Protocol, SessionAffinityMode,
};
use crate::server::AppState;

/// POST /v1/images/generations
pub async fn images_generations(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<impl IntoResponse, AppError> {
    crate::protocol::common::handle_llm_request(
        state,
        &headers,
        body,
        Protocol::Images,
        Operation::ImagesGenerations,
        SessionAffinityMode::Disabled,
    )
    .await
}

/// POST /v1/images/edits
pub async fn images_edits(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let headers = request.headers().clone();
    crate::middleware::auth::authenticate(&state, &headers)?;
    let is_multipart = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("multipart/form-data"));

    if !is_multipart {
        let bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
            .await
            .map_err(|error| AppError::BadRequest(error.to_string()))?;
        let body = String::from_utf8(bytes.to_vec())
            .map_err(|error| AppError::BadRequest(error.to_string()))?;
        return crate::protocol::common::handle_llm_request(
            state,
            &headers,
            body,
            Protocol::Images,
            Operation::ImagesEdits,
            SessionAffinityMode::Disabled,
        )
        .await;
    }

    let mut multipart = Multipart::from_request(request, &state)
        .await
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    let mut parts = Vec::new();
    let mut audit_body = serde_json::Map::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::BadRequest(error.to_string()))?
    {
        let name = field
            .name()
            .map(ToString::to_string)
            .ok_or_else(|| AppError::BadRequest("multipart field name is required".into()))?;
        let filename = field.file_name().map(ToString::to_string);
        let content_type = field.content_type().map(ToString::to_string);
        let is_file = filename.is_some() || matches!(name.as_str(), "image" | "image[]" | "mask");
        let data = field
            .bytes()
            .await
            .map_err(|error| AppError::BadRequest(error.to_string()))?;

        let value = if is_file {
            insert_audit_value(
                &mut audit_body,
                name.clone(),
                file_audit_value(filename.as_deref(), content_type.as_deref(), data.len()),
            );
            MultipartPartValue::File {
                filename,
                content_type,
                data,
            }
        } else {
            match String::from_utf8(data.to_vec()) {
                Ok(text) => {
                    let audit_value = if name == "stream" {
                        match text.as_str() {
                            "true" => serde_json::Value::Bool(true),
                            "false" => serde_json::Value::Bool(false),
                            _ => serde_json::Value::String(text.clone()),
                        }
                    } else {
                        serde_json::Value::String(text.clone())
                    };
                    insert_audit_value(&mut audit_body, name.clone(), audit_value);
                    MultipartPartValue::Text(text)
                }
                Err(_) => {
                    insert_audit_value(
                        &mut audit_body,
                        name.clone(),
                        file_audit_value(filename.as_deref(), content_type.as_deref(), data.len()),
                    );
                    MultipartPartValue::File {
                        filename,
                        content_type,
                        data,
                    }
                }
            }
        };
        parts.push(MultipartPart { name, value });
    }

    crate::protocol::common::handle_llm_multipart_request(
        state,
        &headers,
        serde_json::Value::Object(audit_body),
        Protocol::Images,
        Operation::ImagesEdits,
        SessionAffinityMode::Disabled,
        MultipartFormData { parts },
    )
    .await
}

fn file_audit_value(
    filename: Option<&str>,
    content_type: Option<&str>,
    size: usize,
) -> serde_json::Value {
    serde_json::json!({
        "filename": filename,
        "content_type": content_type,
        "size": size
    })
}

fn insert_audit_value(
    body: &mut serde_json::Map<String, serde_json::Value>,
    name: String,
    value: serde_json::Value,
) {
    match body.get_mut(&name) {
        Some(serde_json::Value::Array(values)) => values.push(value),
        Some(existing) => {
            let first = std::mem::take(existing);
            *existing = serde_json::Value::Array(vec![first, value]);
        }
        None => {
            body.insert(name, value);
        }
    }
}
