-- Keep high-frequency dashboard and list queries away from the request/response
-- BLOBs in request_logs. The full record remains the source for audit details.
CREATE TABLE request_log_metrics (
    id TEXT PRIMARY KEY REFERENCES request_logs(id) ON DELETE CASCADE,
    api_key_id TEXT NOT NULL,
    session_hash TEXT,
    provider_name TEXT NOT NULL,
    protocol TEXT NOT NULL,
    model TEXT NOT NULL,
    status TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cached_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    cost_cents INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    first_byte_latency_ms INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

INSERT INTO request_log_metrics (
    id,
    api_key_id,
    session_hash,
    provider_name,
    protocol,
    model,
    status,
    status_code,
    input_tokens,
    output_tokens,
    cached_tokens,
    cache_write_tokens,
    cost_cents,
    latency_ms,
    first_byte_latency_ms,
    created_at
)
SELECT
    id,
    api_key_id,
    session_hash,
    provider_name,
    protocol,
    model,
    status,
    status_code,
    input_tokens,
    output_tokens,
    COALESCE(CAST(json_extract(meta, '$.usage.cached_tokens') AS INTEGER), 0),
    COALESCE(CAST(json_extract(meta, '$.usage.cache_write_tokens') AS INTEGER), 0),
    cost_cents,
    latency_ms,
    first_byte_latency_ms,
    created_at
FROM request_logs;

CREATE INDEX idx_request_log_metrics_analytics_created_at
ON request_log_metrics (
    created_at,
    api_key_id,
    provider_name,
    model,
    status,
    input_tokens,
    output_tokens,
    cached_tokens,
    cache_write_tokens,
    cost_cents,
    latency_ms,
    first_byte_latency_ms
);

CREATE INDEX idx_request_log_metrics_api_key_created_at
ON request_log_metrics (api_key_id, created_at DESC);

CREATE INDEX idx_request_log_metrics_model_created_at
ON request_log_metrics (model, created_at DESC);

CREATE INDEX idx_request_log_metrics_provider_created_at
ON request_log_metrics (provider_name, created_at DESC);

CREATE INDEX idx_request_log_metrics_status_created_at
ON request_log_metrics (status, created_at DESC);

CREATE INDEX idx_request_log_metrics_status_code_created_at
ON request_log_metrics (status_code, created_at DESC);

CREATE INDEX idx_request_log_metrics_protocol_created_at
ON request_log_metrics (protocol, created_at DESC);

CREATE INDEX idx_request_log_metrics_session_created_at
ON request_log_metrics (session_hash, created_at DESC);
