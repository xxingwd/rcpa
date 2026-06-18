use rusqlite::Connection;

const CURRENT_SCHEMA_VERSION: i64 = 9;

struct Migration {
    version: i64,
    name: &'static str,
    up: fn(&Connection) -> anyhow::Result<()>,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "create_request_logs",
        up: create_request_logs,
    },
    Migration {
        version: 2,
        name: "require_request_log_api_key",
        up: require_request_log_api_key,
    },
    Migration {
        version: 3,
        name: "historical_schema_version_3",
        up: no_op,
    },
    Migration {
        version: 4,
        name: "historical_schema_version_4",
        up: no_op,
    },
    Migration {
        version: 5,
        name: "historical_schema_version_5",
        up: no_op,
    },
    Migration {
        version: 6,
        name: "historical_schema_version_6",
        up: no_op,
    },
    Migration {
        version: 7,
        name: "historical_schema_version_7",
        up: no_op,
    },
    Migration {
        version: 8,
        name: "historical_schema_version_8",
        up: no_op,
    },
    Migration {
        version: 9,
        name: "normalize_request_log_protocol_names",
        up: normalize_request_log_protocol_names,
    },
];

const REQUEST_LOGS_TABLE: &str = r#"
CREATE TABLE request_logs (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    api_key_id TEXT NOT NULL,
    provider_name TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    operation TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    cached_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    cost_cents INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    first_byte_latency_ms INTEGER NOT NULL DEFAULT 0,
    error_code TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    request_body BLOB,
    response_body BLOB
);
"#;

const REQUEST_LOGS_INDEXES: &str = r#"
CREATE INDEX idx_request_logs_created_at ON request_logs(created_at);
CREATE INDEX idx_request_logs_api_key_id ON request_logs(api_key_id);
CREATE INDEX idx_request_logs_model ON request_logs(model);
CREATE INDEX idx_request_logs_provider_name ON request_logs(provider_name);
CREATE INDEX idx_request_logs_provider ON request_logs(provider);
CREATE INDEX idx_request_logs_status_code ON request_logs(status_code);
"#;

const REQUEST_LOG_COLUMNS: &str = r#"
id, request_id, api_key_id, provider_name, provider, model, operation,
status_code, input_tokens, output_tokens, total_tokens, cached_tokens,
cache_write_tokens, cost_cents, latency_ms, first_byte_latency_ms,
error_code, error, created_at, request_body, response_body
"#;

pub fn run_migrations(conn: &mut Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );
        "#,
    )?;

    let applied = applied_versions(conn)?;
    if let Some(version) = applied.iter().max() {
        if *version > CURRENT_SCHEMA_VERSION {
            anyhow::bail!(
                "database schema version {} is newer than supported version {}",
                version,
                CURRENT_SCHEMA_VERSION
            );
        }
    }

    validate_no_version_gaps(&applied)?;
    let current_version = applied.last().copied().unwrap_or(0);

    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > current_version)
    {
        tracing::info!(
            version = migration.version,
            name = migration.name,
            "Applying database migration"
        );
        let tx = conn.transaction()?;
        (migration.up)(&tx)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![migration.version, chrono::Utc::now().to_rfc3339()],
        )?;
        tx.commit()?;
    }

    validate_applied_versions(conn)?;
    validate_request_logs_schema(conn)?;
    Ok(())
}

fn create_request_logs(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(REQUEST_LOGS_TABLE)?;
    conn.execute_batch(REQUEST_LOGS_INDEXES)?;
    Ok(())
}

fn require_request_log_api_key(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM request_logs WHERE api_key_id IS NULL OR api_key_id = ''",
        [],
    )?;
    conn.execute_batch(&format!(
        r#"
            CREATE TABLE request_logs_v2 (
                id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL,
                api_key_id TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                operation TEXT NOT NULL,
                status_code INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                cached_tokens INTEGER NOT NULL DEFAULT 0,
                cache_write_tokens INTEGER NOT NULL DEFAULT 0,
                cost_cents INTEGER NOT NULL DEFAULT 0,
                latency_ms INTEGER NOT NULL DEFAULT 0,
                first_byte_latency_ms INTEGER NOT NULL DEFAULT 0,
                error_code TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                request_body BLOB,
                response_body BLOB
            );

            INSERT INTO request_logs_v2 ({columns})
            SELECT {columns}
            FROM request_logs;

            DROP TABLE request_logs;
            ALTER TABLE request_logs_v2 RENAME TO request_logs;
            "#,
        columns = REQUEST_LOG_COLUMNS,
    ))?;
    conn.execute_batch(REQUEST_LOGS_INDEXES)?;
    Ok(())
}

