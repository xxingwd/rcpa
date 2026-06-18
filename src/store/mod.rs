//! SQLite-backed persistence layer for the RCPA gateway.
//!
//! This module provides a `Store` type backed by a single SQLite connection
//! wrapped in `Arc<Mutex<_>>`. All database operations are executed via
//! `tokio::task::spawn_blocking` so the async runtime is never blocked.

mod migrations;
pub mod models;

mod analytics;
mod request_log_repo;

use std::sync::{Arc, Mutex};

pub use models::*;
pub use request_log_repo::NewRequestLog;

/// Errors produced by the persistence layer.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Internal store error: {0}")]
    Internal(String),
}

/// Result alias for store operations.
pub type StoreResult<T> = Result<T, StoreError>;

/// SQLite-backed persistence store.
///
/// Internally wraps a `rusqlite::Connection` in `Arc<Mutex<_>>` so the
/// store can be cloned and shared across tasks safely.
#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl Store {
    /// Open (or create) a database at the given file path and run all
    /// pending migrations.
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let db_path = std::path::Path::new(path);
        if let Some(parent) = db_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let conn = rusqlite::Connection::open(path)?;
        Self::init(conn)
    }

    /// Create an in-memory database — useful for unit and integration tests.
    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(mut conn: rusqlite::Connection) -> anyhow::Result<Self> {
        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        migrations::run_migrations(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Acquire the connection lock. Panics if the mutex is poisoned.
    fn conn(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.conn.lock().expect("store mutex poisoned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let store = Store::open_in_memory().unwrap();
        // Verify we can use the connection
        let conn = store.conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_store_is_clone() {
        let store = Store::open_in_memory().unwrap();
        let _clone = store.clone();
    }

    #[test]
    fn test_open_creates_parent_directory() {
        let temp_dir =
            std::env::temp_dir().join(format!("rcpa-store-test-{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("nested").join("rcpa.db");

        let store = Store::open(db_path.to_str().unwrap()).unwrap();
        drop(store);
        assert!(db_path.exists());

        std::fs::remove_dir_all(temp_dir).unwrap();
    }
}
