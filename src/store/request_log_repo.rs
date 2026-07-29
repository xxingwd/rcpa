use super::{models::RequestLogFilter, DbRequestLog, Store, StoreResult};
use chrono::Utc;
use sqlx::{QueryBuilder, Sqlite};
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

#[derive(Debug, Clone)]
pub struct NewRunningRequestLog<'a> {
    pub request_id: &'a str,
    pub api_key_id: &'a str,
    pub session_hash: Option<&'a str>,
    pub provider_name: &'a str,
    pub protocol: &'a str,
    pub model: &'a str,
    pub operation: &'a str,
    pub status_code: i64,
    pub retry_count: i64,
    pub first_byte_latency_ms: i64,
    pub metadata_json: &'a str,
    pub request_body: Option<&'a [u8]>,
}

#[derive(Debug, Clone)]
pub struct RequestLogProgress<'a> {
    pub provider_name: &'a str,
    pub protocol: &'a str,
    pub model: &'a str,
    pub status_code: i64,
    pub retry_count: i64,
    pub first_byte_latency_ms: i64,
    pub metadata_json: &'a str,
    pub request_body: Option<&'a [u8]>,
}

const REQUEST_LOG_SELECT_FIELDS: &str = r#"l.id, l.request_id, l.run_id, l.api_key_id, l.session_hash,
                    l.provider_name, l.protocol, l.model, l.operation, l.status, l.status_code,
                    CASE WHEN l.status = 'success' THEN 1 ELSE 0 END as success,
                    l.input_tokens, l.output_tokens,
                    l.input_tokens + l.output_tokens as total_tokens,
                    l.cached_tokens, l.cache_write_tokens,
                    l.cost_cents,
                    CASE WHEN l.status = 'running'
                        THEN MAX(0, CAST((julianday('now') - julianday(l.created_at)) * 86400000 AS INTEGER))
                        ELSE l.latency_ms
                    END as latency_ms,
                    l.first_byte_latency_ms, l.retry_count,
                    l.meta, json_extract(l.meta, '$.error.code') as error_code,
                    json_extract(l.meta, '$.error.message') as error, l.created_at, l.finished_at"#;

impl Store {
    pub async fn insert_request_log_entry(
        &self,
        entry: NewRequestLog<'_>,
    ) -> StoreResult<DbRequestLog> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let retry_count = retry_count_from_meta(entry.metadata_json);
        let status = if entry.success { "success" } else { "failed" };
        let meta = normalized_meta(
            entry.metadata_json,
            entry.cached_tokens,
            entry.cache_write_tokens,
        );

        sqlx::query(
            r#"INSERT INTO request_logs (
                id, request_id, run_id, api_key_id, session_hash, provider_name, protocol,
                model, operation, status, status_code, retry_count, input_tokens, output_tokens,
                cached_tokens, cache_write_tokens, cost_cents, latency_ms,
                first_byte_latency_ms, meta, created_at, updated_at, finished_at,
                request_body, response_body
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(entry.request_id)
        .bind(entry.request_id)
        .bind(entry.api_key_id)
        .bind(entry.session_hash)
        .bind(entry.provider_name)
        .bind(entry.protocol)
        .bind(entry.model)
        .bind(entry.operation)
        .bind(status)
        .bind(entry.status_code)
        .bind(retry_count)
        .bind(entry.input_tokens)
        .bind(entry.output_tokens)
        .bind(entry.cached_tokens)
        .bind(entry.cache_write_tokens)
        .bind(entry.cost_cents)
        .bind(entry.latency_ms)
        .bind(entry.first_byte_latency_ms)
        .bind(&meta)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(entry.request_body)
        .bind(entry.response_body)
        .execute(&self.pool)
        .await?;

        Ok(DbRequestLog {
            id,
            request_id: entry.request_id.to_string(),
            run_id: entry.request_id.to_string(),
            api_key_id: entry.api_key_id.to_string(),
            session_hash: entry.session_hash.map(ToString::to_string),
            provider_name: entry.provider_name.to_string(),
            protocol: entry.protocol.to_string(),
            model: entry.model.to_string(),
            operation: entry.operation.to_string(),
            status: status.to_string(),
            status_code: entry.status_code,
            success: i64::from(entry.success),
            input_tokens: entry.input_tokens,
            output_tokens: entry.output_tokens,
            total_tokens: entry.input_tokens + entry.output_tokens,
            cached_tokens: entry.cached_tokens,
            cache_write_tokens: entry.cache_write_tokens,
            cost_cents: entry.cost_cents,
            latency_ms: entry.latency_ms,
            first_byte_latency_ms: entry.first_byte_latency_ms,
            retry_count,
            meta: meta.clone(),
            error_code: metadata_error_field(&meta, "code"),
            error: metadata_error_field(&meta, "message"),
            created_at: now.clone(),
            finished_at: Some(now),
            request_body: None,
            response_body: None,
        })
    }

