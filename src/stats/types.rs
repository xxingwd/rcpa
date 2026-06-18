use serde::{Deserialize, Serialize};

/// Per-request statistics record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestStats {
    pub request_id: String,
    pub model: String,
    pub provider: String,
    pub status: u16,
    pub latency_ms: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cost_cents: u64,
    pub timestamp: i64,
}

/// Aggregated statistics for a time window
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregatedStats {
    pub total_requests: u64,
    pub total_success: u64,
    pub total_errors: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub avg_latency_ms: f64,
    pub total_cost_cents: u64,
}
