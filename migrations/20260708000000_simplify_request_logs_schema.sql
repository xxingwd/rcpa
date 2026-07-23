CREATE TABLE request_logs_new (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    api_key_id TEXT NOT NULL,
    session_hash TEXT,
    provider_name TEXT NOT NULL,
    protocol TEXT NOT NULL,
    model TEXT NOT NULL,
    operation TEXT NOT NULL,
    status TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_cents INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    first_byte_latency_ms INTEGER NOT NULL DEFAULT 0,
    meta TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    finished_at TEXT,
    request_body BLOB,
    response_body BLOB
);

INSERT INTO request_logs_new (
    id,
    request_id,
    run_id,
    api_key_id,
    session_hash,
    provider_name,
    protocol,
    model,
    operation,
    status,
    status_code,
    retry_count,
    input_tokens,
    output_tokens,
    cost_cents,
    latency_ms,
    first_byte_latency_ms,
    meta,
    created_at,
    finished_at,
    request_body,
    response_body
)
SELECT
    id,
    request_id,
    COALESCE(NULLIF(request_id, ''), id) AS run_id,
    api_key_id,
    session_hash,
    provider_name,
    protocol,
    model,
    operation,
    CASE
        WHEN success = 1 THEN 'success'
        ELSE 'failed'
    END AS status,
    status_code,
    COALESCE(CAST(json_extract(metadata_json, '$.retry.retry_count') AS INTEGER), 0) AS retry_count,
    input_tokens,
    output_tokens,
    cost_cents,
    latency_ms,
    first_byte_latency_ms,
    CASE
        WHEN json_valid(metadata_json) THEN json_set(
            metadata_json,
            '$.usage.cached_tokens', cached_tokens,
            '$.usage.cache_write_tokens', cache_write_tokens
        )
        ELSE json_object(
            'legacy_meta', metadata_json,
            'usage', json_object(
                'cached_tokens', cached_tokens,
                'cache_write_tokens', cache_write_tokens
            )
        )
    END AS meta,
    created_at,
    created_at AS finished_at,
    request_body,
    response_body
FROM request_logs;

DROP TABLE request_logs;
ALTER TABLE request_logs_new RENAME TO request_logs;

CREATE INDEX idx_request_logs_created_at ON request_logs(created_at);
CREATE INDEX idx_request_logs_api_key_id ON request_logs(api_key_id);
CREATE INDEX idx_request_logs_session_hash ON request_logs(session_hash);
CREATE INDEX idx_request_logs_model ON request_logs(model);
CREATE INDEX idx_request_logs_provider_name ON request_logs(provider_name);
CREATE INDEX idx_request_logs_protocol ON request_logs(protocol);
CREATE INDEX idx_request_logs_operation ON request_logs(operation);
CREATE INDEX idx_request_logs_status_code ON request_logs(status_code);
CREATE INDEX idx_request_logs_status ON request_logs(status);
CREATE INDEX idx_request_logs_run_id ON request_logs(run_id);
