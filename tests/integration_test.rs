use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rcpa::config::{
    AppConfig, AuthKey, CostConfig, EndpointConfig, ModelRule, ProviderConfig, ProviderProtocol,
    RetryConfig, RoutingConfig, StickyConfig, UpstreamConfig,
};
use rcpa::config_service::ConfigService;
use rcpa::server::RuntimeConfig;
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
    format!("http://{}/v1/chat/completions", addr)
}

async fn spawn_anthropic_mock_provider(
    status: axum::http::StatusCode,
    response: serde_json::Value,
) -> String {
    use axum::{routing::post, Json, Router};
    use tokio::net::TcpListener;

    let app = Router::new().route(
        "/v1/messages",
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
                            "id": "msg-test",
                            "type": "message",
                            "role": "assistant",
                            "model": model,
                            "content": [{"type": "text", "text": "ok"}],
                            "usage": response.get("usage").cloned().unwrap_or_else(|| serde_json::json!({
                                "input_tokens": 5,
                                "output_tokens": 7
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
    format!("http://{}/v1/messages", addr)
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
    format!("http://{}/v1/chat/completions", addr)
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
    format!("http://{}/v1/chat/completions", addr)
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
    format!("http://{}/v1/chat/completions", addr)
}

async fn spawn_openai_mock_provider_with_label(
    label: &'static str,
    hits: Arc<std::sync::Mutex<Vec<String>>>,
) -> String {
    use axum::{routing::post, Json, Router};
    use tokio::net::TcpListener;

    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post({
                let hits = hits.clone();
                move |Json(body): Json<serde_json::Value>| {
                    let hits = hits.clone();
                    async move {
                        hits.lock().unwrap().push(format!("{label}:completions"));
                        let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("unknown");
                        (
                            axum::http::StatusCode::OK,
                            Json(serde_json::json!({
                                "id": "chatcmpl-test",
                                "object": "chat.completion",
                                "model": model,
                                "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
                                "usage": {
                                    "prompt_tokens": 1,
                                    "completion_tokens": 1,
                                    "total_tokens": 2
                                }
                            })),
                        )
                    }
                }
            }),
        )
        .route(
            "/v1/responses",
            post({
                let hits = hits.clone();
                move |Json(body): Json<serde_json::Value>| {
                    let hits = hits.clone();
                    async move {
                        hits.lock().unwrap().push(format!("{label}:responses"));
                        let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("unknown");
                        (
                            axum::http::StatusCode::OK,
                            Json(serde_json::json!({
                                "id": "resp-test",
                                "object": "response",
                                "model": model,
                                "output": [{"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "ok"}]}],
                                "usage": {
                                    "input_tokens": 1,
                                    "output_tokens": 1,
                                    "total_tokens": 2
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

fn responses_endpoint(base_url: &str) -> String {
    base_url.replace("/v1/chat/completions", "/v1/responses")
}

async fn spawn_streaming_openai_mock_provider() -> String {
    use axum::{response::IntoResponse, routing::post, Json, Router};
    use tokio::net::TcpListener;

    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<serde_json::Value>| async move {
            let model = body
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            (
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                format!(
                    concat!(
                        "data: {{\"id\":\"chatcmpl-stream\",\"object\":\"chat.completion.chunk\",\"model\":\"{model}\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"hello\"}},\"finish_reason\":null}}]}}\n\n",
                        "data: {{\"id\":\"chatcmpl-stream\",\"object\":\"chat.completion.chunk\",\"model\":\"{model}\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":2,\"completion_tokens\":3,\"total_tokens\":5}}}}\n\n",
                        "data: [DONE]\n\n"
                    ),
                    model = model
                ),
            )
                .into_response()
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}/v1/chat/completions", addr)
}

async fn spawn_lingering_streaming_openai_mock_provider() -> String {
    use axum::{body::Body, response::IntoResponse, routing::post, Json, Router};
    use bytes::Bytes;
    use std::convert::Infallible;
    use std::time::Duration;
    use tokio::net::TcpListener;

    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<serde_json::Value>| async move {
            let model = body
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let payload = Bytes::from(format!(
                concat!(
                    "data: {{\"id\":\"chatcmpl-stream\",\"object\":\"chat.completion.chunk\",\"model\":\"{model}\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"hello\"}},\"finish_reason\":null}}]}}\n\n",
                    "data: {{\"id\":\"chatcmpl-stream\",\"object\":\"chat.completion.chunk\",\"model\":\"{model}\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":2,\"completion_tokens\":3,\"total_tokens\":5}}}}\n\n",
                    "data: [DONE]\n\n"
                ),
                model = model
            ));
            let stream = futures::stream::unfold(Some(payload), |state| async move {
                match state {
                    Some(payload) => Some((Ok::<Bytes, Infallible>(payload), None)),
                    None => {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        None
                    }
                }
            });
            (
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                Body::from_stream(stream),
            )
                .into_response()
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}/v1/chat/completions", addr)
}

async fn spawn_fragmented_newline_streaming_openai_mock_provider() -> String {
    use axum::{body::Body, response::IntoResponse, routing::post, Router};
    use bytes::Bytes;
    use std::collections::VecDeque;
    use std::convert::Infallible;
    use std::time::Duration;
    use tokio::net::TcpListener;

    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async move {
            let content_event = "data: {\"id\":\"chatcmpl-fragmented\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"你好😀🎉\"},\"finish_reason\":null}]}\n";
            let emoji_start = content_event
                .as_bytes()
                .windows("😀".len())
                .position(|window| window == "😀".as_bytes())
                .unwrap();
            let split_inside_emoji = emoji_start + 2;
            let chunks = VecDeque::from([
                Bytes::copy_from_slice(&content_event.as_bytes()[..split_inside_emoji]),
                Bytes::copy_from_slice(&content_event.as_bytes()[split_inside_emoji..]),
                Bytes::from_static(
                    b"data: {\"id\":\"chatcmpl-fragmented\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n",
                ),
                Bytes::from_static(b"data: [DONE]\n"),
            ]);
            let stream = futures::stream::unfold(chunks, |mut chunks| async move {
                match chunks.pop_front() {
                    Some(chunk) => Some((Ok::<Bytes, Infallible>(chunk), chunks)),
                    None => {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        None
                    }
                }
            });
            (
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                Body::from_stream(stream),
            )
                .into_response()
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}/v1/chat/completions", addr)
}

async fn spawn_streaming_anthropic_mock_provider() -> String {
    use axum::{response::IntoResponse, routing::post, Json, Router};
    use tokio::net::TcpListener;

    let app = Router::new().route(
        "/v1/messages",
        post(move |Json(body): Json<serde_json::Value>| async move {
            let model = body
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            (
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                format!(
                    concat!(
                        "event: message_start\n",
                        "data: {{\"type\":\"message_start\",\"message\":{{\"id\":\"msg-stream\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"{model}\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{{\"input_tokens\":0,\"output_tokens\":0}}}}}}\n\n",
                        "event: content_block_start\n",
                        "data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n",
                        "event: content_block_delta\n",
                        "data: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"hello\"}}}}\n\n",
                        "event: content_block_stop\n",
                        "data: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n",
                        "event: message_delta\n",
                        "data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"end_turn\",\"stop_sequence\":null}},\"usage\":{{\"input_tokens\":2,\"output_tokens\":3}}}}\n\n",
                        "event: message_stop\n",
                        "data: {{\"type\":\"message_stop\"}}\n\n"
                    ),
                    model = model
                ),
            )
                .into_response()
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}/v1/messages", addr)
}

fn enabled_model(name: &str) -> ModelRule {
    ModelRule::enabled(name)
}

fn provider(name: &str, protocol: &str, base_url: &str, models: Vec<ModelRule>) -> ProviderConfig {
    provider_with_endpoints(
        name,
        vec![EndpointConfig {
            protocol: provider_protocol(protocol),
            base_url: base_url.to_string(),
        }],
        models,
    )
}

fn provider_with_endpoints(
    name: &str,
    endpoints: Vec<EndpointConfig>,
    models: Vec<ModelRule>,
) -> ProviderConfig {
    ProviderConfig {
        name: name.to_string(),
        api_key: "sk-mock-key".to_string(),
        models,
        endpoints,
        headers: HashMap::new(),
        status: "enabled".to_string(),
        priority: 5,
    }
}

fn provider_protocol(value: &str) -> ProviderProtocol {
    match value {
        "completions" => ProviderProtocol::Completions,
        "responses" => ProviderProtocol::Responses,
        "messages" => ProviderProtocol::Messages,
        "embeddings" => ProviderProtocol::Embeddings,
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
        allowed_providers: Vec::new(),
        status: "enabled".to_string(),
        labels: None,
    }
}

fn test_config() -> AppConfig {
    AppConfig {
        providers: vec![provider(
            "openai-test-provider",
            "completions",
            "https://api.openai.com/v1/chat/completions",
            vec![enabled_model("gpt-4o")],
        )],
        upstream: UpstreamConfig { timeout_secs: 60 },
        routing: RoutingConfig {
            sticky: StickyConfig::default(),
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
        keys: vec![],
    }
}

async fn wait_for_request_logs(
    state: &Arc<rcpa::server::AppState>,
    expected: usize,
) -> Vec<rcpa::store::models::DbRequestLog> {
    for _ in 0..20 {
        let logs = state
            .store
            .query_request_logs(&rcpa::store::models::RequestLogFilter::default())
            .await
            .unwrap();
        if logs.len() >= expected {
            return logs;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    state
        .store
        .query_request_logs(&rcpa::store::models::RequestLogFilter::default())
        .await
        .unwrap()
}

async fn state_from_config(config: AppConfig) -> Arc<rcpa::server::AppState> {
    let path = std::env::temp_dir().join(format!("rcpa-test-{}.yaml", uuid::Uuid::new_v4()));
    std::fs::write(&path, serde_yaml::to_string(&config).unwrap()).unwrap();
    let config_service = Arc::new(ConfigService::new(&path).unwrap());
    Arc::new(
        rcpa::server::AppState::new(config_service, RuntimeConfig::in_memory("admin-token"))
            .await
            .unwrap(),
    )
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
        .get("openai-test-provider", ProviderProtocol::Completions)
        .is_some());

    state
        .config_service
        .update(|config| {
            config.providers.push(provider(
                "anthropic-test-provider",
                "messages",
                "https://api.anthropic.com/v1/messages",
                vec![enabled_model("claude-sonnet")],
            ));
            Ok(())
        })
        .unwrap();

    let snapshot = state.config_service.snapshot();
    assert_eq!(snapshot.provider_count(), 2);
    assert!(snapshot
        .registry
        .get("anthropic-test-provider", ProviderProtocol::Messages)
        .is_some());
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
    config.providers = vec![provider_with_endpoints(
        "openai-multi",
        vec![
            EndpointConfig {
                protocol: ProviderProtocol::Completions,
                base_url: mock_base_url.clone(),
            },
            EndpointConfig {
                protocol: ProviderProtocol::Responses,
                base_url: responses_endpoint(&mock_base_url),
            },
        ],
        vec![enabled_model("gpt-4o")],
    )];
    config.keys.push(auth_key(
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

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "openai-multi");
    assert_eq!(logs[0].protocol, "responses");
    assert_eq!(logs[0].operation, "responses");
}

#[tokio::test]
async fn test_multi_endpoint_provider_uses_protocol_specific_base_url() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let hits = Arc::new(std::sync::Mutex::new(Vec::new()));
    let completions_url = spawn_openai_mock_provider_with_label("a", hits.clone()).await;
    let responses_url = spawn_openai_mock_provider_with_label("b", hits.clone()).await;

    let mut config = test_config();
    config.providers = vec![provider_with_endpoints(
        "openai-multi",
        vec![
            EndpointConfig {
                protocol: ProviderProtocol::Completions,
                base_url: format!("{completions_url}/v1/chat/completions"),
            },
            EndpointConfig {
                protocol: ProviderProtocol::Responses,
                base_url: format!("{responses_url}/v1/responses"),
            },
        ],
        vec![enabled_model("gpt-4o")],
    )];
    config.keys.push(auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("gpt-4o")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state);

    let completions_req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let completions_res = app.clone().oneshot(completions_req).await.unwrap();
    assert_eq!(completions_res.status(), StatusCode::OK);

    let responses_req = Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"model":"gpt-4o","input":"hello"}"#))
        .unwrap();
    let responses_res = app.oneshot(responses_req).await.unwrap();
    assert_eq!(responses_res.status(), StatusCode::OK);

    let recorded_hits = hits.lock().unwrap().clone();
    assert!(recorded_hits.contains(&"a:completions".to_string()));
    assert!(recorded_hits.contains(&"b:responses".to_string()));
}

#[tokio::test]
async fn test_responses_prefers_native_endpoint_before_conversion() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let hits = Arc::new(std::sync::Mutex::new(Vec::new()));
    let completions_url = spawn_openai_mock_provider_with_label("completions", hits.clone()).await;
    let responses_url = spawn_openai_mock_provider_with_label("responses", hits.clone()).await;

    let mut config = test_config();
    config.providers = vec![
        provider(
            "fallback-completions",
            "completions",
            &format!("{completions_url}/v1/chat/completions"),
            vec![enabled_model("gpt-4o")],
        ),
        provider(
            "native-responses",
            "responses",
            &format!("{responses_url}/v1/responses"),
            vec![enabled_model("gpt-4o")],
        ),
    ];
    config.providers[0].priority = 0;
    config.providers[1].priority = 100;
    config.keys.push(auth_key(
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

    let recorded_hits = hits.lock().unwrap().clone();
    assert_eq!(recorded_hits, vec!["responses:responses".to_string()]);

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "native-responses");
    assert_eq!(logs[0].protocol, "responses");
}

#[tokio::test]
async fn test_responses_does_not_fall_back_to_chat_completions() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_openai_mock_provider(StatusCode::OK, serde_json::json!({})).await;

    let mut config = test_config();
    config.providers = vec![provider(
        "chat-only",
        "completions",
        &mock_base_url,
        vec![enabled_model("gpt-4o")],
    )];
    config.keys.push(auth_key(
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
        .body(Body::from(
            r#"{"model":"gpt-4o","instructions":"be brief","input":"hello","max_output_tokens":9}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(response["error"]["code"], "model_not_found");

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "unrouted");
    assert_eq!(logs[0].protocol, "responses");
    assert_eq!(logs[0].operation, "responses");
}

#[tokio::test]
async fn test_responses_does_not_fall_back_to_messages() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_anthropic_mock_provider(StatusCode::OK, serde_json::json!({})).await;

    let mut config = test_config();
    config.providers = vec![provider(
        "messages-only",
        "messages",
        &mock_base_url,
        vec![enabled_model("claude-sonnet")],
    )];
    config.keys.push(auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("claude-sonnet")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-sonnet","input":"hello","previous_response_id":"resp_previous","prompt_cache_key":"prompt-cache-a","metadata":{"user_id":"user_hash_account_acc_session_claude-session","extra":"ignored"},"conversation_id":"conversation-a","user":"openai-user"}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(response["error"]["code"], "model_not_found");

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "unrouted");
    assert_eq!(logs[0].protocol, "responses");
    assert_eq!(logs[0].operation, "responses");
}

#[tokio::test]
async fn test_provider_health_and_connection_tracking_are_provider_scoped() {
    let config = AppConfig {
        providers: vec![provider_with_endpoints(
            "openai-multi",
            vec![
                EndpointConfig {
                    protocol: ProviderProtocol::Completions,
                    base_url: "https://api.example.com/v1/chat/completions".to_string(),
                },
                EndpointConfig {
                    protocol: ProviderProtocol::Responses,
                    base_url: "https://api.example.com/v1/responses".to_string(),
                },
            ],
            vec![enabled_model("gpt-4o")],
        )],
        ..test_config()
    };

    let state = state_from_config(config).await;
    let snapshot = state.config_service.snapshot();

    snapshot
        .registry
        .record_connection("openai-multi", ProviderProtocol::Completions);
    snapshot
        .registry
        .record_connection("openai-multi", ProviderProtocol::Responses);
    assert_eq!(snapshot.registry.connection_count("openai-multi"), 2);

    for _ in 0..4 {
        state.router.record_provider_failure("openai-multi");
    }
    assert!(!state.router.is_provider_healthy("openai-multi"));
}

#[tokio::test]
async fn test_messages_protocol_has_own_endpoint_and_audit_log() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_anthropic_mock_provider(StatusCode::OK, serde_json::json!({})).await;
    let mut config = test_config();
    config.providers = vec![provider(
        "anthropic-messages",
        "messages",
        &mock_base_url,
        vec![enabled_model("claude-sonnet")],
    )];
    config.keys.push(auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("claude-sonnet")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-sonnet","max_tokens":128,"messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(response["model"], "claude-sonnet");

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "anthropic-messages");
    assert_eq!(logs[0].protocol, "messages");
    assert_eq!(logs[0].operation, "messages");
}

#[tokio::test]
async fn test_llm_requests_require_matching_key() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_openai_mock_provider(StatusCode::OK, serde_json::json!({})).await;
    let mut config = test_config();
    config.providers = vec![provider(
        "openai-test-provider",
        "completions",
        &mock_base_url,
        vec![enabled_model("gpt-4o")],
    )];

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
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
        .uri("/v1/admin/providers")
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"priority-provider","api_key":"sk-priority","models":[{"name":"priority-model","status":"enabled","aliases":[]}],"endpoints":[{"protocol":"completions","base_url":"https://api.example.com/v1/chat/completions"}],"priority":9}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let provider_body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(provider_body["priority"], 9);
    assert!(provider_body.get("timeout_secs").is_none());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/providers")
        .header("x-admin-token", "admin-token")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"priority-provider","api_key":"sk-priority-2","models":[{"name":"priority-model","status":"enabled","aliases":[]}],"endpoints":[{"protocol":"completions","base_url":"https://api2.example.com/v1/chat/completions"}]}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let provider_body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(provider_body["priority"], 9);

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
            operation: "completions",
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
            operation: "completions",
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

    let req = Request::builder()
        .method("GET")
        .uri(
            "/v1/admin/analytics/dashboard?from=2000-01-01T00:00:00Z&to=2099-12-31T23:59:59Z&bucket=day",
        )
        .header("x-admin-token", "admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let dashboard: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(dashboard["total"]["request_count"], 2);
    assert_eq!(dashboard["by_model"].as_array().unwrap().len(), 2);
    assert_eq!(dashboard["by_model"][0]["success_rate"], 1.0);
    assert_eq!(dashboard["by_key"].as_array().unwrap().len(), 2);
    assert_eq!(dashboard["by_provider"].as_array().unwrap().len(), 1);
    assert_eq!(dashboard["by_protocol"].as_array().unwrap().len(), 1);
    assert_eq!(dashboard["by_status_code"].as_array().unwrap().len(), 1);
    assert_eq!(dashboard["timeline"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_llm_request_errors_are_persisted_in_stats() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mut config = test_config();
    config
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

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].operation, "completions");
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
    let metadata: serde_json::Value = serde_json::from_str(&detail.meta).unwrap();
    assert_eq!(metadata["error"]["code"], "model_not_found");
}

#[tokio::test]
async fn test_chat_completions_uses_completions_provider_protocol() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_openai_mock_provider(StatusCode::OK, serde_json::json!({})).await;

    let mut config = test_config();
    config.providers = vec![provider(
        "plain-completions-only",
        "completions",
        &mock_base_url,
        vec![enabled_model("gpt-4o")],
    )];
    config.keys.push(auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("gpt-4o")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "plain-completions-only");
    assert_eq!(logs[0].protocol, "completions");
    assert_eq!(logs[0].operation, "completions");
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
            operation: "completions",
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
            operation: "completions",
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
    config.keys.push(key);
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
async fn test_models_endpoint_filters_allowed_providers_owner() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mut config = test_config();
    config.providers = vec![
        provider(
            "blocked-provider",
            "completions",
            "https://blocked.example.com/v1/chat/completions",
            vec![enabled_model("shared-model")],
        ),
        provider(
            "allowed-provider",
            "completions",
            "https://allowed.example.com/v1/chat/completions",
            vec![enabled_model("shared-model")],
        ),
    ];
    let mut key = auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("shared-model")],
    );
    key.allowed_providers = vec!["allowed-provider".to_string()];
    config.keys.push(key);

    let state = state_from_config(config).await;
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
    let data = models["data"].as_array().unwrap();

    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["id"], "shared-model");
    assert_eq!(data[0]["owned_by"], "allowed-provider");
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
    let mut key = auth_key("user-key", "user-secret-key", vec![enabled_model("quick")]);
    key.model_aliases
        .insert("quick".to_string(), "global-fast".to_string());
    config.keys.push(key);

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "user-secret-key")
        .header("session-id", "codex-session-a")
        .header("x-client-trace", "trace-123")
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

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].api_key_id, "user-key");
    assert_eq!(logs[0].model, "gpt-4o");
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
    let metadata: serde_json::Value = serde_json::from_str(&detail.meta).unwrap();
    assert_eq!(metadata["session"]["id"], "codex-session-a");
    assert_eq!(metadata["session"]["source"], "session_id_header");
    assert_eq!(metadata["models"]["requested"], "quick");
    assert_eq!(metadata["models"]["resolved"], "global-fast");
    assert_eq!(metadata["models"]["provider"], "gpt-4o");
    assert_eq!(metadata["routing"]["sticky_enabled"], false);
    assert_eq!(metadata["request"]["protocol"], "completions");
    assert_eq!(metadata["request"]["operation"], "completions");
    assert_eq!(metadata["request"]["model"], "quick");
    assert_eq!(metadata["request"]["resolved_model"], "global-fast");
    assert_eq!(metadata["request"]["headers"]["x-api-key"], "<redacted>");
    assert_eq!(
        metadata["request"]["headers"]["session-id"],
        "codex-session-a"
    );
    assert_eq!(
        metadata["request"]["headers"]["x-client-trace"],
        "trace-123"
    );
    assert_eq!(
        metadata["routing"]["selected_provider"],
        "openai-test-provider"
    );
    assert_eq!(metadata["routing"]["target_protocol"], "completions");
    assert_eq!(metadata["routing"]["target_operation"], "completions");
    assert_eq!(metadata["routing"]["target_model"], "gpt-4o");
    assert_eq!(metadata["upstream"]["base_url"], mock_base_url);
    assert_eq!(metadata["upstream"]["operation"], "completions");
    assert_eq!(metadata["upstream"]["protocol"], "completions");
    assert_eq!(metadata["upstream"]["model"], "gpt-4o");
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
    assert_eq!(model_rollup[0].group_key, "gpt-4o");
}

#[tokio::test]
async fn test_routed_provider_failure_logs_actual_provider_model() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let unavailable_base_url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    drop(listener);

    let mut config = test_config();
    config.providers = vec![provider(
        "unavailable-provider",
        "completions",
        &unavailable_base_url,
        vec![ModelRule {
            name: "provider-gpt-4o".into(),
            status: "enabled".into(),
            pricing: None,
            aliases: vec!["public-gpt".into()],
        }],
    )];
    config.retry.max_attempts = 1;
    config.keys.push(auth_key(
        "failure-key",
        "failure-secret-key",
        vec![enabled_model("public-gpt")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "failure-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"public-gpt","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert!(res.status().is_server_error());

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "unavailable-provider");
    assert_eq!(logs[0].model, "provider-gpt-4o");
    let metadata: serde_json::Value = serde_json::from_str(&logs[0].meta).unwrap();
    assert_eq!(metadata["models"]["requested"], "public-gpt");
    assert_eq!(metadata["models"]["resolved"], "public-gpt");
    assert_eq!(metadata["models"]["provider"], "provider-gpt-4o");
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
    config.keys.push(auth_key(
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

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].status_code, 200);
    assert_eq!(logs[0].success, 1);

    let detail = state
        .store
        .get_request_log_detail(&logs[0].id)
        .await
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&detail.meta).unwrap();
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
    config.keys.push(auth_key(
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

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].status_code, 429);
    assert_eq!(logs[0].error_code.as_deref(), Some("rate_limit_exceeded"));
    let detail = state
        .store
        .get_request_log_detail(&logs[0].id)
        .await
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&detail.meta).unwrap();
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
async fn test_non_retryable_provider_status_does_not_retry_or_switch_provider() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let provider_a_attempts = Arc::new(AtomicUsize::new(0));
    let provider_b_attempts = Arc::new(AtomicUsize::new(0));
    let provider_a_url =
        spawn_tracking_openai_mock_provider(StatusCode::NOT_FOUND, provider_a_attempts.clone())
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
    config.retry.max_attempts = 10;
    config.retry.initial_backoff_ms = 1;
    config.retry.max_backoff_ms = 1;
    config.keys.push(auth_key(
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
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    assert_eq!(provider_a_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(provider_b_attempts.load(Ordering::SeqCst), 0);

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "provider-a");
    assert_eq!(logs[0].status_code, 404);
    assert_eq!(logs[0].retry_count, 0);
    let detail = state
        .store
        .get_request_log_detail(&logs[0].id)
        .await
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&detail.meta).unwrap();
    let attempts = metadata["retry"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["provider_name"], "provider-a");
    assert_eq!(attempts[0]["status_code"], 404);
    assert_eq!(attempts[0]["retryable"], false);
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
            vec![ModelRule {
                name: "provider-a-gpt".into(),
                status: "enabled".into(),
                pricing: None,
                aliases: vec!["public-gpt".into()],
            }],
        ),
        provider(
            "provider-b",
            "completions",
            &provider_b_url,
            vec![ModelRule {
                name: "provider-b-gpt".into(),
                status: "enabled".into(),
                pricing: None,
                aliases: vec!["public-gpt".into()],
            }],
        ),
    ];
    config.providers[0].priority = 1;
    config.providers[1].priority = 2;
    config.retry.initial_backoff_ms = 1;
    config.retry.max_backoff_ms = 1;
    config.keys.push(auth_key(
        "retry-key",
        "retry-secret-key",
        vec![enabled_model("public-gpt")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "retry-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"public-gpt","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    assert_eq!(provider_a_attempts.load(Ordering::SeqCst), 3);
    assert_eq!(provider_b_attempts.load(Ordering::SeqCst), 1);

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "provider-b");
    assert_eq!(logs[0].model, "provider-b-gpt");
    let detail = state
        .store
        .get_request_log_detail(&logs[0].id)
        .await
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&detail.meta).unwrap();
    assert_eq!(metadata["models"]["provider"], "provider-b-gpt");
    let attempts = metadata["retry"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 4);
    assert_eq!(attempts[0]["provider_name"], "provider-a");
    assert_eq!(attempts[0]["status_code"], 429);
    assert_eq!(attempts[0]["retryable"], true);
    assert_eq!(attempts[1]["provider_name"], "provider-a");
    assert_eq!(attempts[1]["status_code"], 429);
    assert_eq!(attempts[1]["retryable"], true);
    assert_eq!(attempts[2]["provider_name"], "provider-a");
    assert_eq!(attempts[2]["status_code"], 429);
    assert_eq!(attempts[2]["retryable"], false);
    assert_eq!(attempts[3]["provider_name"], "provider-b");
    assert_eq!(attempts[3]["status_code"], 200);
    assert_eq!(attempts[3]["retryable"], false);
    assert_eq!(attempts[3]["selected_via"], "healthy");
}

#[tokio::test]
async fn test_streaming_chat_completions_falls_back_to_messages_conversion() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_streaming_anthropic_mock_provider().await;

    let mut config = test_config();
    config.providers = vec![provider(
        "messages-only",
        "messages",
        &mock_base_url,
        vec![ModelRule {
            name: "claude-3-7-sonnet-provider".into(),
            status: "enabled".into(),
            pricing: None,
            aliases: vec!["claude-sonnet".into()],
        }],
    )];
    config.keys.push(auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("claude-sonnet")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-sonnet","stream":true,"messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()["content-type"].to_str().unwrap(),
        "text/event-stream"
    );
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body.contains("\"object\":\"chat.completion.chunk\""));
    assert!(body.contains("\"model\":\"claude-sonnet\""));
    assert!(body.contains("\"content\":\"hello\""));
    assert!(body.contains("data: [DONE]"));

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "messages-only");
    assert_eq!(logs[0].protocol, "messages");
    assert_eq!(logs[0].operation, "completions");
    assert_eq!(logs[0].model, "claude-3-7-sonnet-provider");
}

#[tokio::test]
async fn test_streaming_messages_falls_back_to_chat_completions_conversion() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let mock_base_url = spawn_streaming_openai_mock_provider().await;

    let mut config = test_config();
    config.providers = vec![provider(
        "chat-only",
        "completions",
        &mock_base_url,
        vec![enabled_model("gpt-4o")],
    )];
    config.keys.push(auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("gpt-4o")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4o","stream":true,"max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()["content-type"].to_str().unwrap(),
        "text/event-stream"
    );
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body.contains("event: message_start"));
    assert!(body.contains("\"type\":\"message_start\""));
    assert!(body.contains("\"model\":\"gpt-4o\""));
    assert!(body.contains("\"type\":\"text_delta\""));
    assert!(body.contains("\"text\":\"hello\""));
    assert!(body.contains("event: message_stop"));

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].provider_name, "chat-only");
    assert_eq!(logs[0].protocol, "completions");
    assert_eq!(logs[0].operation, "messages");
}

#[tokio::test]
async fn test_streaming_messages_conversion_does_not_wait_for_upstream_socket_close() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::time::Duration;
    use tower::ServiceExt;

    let mock_base_url = spawn_lingering_streaming_openai_mock_provider().await;

    let mut config = test_config();
    config.providers = vec![provider(
        "chat-only",
        "completions",
        &mock_base_url,
        vec![enabled_model("gpt-4o")],
    )];
    config.keys.push(auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("gpt-4o")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4o","stream":true,"max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = tokio::time::timeout(
        Duration::from_millis(300),
        axum::body::to_bytes(res.into_body(), 1024 * 1024),
    )
    .await
    .expect("proxy stream should stop after translated terminal event")
    .unwrap();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body.contains("event: message_start"));
    assert!(body.contains("event: message_stop"));

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].protocol, "completions");
    assert_eq!(logs[0].operation, "messages");
}

#[tokio::test]
async fn test_streaming_messages_conversion_handles_fragmented_single_newline_sse() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::time::Duration;
    use tower::ServiceExt;

    let mock_base_url = spawn_fragmented_newline_streaming_openai_mock_provider().await;

    let mut config = test_config();
    config.providers = vec![provider(
        "chat-only",
        "completions",
        &mock_base_url,
        vec![enabled_model("gpt-4o")],
    )];
    config.keys.push(auth_key(
        "user-key",
        "user-secret-key",
        vec![enabled_model("gpt-4o")],
    ));

    let state = state_from_config(config).await;
    let app = rcpa::server::router::build(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("x-api-key", "user-secret-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4o","stream":true,"max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = tokio::time::timeout(
        Duration::from_millis(300),
        axum::body::to_bytes(res.into_body(), 1024 * 1024),
    )
    .await
    .expect("fragmented newline-delimited SSE should not wait for the upstream timeout")
    .unwrap();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body.contains("event: message_start"));
    assert!(body.contains("\"type\":\"text_delta\""));
    assert!(body.contains("\"text\":\"你好😀🎉\""));
    assert!(body.contains("event: message_stop"));

    let logs = wait_for_request_logs(&state, 1).await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].protocol, "completions");
    assert_eq!(logs[0].operation, "messages");
}

#[tokio::test]
async fn test_disabled_provider_model_disables_alias() {
    let mut config = test_config();
    config.providers[0].models[0].status = "disabled".into();
    let mut key = auth_key("user-key", "user-secret-key", vec![enabled_model("quick")]);
    key.model_aliases
        .insert("quick".to_string(), "global-fast".to_string());
    config.keys.push(key);

    let err = match ConfigService::from_config(config) {
        Ok(_) => panic!("config should reject a key alias targeting a disabled platform model"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("targets unknown platform model"));
}
