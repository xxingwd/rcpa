use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

use super::{models::AggregateRow, models::TotalStats, Store, StoreResult};

const SUCCESS_CONDITION: &str = "status = 'success'";

/// Dashboard-oriented LLM API metrics derived from persisted request logs.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardStats {
    pub requests: RequestMetrics,
    pub tokens: TokenMetrics,
    pub latency: LatencyMetrics,
    pub cost: CostMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardAnalytics {
    pub total: TotalStats,
    pub by_model: Vec<AggregateRow>,
    pub by_key: Vec<AggregateRow>,
    pub by_provider: Vec<AggregateRow>,
    pub by_protocol: Vec<AggregateRow>,
    pub by_status_code: Vec<AggregateRow>,
    pub timeline: Vec<AggregateRow>,
}

#[derive(Debug, Clone, Copy)]
pub enum AnalyticsTimeBucket {
    Hour,
    Day,
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
    pub async fn aggregate_by_model(&self, from: &str, to: &str) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_column("model", from, to).await
    }

    /// Aggregate request log stats grouped by provider instance within a time range.
    pub async fn aggregate_by_provider(
        &self,
        from: &str,
        to: &str,
    ) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_column("provider_name", from, to).await
    }

    /// Aggregate request log stats grouped by API key within a time range.
    pub async fn aggregate_by_key(&self, from: &str, to: &str) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_column("api_key_id", from, to).await
    }

    /// Time-series aggregation by hour (truncated ISO-8601 hour).
    pub async fn aggregate_by_hour(&self, from: &str, to: &str) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_time("substr(created_at, 1, 13)", from, to)
            .await
    }

    /// Time-series aggregation by day (truncated ISO-8601 date).
    pub async fn aggregate_by_day(&self, from: &str, to: &str) -> StoreResult<Vec<AggregateRow>> {
        self.aggregate_by_time("substr(created_at, 1, 10)", from, to)
            .await
    }

    /// Overall totals across all request logs in a time range.
    pub async fn total_stats(&self, from: &str, to: &str) -> StoreResult<TotalStats> {
        let row = sqlx::query_as::<_, TotalStatsRow>(
            "SELECT
                COUNT(*) as request_count,
                COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as success_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(cached_tokens), 0) as total_cached_tokens,
                COALESCE(SUM(cache_write_tokens), 0) as total_cache_write_tokens,
                COALESCE(SUM(input_tokens + output_tokens), 0) as total_tokens,
                COALESCE(AVG(input_tokens + output_tokens), 0.0) as avg_tokens_per_request,
                COALESCE(SUM(cost_cents), 0) as total_cost_cents,
                COALESCE(AVG(latency_ms), 0.0) as avg_latency_ms,
                COALESCE(MAX(latency_ms), 0) as max_latency_ms,
                COALESCE(AVG(NULLIF(first_byte_latency_ms, 0)), 0.0) as avg_first_byte_latency_ms,
                COALESCE(MAX(first_byte_latency_ms), 0) as max_first_byte_latency_ms,
                COALESCE(SUM(CASE WHEN status <> 'success' THEN 1 ELSE 0 END), 0) as error_count
             FROM request_log_metrics
             WHERE created_at >= ?1 AND created_at <= ?2",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into_total_stats())
    }

    /// Snapshot shape used by `/stats`.
    pub async fn dashboard_stats(&self, from: &str, to: &str) -> StoreResult<DashboardStats> {
        let totals = self.total_stats(from, to).await?;
        Ok(DashboardStats {
            requests: RequestMetrics {
                total: totals.request_count,
                success: totals.success_count,
                errors: totals.error_count,
                error_rate: rate(totals.error_count, totals.request_count),
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

    /// Build every dashboard aggregation from one narrow metrics-table scan.
    pub async fn dashboard_analytics(
        &self,
        from: &str,
        to: &str,
        time_bucket: AnalyticsTimeBucket,
    ) -> StoreResult<DashboardAnalytics> {
        let rows = sqlx::query_as::<_, DashboardMetricRow>(
            "SELECT created_at, api_key_id, provider_name, protocol, model, status,
                    status_code, input_tokens, output_tokens, cached_tokens,
                    cache_write_tokens, cost_cents, latency_ms, first_byte_latency_ms
             FROM request_log_metrics
             WHERE created_at >= ?1 AND created_at <= ?2",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;

        let mut total = MetricsAccumulator::default();
        let mut by_model = HashMap::<String, MetricsAccumulator>::new();
        let mut by_key = HashMap::<String, MetricsAccumulator>::new();
        let mut by_provider = HashMap::<String, MetricsAccumulator>::new();
        let mut by_protocol = HashMap::<String, MetricsAccumulator>::new();
        let mut by_status_code = HashMap::<String, MetricsAccumulator>::new();
        let mut timeline = BTreeMap::<String, MetricsAccumulator>::new();

        for row in rows {
            total.record(&row);
            by_model.entry(row.model.clone()).or_default().record(&row);
            by_key
                .entry(row.api_key_id.clone())
                .or_default()
                .record(&row);
            by_provider
                .entry(row.provider_name.clone())
                .or_default()
                .record(&row);
            by_protocol
                .entry(row.protocol.clone())
                .or_default()
                .record(&row);
            by_status_code
                .entry(row.status_code.to_string())
                .or_default()
                .record(&row);
            timeline
                .entry(time_bucket.key(&row.created_at))
                .or_default()
                .record(&row);
        }

        Ok(DashboardAnalytics {
            total: total.into_total_stats(),
            by_model: sorted_aggregate_rows(by_model),
            by_key: sorted_aggregate_rows(by_key),
            by_provider: sorted_aggregate_rows(by_provider),
            by_protocol: sorted_aggregate_rows(by_protocol),
            by_status_code: sorted_aggregate_rows(by_status_code),
            timeline: timeline
                .into_iter()
                .map(|(group_key, metrics)| metrics.into_aggregate_row(group_key))
                .collect(),
        })
    }

    // ── private helpers ────────────────────────────────────────────

    async fn aggregate_by_column(
        &self,
        column: &str,
        from: &str,
        to: &str,
    ) -> StoreResult<Vec<AggregateRow>> {
        let sql = format!(
            "SELECT
                {col} as group_key,
                COUNT(*) as request_count,
                COALESCE(SUM(CASE WHEN {success_condition} THEN 1 ELSE 0 END), 0) as success_count,
                COALESCE(CAST(SUM(CASE WHEN {success_condition} THEN 1 ELSE 0 END) AS REAL) / NULLIF(COUNT(*), 0), 0.0) as success_rate,
                COALESCE(SUM(CASE WHEN status <> 'success' THEN 1 ELSE 0 END), 0) as error_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(cached_tokens), 0) as total_cached_tokens,
                COALESCE(SUM(cache_write_tokens), 0) as total_cache_write_tokens,
                COALESCE(SUM(input_tokens + output_tokens), 0) as total_tokens,
                COALESCE(SUM(cost_cents), 0) as total_cost_cents,
                COALESCE(AVG(latency_ms), 0.0) as avg_latency_ms,
                COALESCE(AVG(NULLIF(first_byte_latency_ms, 0)), 0.0) as avg_first_byte_latency_ms
             FROM request_log_metrics
             WHERE created_at >= ?1 AND created_at <= ?2
             GROUP BY {col}
             ORDER BY total_cost_cents DESC",
            col = column,
            success_condition = SUCCESS_CONDITION,
        );

        Ok(sqlx::query_as::<_, AggregateRow>(&sql)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pool)
            .await?)
    }

    async fn aggregate_by_time(
        &self,
        time_expr: &str,
        from: &str,
        to: &str,
    ) -> StoreResult<Vec<AggregateRow>> {
        let sql = format!(
            "SELECT
                {expr} as group_key,
                COUNT(*) as request_count,
                COALESCE(SUM(CASE WHEN {success_condition} THEN 1 ELSE 0 END), 0) as success_count,
                COALESCE(CAST(SUM(CASE WHEN {success_condition} THEN 1 ELSE 0 END) AS REAL) / NULLIF(COUNT(*), 0), 0.0) as success_rate,
                COALESCE(SUM(CASE WHEN status <> 'success' THEN 1 ELSE 0 END), 0) as error_count,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(cached_tokens), 0) as total_cached_tokens,
                COALESCE(SUM(cache_write_tokens), 0) as total_cache_write_tokens,
                COALESCE(SUM(input_tokens + output_tokens), 0) as total_tokens,
                COALESCE(SUM(cost_cents), 0) as total_cost_cents,
                COALESCE(AVG(latency_ms), 0.0) as avg_latency_ms,
                COALESCE(AVG(NULLIF(first_byte_latency_ms, 0)), 0.0) as avg_first_byte_latency_ms
             FROM request_log_metrics
             WHERE created_at >= ?1 AND created_at <= ?2
             GROUP BY {expr}
             ORDER BY group_key ASC",
            expr = time_expr,
            success_condition = SUCCESS_CONDITION,
        );

        Ok(sqlx::query_as::<_, AggregateRow>(&sql)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pool)
            .await?)
    }
}

