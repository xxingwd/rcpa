CREATE TABLE request_logs (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    api_key_id TEXT NOT NULL,
    session_hash TEXT,
    provider_name TEXT NOT NULL,
    protocol TEXT NOT NULL,
    model TEXT NOT NULL,
    operation TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    success INTEGER NOT NULL DEFAULT 0,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    cached_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    cost_cents INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    first_byte_latency_ms INTEGER NOT NULL DEFAULT 0,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    request_body BLOB,
    response_body BLOB
);

CREATE INDEX idx_request_logs_created_at ON request_logs(created_at);
CREATE INDEX idx_request_logs_api_key_id ON request_logs(api_key_id);
CREATE INDEX idx_request_logs_session_hash ON request_logs(session_hash);
CREATE INDEX idx_request_logs_model ON request_logs(model);
CREATE INDEX idx_request_logs_provider_name ON request_logs(provider_name);
CREATE INDEX idx_request_logs_protocol ON request_logs(protocol);
CREATE INDEX idx_request_logs_status_code ON request_logs(status_code);
CREATE INDEX idx_request_logs_success ON request_logs(success);
