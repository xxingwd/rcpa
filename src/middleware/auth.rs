use crate::config::AuthKey;
use crate::error::AppError;
use crate::server::AppState;
use sha2::{Digest, Sha256};

/// Result of authentication — the matched key and its info
pub struct AuthResult {
    pub key: AuthKey,
}

/// Helper to hash an API key using SHA-256
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let result = hasher.finalize();
    result
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

fn extract_api_key(headers: &axum::http::HeaderMap) -> Result<String, AppError> {
    let mut api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if api_key.is_empty() {
        let auth_header = headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Some(stripped) = auth_header.strip_prefix("Bearer ") {
            api_key = stripped.to_string();
        } else {
            api_key = auth_header.to_string();
        }
    }

    if api_key.is_empty() {
        return Err(AppError::Unauthorized("Missing API key".into()));
    }

    Ok(api_key)
}

/// Authenticate a request. Returns the matched AuthKey if auth passes.
pub fn authenticate(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<AuthResult, AppError> {
    let snapshot = state.config_service.snapshot();
    let config = &snapshot.config;

    if !config.auth.enabled {
        // Return a default anonymous key when auth is disabled
        return Ok(AuthResult {
            key: AuthKey {
                id: "anonymous".into(),
                name: None,
                key: "anonymous".into(),
                models: vec![],
                model_aliases: std::collections::HashMap::new(),
                status: "enabled".into(),
                labels: None,
            },
        });
    }

    let api_key = extract_api_key(headers)?;
    let auth_key = snapshot
        .auth_key_for_secret(&api_key)
        .ok_or_else(|| AppError::Unauthorized("Invalid API key".into()))?;

    Ok(AuthResult { key: auth_key })
}

pub fn authenticate_llm_api_key(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<AuthResult, AppError> {
    authenticate(state, headers)
}

pub fn persisted_api_key_id(key: &AuthKey) -> &str {
    &key.id
}

/// Check if the authenticated key has access to the requested model
pub fn check_model_access(key: &AuthKey, model: &str) -> Result<(), AppError> {
    if !crate::config::AppConfig::key_can_use_model(key, model) {
        return Err(AppError::Unauthorized(format!(
            "API key does not have access to model '{}'",
            model
        )));
    }
    Ok(())
}

/// Check model access against the user-facing model name on the request.
pub fn check_model_access_for_request(
    key: &AuthKey,
    requested_model: &str,
    _resolved_model: &str,
) -> Result<(), AppError> {
    if crate::config::AppConfig::key_can_use_model(key, requested_model) {
        return Ok(());
    }

    Err(AppError::Unauthorized(format!(
        "API key does not have access to model '{}'",
        requested_model
    )))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn auth_key_with_models(models: Vec<&str>) -> AuthKey {
        AuthKey {
            id: "key-test".to_string(),
            name: None,
            key: "test-key".to_string(),
            models: models
                .into_iter()
                .map(crate::config::ModelRule::enabled)
                .collect(),
            model_aliases: HashMap::new(),
            status: "enabled".to_string(),
            labels: None,
        }
    }

    #[test]
    fn check_model_access_for_request_only_accepts_requested_model() {
        let alias_key = auth_key_with_models(vec!["fast"]);
        assert!(check_model_access_for_request(&alias_key, "fast", "gpt-4o").is_ok());

        let resolved_key = auth_key_with_models(vec!["gpt-4o"]);
        assert!(check_model_access_for_request(&resolved_key, "fast", "gpt-4o").is_err());

        let denied_key = auth_key_with_models(vec!["claude-*"]);
        assert!(check_model_access_for_request(&denied_key, "fast", "gpt-4o").is_err());
    }
}
