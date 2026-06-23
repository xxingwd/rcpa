use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rcpa::config::{
    AdminConfig, AppConfig, AuthConfig, AuthKey, CostConfig, ModelRule,
    ProviderConfig, ProviderProtocol, RetryConfig, RoutingConfig, ServerConfig, StickyConfig,
    TlsConfig,
};
use rcpa::config_service::ConfigService;
use rcpa::store::NewRequestLog;

async fn spawn_openai_mock_provider(
    status: axum::http::StatusCode,
    response: serde_json::Value,
) -> String {
    use axum::{routing::post, Json, Router};
    use tokio::net::TcpListener;

    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post({
                let response = response.clone();
                move |Json(body): Json<serde_json::Value>| {
                    let response = response.clone();
                    async move {
                        let model = body
                            .get("model")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        (
                            status,
                            Json(serde_json::json!({
                                "id": "chatcmpl-test",
                                "object": "chat.completion",
                                "model": model,
                                "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
                                "usage": response.get("usage").cloned().unwrap_or_else(|| serde_json::json!({
                                    "prompt_tokens": 5,
                                    "completion_tokens": 7,
                                    "total_tokens": 12,
                                    "prompt_tokens_details": {
                                        "cached_tokens": 2,
                                        "cache_write_tokens": 1
                                    }
                                })),
                                "echo": body,
                                "error": response.get("error").cloned()
                            })),
                        )
                    }
                }
            }),
        )
        .route(
            "/v1/responses",
            post({
                let response = response.clone();
                move |Json(body): Json<serde_json::Value>| {
                    let response = response.clone();
                    async move {
                        let model = body
                            .get("model")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        (
                            status,
                            Json(serde_json::json!({
                                "id": "resp-test",
                                "object": "response",
                                "model": model,
                                "output": [{"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "ok"}]}],
                                "usage": response.get("usage").cloned().unwrap_or_else(|| serde_json::json!({
                                    "input_tokens": 5,
                                    "output_tokens": 7,
                                    "total_tokens": 12
                                })),
                                "echo": body,
                                "error": response.get("error").cloned()
                            })),
                        )
                    }
                }
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

async fn spawn_retrying_openai_mock_provider() -> String {
    use axum::{routing::post, Json, Router};
    use tokio::net::TcpListener;

    let attempts = Arc::new(AtomicUsize::new(0));
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<serde_json::Value>| {
            let attempts = attempts.clone();
            async move {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                let model = body
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                if attempt == 0 {
                    (
                        axum::http::StatusCode::TOO_MANY_REQUESTS,
                        Json(serde_json::json!({
                            "error": {
                                "code": "rate_limit_exceeded",
                                "message": "retry me"
                            }
                        })),
                    )
                } else {
                    (
                        axum::http::StatusCode::OK,
                        Json(serde_json::json!({
                            "id": "chatcmpl-retry-test",
                            "object": "chat.completion",
                            "model": model,
                            "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
                            "usage": {
                                "prompt_tokens": 3,
                                "completion_tokens": 4,
                                "total_tokens": 7
                            }
                        })),
                    )
                }
            }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

async fn spawn_always_rate_limited_openai_mock_provider() -> String {
    use axum::{routing::post, Json, Router};
    use tokio::net::TcpListener;

    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || async move {
            (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": {
                        "code": "rate_limit_exceeded",
                        "message": "still limited"
                    }
                })),
            )
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

async fn spawn_tracking_openai_mock_provider(
    status: axum::http::StatusCode,
    attempts: Arc<AtomicUsize>,
) -> String {
    use axum::{routing::post, Json, Router};
    use tokio::net::TcpListener;

    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<serde_json::Value>| {
            let attempts = attempts.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                let model = body
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                if status == axum::http::StatusCode::OK {
                    (
                        status,
                        Json(serde_json::json!({
                            "id": "chatcmpl-track",
                            "object": "chat.completion",
                            "model": model,
                            "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
                            "usage": {
                                "prompt_tokens": 2,
                                "completion_tokens": 3,
                                "total_tokens": 5
                            }
                        })),
                    )
                } else {
                    (
                        status,
                        Json(serde_json::json!({
                            "error": {
                                "code": "rate_limit_exceeded",
                                "message": "tracked failure"
                            }
                        })),
                    )
                }
            }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

fn enabled_model(name: &str) -> ModelRule {
    ModelRule::enabled(name)
}

fn provider(name: &str, protocol: &str, base_url: &str, models: Vec<ModelRule>) -> ProviderConfig {
    provider_with_protocols(name, vec![provider_protocol(protocol)], base_url, models)
}

fn provider_with_protocols(
    name: &str,
    protocols: Vec<ProviderProtocol>,
    base_url: &str,
    models: Vec<ModelRule>,
) -> ProviderConfig {
    ProviderConfig {
        name: name.to_string(),
        protocols,
        base_url: base_url.to_string(),
        api_key: "sk-mock-key".to_string(),
        models,
        weight: 10,
        max_connections: 50,
        timeout_secs: 60,
        headers: HashMap::new(),
        api_version: None,
        status: "enabled".to_string(),
        priority: 5,
        group: "default".to_string(),
    }
}

fn provider_protocol(value: &str) -> ProviderProtocol {
    match value {
        "completions" => ProviderProtocol::Completions,
        "responses" => ProviderProtocol::Responses,
        "messages" => ProviderProtocol::Messages,
        other => panic!("unknown provider protocol {other}"),
    }
}

fn auth_key(id: &str, key: &str, models: Vec<ModelRule>) -> AuthKey {
    AuthKey {
        id: id.to_string(),
        name: None,
        key: key.to_string(),
        models,
        model_aliases: HashMap::new(),
        status: "enabled".to_string(),
        labels: None,
    }
}

fn test_config() -> AppConfig {
    AppConfig {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 9090,
            tls: TlsConfig::default(),
        },
        providers: vec![provider(
            "openai-test-provider",
            "completions",
            "https://api.openai.com",
            vec![enabled_model("gpt-4o")],
        )],
        routing: RoutingConfig {
            strategy: "round_robin".into(),
            sticky: StickyConfig::default(),
            default_model: Some("gpt-4o".into()),
        },
        retry: RetryConfig {
            max_attempts: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 10000,
            retryable_statuses: vec![429, 502, 503],
        },
        cost: CostConfig {
            currency: "CNY".into(),
            default_input_per_1k: 0.0,
            default_output_per_1k: 0.0,
            models: HashMap::new(),
        },
        admin: AdminConfig {
            token: "admin-token".into(),
        },
        auth: AuthConfig {
            enabled: false,
            keys: vec![],
        },
        database: rcpa::config::DatabaseConfig {
            path: ":memory:".to_string(),
        },
    }
}

async fn state_from_config(config: AppConfig) -> Arc<rcpa::server::AppState> {
    let path = std::env::temp_dir().join(format!("rcpa-test-{}.yaml", uuid::Uuid::new_v4()));
    std::fs::write(&path, serde_yaml::to_string(&config).unwrap()).unwrap();
    let config_service = Arc::new(ConfigService::new(&path).unwrap());
    Arc::new(rcpa::server::AppState::new(config_service).await.unwrap())
}

#[tokio::test]
async fn test_empty_providers_config_validation() {
    let mut config = test_config();
    config.providers.clear();
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn test_build_router() {
    let state = state_from_config(test_config()).await;
    let _router = rcpa::server::router::build(state);
}

#[tokio::test]
async fn test_config_service_updates_provider_snapshot() {
    let state = state_from_config(test_config()).await;
    assert_eq!(state.config_service.snapshot().provider_count(), 1);
    assert!(state
        .config_service
        .snapshot()
        .registry
        .get("openai-test-provider")
        .is_some());

    state
        .config_service
        .update(|config| {
            config.providers.push(provider(
                "anthropic-test-provider",
                "messages",
                "https://api.anthropic.com",
                vec![enabled_model("claude-sonnet")],
            ));
            Ok(())
        })
        .unwrap();

    let snapshot = state.config_service.snapshot();
    assert_eq!(snapshot.provider_count(), 2);
    assert!(snapshot.registry.get("anthropic-test-provider").is_some());
    assert!(
        snapshot.provider_supports_protocol("anthropic-test-provider", ProviderProtocol::Messages)
    );
}

#[tokio::test]
async fn test_multi_protocol_provider_routes_responses_requests() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_openai_mock_provider(StatusCode::OK, serde_json::json!({})).await;
    let mut config = test_config();
    config.providers = vec![provider_with_protocols(
        "openai-multi",
        vec![ProviderProtocol::Completions, ProviderProtocol::Responses],
        &mock_base_url,
        vec![enabled_model("gpt-4o")],
    )];
    config.auth.enabled = true;
    config.auth.keys.push(auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("gpt-4o")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"model":"gpt-4o","input":"hello"}"#))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(response["model"], "gpt-4o");

    let logs = state
        .store
        .query_request_logs(&rcpa::store::models::RequestLogFilter::default())
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "openai-multi");
    assert_eq!(logs[0].protocol, "responses");
    assert_eq!(logs[0].operation, "responses");
}

#[test]
fn test_config_service_rejects_database_path_hot_reload() {
    let temp_dir = std::env::temp_dir().join(format!("rcpa-config-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join("config.yaml");

    let mut config = test_config();
    config.database.path = temp_dir.join("initial.db").to_string_lossy().into_owned();
    std::fs::write(&config_path, serde_yaml::to_string(&config).unwrap()).unwrap();

    let service = ConfigService::new(&config_path).unwrap();
    let mut next_config = config.clone();
    next_config.database.path = temp_dir.join("next.db").to_string_lossy().into_owned();

    let err = match service.replace_raw_yaml(&serde_yaml::to_string(&next_config).unwrap()) {
        Ok(_) => panic!("database.path hot reload should be rejected"),
        Err(err) => err.to_string(),
    };

    assert!(err.contains("database.path cannot be changed"));
    assert_eq!(
        service.snapshot().config.database.path,
        config.database.path
    );

    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[tokio::test]
async fn test_admin_api_endpoints() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let config = test_config();
    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/keys")
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"test-key","model_aliases":{"fast":"gpt-4o"}}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let key_res: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let key_id = key_res["id"].as_str().unwrap();
    assert_eq!(key_res["model_aliases"]["fast"], "gpt-4o");
    let generated_key = key_res["key"].as_str().unwrap().to_string();

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/admin/keys/{}", key_id))
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"edited-key","allowed_models":[{"name":"gpt-4o","status":"enabled"}],"labels":"edited-key"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/keys")
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let keys: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(keys[0]["key"], generated_key);
    assert_eq!(keys[0]["name"], "edited-key");
    assert_eq!(keys[0]["allowed_models"][0]["name"], "gpt-4o");
    assert_eq!(keys[0]["model_aliases"]["fast"], "gpt-4o");
    assert_eq!(keys[0]["labels"], "edited-key");

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/admin/keys/{}", key_id))
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"labels":"patched-label"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/keys")
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let keys: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(keys[0]["name"], "edited-key");
    assert_eq!(keys[0]["allowed_models"][0]["name"], "gpt-4o");
    assert_eq!(keys[0]["model_aliases"]["fast"], "gpt-4o");
    assert_eq!(keys[0]["labels"], "patched-label");

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/admin/keys/{}", key_id))
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"allowed_models":null,"model_aliases":null}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/keys")
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let keys: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(keys[0]["allowed_models"].as_array().unwrap().is_empty());
    assert!(keys[0]["model_aliases"].as_object().unwrap().is_empty());

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/admin/keys/{}/status", key_id))
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"status":"disabled"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/aliases")
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"alias":"custom-alias","target_model":"gpt-4o","provider_name":"openai-test-provider"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/providers")
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/pricing")
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"scope_type":"provider","scope_name":"openai-test-provider","model":"gpt-4o","input_per_1k":0.001,"output_per_1k":0.002,"currency":"USD"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/pricing")
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let pricing_rules: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let rule_id = pricing_rules[0]["id"].as_str().unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/admin/pricing/{}", rule_id))
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/logs?limit=10")
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let logs_page: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(logs_page["items"].as_array().unwrap().is_empty());
    assert_eq!(logs_page["total"], 0);

    let req = Request::builder()
        .method("GET")
        .uri("/stats")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let stats: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(stats["requests"]["total"], 0);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/logs/non-existent-id")
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    state
        .store
        .insert_request_log_entry(NewRequestLog {
            request_id: "req-admin-filter-1",
            api_key_id: "key-alpha",
            session_hash: None,
            provider_name: "openai-1",
            protocol: "completions",
            model: "gpt-4o",
            operation: "chat_completions",
            status_code: 200,
            success: true,
            input_tokens: 12,
            output_tokens: 6,
            total_tokens: 18,
            cached_tokens: 0,
            cache_write_tokens: 0,
            cost_cents: 3,
            latency_ms: 120,
            first_byte_latency_ms: 120,
            metadata_json: "{}",
            request_body: Some(br#"{"model":"gpt-4o"}"#),
            response_body: None,
        })
        .await
        .unwrap();
    state
        .store
        .insert_request_log_entry(NewRequestLog {
            request_id: "req-admin-filter-2",
            api_key_id: "key-beta",
            session_hash: None,
            provider_name: "openai-1",
            protocol: "completions",
            model: "gpt-4o-mini",
            operation: "chat_completions",
            status_code: 200,
            success: true,
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
            cached_tokens: 0,
            cache_write_tokens: 0,
            cost_cents: 2,
            latency_ms: 90,
            first_byte_latency_ms: 90,
            metadata_json: "{}",
            request_body: Some(br#"{"model":"gpt-4o-mini"}"#),
            response_body: None,
        })
        .await
        .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/logs?limit=10&api_key_id=key-alpha")
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let filtered_logs: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let items = filtered_logs["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["api_key_id"], "key-alpha");
    assert_eq!(filtered_logs["total"], 1);
}

#[tokio::test]
async fn test_llm_request_errors_are_persisted_in_stats() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mut config = test_config();
    config.auth.enabled = true;
    config
        .auth
        .keys
        .push(auth_key("user-key", "user-secret-key", vec![]));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"model":"missing-model","messages":[]}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let req = Request::builder()
        .method("GET")
        .uri("/stats")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let stats: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(stats["requests"]["total"], 1);
    assert_eq!(stats["requests"]["success"], 0);
    assert_eq!(stats["requests"]["errors"], 1);

    let logs = state
        .store
        .query_request_logs(&rcpa::store::models::RequestLogFilter::default())
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].operation, "chat_completions");
    assert_eq!(logs[0].protocol, "completions");
    assert_eq!(logs[0].api_key_id, "user-key");
    assert_eq!(logs[0].provider_name, "unrouted");
    assert_eq!(logs[0].model, "missing-model");
    assert_eq!(logs[0].success, 0);
    assert_eq!(logs[0].error_code.as_deref(), Some("model_not_found"));
    let detail = state
        .store
        .get_request_log_detail(&logs[0].id)
        .await
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&detail.metadata_json).unwrap();
    assert_eq!(metadata["error"]["code"], "model_not_found");
}

#[tokio::test]
async fn test_admin_key_model_catalog_validation_and_log_key_display_name() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mut config = test_config();
    config.providers[0]
        .models
        .push(enabled_model("plain-model"));
    config.providers[0].models[0]
        .aliases
        .push("global-fast".into());

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/model-catalog")
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let catalog: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let catalog_items = catalog.as_array().unwrap();
    assert!(catalog_items
        .iter()
        .any(|item| item["kind"] == "model" && item["name"] == "plain-model"));
    assert!(catalog_items
        .iter()
        .any(|item| { item["kind"] == "model" && item["name"] == "global-fast" }));
    assert!(!catalog_items.iter().any(|item| item["name"] == "gpt-4o"));
    assert!(catalog_items
        .iter()
        .all(|item| item.get("target_model").is_none()
            && item.get("model_name").is_none()
            && item.get("aliases").is_none()
            && item.get("selectable_names").is_none()));

    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/keys")
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"catalog-key","allowed_models":[{"name":"plain-model","status":"enabled"},{"name":"global-fast","status":"disabled"},{"name":"quick","status":"enabled"}],"model_aliases":{"quick":"global-fast"}}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let named_key: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let named_key_id = named_key["id"].as_str().unwrap().to_string();
    assert_eq!(named_key["allowed_models"][1]["status"], "disabled");

    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/keys")
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"allowed_models":[{"name":"gpt-4o","status":"enabled"}]}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/keys")
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"model_aliases":{"bad":"missing-model"}}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/keys")
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let unnamed_key: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let unnamed_key_id = unnamed_key["id"].as_str().unwrap().to_string();
    let unnamed_key_value = unnamed_key["key"].as_str().unwrap().to_string();

    state
        .store
        .insert_request_log_entry(NewRequestLog {
            request_id: "req-display-name",
            api_key_id: &named_key_id,
            session_hash: None,
            provider_name: "openai-test-provider",
            protocol: "completions",
            model: "gpt-4o",
            operation: "chat_completions",
            status_code: 200,
            success: true,
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cached_tokens: 0,
            cache_write_tokens: 0,
            cost_cents: 0,
            latency_ms: 20,
            first_byte_latency_ms: 20,
            metadata_json: "{}",
            request_body: None,
            response_body: None,
        })
        .await
        .unwrap();
    state
        .store
        .insert_request_log_entry(NewRequestLog {
            request_id: "req-display-key",
            api_key_id: &unnamed_key_id,
            session_hash: None,
            provider_name: "openai-test-provider",
            protocol: "completions",
            model: "gpt-4o",
            operation: "chat_completions",
            status_code: 200,
            success: true,
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cached_tokens: 0,
            cache_write_tokens: 0,
            cost_cents: 0,
            latency_ms: 20,
            first_byte_latency_ms: 20,
            metadata_json: "{}",
            request_body: None,
            response_body: None,
        })
        .await
        .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/admin/logs?api_key_id={}", named_key_id))
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let logs: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(logs["items"][0]["key_display_name"], "catalog-key");

    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/admin/logs?api_key_id={}", unnamed_key_id))
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let logs: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(logs["items"][0]["key_display_name"], unnamed_key_id);
    assert_ne!(logs["items"][0]["key_display_name"], unnamed_key_value);
}