impl AnalyticsTimeBucket {
    fn key(self, created_at: &str) -> String {
        let length = match self {
            Self::Hour => 13,
            Self::Day => 10,
        };
        created_at.get(..length).unwrap_or(created_at).to_string()
    }
}

#[derive(sqlx::FromRow)]
struct DashboardMetricRow {
    created_at: String,
    api_key_id: String,
    provider_name: String,
    protocol: String,
    model: String,
    status: String,
    status_code: i64,
    input_tokens: i64,
    output_tokens: i64,
    cached_tokens: i64,
    cache_write_tokens: i64,
    cost_cents: i64,
    latency_ms: i64,
    first_byte_latency_ms: i64,
}

#[derive(Default)]
struct MetricsAccumulator {
    request_count: i64,
    success_count: i64,
    error_count: i64,
    total_input_tokens: i64,
    total_output_tokens: i64,
    total_cached_tokens: i64,
    total_cache_write_tokens: i64,
    total_cost_cents: i64,
    total_latency_ms: i64,
    max_latency_ms: i64,
    total_first_byte_latency_ms: i64,
    first_byte_count: i64,
    max_first_byte_latency_ms: i64,
}

impl MetricsAccumulator {
    fn record(&mut self, row: &DashboardMetricRow) {
        self.request_count += 1;
        if row.status == "success" {
            self.success_count += 1;
        } else {
            self.error_count += 1;
        }
        self.total_input_tokens += row.input_tokens;
        self.total_output_tokens += row.output_tokens;
        self.total_cached_tokens += row.cached_tokens;
        self.total_cache_write_tokens += row.cache_write_tokens;
        self.total_cost_cents += row.cost_cents;
        self.total_latency_ms += row.latency_ms;
        self.max_latency_ms = self.max_latency_ms.max(row.latency_ms);
        self.max_first_byte_latency_ms = self
            .max_first_byte_latency_ms
            .max(row.first_byte_latency_ms);
        if row.first_byte_latency_ms > 0 {
            self.total_first_byte_latency_ms += row.first_byte_latency_ms;
            self.first_byte_count += 1;
        }
    }

