-- Keep daily body retention scans off rows whose bodies have already been cleared.
CREATE INDEX idx_request_logs_body_gc
ON request_logs (status, created_at)
WHERE request_body IS NOT NULL OR response_body IS NOT NULL;
