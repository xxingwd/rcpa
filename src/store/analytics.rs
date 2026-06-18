use serde::Serialize;

use super::{models::AggregateRow, models::TotalStats, Store, StoreResult};

const SUCCESS_CONDITION: &str = "status_code < 400 AND error IS NULL";

/// Dashboard-oriented LLM API metrics derived from persisted request logs.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardStats {
    pub requests: RequestMetrics,
    pub tokens: TokenMetrics,
    pub latency: LatencyMetrics,
    pub cost: CostMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestMetrics {
    pub total: i64,
    pub success: i64,
    pub errors: i64,
    pub error_rate: f64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenMetrics {
    pub input: i64,
    pub output: i64,
    pub cached: i64,
    pub cache_write: i64,
    pub cache_hit_rate: f64,
    pub total: i64,
    pub avg_per_request: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatencyMetrics {
    pub avg_ms: f64,
    pub max_ms: i64,
    pub first_byte_avg_ms: f64,
    pub first_byte_max_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostMetrics {
    pub total_cents: i64,
}

impl Store {
    /// Aggregate request log stats grouped by model within a time range.
    pub fn aggregate_by_model(&self, from: &str, to: &str) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_column("model", from, to)
    }

    /// Aggregate request log stats grouped by provider instance within a time range.
    pub fn aggregate_by_provider(&self, from: &str, to: &str) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_column("provider_name", from, to)
    }

    /// Aggregate request log stats grouped by API key within a time range.
    pub fn aggregate_by_key(&self, from: &str, to: &str) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_column("api_key_id", from, to)
    }

    /// Time-series aggregation by hour (truncated ISO-8601 hour).
    pub fn aggregate_by_hour(&self, from: &str, to: &str) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_time("substr(created_at, 1, 13)", from, to)
    }

    /// Time-series aggregation by day (truncated ISO-8601 date).
    pub fn aggregate_by_day(&self, from: &str, to: &str) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_time("substr(created_at, 1, 10)", from, to)
    }

    /// Overall totals across all request logs in a time range.
    pub fn total_stats(&self, from: &str, to: &str) -> StoreResult<TotalStats> {
        let conn = self.conn();
        let result = conn.query_row(
            "SELECT
                COUNT(*) as request_count,
                COALESCE(SUM(CASE WHEN status_code < 400 AND error IS NULL THEN 1 ELSE 0 END), 0) as success_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(cached_tokens), 0) as total_cached_tokens,
                COALESCE(SUM(cache_write_tokens), 0) as total_cache_write_tokens,
                COALESCE(SUM(total_tokens), 0) as total_tokens,
                COALESCE(AVG(total_tokens), 0.0) as avg_tokens_per_request,
                COALESCE(SUM(cost_cents), 0) as total_cost_cents,
                COALESCE(AVG(latency_ms), 0.0) as avg_latency_ms,
                COALESCE(MAX(latency_ms), 0) as max_latency_ms,
                COALESCE(AVG(NULLIF(first_byte_latency_ms, 0)), 0.0) as avg_first_byte_latency_ms,
                COALESCE(MAX(first_byte_latency_ms), 0) as max_first_byte_latency_ms,
                COALESCE(SUM(CASE WHEN status_code >= 400 OR error IS NOT NULL THEN 1 ELSE 0 END), 0) as error_count
             FROM request_logs
             WHERE created_at >= ?1 AND created_at <= ?2",
            rusqlite::params![from, to],
            |row| {
                let request_count = row.get(0)?;
                let success_count = row.get(1)?;
                Ok(TotalStats {
                    request_count,
                    success_count,
                    success_rate: success_rate(success_count, request_count),
                    total_input_tokens: row.get(2)?,
                    total_output_tokens: row.get(3)?,
                    total_cached_tokens: row.get(4)?,
                    total_cache_write_tokens: row.get(5)?,
                    cache_hit_rate: cache_hit_rate(row.get(4)?, row.get(2)?),
                    total_tokens: row.get(6)?,
                    avg_tokens_per_request: row.get(7)?,
                    total_cost_cents: row.get(8)?,
                    avg_latency_ms: row.get(9)?,
                    max_latency_ms: row.get(10)?,
                    avg_first_byte_latency_ms: row.get(11)?,
                    max_first_byte_latency_ms: row.get(12)?,
                    error_count: row.get(13)?,
                })
            },
        )?;
        Ok(result)
    }

    /// Snapshot shape used by `/stats`.
    ///
    /// This intentionally reads from `request_logs` instead of process memory
    /// so the displayed metrics survive restarts and match audit data exactly.
    pub fn dashboard_stats(&self, from: &str, to: &str) -> StoreResult<DashboardStats> {
        let totals = self.total_stats(from, to)?;
        Ok(DashboardStats {
            requests: RequestMetrics {
                total: totals.request_count,
                success: totals.success_count,
                errors: totals.error_count,
                error_rate: success_rate(totals.error_count, totals.request_count),
                success_rate: totals.success_rate,
            },
            tokens: TokenMetrics {
                input: totals.total_input_tokens,
                output: totals.total_output_tokens,
                cached: totals.total_cached_tokens,
                cache_write: totals.total_cache_write_tokens,
                cache_hit_rate: totals.cache_hit_rate,
                total: totals.total_tokens,
                avg_per_request: totals.avg_tokens_per_request,
            },
            latency: LatencyMetrics {
                avg_ms: totals.avg_latency_ms,
                max_ms: totals.max_latency_ms,
                first_byte_avg_ms: totals.avg_first_byte_latency_ms,
                first_byte_max_ms: totals.max_first_byte_latency_ms,
            },
            cost: CostMetrics {
                total_cents: totals.total_cost_cents,
            },
        })
    }

    // ── private helpers ────────────────────────────────────────────

    fn aggregate_by_column(
        &self,
        column: &str,
        from: &str,
        to: &str,
    ) -> StoreResult<Vec<AggregateRow>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT
                {col} as group_key,
                COUNT(*) as request_count,
                COALESCE(SUM(CASE WHEN {success_condition} THEN 1 ELSE 0 END), 0) as success_count,
                COALESCE(SUM(CASE WHEN status_code >= 400 OR error IS NOT NULL THEN 1 ELSE 0 END), 0) as error_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(cached_tokens), 0) as total_cached_tokens,
                COALESCE(SUM(cache_write_tokens), 0) as total_cache_write_tokens,
                COALESCE(SUM(total_tokens), 0) as total_tokens,
                COALESCE(SUM(cost_cents), 0) as total_cost_cents,
                COALESCE(AVG(latency_ms), 0.0) as avg_latency_ms,
                COALESCE(AVG(NULLIF(first_byte_latency_ms, 0)), 0.0) as avg_first_byte_latency_ms
             FROM request_logs
             WHERE created_at >= ?1 AND created_at <= ?2
             GROUP BY {col}
             ORDER BY total_cost_cents DESC",
            col = column,
            success_condition = SUCCESS_CONDITION,
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params![from, to], Self::map_aggregate_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn aggregate_by_time(
        &self,
        time_expr: &str,
        from: &str,
        to: &str,
    ) -> StoreResult<Vec<AggregateRow>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT
                {expr} as group_key,
                COUNT(*) as request_count,
                COALESCE(SUM(CASE WHEN {success_condition} THEN 1 ELSE 0 END), 0) as success_count,
                COALESCE(SUM(CASE WHEN status_code >= 400 OR error IS NOT NULL THEN 1 ELSE 0 END), 0) as error_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(cached_tokens), 0) as total_cached_tokens,
                COALESCE(SUM(cache_write_tokens), 0) as total_cache_write_tokens,
                COALESCE(SUM(total_tokens), 0) as total_tokens,
                COALESCE(SUM(cost_cents), 0) as total_cost_cents,
                COALESCE(AVG(latency_ms), 0.0) as avg_latency_ms,
                COALESCE(AVG(NULLIF(first_byte_latency_ms, 0)), 0.0) as avg_first_byte_latency_ms
             FROM request_logs
             WHERE created_at >= ?1 AND created_at <= ?2
             GROUP BY {expr}
             ORDER BY group_key ASC",
            expr = time_expr,
            success_condition = SUCCESS_CONDITION,
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params![from, to], Self::map_aggregate_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn map_aggregate_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AggregateRow> {
        let request_count = row.get(1)?;
        let success_count = row.get(2)?;
        Ok(AggregateRow {
            group_key: row.get(0)?,
            request_count,
            success_count,
            error_count: row.get(3)?,
            success_rate: success_rate(success_count, request_count),
            total_input_tokens: row.get(4)?,
            total_output_tokens: row.get(5)?,
            total_cached_tokens: row.get(6)?,
            total_cache_write_tokens: row.get(7)?,
            total_tokens: row.get(8)?,
            total_cost_cents: row.get(9)?,
            avg_latency_ms: row.get(10)?,
            avg_first_byte_latency_ms: row.get(11)?,
        })
    }
}

