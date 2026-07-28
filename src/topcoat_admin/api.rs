use serde::{Deserialize, Serialize};
use topcoat::context::Cx;

/// Dashboard statistics from the analytics endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardStats {
    pub requests: RequestStats,
    pub tokens: TokenStats,
    pub latency: LatencyStats,
    pub cost: CostStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestStats {
    pub total: i64,
    pub success: i64,
    pub errors: i64,
    pub error_rate: f64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenStats {
    pub input: i64,
    pub output: i64,
    pub cached: i64,
    pub cache_write: i64,
    pub cache_hit_rate: f64,
    pub total: i64,
    pub avg_per_request: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencyStats {
    pub avg_ms: f64,
    pub max_ms: i64,
    pub first_byte_avg_ms: f64,
    pub first_byte_max_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostStats {
    pub total_cents: i64,
}

/// Fetches dashboard stats from the local API.
pub async fn fetch_dashboard_stats(cx: &Cx) -> DashboardStats {
    let state = crate::topcoat_admin::app::app_state(cx);
    let from = "1970-01-01T00:00:00Z";
    let to = "9999-12-31T23:59:59Z";

    match state.store.dashboard_stats(from, to).await {
        Ok(stats) => serde_json::from_value(serde_json::to_value(stats).unwrap_or_default())
            .unwrap_or_default(),
        Err(_) => DashboardStats::default(),
    }
}

/// Fetches providers from the shared state.
pub async fn fetch_providers(cx: &Cx) -> Vec<crate::config_service::ProviderView> {
    let state = crate::topcoat_admin::app::app_state(cx);
    state.config_service.snapshot().providers()
}

/// Fetches API keys from the shared state.
pub async fn fetch_keys(cx: &Cx) -> Vec<crate::config_service::AuthKeyView> {
    let state = crate::topcoat_admin::app::app_state(cx);
    state.config_service.snapshot().auth_keys()
}

/// Fetches the raw YAML config content.
pub async fn fetch_config_yaml(cx: &Cx) -> String {
    let state = crate::topcoat_admin::app::app_state(cx);
    state.config_service.read_raw_yaml().unwrap_or_default()
}
