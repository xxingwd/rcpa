use serde::{Deserialize, Serialize};

/// Database representation of a request log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbRequestLog {
    pub id: String,
    pub request_id: String,
    pub api_key_id: String,
    pub provider_name: String,
    pub provider: String,
    pub model: String,
    pub operation: String,
    pub status_code: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_cents: i64,
    pub latency_ms: i64,
    pub first_byte_latency_ms: i64,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    /// Only populated by detail query; always None in list results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<Vec<u8>>,
    /// Only populated by detail query; always None in list results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<Vec<u8>>,
}

/// Aggregated statistics result row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateRow {
    pub group_key: String,
    pub request_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub success_rate: f64,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub model: Option<String>,
    pub provider_name: Option<String>,
    pub provider: Option<String>,
    pub status_code: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
