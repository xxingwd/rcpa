use super::{models::RequestLogFilter, DbRequestLog, Store, StoreResult};
use chrono::Utc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewRequestLog<'a> {
    pub request_id: &'a str,
    pub api_key_id: &'a str,
    pub session_hash: Option<&'a str>,
    pub provider_name: &'a str,
    pub protocol: &'a str,
    pub model: &'a str,
    pub operation: &'a str,
    pub status_code: i64,
    pub success: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_cents: i64,
    pub latency_ms: i64,
    pub first_byte_latency_ms: i64,
    pub metadata_json: &'a str,
    pub request_body: Option<&'a [u8]>,
    pub response_body: Option<&'a [u8]>,
}

impl Store {
    pub async fn insert_request_log_entry(
        &self,
        entry: NewRequestLog<'_>,
    ) -> StoreResult<DbRequestLog> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"INSERT INTO request_logs (
                id, request_id, api_key_id, session_hash, provider_name, protocol,
                model, operation, status_code, success, input_tokens, output_tokens,
                total_tokens, cached_tokens, cache_write_tokens, cost_cents,
                latency_ms, first_byte_latency_ms, metadata_json, created_at,
                request_body, response_body
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(entry.request_id)
        .bind(entry.api_key_id)
        .bind(entry.session_hash)
        .bind(entry.provider_name)
        .bind(entry.protocol)
        .bind(entry.model)
        .bind(entry.operation)
        .bind(entry.status_code)
        .bind(i64::from(entry.success))
        .bind(entry.input_tokens)
        .bind(entry.output_tokens)
        .bind(entry.total_tokens)
        .bind(entry.cached_tokens)
        .bind(entry.cache_write_tokens)
        .bind(entry.cost_cents)
        .bind(entry.latency_ms)
        .bind(entry.first_byte_latency_ms)
        .bind(entry.metadata_json)
        .bind(&now)
        .bind(entry.request_body)
        .bind(entry.response_body)
        .execute(&self.pool)
        .await?;

