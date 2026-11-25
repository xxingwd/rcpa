-- Dashboard performance metrics must not fetch request/response body pages.
-- Store the small JSON-derived values in the index, not the full metadata.
DROP INDEX idx_request_logs_analytics_created_at;

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
    first_byte_latency_ms,
    retry_count,
    CAST(json_extract(meta, '$.request.stream') AS INTEGER),
    (json_extract(meta, '$.error.code') IS NOT NULL OR
     json_extract(meta, '$.error.message') IS NOT NULL)
);
