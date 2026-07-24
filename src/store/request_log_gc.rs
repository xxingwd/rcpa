use super::{Store, StoreError, StoreResult};
use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, NaiveTime, TimeZone, Utc};
use std::time::Duration;
use tokio::task::JoinHandle;

const SUCCESS_BODY_RETENTION: ChronoDuration = ChronoDuration::hours(24);
const FAILED_BODY_RETENTION: ChronoDuration = ChronoDuration::days(7);
const GC_BATCH_SIZE: usize = 500;
const BEIJING_UTC_OFFSET_SECONDS: i32 = 8 * 60 * 60;
const DAILY_GC_HOUR: u32 = 3;
const CLEAR_EXPIRED_BODIES_SQL: &str = r#"UPDATE request_logs
    SET request_body = NULL,
        response_body = NULL
    WHERE id IN (
        SELECT id
        FROM request_logs
        WHERE status = ?
          AND created_at < ?
          AND (request_body IS NOT NULL OR response_body IS NOT NULL)
        ORDER BY created_at
        LIMIT ?
    )"#;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RequestLogBodyGcResult {
    pub success_logs_cleared: u64,
    pub failed_logs_cleared: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SqliteMaintenanceResult {
    vacuum_ran: bool,
    pages_before: u64,
    pages_after: u64,
    freelist_pages_before: u64,
}

impl RequestLogBodyGcResult {
    pub fn total_logs_cleared(self) -> u64 {
        self.success_logs_cleared + self.failed_logs_cleared
    }
}

impl Store {
    /// Clear expired request and response bodies while preserving the log row and metrics.
    pub async fn gc_request_log_bodies(
        &self,
        now: DateTime<Utc>,
        batch_size: usize,
    ) -> StoreResult<RequestLogBodyGcResult> {
        if batch_size == 0 {
            return Err(StoreError::InvalidData(
                "request log body GC batch size must be greater than zero".to_string(),
            ));
        }

        let success_cutoff = (now - SUCCESS_BODY_RETENTION).to_rfc3339();
        let failed_cutoff = (now - FAILED_BODY_RETENTION).to_rfc3339();
        let success_logs_cleared = self
            .clear_expired_request_log_bodies("success", &success_cutoff, batch_size)
            .await?;
        let failed_logs_cleared = self
            .clear_expired_request_log_bodies("failed", &failed_cutoff, batch_size)
            .await?;

        Ok(RequestLogBodyGcResult {
            success_logs_cleared,
            failed_logs_cleared,
        })
    }

    async fn clear_expired_request_log_bodies(
        &self,
        status: &str,
        cutoff: &str,
        batch_size: usize,
    ) -> StoreResult<u64> {
        let mut total_cleared = 0;
        let batch_size = i64::try_from(batch_size).map_err(|_| {
            StoreError::InvalidData("request log body GC batch size is too large".to_string())
        })?;

        loop {
            let result = sqlx::query(CLEAR_EXPIRED_BODIES_SQL)
                .bind(status)
                .bind(cutoff)
                .bind(batch_size)
                .execute(&self.pool)
                .await?;

            let cleared = result.rows_affected();
            total_cleared += cleared;
            if cleared < batch_size as u64 {
                break;
            }
            tokio::task::yield_now().await;
        }

        Ok(total_cleared)
    }

    async fn maintain_sqlite_storage(
        &self,
        bodies_cleared: bool,
    ) -> StoreResult<SqliteMaintenanceResult> {
        let mut connection = self.pool.acquire().await?;
        checkpoint_wal(&mut connection).await?;

        let pages_before = pragma_u64(&mut connection, "PRAGMA page_count").await?;
        let freelist_pages_before = pragma_u64(&mut connection, "PRAGMA freelist_count").await?;
        let vacuum_ran = bodies_cleared || freelist_pages_before > 0;

        if vacuum_ran {
            sqlx::query("VACUUM").execute(&mut *connection).await?;
            checkpoint_wal(&mut connection).await?;
        }

        let pages_after = pragma_u64(&mut connection, "PRAGMA page_count").await?;
        Ok(SqliteMaintenanceResult {
            vacuum_ran,
            pages_before,
            pages_after,
            freelist_pages_before,
        })
    }
}

async fn checkpoint_wal(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
) -> StoreResult<()> {
    let (busy, log_frames, checkpointed_frames): (i64, i64, i64) =
        sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(&mut **connection)
            .await?;
    if busy != 0 {
        return Err(StoreError::Maintenance(format!(
            "WAL checkpoint remained busy with {log_frames} frames and {checkpointed_frames} checkpointed"
        )));
    }
    Ok(())
}