        Ok(DbRequestLog {
            id,
            request_id: entry.request_id.to_string(),
            api_key_id: entry.api_key_id.to_string(),
            session_hash: entry.session_hash.map(ToString::to_string),
            provider_name: entry.provider_name.to_string(),
            protocol: entry.protocol.to_string(),
            model: entry.model.to_string(),
            operation: entry.operation.to_string(),
            status_code: entry.status_code,
            success: i64::from(entry.success),
            input_tokens: entry.input_tokens,
            output_tokens: entry.output_tokens,
            total_tokens: entry.total_tokens,
            cached_tokens: entry.cached_tokens,
            cache_write_tokens: entry.cache_write_tokens,
            cost_cents: entry.cost_cents,
            latency_ms: entry.latency_ms,
            first_byte_latency_ms: entry.first_byte_latency_ms,
            metadata_json: entry.metadata_json.to_string(),
            error_code: metadata_error_field(entry.metadata_json, "code"),
            error: metadata_error_field(entry.metadata_json, "message"),
            created_at: now,
            request_body: None,
            response_body: None,
        })
    }

    /// Query request logs with optional filters. Body BLOBs are excluded
    /// from list results — use `get_request_log_detail` for full records.
    pub async fn query_request_logs(
        &self,
        filter: &RequestLogFilter,
    ) -> StoreResult<Vec<DbRequestLog>> {
        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        let logs = sqlx::query_as::<_, DbRequestLog>(
            "SELECT id, request_id, api_key_id, session_hash, provider_name, protocol,
                    model, operation, status_code, success, input_tokens, output_tokens,
                    total_tokens, cached_tokens, cache_write_tokens, cost_cents,
                    latency_ms, first_byte_latency_ms, '' as metadata_json,
                    json_extract(metadata_json, '$.error.code') as error_code,
                    json_extract(metadata_json, '$.error.message') as error,
                    created_at
             FROM request_logs
             WHERE (created_at >= ? OR ? IS NULL)
               AND (created_at <= ? OR ? IS NULL)
               AND (api_key_id = ? OR ? IS NULL)
               AND (session_hash = ? OR ? IS NULL)
               AND (model = ? OR ? IS NULL)
               AND (provider_name = ? OR ? IS NULL)
               AND (protocol = ? OR ? IS NULL)
               AND (status_code = ? OR ? IS NULL)
               AND (success = ? OR ? IS NULL)
             ORDER BY created_at DESC
             LIMIT ? OFFSET ?",
        )
        .bind(&filter.from)
        .bind(&filter.from)
        .bind(&filter.to)
        .bind(&filter.to)
        .bind(&filter.api_key_id)
        .bind(&filter.api_key_id)
        .bind(&filter.session_hash)
        .bind(&filter.session_hash)
        .bind(&filter.model)
        .bind(&filter.model)
        .bind(&filter.provider_name)
        .bind(&filter.provider_name)
        .bind(&filter.protocol)
        .bind(&filter.protocol)
        .bind(filter.status_code)
        .bind(filter.status_code)
        .bind(filter.success)
        .bind(filter.success)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(logs)
    }

    /// Count request logs matching the same filters used by `query_request_logs`.
    pub async fn count_request_logs(&self, filter: &RequestLogFilter) -> StoreResult<i64> {
        let count = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM request_logs
             WHERE (created_at >= ? OR ? IS NULL)
               AND (created_at <= ? OR ? IS NULL)
               AND (api_key_id = ? OR ? IS NULL)
               AND (session_hash = ? OR ? IS NULL)
               AND (model = ? OR ? IS NULL)
               AND (provider_name = ? OR ? IS NULL)
               AND (protocol = ? OR ? IS NULL)
               AND (status_code = ? OR ? IS NULL)
               AND (success = ? OR ? IS NULL)",
        )
        .bind(&filter.from)
        .bind(&filter.from)
        .bind(&filter.to)
        .bind(&filter.to)
        .bind(&filter.api_key_id)
        .bind(&filter.api_key_id)
        .bind(&filter.session_hash)
        .bind(&filter.session_hash)
        .bind(&filter.model)
        .bind(&filter.model)
        .bind(&filter.provider_name)
        .bind(&filter.provider_name)
        .bind(&filter.protocol)
        .bind(&filter.protocol)
        .bind(filter.status_code)
        .bind(filter.status_code)
        .bind(filter.success)
        .bind(filter.success)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// Fetch a single log entry including body BLOBs.
    pub async fn get_request_log_detail(&self, id: &str) -> StoreResult<Option<DbRequestLog>> {
        let log = sqlx::query_as::<_, DbRequestLog>(
            "SELECT id, request_id, api_key_id, session_hash, provider_name, protocol,
                    model, operation, status_code, success, input_tokens, output_tokens,
                    total_tokens, cached_tokens, cache_write_tokens, cost_cents,
                    latency_ms, first_byte_latency_ms, metadata_json, created_at,
                    request_body, response_body
             FROM request_logs
             WHERE id = ?
             LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(log.map(with_metadata_error_fields))
    }
}

fn metadata_error_field(metadata_json: &str, field: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(metadata_json)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get(field))
                .cloned()
        })
        .and_then(|value| match value {
            serde_json::Value::String(value) => Some(value),
            serde_json::Value::Number(value) => Some(value.to_string()),
            serde_json::Value::Bool(value) => Some(value.to_string()),
            _ => None,
        })
}

fn with_metadata_error_fields(mut log: DbRequestLog) -> DbRequestLog {
    log.error_code = metadata_error_field(&log.metadata_json, "code");
    log.error = metadata_error_field(&log.metadata_json, "message");
    log
}

#[cfg(test)]
mod tests {
    use super::super::{models::RequestLogFilter, NewRequestLog, Store};

    fn metadata(error_code: Option<&str>, error: Option<&str>) -> String {
        serde_json::json!({
            "error": error_code.or(error).map(|_| serde_json::json!({
                "code": error_code,
                "message": error,
                "retryable": false
            }))
        })
        .to_string()
    }

