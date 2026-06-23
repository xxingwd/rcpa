use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};

use crate::middleware::metrics::metrics_middleware;
use crate::middleware::request_id::request_id_middleware;
use crate::protocol::anthropic;
use crate::protocol::openai;
use crate::server::AppState;

pub fn build(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api_v1 = Router::new()
        .route("/models", get(openai::models::list_models))
        .route("/chat/completions", post(openai::chat::chat_completions))
        .route("/completions", post(openai::completions::completions))
        .route("/responses", post(openai::responses::responses))
        .route("/embeddings", post(openai::embeddings::embeddings))
        .route("/messages", post(anthropic::messages::messages))
        .route(
            "/admin/providers",
            get(crate::admin::list_providers).post(crate::admin::create_provider),
        )
        .route(
            "/admin/providers/{name}",
            delete(crate::admin::delete_provider),
        )
        .route(
            "/admin/providers/{name}/status",
            put(crate::admin::update_provider_status),
        )
        .route(
            "/admin/providers/{name}/models/{model}/status",
            put(crate::admin::update_provider_model_status),
        )
        .route(
            "/admin/keys",
            get(crate::admin::list_all_keys).post(crate::admin::create_key),
        )
        .route("/admin/keys/{id}", put(crate::admin::update_key))
        .route("/admin/keys/revoke/{id}", delete(crate::admin::delete_key))
        .route(
            "/admin/keys/{id}/status",
            put(crate::admin::update_key_status_handler),
        )
        .route(
            "/admin/keys/{id}/models/{model}/status",
            put(crate::admin::update_key_model_status),
        )
        .route(
            "/admin/aliases",
            get(crate::admin::list_aliases).post(crate::admin::create_alias),
        )
        .route(
            "/admin/model-catalog",
            get(crate::admin::list_model_catalog),
        )
        .route(
            "/admin/config-file",
            get(crate::admin::get_config_file).put(crate::admin::update_config_file),
        )
        .route("/admin/aliases/{alias}", delete(crate::admin::delete_alias))
        .route(
            "/admin/pricing",
            get(crate::admin::list_pricing_rules).post(crate::admin::create_pricing_rule),
        )
        .route(
            "/admin/pricing/{id}",
            delete(crate::admin::delete_pricing_rule),
        )
        .route("/admin/logs", get(crate::admin::list_request_logs))
        .route(
            "/admin/logs/{id}",
            get(crate::admin::get_request_log_detail),
        )
        .route(
            "/admin/analytics/model",
            get(crate::admin::get_analytics_by_model),
        )
        .route(
            "/admin/analytics/provider",
            get(crate::admin::get_analytics_by_provider),
        )
        .route(
            "/admin/analytics/key",
            get(crate::admin::get_analytics_by_key),
        )
        .route(
            "/admin/analytics/hour",
            get(crate::admin::get_analytics_by_hour),
        )
        .route(
            "/admin/analytics/day",
            get(crate::admin::get_analytics_by_day),
        )
        .route(
            "/admin/analytics/total",
            get(crate::admin::get_analytics_totals),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics_middleware,
        ))
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state.clone());

    let meta = Router::new()
        .route("/health", get(health_check))
        .route("/stats", get(stats_handler))
        .route("/admin", get(crate::admin::dashboard))
        .route("/assets/{*path}", get(crate::admin::static_handler))
        .route("/vite.svg", get(crate::admin::vite_svg))
        .with_state(state.clone());

    Router::new()
        .nest("/v1", api_v1)
        .merge(meta)
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(32 * 1024 * 1024))
        .layer(cors)
}

async fn health_check(state: axum::extract::State<Arc<AppState>>) -> axum::Json<serde_json::Value> {
    let uptime = chrono::Utc::now() - state.start_time;
    axum::Json(serde_json::json!({
        "status": "ok",
        "uptime_secs": uptime.num_seconds(),
        "providers": state.config_service.snapshot().provider_count(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn stats_handler(
    state: axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let from = "1970-01-01T00:00:00Z";
    let to = "9999-12-31T23:59:59Z";
    let stats = match state.store.dashboard_stats(from, to).await {
        Ok(stats) => serde_json::to_value(stats).unwrap_or_else(|err| {
            tracing::error!(error = %err, "Failed to serialize dashboard stats");
            default_dashboard_stats()
        }),
        Err(err) => {
            tracing::error!(error = %err, "Failed to load dashboard stats");
            default_dashboard_stats()
        }
    };
    axum::Json(stats)
}

fn default_dashboard_stats() -> serde_json::Value {
    serde_json::json!({
        "requests": {
            "total": 0,
            "success": 0,
            "errors": 0,
            "error_rate": 0.0,
            "success_rate": 0.0,
        },
        "tokens": {
            "input": 0,
            "output": 0,
            "cached": 0,
            "cache_write": 0,
            "cache_hit_rate": 0.0,
            "total": 0,
            "avg_per_request": 0.0,
        },
        "latency": {
            "avg_ms": 0.0,
            "max_ms": 0,
            "first_byte_avg_ms": 0.0,
            "first_byte_max_ms": 0,
        },
        "cost": {
            "total_cents": 0,
        },
    })
}
