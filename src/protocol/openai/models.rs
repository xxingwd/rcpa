use crate::error::AppError;
use crate::middleware::auth;
use crate::server::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// GET /v1/models
/// Returns the user-facing model names available to the authenticated key.
pub async fn list_models(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let auth_result = auth::authenticate(&state, &headers)?;

    let snapshot = state.config_service.snapshot();
    let mut available: BTreeMap<String, String> = BTreeMap::new();

    let mut visible_names = BTreeSet::new();
    for entry in snapshot.model_catalog() {
        for model_name in entry.selectable_names {
            visible_names.insert(model_name);
        }
    }
    visible_names.extend(auth_result.key.model_aliases.keys().cloned());

    for model_name in visible_names {
        if crate::config::AppConfig::key_can_use_model(&auth_result.key, &model_name)
            && state
                .validate_model_name_for_key(&model_name, &auth_result.key)
                .is_ok()
        {
            let owner = if auth_result.key.model_aliases.contains_key(&model_name) {
                "key_alias".to_string()
            } else {
                snapshot
                    .endpoints_for_alias(&model_name, Some(&auth_result.key))
                    .first()
                    .map(|endpoint| endpoint.provider_name.clone())
                    .unwrap_or_else(|| "provider".to_string())
            };
            available.insert(model_name, owner);
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