async fn pragma_u64(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    statement: &str,
) -> StoreResult<u64> {
    let value: i64 = sqlx::query_scalar(statement)
        .fetch_one(&mut **connection)
        .await?;
    u64::try_from(value).map_err(|_| {
        StoreError::Maintenance(format!("{statement} returned a negative value: {value}"))
    })
}

/// Start the request-body retention task at 03:00 in UTC+08:00 (Asia/Shanghai).
pub fn spawn_request_log_body_gc(store: Store) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let now = Utc::now();
            let next_run = next_daily_gc_after(now);
            let delay = (next_run - now).to_std().unwrap_or(Duration::ZERO);
            tracing::info!(
                next_run_utc = %next_run,
                schedule = "03:00 Asia/Shanghai",
                "Scheduled next request log body GC"
            );
            tokio::time::sleep(delay).await;
            run_gc(&store).await;
        }
    })
}

async fn run_gc(store: &Store) {
    let started_at = Utc::now();
    match store.gc_request_log_bodies(started_at, GC_BATCH_SIZE).await {
        Ok(result) => {
            tracing::info!(
                success_logs_cleared = result.success_logs_cleared,
                failed_logs_cleared = result.failed_logs_cleared,
                total_logs_cleared = result.total_logs_cleared(),
                "Request log body GC completed"
            );
            match store
                .maintain_sqlite_storage(result.total_logs_cleared() > 0)
                .await
            {
                Ok(maintenance) => tracing::info!(
                    vacuum_ran = maintenance.vacuum_ran,
                    pages_before = maintenance.pages_before,
                    pages_after = maintenance.pages_after,
                    pages_reclaimed = maintenance
                        .pages_before
                        .saturating_sub(maintenance.pages_after),
                    freelist_pages_before = maintenance.freelist_pages_before,
                    "SQLite storage maintenance completed"
                ),
                Err(error) => {
                    tracing::error!(error = %error, "SQLite storage maintenance failed")
                }
            }
        }
        Err(error) => tracing::error!(error = %error, "Request log body GC failed"),
    }
}

