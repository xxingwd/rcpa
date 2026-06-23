//! SQLite-backed persistence layer for the RCPA gateway.

mod analytics;
pub mod models;
mod request_log_repo;

pub use models::*;
pub use request_log_repo::NewRequestLog;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
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
        let expanded_path = expand_tilde(std::path::Path::new(path));
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
}