    fn into_total_stats(self) -> TotalStats {
        let total_tokens = self.total_input_tokens + self.total_output_tokens;
        TotalStats {
            request_count: self.request_count,
            success_count: self.success_count,
            success_rate: rate(self.success_count, self.request_count),
            total_input_tokens: self.total_input_tokens,
            total_output_tokens: self.total_output_tokens,
            total_cached_tokens: self.total_cached_tokens,
            total_cache_write_tokens: self.total_cache_write_tokens,
            cache_hit_rate: rate(self.total_cached_tokens, self.total_input_tokens),
            total_tokens,
            avg_tokens_per_request: average(total_tokens, self.request_count),
            total_cost_cents: self.total_cost_cents,
            avg_latency_ms: average(self.total_latency_ms, self.request_count),
            max_latency_ms: self.max_latency_ms,
            avg_first_byte_latency_ms: average(
                self.total_first_byte_latency_ms,
                self.first_byte_count,
            ),
            max_first_byte_latency_ms: self.max_first_byte_latency_ms,
            error_count: self.error_count,
        }
    }

    fn into_aggregate_row(self, group_key: String) -> AggregateRow {
        let total_tokens = self.total_input_tokens + self.total_output_tokens;
        AggregateRow {
            group_key,
            request_count: self.request_count,
            success_count: self.success_count,
            success_rate: rate(self.success_count, self.request_count),
            error_count: self.error_count,
            total_input_tokens: self.total_input_tokens,
            total_output_tokens: self.total_output_tokens,
            total_cached_tokens: self.total_cached_tokens,
            total_cache_write_tokens: self.total_cache_write_tokens,
            total_tokens,
            total_cost_cents: self.total_cost_cents,
            avg_latency_ms: average(self.total_latency_ms, self.request_count),
            avg_first_byte_latency_ms: average(
                self.total_first_byte_latency_ms,
                self.first_byte_count,
            ),
        }
    }
}

