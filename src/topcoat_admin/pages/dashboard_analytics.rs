use topcoat::{context::Cx, router::page, view::View, Result};

use crate::store::models::{AggregateRow, TotalStats};
use crate::topcoat_admin::app::{app_state, require_admin};
use crate::topcoat_admin::trusted_html;

/// Simplified timeline bucket for JSON serialization.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimeBucket {
    pub label: String,
    pub request_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cached_tokens: i64,
    pub total_cache_write_tokens: i64,
    pub total_tokens: i64,
}

/// Dashboard analytics data for JSON response.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DashboardAnalyticsData {
    pub total: TotalStats,
    pub by_model: Vec<AggregateRow>,
    pub by_key: Vec<AggregateRow>,
    pub by_provider: Vec<AggregateRow>,
    pub by_protocol: Vec<AggregateRow>,
    pub by_status_code: Vec<AggregateRow>,
    pub timeline: Vec<TimeBucket>,
}

/// Returns dashboard analytics data as JSON for htmx/fetch requests.
#[page("/dashboard/analytics")]
pub async fn dashboard_analytics(cx: &Cx) -> Result {
    require_admin(cx)?;
    let state = app_state(cx);
    let from = "1970-01-01T00:00:00Z";
    let to = "9999-12-31T23:59:59Z";

    let mut analytics = match state
        .store
        .dashboard_analytics(from, to, crate::store::AnalyticsTimeBucket::Hour)
        .await
    {
        Ok(data) => data,
        Err(_) => return Ok(View::unescaped_unchecked("[]")),
    };
    let snapshot = state.config_service.snapshot();
    for row in &mut analytics.by_key {
        row.group_key = snapshot.auth_key_display_name(&row.group_key).to_string();
    }

    let timeline: Vec<TimeBucket> = analytics
        .timeline
        .into_iter()
        .map(|row| TimeBucket {
            label: row.group_key,
            request_count: row.request_count,
            success_count: row.success_count,
            error_count: row.error_count,
            total_input_tokens: row.total_input_tokens,
            total_output_tokens: row.total_output_tokens,
            total_cached_tokens: row.total_cached_tokens,
            total_cache_write_tokens: row.total_cache_write_tokens,
            total_tokens: row.total_tokens,
        })
        .collect();

    let data = DashboardAnalyticsData {
        total: analytics.total,
        by_model: analytics.by_model,
        by_key: analytics.by_key,
        by_provider: analytics.by_provider,
        by_protocol: analytics.by_protocol,
        by_status_code: analytics.by_status_code,
        timeline,
    };

    let json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    Ok(trusted_html(json))
}