    async fn insert_sample_logs(store: &Store) {
        let key_id = "request-log-key";
        for i in 0i64..5 {
            let req_body = format!(
                r#"{{"model":"gpt-4","messages":[{{"role":"user","content":"msg-{}"}}]}}"#,
                i
            );
            let res_body = format!(
                r#"{{"choices":[{{"message":{{"content":"reply-{}"}}}}]}}"#,
                i
            );
            let metadata = metadata(None, None);
            store
                .insert_request_log_entry(NewRequestLog {
                    request_id: &format!("req-{}", i),
                    api_key_id: key_id,
                    session_hash: None,
                    provider_name: "openai-1",
                    protocol: "completions",
                    model: "gpt-4",
                    operation: "completions",
                    status_code: 200,
                    success: true,
                    input_tokens: 100 + i,
                    output_tokens: 50 + i,
                    total_tokens: 150 + 2 * i,
                    cached_tokens: 0,
                    cache_write_tokens: 0,
                    cost_cents: 10 + i,
                    latency_ms: 100 + i * 10,
                    first_byte_latency_ms: 100 + i * 10,
                    metadata_json: &metadata,
                    request_body: Some(req_body.as_bytes()),
                    response_body: Some(res_body.as_bytes()),
                })
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_insert_and_query() {
        let store = Store::open_in_memory().await.unwrap();
        insert_sample_logs(&store).await;

        let logs = store
            .query_request_logs(&RequestLogFilter::default())
            .await
            .unwrap();
        assert_eq!(logs.len(), 5);

        for log in &logs {
            assert!(log.request_body.is_none());
            assert!(log.response_body.is_none());
        }

        let logs = store
            .query_request_logs(&RequestLogFilter {
                model: Some("gpt-4".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(logs.len(), 5);

        let logs = store
            .query_request_logs(&RequestLogFilter {
                provider_name: Some("openai-1".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(logs.len(), 5);

        let logs = store
            .query_request_logs(&RequestLogFilter {
                model: Some("claude-3".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(logs.len(), 0);

        let logs = store
            .query_request_logs(&RequestLogFilter {
                limit: Some(2),
                offset: Some(0),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(logs.len(), 2);
    }

    #[tokio::test]
    async fn test_get_request_log_detail() {
        let store = Store::open_in_memory().await.unwrap();
        insert_sample_logs(&store).await;

        let logs = store
            .query_request_logs(&RequestLogFilter {
                limit: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(!logs.is_empty());
        let log_id = &logs[0].id;

        let detail = store.get_request_log_detail(log_id).await.unwrap().unwrap();
        assert_eq!(detail.id, *log_id);
        assert!(detail.request_body.is_some());
        assert!(detail.response_body.is_some());

        let req_str = String::from_utf8(detail.request_body.unwrap()).unwrap();
        assert!(req_str.contains("gpt-4"));

        let res_str = String::from_utf8(detail.response_body.unwrap()).unwrap();
        assert!(res_str.contains("reply-"));

        let missing = store
            .get_request_log_detail("does-not-exist")
            .await
            .unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_insert_request_log_entry_stores_first_byte_error_code_and_allows_no_response_body(
    ) {
        let store = Store::open_in_memory().await.unwrap();
        let metadata = metadata(Some("rate_limit_exceeded"), Some("too many requests"));

        let entry = store
            .insert_request_log_entry(NewRequestLog {
                request_id: "req-error",
                api_key_id: "request-log-entry-key",
                session_hash: Some("session-hash-a"),
                provider_name: "openai-1",
                protocol: "completions",
                model: "gpt-4",
                operation: "chat_completions",
                status_code: 429,
                success: false,
                input_tokens: 1,
                output_tokens: 2,
                total_tokens: 3,
                cached_tokens: 1,
                cache_write_tokens: 0,
                cost_cents: 4,
                latency_ms: 250,
                first_byte_latency_ms: 80,
                metadata_json: &metadata,
                request_body: Some(br#"{"model":"gpt-4"}"#),
                response_body: None,
            })
            .await
            .unwrap();

        assert_eq!(entry.first_byte_latency_ms, 80);
        assert_eq!(entry.error_code.as_deref(), Some("rate_limit_exceeded"));
        assert_eq!(entry.session_hash.as_deref(), Some("session-hash-a"));

        let detail = store
            .get_request_log_detail(&entry.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.first_byte_latency_ms, 80);
        assert_eq!(detail.error_code.as_deref(), Some("rate_limit_exceeded"));
        assert_eq!(detail.error.as_deref(), Some("too many requests"));
        assert_eq!(detail.cached_tokens, 1);
        assert_eq!(detail.cache_write_tokens, 0);
        assert!(detail.request_body.is_some());
        assert!(detail.response_body.is_none());
    }
}