#[tokio::test]
async fn test_models_endpoint_lists_platform_global_and_key_alias_names() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mut config = test_config();
    config.providers[0]
        .models
        .push(enabled_model("plain-model"));
    config.auth.enabled = true;
    let mut key = auth_key(
        "user-key",
        "user-secret-key",
        vec![
            enabled_model("global-fast"),
            enabled_model("plain-model"),
            enabled_model("quick"),
        ],
    );
    key.model_aliases
        .insert("quick".to_string(), "global-fast".to_string());
    config.auth.keys.push(key);
    config.providers[0].models[0]
        .aliases
        .push("global-fast".into());

    let state = state_from_config(config).await;
    assert!(state.validate_model_name("gpt-4o").is_err());
    assert_eq!(
        state.validate_model_name("global-fast").unwrap(),
        "global-fast"
    );
    let app = rcpa::server::router::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/models")
        .header("x-api-key", "user-secret-key")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let models: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let mut ids: Vec<String> = models["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();

    assert_eq!(ids, vec!["global-fast", "plain-model", "quick"]);
}

#[tokio::test]
async fn test_key_alias_global_alias_and_log_detail_json_are_persisted() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_openai_mock_provider(
        StatusCode::OK,
        serde_json::json!({
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 7,
                "total_tokens": 12,
                "prompt_tokens_details": {
                    "cached_tokens": 2,
                    "cache_write_tokens": 1
                }
            }
        }),
    )
    .await;

    let mut config = test_config();
    config.providers = vec![provider(
        "openai-test-provider",
        "completions",
        &mock_base_url,
        vec![ModelRule {
            name: "gpt-4o".into(),
            status: "enabled".into(),
            pricing: None,
            aliases: vec!["global-fast".into()],
        }],
    )];
    config.auth.enabled = true;
    let mut key = auth_key("user-key", "user-secret-key", vec![enabled_model("quick")]);
    key.model_aliases
        .insert("quick".to_string(), "global-fast".to_string());
    config.auth.keys.push(key);

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "user-secret-key")
        .header("session-id", "codex-session-a")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"quick","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(response["model"], "quick");

    let logs = state
        .store
        .query_request_logs(&rcpa::store::models::RequestLogFilter::default())
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].api_key_id, "user-key");
    assert_eq!(logs[0].model, "global-fast");
    assert_eq!(logs[0].status_code, 200);
    assert_eq!(logs[0].success, 1);
    assert!(logs[0].session_hash.is_some());
    assert_eq!(logs[0].total_tokens, 12);
    assert_eq!(logs[0].cached_tokens, 2);
    assert_eq!(logs[0].cache_write_tokens, 1);
    assert_eq!(logs[0].error_code, None);
    assert!(logs[0].first_byte_latency_ms <= logs[0].latency_ms);

    let detail = state
        .store
        .get_request_log_detail(&logs[0].id)
        .await
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&detail.metadata_json).unwrap();
    assert_eq!(metadata["session"]["id"], "codex-session-a");
    assert_eq!(metadata["session"]["source"], "session_id_header");
    assert_eq!(metadata["models"]["requested"], "quick");
    assert_eq!(metadata["models"]["resolved"], "global-fast");
    assert_eq!(metadata["models"]["provider"], "gpt-4o");
    assert_eq!(metadata["routing"]["sticky_enabled"], false);
    let request_body: serde_json::Value =
        serde_json::from_slice(&detail.request_body.unwrap()).unwrap();
    assert_eq!(request_body["model"], "gpt-4o");
    let response_body: serde_json::Value =
        serde_json::from_slice(&detail.response_body.unwrap()).unwrap();
    assert_eq!(response_body["model"], "gpt-4o");

    let model_rollup = state
        .store
        .aggregate_by_model("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
        .await
        .unwrap();
    assert_eq!(model_rollup.len(), 1);
    assert_eq!(model_rollup[0].group_key, "global-fast");
}

