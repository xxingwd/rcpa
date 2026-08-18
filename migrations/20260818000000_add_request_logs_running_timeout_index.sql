-- Keep timeout monitoring proportional to the small set of running requests.
CREATE INDEX idx_request_logs_running_timeout
ON request_logs (created_at)
WHERE status = 'running';
