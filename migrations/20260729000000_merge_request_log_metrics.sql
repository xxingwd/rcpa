ALTER TABLE request_logs
ADD COLUMN cached_tokens INTEGER NOT NULL DEFAULT 0;

ALTER TABLE request_logs
ADD COLUMN cache_write_tokens INTEGER NOT NULL DEFAULT 0;

ALTER TABLE request_logs
ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

UPDATE request_logs
SET cached_tokens = (
        SELECT request_log_metrics.cached_tokens
        FROM request_log_metrics
        WHERE request_log_metrics.id = request_logs.id
    ),
    cache_write_tokens = (
        SELECT request_log_metrics.cache_write_tokens
        FROM request_log_metrics
        WHERE request_log_metrics.id = request_logs.id
    ),
    model = (
        SELECT request_log_metrics.model
        FROM request_log_metrics
        WHERE request_log_metrics.id = request_logs.id
    ),
    updated_at = COALESCE(finished_at, created_at)
WHERE EXISTS (
    SELECT 1
    FROM request_log_metrics
    WHERE request_log_metrics.id = request_logs.id
);

UPDATE request_logs
SET updated_at = COALESCE(updated_at, finished_at, created_at);

DROP TABLE request_log_metrics;

CREATE INDEX idx_request_logs_analytics_created_at
ON request_logs (
    created_at,
    status,
    api_key_id,
    provider_name,
    protocol,
    model,
    status_code,
    input_tokens,
    output_tokens,
    cached_tokens,
    cache_write_tokens,
    cost_cents,
    latency_ms,
    first_byte_latency_ms
);
