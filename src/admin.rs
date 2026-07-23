use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::config::{AppConfig, AuthKey, EndpointConfig, ModelPricing, ModelRule, ProviderConfig};
use crate::error::AppError;
use crate::server::AppState;

fn check_admin(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let token = headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Admin token required".into()))?;
    if token != state.admin_token {
        return Err(AppError::Unauthorized("Invalid admin token".into()));
    }
    Ok(())
}

// === API Key Management ===

#[derive(Debug, Deserialize)]
pub struct CreateKeyPayload {
    pub name: Option<String>,
    pub labels: Option<String>,
    pub allowed_models: Option<Vec<KeyModelPayload>>,
    pub model_aliases: Option<HashMap<String, String>>,
    pub allowed_providers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateKeyPayload {
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub name: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub labels: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub allowed_models: PatchField<Vec<KeyModelPayload>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub model_aliases: PatchField<HashMap<String, String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub allowed_providers: PatchField<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub enum PatchField<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<PatchField<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
        .map(|value| value.map(PatchField::Value).unwrap_or(PatchField::Null))
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyModelPayload {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStatusPayload {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateModelStatusPayload {
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct CreateKeyResponse {
    #[serde(flatten)]
    pub info: crate::config_service::AuthKeyView,
}

fn generate_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let random_chars: String = (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..62);
            let chars = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            chars[idx] as char
        })
        .collect();
    format!("rcpa_{}", random_chars)
}

fn ensure_model_name_is_canonical(value: &str, field: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::BadRequest(format!("{} cannot be empty", field)));
    }
    if value.trim() != value {
        return Err(AppError::BadRequest(format!(
            "{} '{}' must not contain leading or trailing whitespace",
            field, value
        )));
    }
    Ok(())
}

fn validate_key_model_config(
    config: &AppConfig,
    allowed_models: &[ModelRule],
    model_aliases: &HashMap<String, String>,
    allowed_providers: &[String],
) -> Result<(), AppError> {
    let target_names = config.key_visible_model_names();
    let private_alias_names: HashSet<String> = model_aliases.keys().cloned().collect();
    let provider_names: HashSet<&String> = config
        .providers
        .iter()
        .map(|provider| &provider.name)
        .collect();

    let mut seen_allowed_providers = HashSet::new();
    for provider_name in allowed_providers {
        ensure_model_name_is_canonical(provider_name, "allowed provider")?;
        if !seen_allowed_providers.insert(provider_name) {
            return Err(AppError::BadRequest(format!(
                "Allowed provider '{}' is duplicated",
                provider_name
            )));
        }
        if !provider_names.contains(provider_name) {
            return Err(AppError::BadRequest(format!(
                "Allowed provider '{}' is not a configured provider",
                provider_name
            )));
        }
    }

    for (alias, target) in model_aliases {
        ensure_model_name_is_canonical(alias, "model alias")?;
        ensure_model_name_is_canonical(target, "model alias target")?;
        if target_names.contains(alias) {
            return Err(AppError::BadRequest(format!(
                "Key model alias '{}' conflicts with a platform model name",
                alias
            )));
        }
        if !target_names.contains(target) {
            return Err(AppError::BadRequest(format!(
                "Key model alias '{}' targets unknown platform model '{}'",
                alias, target
            )));
        }
    }

    for model in allowed_models {
        ensure_model_name_is_canonical(&model.name, "allowed model")?;
        match model.status.as_str() {
            "enabled" | "disabled" => {}
            other => {
                return Err(AppError::BadRequest(format!(
                    "Allowed model '{}' has invalid status '{}'",
                    model.name, other
                )));
            }
        }
        if !target_names.contains(&model.name) && !private_alias_names.contains(&model.name) {
            return Err(AppError::BadRequest(format!(
                "Allowed model '{}' is not a platform model or key alias",
                model.name
            )));
        }
    }

    Ok(())
}

fn key_model_payloads_to_rules(models: Vec<KeyModelPayload>) -> Vec<ModelRule> {
    models
        .into_iter()
        .map(|model| ModelRule {
            name: model.name,
            status: model.status,
            pricing: None,
            aliases: Vec::new(),
        })
        .collect()
}