fn sorted_aggregate_rows(groups: HashMap<String, MetricsAccumulator>) -> Vec<AggregateRow> {
    let mut rows: Vec<_> = groups
        .into_iter()
        .map(|(group_key, metrics)| metrics.into_aggregate_row(group_key))
        .collect();
    rows.sort_by(|left, right| {
        right
            .total_cost_cents
            .cmp(&left.total_cost_cents)
            .then_with(|| left.group_key.cmp(&right.group_key))
    });
    rows
}

#[derive(sqlx::FromRow)]
struct TotalStatsRow {
    request_count: i64,
    success_count: i64,
    total_input_tokens: i64,
    total_output_tokens: i64,
    total_cached_tokens: i64,
    total_cache_write_tokens: i64,
    total_tokens: i64,
    avg_tokens_per_request: f64,
    total_cost_cents: i64,
    avg_latency_ms: f64,
    max_latency_ms: i64,
    avg_first_byte_latency_ms: f64,
    max_first_byte_latency_ms: i64,
    error_count: i64,
}

impl TotalStatsRow {
    fn into_total_stats(self) -> TotalStats {
        TotalStats {
            request_count: self.request_count,
            success_count: self.success_count,
            success_rate: rate(self.success_count, self.request_count),
            total_input_tokens: self.total_input_tokens,
            total_output_tokens: self.total_output_tokens,
            total_cached_tokens: self.total_cached_tokens,
            total_cache_write_tokens: self.total_cache_write_tokens,
            cache_hit_rate: rate(self.total_cached_tokens, self.total_input_tokens),
            total_tokens: self.total_tokens,
            avg_tokens_per_request: self.avg_tokens_per_request,
            total_cost_cents: self.total_cost_cents,
            avg_latency_ms: self.avg_latency_ms,
            max_latency_ms: self.max_latency_ms,
            avg_first_byte_latency_ms: self.avg_first_byte_latency_ms,
            max_first_byte_latency_ms: self.max_first_byte_latency_ms,
            error_count: self.error_count,
        }
    }
}

fn rate(part: i64, total: i64) -> f64 {
    if total > 0 {
        part as f64 / total as f64
    } else {
        0.0
    }
}

