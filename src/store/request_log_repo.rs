use super::{models::RequestLogFilter, DbRequestLog, Store, StoreResult};
use chrono::Utc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewRequestLog<'a> {
    pub request_id: &'a str,
    pub api_key_id: &'a str,
    pub provider_name: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub operation: &'a str,
    pub status_code: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_cents: i64,
    pub latency_ms: i64,
    pub first_byte_latency_ms: i64,
    pub error_code: Option<&'a str>,
    pub error: Option<&'a str>,
    pub request_body: Option<&'a [u8]>,
    pub response_body: Option<&'a [u8]>,
}

impl Store {
    /// Insert a new request log entry.
    ///
    /// `request_body` and `response_body` are stored as BLOBs and are only
    /// returned by `get_request_log_detail`, not by `query_request_logs`.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_request_log(
        &self,
        request_id: &str,
        api_key_id: &str,
        provider_name: &str,
        provider: &str,
        model: &str,
        operation: &str,
        status_code: i64,
        input_tokens: i64,
        output_tokens: i64,
        total_tokens: i64,
        cost_cents: i64,
        latency_ms: i64,
        error: Option<&str>,
        request_body: Option<&[u8]>,
        response_body: Option<&[u8]>,
    ) -> StoreResult<DbRequestLog> {
        self.insert_request_log_entry(NewRequestLog {
            request_id,
            api_key_id,
            provider_name,
            provider,
            model,
            operation,
            status_code,
            input_tokens,
            output_tokens,
            total_tokens,
            cached_tokens: 0,
            cache_write_tokens: 0,
            cost_cents,
            latency_ms,
            first_byte_latency_ms: latency_ms,
            error_code: None,
            error,
            request_body,
            response_body,
        })
    }

    pub fn insert_request_log_entry(&self, entry: NewRequestLog<'_>) -> StoreResult<DbRequestLog> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO request_logs (
                id, request_id, api_key_id, provider_name, provider,
                model, operation, status_code, input_tokens, output_tokens,
                total_tokens, cached_tokens, cache_write_tokens, cost_cents,
                latency_ms, first_byte_latency_ms, error_code, error, created_at,
                request_body, response_body
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            rusqlite::params![
                id,
                entry.request_id,
                entry.api_key_id,
                entry.provider_name,
                entry.provider,
                entry.model,
                entry.operation,
                entry.status_code,
                entry.input_tokens,
                entry.output_tokens,
                entry.total_tokens,
                entry.cached_tokens,
                entry.cache_write_tokens,
                entry.cost_cents,
                entry.latency_ms,
                entry.first_byte_latency_ms,
                entry.error_code,
                entry.error,
                now,
                entry.request_body,
                entry.response_body,
            ],
        )?;

        Ok(DbRequestLog {
            id,
            request_id: entry.request_id.to_string(),
            api_key_id: entry.api_key_id.to_string(),
            provider_name: entry.provider_name.to_string(),
            provider: entry.provider.to_string(),
            model: entry.model.to_string(),
            operation: entry.operation.to_string(),
            status_code: entry.status_code,
            input_tokens: entry.input_tokens,
            output_tokens: entry.output_tokens,
            total_tokens: entry.total_tokens,
            cached_tokens: entry.cached_tokens,
            cache_write_tokens: entry.cache_write_tokens,
            cost_cents: entry.cost_cents,
            latency_ms: entry.latency_ms,
            first_byte_latency_ms: entry.first_byte_latency_ms,
            error_code: entry.error_code.map(|s| s.to_string()),
            error: entry.error.map(|s| s.to_string()),
            created_at: now,
            request_body: None,
            response_body: None,
        })
    }

    fn request_log_filter_sql(
        filter: &RequestLogFilter,
    ) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>, usize) {
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref from) = filter.from {
            conditions.push(format!("created_at >= ?{}", param_idx));
            params.push(Box::new(from.clone()));
            param_idx += 1;
        }
        if let Some(ref to) = filter.to {
            conditions.push(format!("created_at <= ?{}", param_idx));
            params.push(Box::new(to.clone()));
            param_idx += 1;
        }
        if let Some(ref api_key_id) = filter.api_key_id {
            conditions.push(format!("api_key_id = ?{}", param_idx));
            params.push(Box::new(api_key_id.clone()));
            param_idx += 1;
        }
        if let Some(ref model) = filter.model {
            conditions.push(format!("model = ?{}", param_idx));
            params.push(Box::new(model.clone()));
            param_idx += 1;
        }
        if let Some(ref provider_name) = filter.provider_name {
            conditions.push(format!("provider_name = ?{}", param_idx));
            params.push(Box::new(provider_name.clone()));
            param_idx += 1;
        }
        if let Some(ref provider) = filter.provider {
            conditions.push(format!("provider = ?{}", param_idx));
            params.push(Box::new(provider.clone()));
            param_idx += 1;
        }
        if let Some(status_code) = filter.status_code {
            conditions.push(format!("status_code = ?{}", param_idx));
            params.push(Box::new(status_code));
            param_idx += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        (where_clause, params, param_idx)
    }

    /// Query request logs with optional filters. Body BLOBs are excluded
    /// from list results — use `get_request_log_detail` for full records.
    pub fn query_request_logs(&self, filter: &RequestLogFilter) -> StoreResult<Vec<DbRequestLog>> {
        let conn = self.conn();
        let (where_clause, mut params, param_idx) = Self::request_log_filter_sql(filter);
        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        let sql = format!(
            "SELECT id, request_id, api_key_id, provider_name, provider,
                    model, operation, status_code, input_tokens, output_tokens,
                    total_tokens, cached_tokens, cache_write_tokens, cost_cents,
                    latency_ms, first_byte_latency_ms,
                    error_code, error, created_at
             FROM request_logs {}
             ORDER BY created_at DESC
             LIMIT ?{} OFFSET ?{}",
            where_clause,
            param_idx,
            param_idx + 1,
        );

        params.push(Box::new(limit));
        params.push(Box::new(offset));

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), Self::map_request_log_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Count request logs matching the same filters used by `query_request_logs`.
    pub fn count_request_logs(&self, filter: &RequestLogFilter) -> StoreResult<i64> {
        let conn = self.conn();
        let (where_clause, params, _) = Self::request_log_filter_sql(filter);
        let sql = format!("SELECT COUNT(*) FROM request_logs {}", where_clause);
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let count = conn.query_row(&sql, param_refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }

    /// Fetch a single log entry including body BLOBs.
    pub fn get_request_log_detail(&self, id: &str) -> StoreResult<Option<DbRequestLog>> {
        let conn = self.conn();
        let sql = "SELECT id, request_id, api_key_id, provider_name, provider,
                          model, operation, status_code, input_tokens, output_tokens,
                          total_tokens, cached_tokens, cache_write_tokens, cost_cents,
                          latency_ms, first_byte_latency_ms,
                          error_code, error, created_at, request_body, response_body
                   FROM request_logs
                   WHERE id = ?1
                   LIMIT 1";

        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query_map(rusqlite::params![id], Self::map_request_log_detail_row)?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Map a row from the list query (no body columns).
    fn map_request_log_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DbRequestLog> {
        Ok(DbRequestLog {
            id: row.get(0)?,
            request_id: row.get(1)?,
            api_key_id: row.get(2)?,
            provider_name: row.get(3)?,
            provider: row.get(4)?,
            model: row.get(5)?,
            operation: row.get(6)?,
            status_code: row.get(7)?,
            input_tokens: row.get(8)?,
            output_tokens: row.get(9)?,
            total_tokens: row.get(10)?,
            cached_tokens: row.get(11)?,
            cache_write_tokens: row.get(12)?,
            cost_cents: row.get(13)?,
            latency_ms: row.get(14)?,
            first_byte_latency_ms: row.get(15)?,
            error_code: row.get(16)?,
            error: row.get(17)?,
            created_at: row.get(18)?,
            request_body: None,
            response_body: None,
        })
    }

    /// Map a row from the detail query (includes body BLOBs).
    fn map_request_log_detail_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DbRequestLog> {
        Ok(DbRequestLog {
            id: row.get(0)?,
            request_id: row.get(1)?,
            api_key_id: row.get(2)?,
            provider_name: row.get(3)?,
            provider: row.get(4)?,
            model: row.get(5)?,
            operation: row.get(6)?,
            status_code: row.get(7)?,
            input_tokens: row.get(8)?,
            output_tokens: row.get(9)?,
            total_tokens: row.get(10)?,
            cached_tokens: row.get(11)?,
            cache_write_tokens: row.get(12)?,
            cost_cents: row.get(13)?,
            latency_ms: row.get(14)?,
            first_byte_latency_ms: row.get(15)?,
            error_code: row.get(16)?,
            error: row.get(17)?,
            created_at: row.get(18)?,
            request_body: row.get(19)?,
            response_body: row.get(20)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::store::{models::RequestLogFilter, NewRequestLog, Store};

    fn insert_sample_logs(store: &Store) {
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
            store
                .insert_request_log(
                    &format!("req-{}", i),
                    key_id,
                    "openai-1",
                    "completions",
                    "gpt-4",
                    "completions",
                    200,
                    100 + i,
                    50 + i,
                    150 + 2 * i,
                    10 + i,
                    100 + i * 10,
                    None,
                    Some(req_body.as_bytes()),
                    Some(res_body.as_bytes()),
                )
                .unwrap();
        }
    }

    #[test]
    fn test_insert_and_query() {
        let store = Store::open_in_memory().unwrap();
        insert_sample_logs(&store);

        // Query all
        let logs = store
            .query_request_logs(&RequestLogFilter::default())
            .unwrap();
        assert_eq!(logs.len(), 5);

        // List query should not include body BLOBs
        for log in &logs {
            assert!(log.request_body.is_none());
            assert!(log.response_body.is_none());
        }

        // Filter by model
        let logs = store
            .query_request_logs(&RequestLogFilter {
                model: Some("gpt-4".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(logs.len(), 5);

        // Filter by provider instance name
        let logs = store
            .query_request_logs(&RequestLogFilter {
                provider_name: Some("openai-1".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(logs.len(), 5);

        // Filter by non-existent model
        let logs = store
            .query_request_logs(&RequestLogFilter {
                model: Some("claude-3".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(logs.len(), 0);

        // Pagination
        let logs = store
            .query_request_logs(&RequestLogFilter {
                limit: Some(2),
                offset: Some(0),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(logs.len(), 2);
    }

    #[test]
    fn test_get_request_log_detail() {
        let store = Store::open_in_memory().unwrap();
        insert_sample_logs(&store);

        // Get the first log's ID from list query
        let logs = store
            .query_request_logs(&RequestLogFilter {
                limit: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert!(!logs.is_empty());
        let log_id = &logs[0].id;

        // Detail query should return request body BLOBs. Response bodies are
        // intentionally optional because LLM output content is not persisted.
        let detail = store.get_request_log_detail(log_id).unwrap().unwrap();
        assert_eq!(detail.id, *log_id);
        assert!(detail.request_body.is_some());
        assert!(detail.response_body.is_some());

        let req_bytes = detail.request_body.unwrap();
        let req_str = String::from_utf8(req_bytes).unwrap();
        assert!(req_str.contains("gpt-4"));

        let res_bytes = detail.response_body.unwrap();
        let res_str = String::from_utf8(res_bytes).unwrap();
        assert!(res_str.contains("reply-"));

        // Non-existent ID
        let missing = store.get_request_log_detail("does-not-exist").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_insert_request_log_entry_stores_first_byte_error_code_and_allows_no_response_body() {
        let store = Store::open_in_memory().unwrap();

        let entry = store
            .insert_request_log_entry(NewRequestLog {
                request_id: "req-error",
                api_key_id: "request-log-entry-key",
                provider_name: "openai-1",
                provider: "completions",
                model: "gpt-4",
                operation: "chat_completions",
                status_code: 429,
                input_tokens: 1,
                output_tokens: 2,
                total_tokens: 3,
                cached_tokens: 1,
                cache_write_tokens: 0,
                cost_cents: 4,
                latency_ms: 250,
                first_byte_latency_ms: 80,
                error_code: Some("rate_limit_exceeded"),
                error: Some("too many requests"),
                request_body: Some(br#"{"model":"gpt-4"}"#),
                response_body: None,
            })
            .unwrap();

        assert_eq!(entry.first_byte_latency_ms, 80);
        assert_eq!(entry.error_code.as_deref(), Some("rate_limit_exceeded"));

        let detail = store.get_request_log_detail(&entry.id).unwrap().unwrap();
        assert_eq!(detail.first_byte_latency_ms, 80);
        assert_eq!(detail.error_code.as_deref(), Some("rate_limit_exceeded"));
        assert_eq!(detail.cached_tokens, 1);
        assert_eq!(detail.cache_write_tokens, 0);
        assert!(detail.request_body.is_some());
        assert!(detail.response_body.is_none());
    }
}
