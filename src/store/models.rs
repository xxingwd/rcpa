use serde::{Deserialize, Serialize};

/// Database representation of a request log entry.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DbRequestLog {
    pub id: String,
    pub request_id: String,
    pub run_id: String,
    pub api_key_id: String,
    pub session_hash: Option<String>,
    pub provider_name: String,
    pub protocol: String,
    pub model: String,
    pub operation: String,
    pub status: String,
    pub status_code: i64,
    #[sqlx(default)]
    pub success: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    #[sqlx(default)]
    pub total_tokens: i64,
    #[sqlx(default)]
    pub cached_tokens: i64,
    #[sqlx(default)]
    pub cache_write_tokens: i64,
    pub cost_cents: i64,
    pub latency_ms: i64,
    pub first_byte_latency_ms: i64,
    pub retry_count: i64,
    pub meta: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub error: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    /// Only populated by detail query; always None in list results.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub request_body: Option<Vec<u8>>,
    /// Only populated by detail query; always None in list results.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub response_body: Option<Vec<u8>>,
}

/// Aggregated statistics result row.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AggregateRow {
    pub group_key: String,
    pub request_count: i64,
    pub success_count: i64,
    pub success_rate: f64,
    pub error_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cached_tokens: i64,
    pub total_cache_write_tokens: i64,
    pub total_tokens: i64,
    pub total_cost_cents: i64,
    pub avg_latency_ms: f64,
    pub avg_first_byte_latency_ms: f64,
}

/// Overall total statistics.
#[derive(Debug, Clone, Serialize)]
pub struct TotalStats {
    pub request_count: i64,
    pub success_count: i64,
    pub success_rate: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cached_tokens: i64,
    pub total_cache_write_tokens: i64,
    pub cache_hit_rate: f64,
    pub total_tokens: i64,
    pub avg_tokens_per_request: f64,
    pub total_cost_cents: i64,
    pub avg_latency_ms: f64,
    pub max_latency_ms: i64,
    pub avg_first_byte_latency_ms: f64,
    pub max_first_byte_latency_ms: i64,
    pub error_count: i64,
}

/// Filters for querying request logs.
#[derive(Debug, Clone, Default)]
pub struct RequestLogFilter {
    pub from: Option<String>,
    pub to: Option<String>,
    pub api_key_id: Option<String>,
    pub session_hash: Option<String>,
    pub model: Option<String>,
    pub provider_name: Option<String>,
    pub protocol: Option<String>,
    pub status_code: Option<i64>,
    pub status: Option<String>,
    pub success: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
