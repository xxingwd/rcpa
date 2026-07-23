//! SQLite-backed persistence layer for the RCPA gateway.

mod analytics;
pub mod models;
mod request_log_gc;
mod request_log_repo;

pub use analytics::{AnalyticsTimeBucket, DashboardAnalytics, DashboardStats};
pub use models::*;
pub use request_log_gc::{spawn_request_log_body_gc, RequestLogBodyGcResult};
pub use request_log_repo::NewRequestLog;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

use crate::config::expand_tilde;

/// Errors produced by the persistence layer.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Database error: {0}")]
    Sql(#[from] sqlx::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),
}

/// Result alias for store operations.
pub type StoreResult<T> = Result<T, StoreError>;

/// SQLite-backed persistence store.
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Open (or create) a database at the given file path and run migrations.
    pub async fn open(path: &str) -> anyhow::Result<Self> {
        Self::open_path(std::path::Path::new(path)).await
    }

    /// Open (or create) a database at the given file path and run migrations.
    pub async fn open_path(path: &Path) -> anyhow::Result<Self> {
        let expanded_path = expand_tilde(path);
        let path_str = expanded_path.to_string_lossy().into_owned();

        if let Some(parent) = expanded_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{path_str}"))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    /// Create an in-memory database — useful for unit and integration tests.
    pub async fn open_in_memory() -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_open_in_memory() {
        let store = Store::open_in_memory().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_logs")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_store_is_clone() {
        let store = Store::open_in_memory().await.unwrap();
        let _clone = store.clone();
    }

    #[tokio::test]
    async fn test_open_creates_parent_directory() {
        let temp_dir =
            std::env::temp_dir().join(format!("rcpa-store-test-{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("nested").join("rcpa.db");

        let store = Store::open(db_path.to_str().unwrap()).await.unwrap();
        drop(store);
        assert!(db_path.exists());

        std::fs::remove_dir_all(temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_routing_strategy_metadata_migration_keeps_request_log_data() {
        use sqlx::{Connection, Executor};

        let temp_dir = std::env::temp_dir().join(format!(
            "rcpa-routing-migration-test-{}",
            uuid::Uuid::new_v4()
        ));
        let db_path = temp_dir.join("rcpa.db");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let path_str = db_path.to_string_lossy().into_owned();
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{path_str}"))
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true);
        let mut conn = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();

        conn.execute(include_str!(
            "../../migrations/20250115000000_create_request_logs.sql"
        ))
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO request_logs (
                id, request_id, api_key_id, session_hash, provider_name, protocol,
                model, operation, status_code, success, input_tokens, output_tokens,
                total_tokens, cached_tokens, cache_write_tokens, cost_cents,
                latency_ms, first_byte_latency_ms, metadata_json, created_at,
                request_body, response_body
            ) VALUES (
                'log-a', 'request-a', 'key-a', 'session-hash-a', 'provider-a', 'completions',
                'gpt-4o', 'completions', 200, 1, 3, 5,
                8, 1, 0, 2,
                11, 7,
                '{"routing":{"strategy":"round_robin","sticky_enabled":true,"sticky_hit":false},"retry":{"attempt_count":1},"session":{"id":"session-a"}}',
                '2026-07-07T00:00:00Z',
                X'7B226D6F64656C223A226770742D346F227D',
                X'7B226964223A2263686174636D706C2D61227D'
            )"#,
        )
        .execute(&mut conn)
        .await
        .unwrap();

        conn.execute(include_str!(
            "../../migrations/20260707000000_remove_routing_strategy_from_request_logs.sql"
        ))
        .await
        .unwrap();

        let metadata_json: String =
            sqlx::query_scalar("SELECT metadata_json FROM request_logs WHERE id = 'log-a'")
                .fetch_one(&mut conn)
                .await
                .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap();
        assert!(metadata.pointer("/routing/strategy").is_none());
        assert_eq!(metadata["routing"]["sticky_enabled"], true);
        assert_eq!(metadata["routing"]["sticky_hit"], false);
        assert_eq!(metadata["retry"]["attempt_count"], 1);
        assert_eq!(metadata["session"]["id"], "session-a");
        let row = sqlx::query_as::<_, (String, i64, Vec<u8>, Vec<u8>)>(
            "SELECT session_hash, total_tokens, request_body, response_body FROM request_logs WHERE id = 'log-a'",
        )
        .fetch_one(&mut conn)
        .await
        .unwrap();
        assert_eq!(row.0, "session-hash-a");
        assert_eq!(row.1, 8);
        assert!(!row.2.is_empty());
        assert!(!row.3.is_empty());

        drop(conn);
        std::fs::remove_dir_all(temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_open_path_migrates_previous_request_log_metadata_without_data_loss() {
        use sqlx::{Connection, Executor};

        let temp_dir =
            std::env::temp_dir().join(format!("rcpa-migration-test-{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("rcpa.db");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let path_str = db_path.to_string_lossy().into_owned();
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{path_str}"))
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true);
        let mut conn = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();

        conn.execute(include_str!(
            "../../migrations/20250115000000_create_request_logs.sql"
        ))
        .await
        .unwrap();
        conn.execute(
            r#"CREATE TABLE _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            )"#,
        )
        .await
        .unwrap();

        let first_migration = sqlx::migrate!("./migrations")
            .iter()
            .find(|migration| migration.version == 20250115000000)
            .unwrap();
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (?, ?, 1, ?, 0)",
        )
        .bind(first_migration.version)
        .bind(first_migration.description.as_ref())
        .bind(first_migration.checksum.as_ref())
        .execute(&mut conn)
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO request_logs (
                id, request_id, api_key_id, session_hash, provider_name, protocol,
                model, operation, status_code, success, input_tokens, output_tokens,
                total_tokens, cached_tokens, cache_write_tokens, cost_cents,
                latency_ms, first_byte_latency_ms, metadata_json, created_at,
                request_body, response_body
            ) VALUES (
                'log-a', 'request-a', 'key-a', 'session-hash-a', 'provider-a', 'completions',
                'global-fast', 'completions', 200, 1, 3, 5,
                8, 1, 0, 2,
                11, 7,
                '{"routing":{"strategy":"round_robin","sticky_enabled":true,"sticky_hit":false},"retry":{"attempt_count":2,"retry_count":1},"session":{"id":"session-a"},"models":{"requested":"quick","resolved":"global-fast","provider":"provider-gpt-4o"}}',
                '2026-07-07T00:00:00Z',
                X'7B226D6F64656C223A226770742D346F227D',
                X'7B226964223A2263686174636D706C2D61227D'
            )"#,
        )
        .execute(&mut conn)
        .await
        .unwrap();
        drop(conn);

        let store = Store::open_path(&db_path).await.unwrap();
        let detail = store
            .get_request_log_detail("log-a")
            .await
            .unwrap()
            .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&detail.meta).unwrap();
        assert!(metadata.pointer("/routing/strategy").is_none());
        assert_eq!(metadata["routing"]["sticky_enabled"], true);
        assert_eq!(metadata["routing"]["sticky_hit"], false);
        assert_eq!(metadata["retry"]["attempt_count"], 2);
        assert_eq!(metadata["retry"]["retry_count"], 1);
        assert_eq!(metadata["session"]["id"], "session-a");
        assert_eq!(metadata["usage"]["cached_tokens"], 1);
        assert_eq!(metadata["usage"]["cache_write_tokens"], 0);
        assert_eq!(detail.run_id, "request-a");
        assert_eq!(detail.status, "success");
        assert_eq!(detail.model, "provider-gpt-4o");
        assert_eq!(detail.retry_count, 1);
        assert_eq!(detail.session_hash.as_deref(), Some("session-hash-a"));
        assert_eq!(detail.total_tokens, 8);
        assert_eq!(detail.finished_at.as_deref(), Some("2026-07-07T00:00:00Z"));
        assert!(detail.request_body.is_some());
        assert!(detail.response_body.is_some());

        let migrated_metrics: (String, i64, i64, i64) = sqlx::query_as(
            "SELECT model, cached_tokens, cache_write_tokens, input_tokens
             FROM request_log_metrics
             WHERE id = 'log-a'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(migrated_metrics, ("provider-gpt-4o".to_string(), 1, 0, 3));

        let migrated_versions: Vec<i64> =
            sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&store.pool)
                .await
                .unwrap();
        assert_eq!(
            migrated_versions,
            vec![
                20250115000000,
                20260707000000,
                20260708000000,
                20260723000000,
                20260723010000,
                20260723020000,
            ]
        );

        drop(store);
        std::fs::remove_dir_all(temp_dir).unwrap();
    }
}