fn success_rate(success_count: i64, request_count: i64) -> f64 {
    if request_count > 0 {
        success_count as f64 / request_count as f64
    } else {
        0.0
    }
}

fn cache_hit_rate(cached_tokens: i64, input_tokens: i64) -> f64 {
    if input_tokens > 0 {
        cached_tokens as f64 / input_tokens as f64
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use crate::store::{NewRequestLog, Store};

    fn seed_logs(store: &Store) {
        let key_ids = ["analytics-key-0", "analytics-key-1"];
        let models = ["gpt-4", "gpt-4", "claude-3", "claude-3", "gpt-4"];
        let provider_names = [
            "openai-1",
            "openai-2",
            "anthropic-1",
            "anthropic-1",
            "openai-1",
        ];
        let status_codes: [i64; 5] = [200, 200, 200, 500, 200];

        for i in 0..5 {
            store
                .insert_request_log_entry(NewRequestLog {
                    request_id: &format!("req-{}", i),
                    api_key_id: key_ids[i % 2],
                    provider_name: provider_names[i],
                    provider: if provider_names[i].starts_with("openai") {
                        "completions"
                    } else {
                        "messages"
                    },
                    model: models[i],
                    operation: "completions",
                    status_code: status_codes[i],
                    input_tokens: 100,
                    output_tokens: 50,
                    total_tokens: 150,
                    cached_tokens: 20,
                    cache_write_tokens: 5,
                    cost_cents: 10,
                    latency_ms: 100 + (i as i64) * 10,
                    first_byte_latency_ms: 100 + (i as i64) * 10,
                    error_code: None,
                    error: if status_codes[i] >= 400 {
                        Some("error")
                    } else {
                        None
                    },
                    request_body: None,
                    response_body: None,
                })
                .unwrap();
        }
    }

    #[test]
    fn test_aggregate_by_model() {
        let store = Store::open_in_memory().unwrap();
        seed_logs(&store);

        let rows = store
            .aggregate_by_model("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .unwrap();
        assert_eq!(rows.len(), 2); // gpt-4 and claude-3

        let gpt4 = rows.iter().find(|r| r.group_key == "gpt-4").unwrap();
        assert_eq!(gpt4.request_count, 3);
        assert_eq!(gpt4.success_count, 3);
        assert_eq!(gpt4.error_count, 0);
        assert_eq!(gpt4.success_rate, 1.0);

        let claude = rows.iter().find(|r| r.group_key == "claude-3").unwrap();
        assert_eq!(claude.request_count, 2);
        assert_eq!(claude.success_count, 1);
        assert_eq!(claude.error_count, 1);
        assert_eq!(claude.success_rate, 0.5);
    }

    #[test]
    fn test_aggregate_by_provider() {
        let store = Store::open_in_memory().unwrap();
        seed_logs(&store);

        let rows = store
            .aggregate_by_provider("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn test_aggregate_by_key() {
        let store = Store::open_in_memory().unwrap();
        seed_logs(&store);

        let rows = store
            .aggregate_by_key("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .unwrap();
        assert_eq!(rows.len(), 2); // key-0 and key-1
    }

    #[test]
    fn test_total_stats() {
        let store = Store::open_in_memory().unwrap();
        seed_logs(&store);

        let stats = store
            .total_stats("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .unwrap();
        assert_eq!(stats.request_count, 5);
        assert_eq!(stats.success_count, 4);
        assert_eq!(stats.success_rate, 0.8);
        assert_eq!(stats.total_input_tokens, 500);
        assert_eq!(stats.total_output_tokens, 250);
        assert_eq!(stats.total_cached_tokens, 100);
        assert_eq!(stats.total_cache_write_tokens, 25);
        assert_eq!(stats.cache_hit_rate, 0.2);
        assert_eq!(stats.total_tokens, 750);
        assert_eq!(stats.avg_tokens_per_request, 150.0);
        assert_eq!(stats.total_cost_cents, 50);
        assert_eq!(stats.avg_latency_ms, 120.0);
        assert_eq!(stats.max_latency_ms, 140);
        assert_eq!(stats.avg_first_byte_latency_ms, 120.0);
        assert_eq!(stats.max_first_byte_latency_ms, 140);
        assert_eq!(stats.error_count, 1);
    }

    #[test]
    fn test_dashboard_stats_uses_persisted_request_logs() {
        let store = Store::open_in_memory().unwrap();
        seed_logs(&store);

        let stats = store
            .dashboard_stats("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .unwrap();

        assert_eq!(stats.requests.total, 5);
        assert_eq!(stats.requests.success, 4);
        assert_eq!(stats.requests.errors, 1);
        assert_eq!(stats.requests.error_rate, 0.2);
        assert_eq!(stats.requests.success_rate, 0.8);
        assert_eq!(stats.tokens.input, 500);
        assert_eq!(stats.tokens.output, 250);
        assert_eq!(stats.tokens.cached, 100);
        assert_eq!(stats.tokens.cache_write, 25);
        assert_eq!(stats.tokens.cache_hit_rate, 0.2);
        assert_eq!(stats.tokens.total, 750);
        assert_eq!(stats.tokens.avg_per_request, 150.0);
        assert_eq!(stats.latency.avg_ms, 120.0);
        assert_eq!(stats.latency.max_ms, 140);
        assert_eq!(stats.latency.first_byte_avg_ms, 120.0);
        assert_eq!(stats.latency.first_byte_max_ms, 140);
        assert_eq!(stats.cost.total_cents, 50);
    }

    #[test]
    fn test_aggregate_empty_range() {
        let store = Store::open_in_memory().unwrap();
        seed_logs(&store);

        let rows = store
            .aggregate_by_model("2000-01-01T00:00:00Z", "2000-01-02T00:00:00Z")
            .unwrap();
        assert_eq!(rows.len(), 0);

        let stats = store
            .total_stats("2000-01-01T00:00:00Z", "2000-01-02T00:00:00Z")
            .unwrap();
        assert_eq!(stats.request_count, 0);
    }
}
