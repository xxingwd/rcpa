use std::borrow::Cow;

use rcpa::store::{AnalyticsTimeBucket, Store};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

#[tokio::test]
async fn dashboard_index_migration_preserves_history_and_tracks_updates() {
    let directory = std::env::temp_dir().join(format!("rcpa-dashboard-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let database = directory.join("rcpa.db");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&database)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    let migrations = sqlx::migrate!("./migrations");
    let previous = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            migrations
                .iter()
                .filter(|m| m.version < 20261115000000)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    previous.run(&pool).await.unwrap();
    let metadata = r#"{"request":{"stream":true},"error":{"message":"stream interrupted"}}"#;
    sqlx::query(
        "INSERT INTO request_logs (
            id, request_id, run_id, api_key_id, provider_name, protocol, model,
            operation, status, status_code, retry_count, input_tokens, output_tokens,
            latency_ms, first_byte_latency_ms, meta, created_at, updated_at,
            request_body, response_body
         ) VALUES ('history', 'request', 'run', 'key', 'provider', 'completions', 'model',
            'completions', 'failed', 200, 2, 100, 20, 2000, 1000, ?,
            '2026-11-14T10:00:00Z', '2026-11-14T10:00:02Z', zeroblob(1048576), zeroblob(1048576))",
    )
    .bind(metadata)
    .execute(&pool)
    .await
    .unwrap();

    let store = Store::open_path(&database).await.unwrap();
    let from = "2026-11-14T00:00:00Z";
    let to = "2026-11-15T00:00:00Z";
    let data = store
        .dashboard_analytics(from, to, AnalyticsTimeBucket::Hour)
        .await
        .unwrap();
    assert_eq!(data.total.request_count, 1);
    assert_eq!(data.observed.stream_errors, 1);
    assert_eq!(data.performance.retry_attempts, 2);
    let preserved: (String, i64, i64) = sqlx::query_as(
        "SELECT meta, length(request_body), length(response_body) FROM request_logs WHERE id = 'history'",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(preserved, (metadata.to_owned(), 1048576, 1048576));

    // Indexed JSON expressions must reflect later updates as well as old data.
    sqlx::query("UPDATE request_logs SET meta = '{\"request\":{\"stream\":true}}', status = 'success', retry_count = 0 WHERE id = 'history'")
        .execute(&pool).await.unwrap();
    let data = store
        .dashboard_analytics(from, to, AnalyticsTimeBucket::Hour)
        .await
        .unwrap();
    assert_eq!(data.observed.success_count, 1);
    assert_eq!(data.observed.stream_errors, 0);
    assert_eq!(data.observed.estimated_tps_p50, Some(20.0));
    assert_eq!(data.performance.retry_attempts, 0);

    drop(store);
    // Reopening must validate and reuse the applied migration without rebuilding it.
    drop(Store::open_path(&database).await.unwrap());
    pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}