fn update_key_status(
    config: &mut crate::config::AppConfig,
    id: &str,
    status: &str,
) -> Result<(), AppError> {
    let key = config
        .keys
        .iter_mut()
        .find(|key| key.id == id)
        .ok_or_else(|| AppError::BadRequest(format!("API key '{}' not found", id)))?;
    key.status = status.to_string();
    Ok(())
}

pub async fn list_all_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    Ok(Json(state.config_service.snapshot().auth_keys()))
}

pub async fn create_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateKeyPayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;

    let allowed_models = key_model_payloads_to_rules(payload.allowed_models.unwrap_or_default());
    let model_aliases = payload.model_aliases.unwrap_or_default();
    let allowed_providers = payload.allowed_providers.unwrap_or_default();
    validate_key_model_config(
        &state.config_service.snapshot().raw_config,
        &allowed_models,
        &model_aliases,
        &allowed_providers,
    )?;

    let key_value = generate_key();
    let id = format!("key-{}", uuid::Uuid::new_v4());
    let key = AuthKey {
        id: id.clone(),
        name: payload
            .name
            .clone()
            .filter(|value| !value.trim().is_empty()),
        key: key_value,
        models: allowed_models,
        model_aliases,
        allowed_providers,
        status: "enabled".to_string(),
        labels: payload
            .labels
            .clone()
            .filter(|value| !value.trim().is_empty()),
    };

    let snapshot = state
        .config_service
        .update(|config| {
            config.keys.push(key);
            Ok(())
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let view = snapshot
        .auth_keys()
        .into_iter()
        .find(|key| key.id == id)
        .ok_or_else(|| AppError::Internal("Created key missing from snapshot".into()))?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateKeyResponse { info: view }),
    ))
}

pub async fn update_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<UpdateKeyPayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let snapshot = state.config_service.snapshot();
    let existing_key = snapshot
        .raw_config
        .keys
        .iter()
        .find(|key| key.id == id)
        .ok_or_else(|| AppError::BadRequest(format!("API key '{}' not found", id)))?;
    let allowed_models = match &payload.allowed_models {
        PatchField::Value(models) => key_model_payloads_to_rules(models.clone()),
        PatchField::Null => Vec::new(),
        PatchField::Missing => existing_key.models.clone(),
    };
    let model_aliases = match &payload.model_aliases {
        PatchField::Value(aliases) => aliases.clone(),
        PatchField::Null => HashMap::new(),
        PatchField::Missing => existing_key.model_aliases.clone(),
    };
    let allowed_providers = match &payload.allowed_providers {
        PatchField::Value(providers) => providers.clone(),
        PatchField::Null => Vec::new(),
        PatchField::Missing => existing_key.allowed_providers.clone(),
    };
    validate_key_model_config(
        &snapshot.raw_config,
        &allowed_models,
        &model_aliases,
        &allowed_providers,
    )?;

    state
        .config_service
        .update(|config| {
            let key = config
                .keys
                .iter_mut()
                .find(|key| key.id == id)
                .ok_or_else(|| anyhow::anyhow!("API key '{}' not found", id))?;
            match &payload.name {
                PatchField::Value(name) => {
                    key.name = Some(name.clone()).filter(|value| !value.trim().is_empty());
                }
                PatchField::Null => key.name = None,
                PatchField::Missing => {}
            }
            key.models = allowed_models;
            key.model_aliases = model_aliases;
            key.allowed_providers = allowed_providers;
            match &payload.labels {
                PatchField::Value(labels) => {
                    key.labels = Some(labels.clone()).filter(|value| !value.trim().is_empty());
                }
                PatchField::Null => key.labels = None,
                PatchField::Missing => {}
            }
            Ok(())
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn delete_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    state
        .config_service
        .update(|config| update_key_status(config, &id, "disabled").map_err(anyhow::Error::from))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn update_key_status_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<UpdateStatusPayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    state
        .config_service
        .update(|config| {
            update_key_status(config, &id, &payload.status).map_err(anyhow::Error::from)
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn update_key_model_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, model)): Path<(String, String)>,
    Json(payload): Json<UpdateModelStatusPayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    state
        .config_service
        .update(|config| {
            let key = config
                .keys
                .iter_mut()
                .find(|key| key.id == id)
                .ok_or_else(|| anyhow::anyhow!("API key '{}' not found", id))?;
            let rule = key
                .models
                .iter_mut()
                .find(|rule| rule.name == model)
                .ok_or_else(|| anyhow::anyhow!("Model '{}' not found on key '{}'", model, id))?;
            rule.status = payload.status.clone();
            Ok(())
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

// === Analytics ===

#[derive(Debug, Deserialize)]
pub struct TimeRangeQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

impl TimeRangeQuery {
    fn resolve_range(&self) -> (String, String) {
        let from = self
            .from
            .clone()
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
        let to = self
            .to
            .clone()
            .unwrap_or_else(|| "2099-12-31T23:59:59Z".to_string());
        (from, to)
    }
}

#[derive(Debug, Deserialize)]
pub struct DashboardAnalyticsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default)]
    pub bucket: DashboardAnalyticsBucket,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DashboardAnalyticsBucket {
    #[default]
    Hour,
    Day,
}

impl DashboardAnalyticsQuery {
    fn resolve_range(&self) -> (String, String) {
        TimeRangeQuery {
            from: self.from.clone(),
            to: self.to.clone(),
        }
        .resolve_range()
    }

    fn time_bucket(&self) -> crate::store::AnalyticsTimeBucket {
        match self.bucket {
            DashboardAnalyticsBucket::Hour => crate::store::AnalyticsTimeBucket::Hour,
            DashboardAnalyticsBucket::Day => crate::store::AnalyticsTimeBucket::Day,
        }
    }
}

pub async fn get_analytics_by_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<TimeRangeQuery>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let (from, to) = query.resolve_range();
    let data = state.store.aggregate_by_model(&from, &to).await?;
    Ok(Json(data))
}

pub async fn get_analytics_by_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<TimeRangeQuery>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let (from, to) = query.resolve_range();
    let data = state.store.aggregate_by_provider(&from, &to).await?;
    Ok(Json(data))
}