fn average(total: i64, count: i64) -> f64 {
    if count > 0 {
        total as f64 / count as f64
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::super::{NewRequestLog, Store};
    use super::AnalyticsTimeBucket;

    async fn seed_logs(store: &Store) {
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
            let metadata = if status_codes[i] >= 400 {
                serde_json::json!({
                    "error": {
                        "code": "error",
                        "message": "error",
                        "retryable": false
                    }
                })
                .to_string()
            } else {
                "{}".to_string()
            };
            store
                .insert_request_log_entry(NewRequestLog {
                    request_id: &format!("req-{}", i),
                    api_key_id: key_ids[i % 2],
                    session_hash: None,
                    provider_name: provider_names[i],
                    protocol: if provider_names[i].starts_with("openai") {
                        "completions"
                    } else {
                        "messages"
                    },
                    model: models[i],
                    operation: "completions",
                    status_code: status_codes[i],
                    success: status_codes[i] < 400,
                    input_tokens: 100,
                    output_tokens: 50,
                    total_tokens: 150,
                    cached_tokens: 20,
                    cache_write_tokens: 5,
                    cost_cents: 10,
                    latency_ms: 100 + (i as i64) * 10,
                    first_byte_latency_ms: 100 + (i as i64) * 10,
                    metadata_json: &metadata,
                    request_body: None,
                    response_body: None,
                })
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_aggregate_by_model() {
        let store = Store::open_in_memory().await.unwrap();
        seed_logs(&store).await;

        let rows = store
            .aggregate_by_model("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);

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

    #[tokio::test]
    async fn test_aggregate_by_provider() {
        let store = Store::open_in_memory().await.unwrap();
        seed_logs(&store).await;

        let rows = store
            .aggregate_by_provider("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[tokio::test]
    async fn test_aggregate_by_key() {
        let store = Store::open_in_memory().await.unwrap();
        seed_logs(&store).await;

        let rows = store
            .aggregate_by_key("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn test_total_stats() {
        let store = Store::open_in_memory().await.unwrap();
        seed_logs(&store).await;

        let stats = store
            .total_stats("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .await
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

    #[tokio::test]
    async fn test_dashboard_stats_uses_persisted_request_logs() {
        let store = Store::open_in_memory().await.unwrap();
        seed_logs(&store).await;

        let stats = store
            .dashboard_stats("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
            .await
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

    #[tokio::test]
    async fn test_dashboard_analytics_groups_from_metrics_table() {
        let store = Store::open_in_memory().await.unwrap();
        seed_logs(&store).await;

        let analytics = store
            .dashboard_analytics(
                "2000-01-01T00:00:00Z",
                "2099-12-31T23:59:59Z",
                AnalyticsTimeBucket::Day,
            )
            .await
            .unwrap();

        assert_eq!(analytics.total.request_count, 5);
        assert_eq!(analytics.total.total_cached_tokens, 100);
        assert_eq!(analytics.by_model.len(), 2);
        assert_eq!(analytics.by_key.len(), 2);
        assert_eq!(analytics.by_provider.len(), 3);
        assert_eq!(analytics.by_protocol.len(), 2);
        assert_eq!(analytics.by_status_code.len(), 2);
        assert_eq!(analytics.by_model[0].success_rate, 1.0);
        assert_eq!(analytics.timeline.len(), 1);
        assert_eq!(analytics.timeline[0].request_count, 5);
        assert_eq!(analytics.timeline[0].success_rate, 0.8);
    }

    #[tokio::test]
    async fn dashboard_analytics_query_uses_the_time_metrics_index() {
        let store = Store::open_in_memory().await.unwrap();
        seed_logs(&store).await;

        let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(
            "EXPLAIN QUERY PLAN
             SELECT created_at, api_key_id, provider_name, protocol, model, status,
                    status_code, input_tokens, output_tokens, cached_tokens,
                    cache_write_tokens, cost_cents, latency_ms, first_byte_latency_ms
             FROM request_log_metrics
             WHERE created_at >= '2000-01-01T00:00:00Z'
               AND created_at <= '2099-12-31T23:59:59Z'",
        )
        .fetch_all(&store.pool)
        .await
        .unwrap();

        assert!(plan.iter().any(|(_, _, _, detail)| {
            detail.contains("idx_request_log_metrics_analytics_created_at")
        }));
    }

    #[tokio::test]
    async fn test_aggregate_empty_range() {
        let store = Store::open_in_memory().await.unwrap();
        seed_logs(&store).await;

        let rows = store
            .aggregate_by_model("2000-01-01T00:00:00Z", "2000-01-02T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(rows.len(), 0);

        let stats = store
            .total_stats("2000-01-01T00:00:00Z", "2000-01-02T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(stats.request_count, 0);
    }
}