#[tokio::test]
async fn test_retryable_provider_status_retries_and_logs_final_attempt_metadata() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_retrying_openai_mock_provider().await;

    let mut config = test_config();
    config.providers = vec![provider(
        "openai-test-provider",
        "completions",
        &mock_base_url,
        vec![enabled_model("gpt-4o")],
    )];
    config.retry.initial_backoff_ms = 1;
    config.retry.max_backoff_ms = 1;
    config.auth.enabled = true;
    config.auth.keys.push(auth_key(
        "retry-key",
        "retry-secret-key",
        vec![enabled_model("gpt-4o")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "retry-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let logs = state
        .store
        .query_request_logs(&rcpa::store::models::RequestLogFilter::default())
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].status_code, 200);
    assert_eq!(logs[0].success, 1);

    let detail = state
        .store
        .get_request_log_detail(&logs[0].id)
        .await
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&detail.metadata_json).unwrap();
    assert_eq!(metadata["retry"]["attempt_count"], 2);
    assert_eq!(metadata["retry"]["retry_count"], 1);
    assert!(metadata["retry"]["total_backoff_ms"].as_u64().unwrap() >= 1);
    let attempts = metadata["retry"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["attempt"], 1);
    assert_eq!(attempts[0]["provider_name"], "openai-test-provider");
    assert_eq!(attempts[0]["selected_via"], "healthy");
    assert_eq!(attempts[0]["status_code"], 429);
    assert_eq!(attempts[0]["error_code"], "rate_limit_exceeded");
    assert_eq!(attempts[0]["retryable"], true);
    assert!(attempts[0]["backoff_ms_before_next"].as_u64().unwrap() >= 1);
    assert_eq!(attempts[1]["attempt"], 2);
    assert_eq!(attempts[1]["provider_name"], "openai-test-provider");
    assert_eq!(attempts[1]["status_code"], 200);
    assert_eq!(attempts[1]["retryable"], false);
    assert!(attempts[1]["backoff_ms_before_next"].is_null());
    assert_eq!(metadata["routing"]["selected_provider_reason"], "healthy");
    assert_eq!(metadata["routing"]["sticky_hit"], false);
}