pub async fn get_analytics_by_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<TimeRangeQuery>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let (from, to) = query.resolve_range();
    let data = state.store.aggregate_by_key(&from, &to).await?;
    Ok(Json(data))
}

pub async fn get_analytics_by_hour(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<TimeRangeQuery>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let (from, to) = query.resolve_range();
    let data = state.store.aggregate_by_hour(&from, &to).await?;
    Ok(Json(data))
}

pub async fn get_analytics_by_day(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<TimeRangeQuery>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let (from, to) = query.resolve_range();
    let data = state.store.aggregate_by_day(&from, &to).await?;
    Ok(Json(data))
}

pub async fn get_analytics_totals(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<TimeRangeQuery>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let (from, to) = query.resolve_range();
    let data = state.store.total_stats(&from, &to).await?;
    Ok(Json(data))
}

pub async fn get_dashboard_analytics(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<DashboardAnalyticsQuery>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let (from, to) = query.resolve_range();
    let data = state
        .store
        .dashboard_analytics(&from, &to, query.time_bucket())
        .await?;
    Ok(Json(data))
}

// === Provider Management ===

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProviderPayload {
    pub name: String,
    pub api_key: String,
    pub models: Vec<ModelRule>,
    pub endpoints: Vec<EndpointConfig>,
    pub headers: Option<HashMap<String, String>>,
    pub status: Option<String>,
    pub priority: Option<i64>,
}

pub async fn list_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    Ok(Json(state.config_service.snapshot().providers()))
}

