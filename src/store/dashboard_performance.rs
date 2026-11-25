use serde::Serialize;
use std::collections::BTreeMap;

use super::analytics::DashboardMetricRow;

/// Observed completed-request metrics. Speed is an estimate based on first byte,
/// not an exact token-generation measurement; absent observations stay absent.
#[derive(Debug, Clone, Serialize)]
pub struct ObservedPerformance {
    pub success_count: i64,
    pub error_count: i64,
    pub first_byte_p50_ms: Option<i64>,
    pub first_byte_p95_ms: Option<i64>,
    pub first_byte_samples: usize,
    pub estimated_tps_p50: Option<f64>,
    pub estimated_tps_p10: Option<f64>,
    pub speed_samples: usize,
    pub latency_p95_ms: Option<i64>,
    pub retried_requests: usize,
    pub stream_errors: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardChannel {
    pub model: String,
    pub provider_name: String,
    pub request_count: usize,
    pub total_cost_cents: i64,
    pub performance: ObservedPerformance,
}

fn percentile<T: Copy>(sorted: &[T], percent: usize) -> Option<T> {
    sorted
        .get((sorted.len() * percent).div_ceil(100).saturating_sub(1))
        .copied()
}

impl ObservedPerformance {
    pub(super) fn from_rows<'a>(rows: impl IntoIterator<Item = &'a DashboardMetricRow>) -> Self {
        let mut first_bytes = Vec::new();
        let mut speeds = Vec::new();
        let mut latencies = Vec::new();
        let (mut success_count, mut error_count, mut retried_requests, mut stream_errors) =
            (0, 0, 0, 0);
        for row in rows {
            retried_requests += usize::from(row.retry_count > 0);
            let success = row.status == "success" && !row.has_error;
            if !success {
                error_count += 1;
                stream_errors += usize::from(row.is_stream == Some(true) && row.status_code < 400);
                continue;
            }
            success_count += 1;
            latencies.push(row.latency_ms);
            if row.first_byte_latency_ms > 0 {
                first_bytes.push(row.first_byte_latency_ms);
                if row.is_stream == Some(true)
                    && row.output_tokens > 0
                    && row.latency_ms > row.first_byte_latency_ms
                {
                    speeds.push(
                        row.output_tokens as f64 * 1_000.0
                            / (row.latency_ms - row.first_byte_latency_ms) as f64,
                    );
                }
            }
        }
        first_bytes.sort_unstable();
        latencies.sort_unstable();
        speeds.sort_unstable_by(f64::total_cmp);
        Self {
            success_count,
            error_count,
            first_byte_p50_ms: percentile(&first_bytes, 50),
            first_byte_p95_ms: percentile(&first_bytes, 95),
            first_byte_samples: first_bytes.len(),
            estimated_tps_p50: percentile(&speeds, 50),
            estimated_tps_p10: percentile(&speeds, 10),
            speed_samples: speeds.len(),
            latency_p95_ms: percentile(&latencies, 95),
            retried_requests,
            stream_errors,
        }
    }
}

pub(super) fn channels(rows: &[DashboardMetricRow]) -> Vec<DashboardChannel> {
    let mut groups = BTreeMap::<(&str, &str), Vec<&DashboardMetricRow>>::new();
    for row in rows {
        groups
            .entry((&row.model, &row.provider_name))
            .or_default()
            .push(row);
    }
    let mut result: Vec<_> = groups
        .into_iter()
        .map(|((model, provider_name), rows)| DashboardChannel {
            model: model.to_owned(),
            provider_name: provider_name.to_owned(),
            request_count: rows.len(),
            total_cost_cents: rows.iter().map(|row| row.cost_cents).sum(),
            performance: ObservedPerformance::from_rows(rows),
        })
        .collect();
    result.sort_by(|a, b| {
        b.request_count
            .cmp(&a.request_count)
            .then_with(|| a.model.cmp(&b.model))
            .then_with(|| a.provider_name.cmp(&b.provider_name))
    });
    result
}

#[cfg(test)]
mod tests {
    use crate::store::{AnalyticsTimeBucket, NewRequestLog, Store};

    #[tokio::test]
    async fn persisted_observations_exclude_unknown_nonstream_and_failed_speed_samples() {
        let store = Store::open_in_memory().await.unwrap();
        for (id, provider, status, latency, first_byte, metadata) in [
            (
                "fast",
                "a",
                true,
                1_500,
                500,
                r#"{"request":{"stream":true}}"#,
            ),
            (
                "slow",
                "a",
                true,
                3_500,
                1_500,
                r#"{"request":{"stream":true}}"#,
            ),
            (
                "nonstream",
                "a",
                true,
                900,
                400,
                r#"{"request":{"stream":false}}"#,
            ),
            ("unknown", "a", true, 900, 400, "{}"),
            (
                "zero-time",
                "a",
                true,
                400,
                400,
                r#"{"request":{"stream":true}}"#,
            ),
            (
                "stream-error",
                "b",
                true,
                1_000,
                100,
                r#"{"request":{"stream":true},"error":{"code":"stream_error"},"retry":{"retry_count":2}}"#,
            ),
            (
                "failed",
                "b",
                false,
                1_000,
                100,
                r#"{"request":{"stream":true}}"#,
            ),
        ] {
            store
                .insert_request_log_entry(NewRequestLog {
                    request_id: id,
                    api_key_id: "key",
                    session_hash: None,
                    provider_name: provider,
                    protocol: "responses",
                    model: "model",
                    operation: "responses",
                    status_code: if status { 200 } else { 500 },
                    success: status,
                    input_tokens: 100,
                    output_tokens: 100,
                    total_tokens: 200,
                    cached_tokens: 0,
                    cache_write_tokens: 0,
                    cost_cents: 5,
                    latency_ms: latency,
                    first_byte_latency_ms: first_byte,
                    metadata_json: metadata,
                    request_body: None,
                    response_body: None,
                })
                .await
                .unwrap();
        }
        let data = store
            .dashboard_analytics(
                "1970-01-01T00:00:00Z",
                "2099-01-01T00:00:00Z",
                AnalyticsTimeBucket::Minute,
            )
            .await
            .unwrap();
        assert_eq!(data.observed.success_count, 5);
        assert_eq!(data.observed.error_count, 2);
        assert_eq!(data.observed.speed_samples, 2);
        assert_eq!(data.observed.estimated_tps_p50, Some(50.0));
        assert_eq!(data.observed.estimated_tps_p10, Some(50.0));
        assert_eq!(data.observed.first_byte_p95_ms, Some(1_500));
        assert_eq!(data.observed.stream_errors, 1);
        assert_eq!(data.observed.retried_requests, 1);
        assert_eq!(data.model_channels.len(), 2);
        assert_eq!(data.model_channels[0].provider_name, "a");
        assert_eq!(data.model_channels[1].performance.estimated_tps_p50, None);
        let empty = store
            .dashboard_analytics(
                "2000-01-01T00:00:00Z",
                "2000-01-02T00:00:00Z",
                AnalyticsTimeBucket::Minute,
            )
            .await
            .unwrap();
        assert_eq!(empty.observed.estimated_tps_p50, None);
        assert_eq!(empty.observed.first_byte_p50_ms, None);
    }
}
