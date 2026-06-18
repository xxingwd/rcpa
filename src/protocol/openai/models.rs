use crate::error::AppError;
use crate::middleware::auth;
use crate::server::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use std::collections::BTreeMap;
use std::sync::Arc;

/// GET /v1/models
/// Returns the user-facing model names available to the authenticated key.
pub async fn list_models(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let auth_result = auth::authenticate_llm_api_key(&state, &headers)?;

    let snapshot = state.config_service.snapshot();
    let mut available: BTreeMap<String, String> = BTreeMap::new();

    for entry in snapshot.model_catalog() {
        let owner = entry
            .provider_name
            .clone()
            .unwrap_or_else(|| "provider".to_string());
        for model_name in entry.selectable_names {
            if crate::config::AppConfig::key_can_use_model(&auth_result.key, &model_name)
                && state.validate_model_name(&model_name).is_ok()
            {
                available.entry(model_name).or_insert_with(|| owner.clone());
            }
        }
    }

    for alias in auth_result.key.model_aliases.keys() {
        if crate::config::AppConfig::key_can_use_model(&auth_result.key, alias)
            && state
                .validate_model_name_for_key(alias, &auth_result.key)
                .is_ok()
        {
            available
                .entry(alias.clone())
                .or_insert_with(|| "key_alias".to_string());
        }
    }

    let models: Vec<serde_json::Value> = available
        .into_iter()
        .map(|(model, owned_by)| {
            serde_json::json!({
                "id": model,
                "object": "model",
                "created": 0,
                "owned_by": owned_by,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "object": "list",
        "data": models,
    })))
}