pub async fn create_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateProviderPayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let provider_name = payload.name.clone();
    let api_key = payload.api_key.clone();
    let models = payload.models.clone();
    let endpoints = payload.endpoints.clone();
    let headers_map = payload.headers.clone().unwrap_or_default();
    let status = payload.status.clone();
    let snapshot = state
        .config_service
        .update(|config| {
            let existing_provider = config
                .providers
                .iter()
                .find(|provider| provider.name == provider_name)
                .cloned();
            let existing_status = existing_provider
                .as_ref()
                .map(|provider| provider.status.clone());
            let priority = payload.priority.unwrap_or_else(|| {
                existing_provider
                    .as_ref()
                    .map(|provider| provider.priority)
                    .unwrap_or(0)
            });
            let provider = ProviderConfig {
                name: provider_name.clone(),
                api_key: api_key.clone(),
                models: models.clone(),
                endpoints: endpoints.clone(),
                headers: headers_map.clone(),
                status: status
                    .clone()
                    .or(existing_status)
                    .unwrap_or_else(|| "enabled".to_string()),
                priority,
            };
            if let Some(existing) = config
                .providers
                .iter_mut()
                .find(|p| p.name == provider_name)
            {
                *existing = provider;
            } else {
                config.providers.push(provider);
            }
            Ok(())
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let view = snapshot
        .providers()
        .into_iter()
        .find(|provider| provider.name == provider_name)
        .ok_or_else(|| AppError::Internal("Saved provider missing from snapshot".into()))?;

    Ok((axum::http::StatusCode::CREATED, Json(view)))
}

pub async fn update_provider_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(payload): Json<UpdateStatusPayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    state
        .config_service
        .update(|config| {
            let provider = config
                .providers
                .iter_mut()
                .find(|provider| provider.name == name)
                .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", name))?;
            provider.status = payload.status.clone();
            Ok(())
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn update_provider_model_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, model)): Path<(String, String)>,
    Json(payload): Json<UpdateModelStatusPayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    state
        .config_service
        .update(|config| {
            let provider = config
                .providers
                .iter_mut()
                .find(|provider| provider.name == name)
                .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", name))?;
            let rule = provider
                .models
                .iter_mut()
                .find(|rule| rule.name == model)
                .ok_or_else(|| {
                    anyhow::anyhow!("Model '{}' not found on provider '{}'", model, name)
                })?;
            rule.status = payload.status.clone();
            Ok(())
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    state
        .config_service
        .update(|config| {
            let before = config.providers.len();
            config.providers.retain(|provider| provider.name != name);
            if before == config.providers.len() {
                anyhow::bail!("Provider '{}' not found", name);
            }
            Ok(())
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

// === Model Aliases ===

#[derive(Debug, Deserialize)]
pub struct CreateAliasPayload {
    pub alias: String,
    pub target_model: String,
    pub provider_name: Option<String>,
}

pub async fn list_aliases(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    Ok(Json(state.config_service.snapshot().aliases()))
}

pub async fn list_model_catalog(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    Ok(Json(state.config_service.snapshot().model_catalog()))
}

#[derive(Debug, Serialize)]
pub struct ConfigFileView {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigFilePayload {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateConfigFileResponse {
    pub path: String,
    pub status: String,
}

pub async fn get_config_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let content = state
        .config_service
        .read_raw_yaml()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(ConfigFileView {
        path: state.config_service.path_display(),
        content,
    }))
}

pub async fn update_config_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<UpdateConfigFilePayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    state
        .config_service
        .replace_raw_yaml(&payload.content)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(UpdateConfigFileResponse {
        path: state.config_service.path_display(),
        status: "ok".to_string(),
    }))
}

pub async fn create_alias(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateAliasPayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    ensure_model_name_is_canonical(&payload.alias, "model alias")?;
    ensure_model_name_is_canonical(&payload.target_model, "model alias target")?;
    if let Some(provider_name) = &payload.provider_name {
        ensure_model_name_is_canonical(provider_name, "provider name")?;
    }
    let alias_name = payload.alias.clone();
    let target_model = payload.target_model.clone();
    let provider_name = payload.provider_name.clone();

    state
        .config_service
        .update(|config| {
            if let Some(provider_name) = &provider_name {
                let provider = config
                    .providers
                    .iter_mut()
                    .find(|provider| provider.name == *provider_name)
                    .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", provider_name))?;
                let mut found_target = false;
                for model in &mut provider.models {
                    model.aliases.retain(|alias| alias != &alias_name);
                    if model.name == target_model {
                        found_target = true;
                        model.aliases.push(alias_name.clone());
                    }
                }
                if !found_target {
                    anyhow::bail!(
                        "Model '{}' not found on provider '{}'",
                        target_model,
                        provider_name
                    );
                }
            } else {
                let mut matches = Vec::new();
                for (provider_index, provider) in config.providers.iter().enumerate() {
                    for (model_index, model) in provider.models.iter().enumerate() {
                        if model.name == target_model {
                            matches.push((provider_index, model_index));
                        }
                    }
                }
                match matches.as_slice() {
                    [(provider_index, model_index)] => {
                        let provider = &mut config.providers[*provider_index];
                        for model in &mut provider.models {
                            model.aliases.retain(|alias| alias != &alias_name);
                        }
                        provider.models[*model_index]
                            .aliases
                            .push(alias_name.clone());
                    }
                    [] => anyhow::bail!("Model '{}' not found", target_model),
                    _ => anyhow::bail!(
                        "Model '{}' exists on multiple providers; provider_name is required",
                        target_model
                    ),
                }
            }
            Ok(())
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({ "status": "ok" })),
    ))
}

pub async fn delete_alias(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(alias): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    state
        .config_service
        .update(|config| {
            let mut removed = false;
            for provider in &mut config.providers {
                for model in &mut provider.models {
                    let before = model.aliases.len();
                    model.aliases.retain(|item| item != &alias);
                    removed |= before != model.aliases.len();
                }
            }
            if !removed {
                anyhow::bail!("Alias '{}' not found", alias);
            }
            Ok(())
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

// === Pricing Rules ===

#[derive(Debug, Deserialize)]
pub struct CreatePricingRulePayload {
    pub scope_type: String,
    pub scope_name: String,
    pub model: String,
    pub input_per_1k: f64,
    pub output_per_1k: f64,
    pub currency: String,
}

#[derive(Debug, Serialize)]
pub struct PricingRuleView {
    pub id: String,
    pub scope_type: String,
    pub scope_name: String,
    pub model: String,
    pub input_per_1k: f64,
    pub output_per_1k: f64,
    pub currency: String,
}

pub async fn list_pricing_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let snapshot = state.config_service.snapshot();
    let currency = snapshot.config.cost.currency.clone();
    let mut rules = Vec::new();

    for provider in &snapshot.raw_config.providers {
        for model in &provider.models {
            if let Some(pricing) = &model.pricing {
                rules.push(PricingRuleView {
                    id: format!("provider:{}:{}", provider.name, model.name),
                    scope_type: "provider".to_string(),
                    scope_name: provider.name.clone(),
                    model: model.name.clone(),
                    input_per_1k: pricing.input_per_1k,
                    output_per_1k: pricing.output_per_1k,
                    currency: currency.clone(),
                });
            }
        }
    }

    Ok(Json(rules))
}

pub async fn create_pricing_rule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreatePricingRulePayload>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let pricing = ModelPricing {
        input_per_1k: payload.input_per_1k,
        output_per_1k: payload.output_per_1k,
    };
    state
        .config_service
        .update(|config| {
            if payload.scope_type == "provider" {
                let provider = config
                    .providers
                    .iter_mut()
                    .find(|provider| provider.name == payload.scope_name)
                    .ok_or_else(|| {
                        anyhow::anyhow!("Provider '{}' not found", payload.scope_name)
                    })?;
                let rule = provider
                    .models
                    .iter_mut()
                    .find(|rule| rule.name == payload.model)
                    .ok_or_else(|| anyhow::anyhow!("Model '{}' not found", payload.model))?;
                rule.pricing = Some(pricing);
                return Ok(());
            }
            anyhow::bail!("Unsupported pricing scope '{}'", payload.scope_type)
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({ "status": "ok" })),
    ))
}

pub async fn delete_pricing_rule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    state
        .config_service
        .update(|config| {
            let mut parts = id.splitn(3, ':');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("provider"), Some(provider_name), Some(model_name)) => {
                    let provider = config
                        .providers
                        .iter_mut()
                        .find(|provider| provider.name == provider_name)
                        .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", provider_name))?;
                    let rule = provider
                        .models
                        .iter_mut()
                        .find(|rule| rule.name == model_name)
                        .ok_or_else(|| anyhow::anyhow!("Model '{}' not found", model_name))?;
                    rule.pricing = None;
                    Ok(())
                }
                _ => anyhow::bail!("Invalid pricing rule id '{}'", id),
            }
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

// === Request Logs ===

#[derive(Debug, Deserialize)]
pub struct QueryLogsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub api_key_id: Option<String>,
    pub model: Option<String>,
    pub provider_name: Option<String>,
    pub protocol: Option<String>,
    pub status: Option<String>,
    pub session_hash: Option<String>,
    pub success: Option<i64>,
    pub status_code: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RequestLogsPage {
    pub items: Vec<RequestLogView>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct RequestLogView {
    #[serde(flatten)]
    pub log: crate::store::models::DbRequestLog,
    pub key_display_name: String,
    pub retry_count: u32,
}

fn key_display_name(snapshot: &crate::config_service::ConfigSnapshot, api_key_id: &str) -> String {
    snapshot
        .raw_config
        .keys
        .iter()
        .find(|key| key.id == api_key_id)
        .map(|key| {
            key.name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(&key.id)
                .to_string()
        })
        .unwrap_or_else(|| api_key_id.to_string())
}

fn request_log_view(
    snapshot: &crate::config_service::ConfigSnapshot,
    log: crate::store::models::DbRequestLog,
) -> RequestLogView {
    let key_display_name = key_display_name(snapshot, &log.api_key_id);
    let retry_count = u32::try_from(log.retry_count).unwrap_or(0);
    RequestLogView {
        log,
        key_display_name,
        retry_count,
    }
}

pub async fn list_request_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<QueryLogsQuery>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);
    let filter = crate::store::models::RequestLogFilter {
        from: query.from,
        to: query.to,
        api_key_id: query.api_key_id,
        session_hash: query.session_hash,
        model: query.model,
        provider_name: query.provider_name,
        protocol: query.protocol,
        status: query.status,
        status_code: query.status_code,
        success: query.success,
        limit: Some(limit),
        offset: Some(offset),
    };
    let logs = state.store.query_request_logs(&filter).await?;
    let total = state.store.count_request_logs(&filter).await?;
    let snapshot = state.config_service.snapshot();
    let items = logs
        .into_iter()
        .map(|log| request_log_view(&snapshot, log))
        .collect();
    Ok(Json(RequestLogsPage {
        items,
        total,
        limit,
        offset,
    }))
}

#[derive(Debug, Serialize)]
pub struct LogDetailResponse {
    #[serde(flatten)]
    pub log: RequestLogView,
    pub request_body: Option<serde_json::Value>,
    pub response_body: Option<serde_json::Value>,
}

fn body_bytes_to_json(bytes: Option<&[u8]>) -> Option<serde_json::Value> {
    let b = bytes?;
    serde_json::from_slice(b).ok()
}

pub async fn get_request_log_detail(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    check_admin(&state, &headers)?;
    let log = state
        .store
        .get_request_log_detail(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Log entry '{}' not found", id)))?;

    let request_body = body_bytes_to_json(log.request_body.as_deref());
    let response_body = body_bytes_to_json(log.response_body.as_deref());
    let mut log_without_body = log;
    log_without_body.request_body = None;
    log_without_body.response_body = None;
    let snapshot = state.config_service.snapshot();

    Ok(Json(LogDetailResponse {
        log: request_log_view(&snapshot, log_without_body),
        request_body,
        response_body,
    }))
}

pub async fn dashboard() -> impl IntoResponse {
    match tokio::fs::read_to_string("frontend/dist/index.html").await {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(_) => (
            axum::http::StatusCode::NOT_FOUND,
            "Index page not found. Please build the frontend first.",
        )
            .into_response(),
    }
}

pub async fn static_handler(Path(path): Path<String>) -> impl IntoResponse {
    let file_path = std::path::PathBuf::from("frontend/dist/assets").join(&path);
    match tokio::fs::read(&file_path).await {
        Ok(content) => {
            let mime = if path.ends_with(".js") {
                "application/javascript"
            } else if path.ends_with(".css") {
                "text/css"
            } else if path.ends_with(".svg") {
                "image/svg+xml"
            } else {
                "application/octet-stream"
            };

            (
                [
                    (axum::http::header::CONTENT_TYPE, mime),
                    (
                        axum::http::header::CACHE_CONTROL,
                        "public, max-age=31536000",
                    ),
                ],
                content,
            )
                .into_response()
        }
        Err(_) => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}
