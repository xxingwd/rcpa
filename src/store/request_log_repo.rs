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

const REQUEST_LOG_SELECT_FIELDS: &str = r#"l.id, l.request_id, l.run_id, m.api_key_id, m.session_hash,
                    m.provider_name, m.protocol, m.model, l.operation, m.status, m.status_code,
                    CASE WHEN m.status = 'success' THEN 1 ELSE 0 END as success,
                    m.input_tokens, m.output_tokens,
                    m.input_tokens + m.output_tokens as total_tokens,
                    m.cached_tokens, m.cache_write_tokens,
                    m.cost_cents, m.latency_ms, m.first_byte_latency_ms, l.retry_count,
                    l.meta, json_extract(l.meta, '$.error.code') as error_code,
                    json_extract(l.meta, '$.error.message') as error, m.created_at, l.finished_at"#;

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

        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO request_logs (
                id, request_id, run_id, api_key_id, session_hash, provider_name, protocol,
                model, operation, status, status_code, retry_count, input_tokens, output_tokens,
                cost_cents, latency_ms, first_byte_latency_ms, meta, created_at,
                finished_at, request_body, response_body
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
        .bind(entry.cost_cents)
        .bind(entry.latency_ms)
        .bind(entry.first_byte_latency_ms)
        .bind(&meta)
        .bind(&now)
        .bind(&now)
        .bind(entry.request_body)
        .bind(entry.response_body)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r#"INSERT INTO request_log_metrics (
                id, api_key_id, session_hash, provider_name, protocol, model, status, status_code,
                input_tokens, output_tokens, cached_tokens, cache_write_tokens, cost_cents,
                latency_ms, first_byte_latency_ms, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(entry.api_key_id)
        .bind(entry.session_hash)
        .bind(entry.provider_name)
        .bind(entry.protocol)
        .bind(entry.model)
        .bind(status)
        .bind(entry.status_code)
        .bind(entry.input_tokens)
        .bind(entry.output_tokens)
        .bind(entry.cached_tokens)
        .bind(entry.cache_write_tokens)
        .bind(entry.cost_cents)
        .bind(entry.latency_ms)
        .bind(entry.first_byte_latency_ms)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;

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
             FROM request_log_metrics m
             JOIN request_logs l ON l.id = m.id
             WHERE 1 = 1",
            fields = REQUEST_LOG_SELECT_FIELDS
        ));
        append_request_log_filters(&mut query, filter);
        query
            .push(" ORDER BY m.created_at DESC LIMIT ")
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
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM request_log_metrics m WHERE 1 = 1");
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
             JOIN request_log_metrics m ON m.id = l.id
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
        query.push(" AND m.created_at >= ").push_bind(from.clone());
    }
    if let Some(to) = &filter.to {
        query.push(" AND m.created_at <= ").push_bind(to.clone());
    }
    if let Some(api_key_id) = &filter.api_key_id {
        query
            .push(" AND m.api_key_id = ")
            .push_bind(api_key_id.clone());
    }
    if let Some(session_hash) = &filter.session_hash {
        query
            .push(" AND m.session_hash = ")
            .push_bind(session_hash.clone());
    }
    if let Some(model) = &filter.model {
        query.push(" AND m.model = ").push_bind(model.clone());
    }
    if let Some(provider_name) = &filter.provider_name {
        query
            .push(" AND m.provider_name = ")
            .push_bind(provider_name.clone());
    }
    if let Some(protocol) = &filter.protocol {
        query.push(" AND m.protocol = ").push_bind(protocol.clone());
    }
    if let Some(status) = &filter.status {
        query.push(" AND m.status = ").push_bind(status.clone());
    }
    if let Some(status_code) = filter.status_code {
        query.push(" AND m.status_code = ").push_bind(status_code);
    }
    if let Some(success) = filter.success {
        query.push(if success == 1 {
            " AND m.status = 'success'"
        } else {
            " AND m.status <> 'success'"
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
}