fn no_op(_conn: &Connection) -> anyhow::Result<()> {
    Ok(())
}

fn normalize_request_log_protocol_names(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE request_logs SET provider = 'responses'
         WHERE provider = 'openai' AND operation = 'responses'",
        [],
    )?;
    conn.execute(
        "UPDATE request_logs SET provider = 'completions'
         WHERE provider = 'openai'",
        [],
    )?;
    conn.execute(
        "UPDATE request_logs SET provider = 'messages'
         WHERE provider = 'anthropic'",
        [],
    )?;
    Ok(())
}

fn applied_versions(conn: &Connection) -> anyhow::Result<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT version FROM schema_migrations ORDER BY version ASC")?;
    let versions = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(versions)
}

fn validate_applied_versions(conn: &Connection) -> anyhow::Result<()> {
    let applied = applied_versions(conn)?;
    let expected = expected_versions(CURRENT_SCHEMA_VERSION);
    if applied != expected {
        anyhow::bail!(
            "database migration history mismatch: expected {:?}, found {:?}",
            expected,
            applied
        );
    }
    Ok(())
}

fn validate_no_version_gaps(applied: &[i64]) -> anyhow::Result<()> {
    let Some(max_version) = applied.last().copied() else {
        return Ok(());
    };
    let expected = expected_versions(max_version);
    if applied != expected {
        anyhow::bail!(
            "database migration history has gaps: expected {:?}, found {:?}",
            expected,
            applied
        );
    }
    Ok(())
}

fn expected_versions(max_version: i64) -> Vec<i64> {
    (1..=max_version).collect()
}