fn next_daily_gc_after(now: DateTime<Utc>) -> DateTime<Utc> {
    let beijing = FixedOffset::east_opt(BEIJING_UTC_OFFSET_SECONDS)
        .expect("UTC+08:00 must be a valid fixed offset");
    let local_now = now.with_timezone(&beijing);
    let gc_time =
        NaiveTime::from_hms_opt(DAILY_GC_HOUR, 0, 0).expect("03:00:00 must be a valid time");
    let next_date = if local_now.time() < gc_time {
        local_now.date_naive()
    } else {
        local_now
            .date_naive()
            .succ_opt()
            .expect("the next calendar date must be representable")
    };
    let next_local = beijing
        .from_local_datetime(&next_date.and_time(gc_time))
        .single()
        .expect("a fixed offset has exactly one local datetime mapping");
    next_local.with_timezone(&Utc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NewRequestLog;

    async fn insert_log(store: &Store, request_id: &str, success: bool) -> String {
        store
            .insert_request_log_entry(NewRequestLog {
                request_id,
                api_key_id: "gc-key",
                session_hash: None,
                provider_name: "gc-provider",
                protocol: "completions",
                model: "gc-model",
                operation: "completions",
                status_code: if success { 200 } else { 500 },
                success,
                input_tokens: 1,
                output_tokens: 2,
                total_tokens: 3,
                cached_tokens: 0,
                cache_write_tokens: 0,
                cost_cents: 4,
                latency_ms: 5,
                first_byte_latency_ms: 3,
                metadata_json: "{}",
                request_body: Some(br#"{"request":true}"#),
                response_body: Some(br#"{"response":true}"#),
            })
            .await
            .unwrap()
            .id
    }

    async fn set_created_at(store: &Store, id: &str, created_at: &str) {
        sqlx::query("UPDATE request_logs SET created_at = ?, finished_at = ? WHERE id = ?")
            .bind(created_at)
            .bind(created_at)
            .bind(id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE request_log_metrics SET created_at = ? WHERE id = ?")
            .bind(created_at)
            .bind(id)
            .execute(&store.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clears_only_bodies_past_the_status_retention_period() {
        let store = Store::open_in_memory().await.unwrap();
        let now = DateTime::parse_from_rfc3339("2026-07-23T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let old_success = insert_log(&store, "old-success", true).await;
        let recent_success = insert_log(&store, "recent-success", true).await;
        let old_failure = insert_log(&store, "old-failure", false).await;
        let recent_failure = insert_log(&store, "recent-failure", false).await;
        set_created_at(&store, &old_success, "2026-07-22T11:59:59+00:00").await;
        set_created_at(&store, &recent_success, "2026-07-22T12:00:01+00:00").await;
        set_created_at(&store, &old_failure, "2026-07-16T11:59:59+00:00").await;
        set_created_at(&store, &recent_failure, "2026-07-16T12:00:01+00:00").await;

        let result = store.gc_request_log_bodies(now, 1).await.unwrap();
        assert_eq!(
            result,
            RequestLogBodyGcResult {
                success_logs_cleared: 1,
                failed_logs_cleared: 1,
            }
        );

        for (id, should_be_cleared) in [
            (&old_success, true),
            (&recent_success, false),
            (&old_failure, true),
            (&recent_failure, false),
        ] {
            let detail = store.get_request_log_detail(id).await.unwrap().unwrap();
            assert_eq!(detail.request_body.is_none(), should_be_cleared);
            assert_eq!(detail.response_body.is_none(), should_be_cleared);
        }

        let metrics_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_log_metrics")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(metrics_count, 4);

        let second_result = store.gc_request_log_bodies(now, 1).await.unwrap();
        assert_eq!(second_result, RequestLogBodyGcResult::default());
    }

    #[tokio::test]
    async fn storage_maintenance_returns_cleared_body_pages_to_filesystem() {
        let temp_dir =
            std::env::temp_dir().join(format!("rcpa-storage-gc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("rcpa.db");
        let store = Store::open_path(&db_path).await.unwrap();
        let log_id = insert_log(&store, "large-old-success", true).await;
        set_created_at(&store, &log_id, "2026-07-22T11:59:59+00:00").await;
        sqlx::query(
            "UPDATE request_logs
             SET request_body = zeroblob(1048576), response_body = zeroblob(1048576)
             WHERE id = ?",
        )
        .bind(&log_id)
        .execute(&store.pool)
        .await
        .unwrap();

        let baseline = store.maintain_sqlite_storage(false).await.unwrap();
        assert!(!baseline.vacuum_ran);
        let file_size_before = std::fs::metadata(&db_path).unwrap().len();

        let now = DateTime::parse_from_rfc3339("2026-07-23T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let gc_result = store.gc_request_log_bodies(now, 500).await.unwrap();
        assert_eq!(gc_result.success_logs_cleared, 1);

        let maintenance = store.maintain_sqlite_storage(true).await.unwrap();
        let file_size_after = std::fs::metadata(&db_path).unwrap().len();
        assert!(maintenance.vacuum_ran);
        assert!(maintenance.pages_after < maintenance.pages_before);
        assert!(file_size_after < file_size_before);

        let mut connection = store.pool.acquire().await.unwrap();
        assert_eq!(
            pragma_u64(&mut connection, "PRAGMA freelist_count")
                .await
                .unwrap(),
            0
        );
        drop(connection);
        drop(store);
        std::fs::remove_dir_all(temp_dir).unwrap();
    }

    #[tokio::test]
    async fn rejects_zero_batch_size() {
        let store = Store::open_in_memory().await.unwrap();
        let error = store
            .gc_request_log_bodies(Utc::now(), 0)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("greater than zero"));
    }

    #[tokio::test]
    async fn cleanup_query_uses_body_gc_index() {
        let store = Store::open_in_memory().await.unwrap();
        let plan: Vec<(i64, i64, i64, String)> =
            sqlx::query_as(&format!("EXPLAIN QUERY PLAN {CLEAR_EXPIRED_BODIES_SQL}"))
                .bind("success")
                .bind("2026-07-22T12:00:00+00:00")
                .bind(500_i64)
                .fetch_all(&store.pool)
                .await
                .unwrap();

        assert!(
            plan.iter()
                .any(|(_, _, _, detail)| detail.contains("idx_request_logs_body_gc")),
            "query plan did not use body GC index: {plan:?}"
        );
    }

    #[test]
    fn schedules_next_run_at_three_in_beijing() {
        let before = DateTime::parse_from_rfc3339("2026-07-22T18:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let after = DateTime::parse_from_rfc3339("2026-07-22T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(
            next_daily_gc_after(before).to_rfc3339(),
            "2026-07-22T19:00:00+00:00"
        );
        assert_eq!(
            next_daily_gc_after(after).to_rfc3339(),
            "2026-07-23T19:00:00+00:00"
        );
    }
}