#[tokio::test]
async fn test_retry_exhaustion_preserves_upstream_status_code() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_always_rate_limited_openai_mock_provider().await;

    let mut config = test_config();
    config.providers = vec![provider(
        "openai-test-provider",
        "completions",
        &mock_base_url,
        vec![enabled_model("gpt-4o")],
    )];
    config.retry.initial_backoff_ms = 1;
    config.retry.max_backoff_ms = 1;
    config.auth.enabled = true;
    config.auth.keys.push(auth_key(
        "retry-key",
        "retry-secret-key",
        vec![enabled_model("gpt-4o")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "retry-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);

    let logs = state
        .store
        .query_request_logs(&rcpa::store::models::RequestLogFilter::default())
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].status_code, 429);
    assert_eq!(logs[0].error_code.as_deref(), Some("rate_limit_exceeded"));
    let detail = state
        .store
        .get_request_log_detail(&logs[0].id)
        .await
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&detail.metadata_json).unwrap();
    let attempts = metadata["retry"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 3);
    assert!(attempts
        .iter()
        .take(2)
        .all(|attempt| attempt["retryable"] == serde_json::Value::Bool(true)));
    assert_eq!(attempts[2]["retryable"], false);
    assert_eq!(attempts[2]["status_code"], 429);
}

#[tokio::test]
async fn test_retry_switches_to_different_provider_within_same_request() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let provider_a_attempts = Arc::new(AtomicUsize::new(0));
    let provider_b_attempts = Arc::new(AtomicUsize::new(0));
    let provider_a_url = spawn_tracking_openai_mock_provider(
        StatusCode::TOO_MANY_REQUESTS,
        provider_a_attempts.clone(),
    )
    .await;
    let provider_b_url =
        spawn_tracking_openai_mock_provider(StatusCode::OK, provider_b_attempts.clone()).await;

    let mut config = test_config();
    config.providers = vec![
        provider(
            "provider-a",
            "completions",
            &provider_a_url,
            vec![enabled_model("gpt-4o")],
        ),
        provider(
            "provider-b",
            "completions",
            &provider_b_url,
            vec![enabled_model("gpt-4o")],
        ),
    ];
    config.providers[0].priority = 1;
    config.providers[1].priority = 2;
    config.retry.initial_backoff_ms = 1;
    config.retry.max_backoff_ms = 1;
    config.auth.enabled = true;
    config.auth.keys.push(auth_key(
        "retry-key",
        "retry-secret-key",
        vec![enabled_model("gpt-4o")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "retry-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    assert_eq!(provider_a_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(provider_b_attempts.load(Ordering::SeqCst), 1);

    let logs = state
        .store
        .query_request_logs(&rcpa::store::models::RequestLogFilter::default())
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    let detail = state
        .store
        .get_request_log_detail(&logs[0].id)
        .await
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&detail.metadata_json).unwrap();
    let attempts = metadata["retry"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["provider_name"], "provider-a");
    assert_eq!(attempts[0]["status_code"], 429);
    assert_eq!(attempts[0]["retryable"], true);
    assert_eq!(attempts[1]["provider_name"], "provider-b");
    assert_eq!(attempts[1]["status_code"], 200);
    assert_eq!(attempts[1]["retryable"], false);
    assert_eq!(attempts[1]["selected_via"], "healthy");
}

#[tokio::test]
async fn test_disabled_provider_model_disables_alias() {
    let mut config = test_config();
    config.providers[0].models[0].status = "disabled".into();
    config.auth.enabled = true;
    let mut key = auth_key("user-key", "user-secret-key", vec![enabled_model("quick")]);
    key.model_aliases
        .insert("quick".to_string(), "global-fast".to_string());
    config.auth.keys.push(key);

    let err = match ConfigService::from_config(config) {
        Ok(_) => panic!("config should reject a key alias targeting a disabled platform model"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("targets unknown platform model"));
}