    pub async fn begin_request_log(&self, entry: NewRunningRequestLog<'_>) -> StoreResult<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let meta = normalized_meta(entry.metadata_json, 0, 0);

        sqlx::query(
            r#"INSERT INTO request_logs (
                id, request_id, run_id, api_key_id, session_hash, provider_name, protocol,
                model, operation, status, status_code, retry_count, input_tokens, output_tokens,
                cached_tokens, cache_write_tokens, cost_cents, latency_ms,
                first_byte_latency_ms, meta, created_at, updated_at, finished_at,
                request_body, response_body
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'running', ?, ?, 0, 0, 0, 0, 0, 0, ?, ?, ?, ?, NULL, ?, NULL)"#,
        )
        .bind(&id)
        .bind(entry.request_id)
        .bind(entry.request_id)
        .bind(entry.api_key_id)
        .bind(entry.session_hash)
        .bind(entry.provider_name)
        .bind(entry.protocol)
        .bind(entry.model)
        .bind(entry.operation)
        .bind(entry.status_code)
        .bind(entry.retry_count)
        .bind(entry.first_byte_latency_ms)
        .bind(meta)
        .bind(&now)
        .bind(&now)
        .bind(entry.request_body)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn update_request_log_progress(
        &self,
        id: &str,
        progress: RequestLogProgress<'_>,
    ) -> StoreResult<bool> {
        let now = Utc::now().to_rfc3339();
        let meta = normalized_meta(progress.metadata_json, 0, 0);
        let result = sqlx::query(
            r#"UPDATE request_logs
               SET provider_name = ?, protocol = ?, model = ?, status_code = ?,
                   retry_count = ?, first_byte_latency_ms = ?, meta = ?, updated_at = ?,
                   request_body = COALESCE(?, request_body)
               WHERE id = ? AND status = 'running'"#,
        )
        .bind(progress.provider_name)
        .bind(progress.protocol)
        .bind(progress.model)
        .bind(progress.status_code)
        .bind(progress.retry_count)
        .bind(progress.first_byte_latency_ms)
        .bind(meta)
        .bind(now)
        .bind(progress.request_body)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn complete_request_log(
        &self,
        id: &str,
        entry: NewRequestLog<'_>,
    ) -> StoreResult<bool> {
        let now = Utc::now().to_rfc3339();
        let retry_count = retry_count_from_meta(entry.metadata_json);
        let status = if entry.success { "success" } else { "failed" };
        let meta = normalized_meta(
            entry.metadata_json,
            entry.cached_tokens,
            entry.cache_write_tokens,
        );
        let result = sqlx::query(
            r#"UPDATE request_logs
               SET api_key_id = ?, session_hash = ?, provider_name = ?, protocol = ?,
                   model = ?, operation = ?, status = ?, status_code = ?, retry_count = ?,
                   input_tokens = ?, output_tokens = ?, cached_tokens = ?,
                   cache_write_tokens = ?, cost_cents = ?, latency_ms = ?,
                   first_byte_latency_ms = ?, meta = ?, updated_at = ?, finished_at = ?,
                   request_body = COALESCE(?, request_body), response_body = ?
               WHERE id = ? AND status = 'running'"#,
        )
        .bind(entry.api_key_id)
        .bind(entry.session_hash)
        .bind(entry.provider_name)
        .bind(entry.protocol)
        .bind(entry.model)
        .bind(entry.operation)
        .bind(status)
        .bind(entry.status_code)
        .bind(retry_count)
        .bind(entry.input_tokens)
        .bind(entry.output_tokens)
        .bind(entry.cached_tokens)
        .bind(entry.cache_write_tokens)
        .bind(entry.cost_cents)
        .bind(entry.latency_ms)
        .bind(entry.first_byte_latency_ms)
        .bind(meta)
        .bind(&now)
        .bind(&now)
        .bind(entry.request_body)
        .bind(entry.response_body)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn interrupt_running_request_logs(&self) -> StoreResult<u64> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"UPDATE request_logs
               SET status = 'interrupted',
                   latency_ms = MAX(0, CAST((julianday(?) - julianday(created_at)) * 86400000 AS INTEGER)),
                   meta = json_set(meta,
                       '$.error.code', 'gateway_restarted',
                       '$.error.message', 'Gateway restarted before the request completed',
                       '$.error.retryable', 0
                   ),
                   updated_at = ?, finished_at = ?
               WHERE status = 'running'"#,
        )
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Query request logs with optional filters. Body BLOBs are excluded
    /// from list results — use `get_request_log_detail` for full records.
    pub async fn query_request_logs(
        &self,
        filter: &RequestLogFilter,
    ) -> StoreResult<Vec<DbRequestLog>> {
        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        let mut query = QueryBuilder::<Sqlite>::new(format!(
            "SELECT {fields}
             FROM request_logs l
             WHERE 1 = 1",
            fields = REQUEST_LOG_SELECT_FIELDS
        ));
        append_request_log_filters(&mut query, filter);
        query
            .push(" ORDER BY l.created_at DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        let logs = query
            .build_query_as::<DbRequestLog>()
            .fetch_all(&self.pool)
            .await?;

        Ok(logs)
    }

    /// Count request logs matching the same filters used by `query_request_logs`.
    pub async fn count_request_logs(&self, filter: &RequestLogFilter) -> StoreResult<i64> {
        let mut query =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM request_logs l WHERE 1 = 1");
        append_request_log_filters(&mut query, filter);
        let count = query
            .build_query_scalar::<i64>()
            .fetch_one(&self.pool)
            .await?;

        Ok(count)
    }

    /// Fetch a single log entry including body BLOBs.
    pub async fn get_request_log_detail(&self, id: &str) -> StoreResult<Option<DbRequestLog>> {
        let log = sqlx::query_as::<_, DbRequestLog>(&format!(
            "SELECT {fields},
                    request_body, response_body
             FROM request_logs l
             WHERE l.id = ?
             LIMIT 1",
            fields = REQUEST_LOG_SELECT_FIELDS
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(log.map(with_metadata_error_fields))
    }
}

fn append_request_log_filters(query: &mut QueryBuilder<'_, Sqlite>, filter: &RequestLogFilter) {
    if let Some(from) = &filter.from {
        query.push(" AND l.created_at >= ").push_bind(from.clone());
    }
    if let Some(to) = &filter.to {
        query.push(" AND l.created_at <= ").push_bind(to.clone());
    }
    if let Some(api_key_id) = &filter.api_key_id {
        query
            .push(" AND l.api_key_id = ")
            .push_bind(api_key_id.clone());
    }
    if let Some(session_hash) = &filter.session_hash {
        query
            .push(" AND l.session_hash = ")
            .push_bind(session_hash.clone());
    }
    if let Some(model) = &filter.model {
        query.push(" AND l.model = ").push_bind(model.clone());
    }
    if let Some(provider_name) = &filter.provider_name {
        query
            .push(" AND l.provider_name = ")
            .push_bind(provider_name.clone());
    }
    if let Some(protocol) = &filter.protocol {
        query.push(" AND l.protocol = ").push_bind(protocol.clone());
    }
    if let Some(status) = &filter.status {
        query.push(" AND l.status = ").push_bind(status.clone());
    }
    if let Some(status_code) = filter.status_code {
        query.push(" AND l.status_code = ").push_bind(status_code);
    }
    if let Some(success) = filter.success {
        query.push(if success == 1 {
            " AND l.status = 'success'"
        } else {
            " AND l.status <> 'success' AND l.status <> 'running'"
        });
    }
}

fn metadata_error_field(meta: &str, field: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(meta)
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
    log.error_code = metadata_error_field(&log.meta, "code");
    log.error = metadata_error_field(&log.meta, "message");
    log
}

fn retry_count_from_meta(meta: &str) -> i64 {
    serde_json::from_str::<serde_json::Value>(meta)
        .ok()
        .and_then(|value| value.pointer("/retry/retry_count").and_then(|v| v.as_i64()))
        .unwrap_or(0)
}

fn normalized_meta(meta: &str, cached_tokens: i64, cache_write_tokens: i64) -> String {
    let base = serde_json::from_str::<serde_json::Value>(meta)
        .unwrap_or_else(|_| serde_json::json!({ "legacy_meta": meta }));
    let mut object = match base {
        serde_json::Value::Object(map) => map,
        other => serde_json::Map::from_iter([("legacy_meta".to_string(), other)]),
    };

    let usage = object
        .remove("usage")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let mut usage = usage;
    usage.insert(
        "cached_tokens".to_string(),
        serde_json::Value::Number(cached_tokens.into()),
    );
    usage.insert(
        "cache_write_tokens".to_string(),
        serde_json::Value::Number(cache_write_tokens.into()),
    );
    object.insert("usage".to_string(), serde_json::Value::Object(usage));
    serde_json::Value::Object(object).to_string()
}

#[cfg(test)]
mod tests {
    use super::super::{
        models::RequestLogFilter, NewRequestLog, NewRunningRequestLog, RequestLogProgress, Store,
    };

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
                operation: "completions",
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

    #[tokio::test]
    async fn running_request_is_visible_and_completion_updates_the_same_row() {
        let store = Store::open_in_memory().await.unwrap();
        let log_id = store
            .begin_request_log(NewRunningRequestLog {
                request_id: "req-running",
                api_key_id: "key-running",
                session_hash: None,
                provider_name: "provider-a",
                protocol: "responses",
                model: "model-a",
                operation: "responses",
                status_code: 0,
                retry_count: 0,
                first_byte_latency_ms: 0,
                metadata_json: "{}",
                request_body: Some(br#"{"model":"model-a"}"#),
            })
            .await
            .unwrap();

        let running = store
            .query_request_logs(&RequestLogFilter::default())
            .await
            .unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, log_id);
        assert_eq!(running[0].status, "running");
        assert!(running[0].finished_at.is_none());
        assert_eq!(
            store
                .total_stats("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
                .await
                .unwrap()
                .request_count,
            0
        );

        let retry_meta = serde_json::json!({
            "error": {"code": "rate_limit", "message": "retry later"},
            "retry": {"retry_count": 1}
        })
        .to_string();
        assert!(store
            .update_request_log_progress(
                &log_id,
                RequestLogProgress {
                    provider_name: "provider-b",
                    protocol: "responses",
                    model: "model-b",
                    status_code: 429,
                    retry_count: 1,
                    first_byte_latency_ms: 0,
                    metadata_json: &retry_meta,
                    request_body: None,
                },
            )
            .await
            .unwrap());

        let retrying = store
            .get_request_log_detail(&log_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrying.status, "running");
        assert_eq!(retrying.provider_name, "provider-b");
        assert_eq!(retrying.retry_count, 1);
        assert_eq!(retrying.error_code.as_deref(), Some("rate_limit"));

        let final_meta = serde_json::json!({"retry": {"retry_count": 1}}).to_string();
        let final_entry = NewRequestLog {
            request_id: "req-running",
            api_key_id: "key-running",
            session_hash: None,
            provider_name: "provider-b",
            protocol: "responses",
            model: "model-b",
            operation: "responses",
            status_code: 200,
            success: true,
            input_tokens: 12,
            output_tokens: 8,
            total_tokens: 20,
            cached_tokens: 4,
            cache_write_tokens: 1,
            cost_cents: 3,
            latency_ms: 250,
            first_byte_latency_ms: 80,
            metadata_json: &final_meta,
            request_body: None,
            response_body: Some(br#"{"ok":true}"#),
        };
        assert!(store
            .complete_request_log(&log_id, final_entry.clone())
            .await
            .unwrap());
        assert!(!store
            .complete_request_log(&log_id, final_entry)
            .await
            .unwrap());

        let completed = store
            .get_request_log_detail(&log_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(completed.status, "success");
        assert_eq!(completed.cached_tokens, 4);
        assert!(completed.finished_at.is_some());
        assert_eq!(
            store
                .total_stats("2000-01-01T00:00:00Z", "2099-12-31T23:59:59Z")
                .await
                .unwrap()
                .request_count,
            1
        );
        let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_logs")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(row_count, 1);
    }

    #[tokio::test]
    async fn startup_recovery_marks_running_requests_interrupted() {
        let store = Store::open_in_memory().await.unwrap();
        let log_id = store
            .begin_request_log(NewRunningRequestLog {
                request_id: "req-interrupted",
                api_key_id: "key",
                session_hash: None,
                provider_name: "provider",
                protocol: "messages",
                model: "model",
                operation: "messages",
                status_code: 0,
                retry_count: 0,
                first_byte_latency_ms: 0,
                metadata_json: "{}",
                request_body: None,
            })
            .await
            .unwrap();

        assert_eq!(store.interrupt_running_request_logs().await.unwrap(), 1);
        let interrupted = store
            .get_request_log_detail(&log_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(interrupted.status, "interrupted");
        assert_eq!(interrupted.error_code.as_deref(), Some("gateway_restarted"));
        assert!(interrupted.finished_at.is_some());
    }
}