#[derive(Debug, PartialEq, Eq)]
struct ColumnSpec {
    name: &'static str,
    type_name: &'static str,
    not_null: bool,
    default_value: Option<&'static str>,
    primary_key: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct ActualColumn {
    name: String,
    type_name: String,
    not_null: bool,
    default_value: Option<String>,
    primary_key: bool,
}

const EXPECTED_REQUEST_LOG_COLUMNS: &[ColumnSpec] = &[
    ColumnSpec {
        name: "id",
        type_name: "TEXT",
        not_null: false,
        default_value: None,
        primary_key: true,
    },
    required_text("request_id"),
    required_text("api_key_id"),
    required_text("provider_name"),
    required_text("provider"),
    required_text("model"),
    required_text("operation"),
    required_integer("status_code", None),
    required_integer("input_tokens", Some("0")),
    required_integer("output_tokens", Some("0")),
    required_integer("total_tokens", Some("0")),
    required_integer("cached_tokens", Some("0")),
    required_integer("cache_write_tokens", Some("0")),
    required_integer("cost_cents", Some("0")),
    required_integer("latency_ms", Some("0")),
    required_integer("first_byte_latency_ms", Some("0")),
    optional_text("error_code"),
    optional_text("error"),
    required_text("created_at"),
    optional_blob("request_body"),
    optional_blob("response_body"),
];

const fn required_text(name: &'static str) -> ColumnSpec {
    ColumnSpec {
        name,
        type_name: "TEXT",
        not_null: true,
        default_value: None,
        primary_key: false,
    }
}

const fn optional_text(name: &'static str) -> ColumnSpec {
    ColumnSpec {
        name,
        type_name: "TEXT",
        not_null: false,
        default_value: None,
        primary_key: false,
    }
}

const fn required_integer(name: &'static str, default_value: Option<&'static str>) -> ColumnSpec {
    ColumnSpec {
        name,
        type_name: "INTEGER",
        not_null: true,
        default_value,
        primary_key: false,
    }
}

const fn optional_blob(name: &'static str) -> ColumnSpec {
    ColumnSpec {
        name,
        type_name: "BLOB",
        not_null: false,
        default_value: None,
        primary_key: false,
    }
}

fn validate_request_logs_schema(conn: &Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(request_logs)")?;
    let columns = stmt
        .query_map([], |row| {
            Ok(ActualColumn {
                name: row.get(1)?,
                type_name: row.get::<_, String>(2)?.to_uppercase(),
                not_null: row.get::<_, i64>(3)? != 0,
                default_value: row.get(4)?,
                primary_key: row.get::<_, i64>(5)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let expected = EXPECTED_REQUEST_LOG_COLUMNS
        .iter()
        .map(|column| ActualColumn {
            name: column.name.to_string(),
            type_name: column.type_name.to_string(),
            not_null: column.not_null,
            default_value: column.default_value.map(ToString::to_string),
            primary_key: column.primary_key,
        })
        .collect::<Vec<_>>();

    if columns != expected {
        anyhow::bail!(
            "request_logs schema mismatch: expected {:?}, found {:?}",
            expected,
            columns
        );
    }

    for index in [
        "idx_request_logs_created_at",
        "idx_request_logs_api_key_id",
        "idx_request_logs_model",
        "idx_request_logs_provider_name",
        "idx_request_logs_provider",
        "idx_request_logs_status_code",
    ] {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'request_logs' AND name = ?1",
            [index],
            |row| row.get(0),
        )?;
        if count != 1 {
            anyhow::bail!("request_logs index '{}' is missing", index);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_count(conn: &Connection, object_type: &str, name: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
            rusqlite::params![object_type, name],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn test_run_migrations_fresh_db() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();

        assert_eq!(object_count(&conn, "table", "request_logs"), 1);
        assert_eq!(object_count(&conn, "table", "schema_migrations"), 1);
        validate_request_logs_schema(&conn).unwrap();

        let versions = applied_versions(&conn).unwrap();
        assert_eq!(versions, expected_versions(CURRENT_SCHEMA_VERSION));
    }

    #[test]
    fn test_run_migrations_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();

        let versions = applied_versions(&conn).unwrap();
        assert_eq!(versions, expected_versions(CURRENT_SCHEMA_VERSION));
    }

    #[test]
    fn test_require_request_log_api_key_migrates_version_one_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at)
            VALUES (1, '2024-01-01T00:00:00Z');

            CREATE TABLE request_logs (
                id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL,
                api_key_id TEXT,
                provider_name TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                operation TEXT NOT NULL,
                status_code INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                cached_tokens INTEGER NOT NULL DEFAULT 0,
                cache_write_tokens INTEGER NOT NULL DEFAULT 0,
                cost_cents INTEGER NOT NULL DEFAULT 0,
                latency_ms INTEGER NOT NULL DEFAULT 0,
                first_byte_latency_ms INTEGER NOT NULL DEFAULT 0,
                error_code TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                request_body BLOB,
                response_body BLOB
            );
            INSERT INTO request_logs (
                id, request_id, api_key_id, provider_name, provider, model, operation,
                status_code, input_tokens, output_tokens, total_tokens, cached_tokens,
                cache_write_tokens, cost_cents, latency_ms, first_byte_latency_ms,
                error_code, error, created_at, request_body, response_body
            )
            VALUES
                ('empty-key', 'req-empty', '', 'openai-1', 'completions', 'gpt-4o', 'chat_completions',
                 200, 1, 1, 2, 0, 0, 1, 10, 10, NULL, NULL, '2024-01-01T00:00:00Z', NULL, NULL),
                ('valid-key', 'req-valid', 'key-1', 'openai-1', 'completions', 'gpt-4o', 'chat_completions',
                 200, 1, 1, 2, 0, 0, 1, 10, 10, NULL, NULL, '2024-01-01T00:00:00Z', NULL, NULL);
            "#,
        )
        .unwrap();

        run_migrations(&mut conn).unwrap();
        validate_request_logs_schema(&conn).unwrap();
        assert_eq!(
            applied_versions(&conn).unwrap(),
            expected_versions(CURRENT_SCHEMA_VERSION)
        );

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let id: String = conn
            .query_row("SELECT id FROM request_logs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(id, "valid-key");
    }

    #[test]
    fn test_version_eight_database_migrates_protocol_names() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at)
            VALUES
                (1, '2024-01-01T00:00:00Z'),
                (2, '2024-01-01T00:00:00Z'),
                (3, '2024-01-01T00:00:00Z'),
                (4, '2024-01-01T00:00:00Z'),
                (5, '2024-01-01T00:00:00Z'),
                (6, '2024-01-01T00:00:00Z'),
                (7, '2024-01-01T00:00:00Z'),
                (8, '2024-01-01T00:00:00Z');

            CREATE TABLE request_logs (
                id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL,
                api_key_id TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                operation TEXT NOT NULL,
                status_code INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                cached_tokens INTEGER NOT NULL DEFAULT 0,
                cache_write_tokens INTEGER NOT NULL DEFAULT 0,
                cost_cents INTEGER NOT NULL DEFAULT 0,
                latency_ms INTEGER NOT NULL DEFAULT 0,
                first_byte_latency_ms INTEGER NOT NULL DEFAULT 0,
                error_code TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                request_body BLOB,
                response_body BLOB
            );
            CREATE INDEX idx_request_logs_created_at ON request_logs(created_at);
            CREATE INDEX idx_request_logs_api_key_id ON request_logs(api_key_id);
            CREATE INDEX idx_request_logs_model ON request_logs(model);
            CREATE INDEX idx_request_logs_provider_name ON request_logs(provider_name);
            CREATE INDEX idx_request_logs_provider ON request_logs(provider);
            CREATE INDEX idx_request_logs_status_code ON request_logs(status_code);

            INSERT INTO request_logs (
                id, request_id, api_key_id, provider_name, provider, model, operation,
                status_code, input_tokens, output_tokens, total_tokens, cached_tokens,
                cache_write_tokens, cost_cents, latency_ms, first_byte_latency_ms,
                error_code, error, created_at, request_body, response_body
            )
            VALUES
                ('chat', 'req-chat', 'key-1', 'openai-1', 'openai', 'gpt-4o', 'chat_completions',
                 200, 1, 1, 2, 0, 0, 1, 10, 10, NULL, NULL, '2024-01-01T00:00:00Z', NULL, NULL),
                ('responses', 'req-responses', 'key-1', 'openai-1', 'openai', 'gpt-4o', 'responses',
                 200, 1, 1, 2, 0, 0, 1, 10, 10, NULL, NULL, '2024-01-01T00:00:00Z', NULL, NULL),
                ('messages', 'req-messages', 'key-1', 'anthropic-1', 'anthropic', 'claude', 'messages',
                 200, 1, 1, 2, 0, 0, 1, 10, 10, NULL, NULL, '2024-01-01T00:00:00Z', NULL, NULL);
            "#,
        )
        .unwrap();

        run_migrations(&mut conn).unwrap();

        assert_eq!(
            applied_versions(&conn).unwrap(),
            expected_versions(CURRENT_SCHEMA_VERSION)
        );
        let values = conn
            .prepare("SELECT id, provider FROM request_logs ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            values,
            vec![
                ("chat".to_string(), "completions".to_string()),
                ("messages".to_string(), "messages".to_string()),
                ("responses".to_string(), "responses".to_string()),
            ]
        );
    }

    #[test]
    fn test_run_migrations_rejects_future_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at)
            VALUES (99, '2024-01-01T00:00:00Z');
            "#,
        )
        .unwrap();

        let err = run_migrations(&mut conn).unwrap_err().to_string();
        assert!(err.contains("newer than supported"));
    }

    #[test]
    fn test_run_migrations_rejects_current_version_schema_mismatch() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at)
            VALUES
                (1, '2024-01-01T00:00:00Z'),
                (2, '2024-01-01T00:00:00Z'),
                (3, '2024-01-01T00:00:00Z'),
                (4, '2024-01-01T00:00:00Z'),
                (5, '2024-01-01T00:00:00Z'),
                (6, '2024-01-01T00:00:00Z'),
                (7, '2024-01-01T00:00:00Z'),
                (8, '2024-01-01T00:00:00Z'),
                (9, '2024-01-01T00:00:00Z');
            CREATE TABLE request_logs (
                id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL
            );
            "#,
        )
        .unwrap();

        let err = run_migrations(&mut conn).unwrap_err().to_string();
        assert!(err.contains("request_logs schema mismatch"));
    }
}
