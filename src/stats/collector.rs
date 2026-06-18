use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::protocol::common::TokenUsage;

/// Lightning-fast in-memory statistics collector
pub struct StatsCollector {
    total_requests: AtomicU64,
    total_success: AtomicU64,
    total_errors: AtomicU64,
    total_prompt_tokens: AtomicU64,
    total_completion_tokens: AtomicU64,
    total_latency_us: AtomicU64,
    /// Per-path request counts
    path_counts: Arc<dashmap::DashMap<String, AtomicU64>>,
    /// Per-model request counts
    model_counts: Arc<dashmap::DashMap<String, AtomicU64>>,
    /// Per-provider request counts
    provider_counts: Arc<dashmap::DashMap<String, AtomicU64>>,
    /// Per-key cost tracking (cents)
    key_costs: Arc<dashmap::DashMap<String, AtomicU64>>,
    /// Per-key model usage counts
    key_model_counts: Arc<dashmap::DashMap<String, AtomicU64>>,
    /// Cost tracking (integer cents for atomic ops)
    total_cost_cents: AtomicU64,
}

impl Default for StatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsCollector {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            total_success: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            total_prompt_tokens: AtomicU64::new(0),
            total_completion_tokens: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
            path_counts: Arc::new(dashmap::DashMap::new()),
            model_counts: Arc::new(dashmap::DashMap::new()),
            provider_counts: Arc::new(dashmap::DashMap::new()),
            key_costs: Arc::new(dashmap::DashMap::new()),
            key_model_counts: Arc::new(dashmap::DashMap::new()),
            total_cost_cents: AtomicU64::new(0),
        }
    }

    pub fn record_request(&self, path: &str, _method: &str) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.path_counts
            .entry(path.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_response(&self, _path: &str, status: u16, latency: Duration) {
        if status < 400 {
            self.total_success.fetch_add(1, Ordering::Relaxed);
        } else {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }
        self.total_latency_us
            .fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_success(
        &self,
        model: &str,
        provider: &str,
        _latency: Duration,
        tokens: Option<&TokenUsage>,
    ) {
        self.model_counts
            .entry(model.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);

        self.provider_counts
            .entry(provider.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);

        if let Some(tokens) = tokens {
            self.total_prompt_tokens
                .fetch_add(tokens.prompt_tokens, Ordering::Relaxed);
            self.total_completion_tokens
                .fetch_add(tokens.completion_tokens, Ordering::Relaxed);
        }
    }

    pub fn record_error(&self, model: &str, provider: &str) {
        self.model_counts
            .entry(model.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);

        self.provider_counts
            .entry(provider.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cost(&self, cost_cents: u64) {
        self.total_cost_cents
            .fetch_add(cost_cents, Ordering::Relaxed);
    }

    /// Record per-key usage: key -> model -> cost
    pub fn record_key_usage(&self, api_key: &str, model: &str, cost_cents: u64) {
        // Per-key total cost
        self.key_costs
            .entry(api_key.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(cost_cents, Ordering::Relaxed);

        // Per-key per-model count
        let key_model = format!("{}:{}", api_key, model);
        self.key_model_counts
            .entry(key_model)
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot for API consumption
    pub fn snapshot(&self) -> serde_json::Value {
        let total = self.total_requests.load(Ordering::Relaxed);
        let success = self.total_success.load(Ordering::Relaxed);
        let errors = self.total_errors.load(Ordering::Relaxed);
        let total_latency_us = self.total_latency_us.load(Ordering::Relaxed);

        let avg_latency_ms = if total > 0 {
            (total_latency_us as f64 / total as f64) / 1000.0
        } else {
            0.0
        };

        serde_json::json!({
            "requests": {
                "total": total,
                "success": success,
                "errors": errors,
            },
            "tokens": {
                "prompt": self.total_prompt_tokens.load(Ordering::Relaxed),
                "completion": self.total_completion_tokens.load(Ordering::Relaxed),
            },
            "latency": {
                "avg_ms": avg_latency_ms,
                "total_us": total_latency_us,
            },
            "cost": {
                "total_cents": self.total_cost_cents.load(Ordering::Relaxed),
            },
            "by_model": self.snapshot_map(&self.model_counts),
            "by_provider": self.snapshot_map(&self.provider_counts),
            "by_path": self.snapshot_map(&self.path_counts),
            "by_key": self.snapshot_map(&self.key_costs),
            "by_key_model": self.snapshot_map(&self.key_model_counts),
        })
    }

    fn snapshot_map(&self, map: &dashmap::DashMap<String, AtomicU64>) -> serde_json::Value {
        let entries: serde_json::Map<String, serde_json::Value> = map
            .iter()
            .map(|entry| {
                (
                    entry.key().clone(),
                    entry.value().load(Ordering::Relaxed).into(),
                )
            })
            .collect();
        serde_json::Value::Object(entries)
    }
}
