-- The audit metadata has always recorded the provider's actual model, but the
-- query-facing metrics row previously stored the resolved public alias. Repair
-- the narrow metrics table without rewriting request_logs and its large BLOBs.
UPDATE request_log_metrics
SET model = (
    SELECT json_extract(request_logs.meta, '$.models.provider')
    FROM request_logs
    WHERE request_logs.id = request_log_metrics.id
)
WHERE provider_name <> 'unrouted'
  AND EXISTS (
      SELECT 1
      FROM request_logs
      WHERE request_logs.id = request_log_metrics.id
        AND json_valid(request_logs.meta)
        AND json_type(request_logs.meta, '$.models.provider') = 'text'
        AND trim(json_extract(request_logs.meta, '$.models.provider')) <> ''
        AND request_log_metrics.model <> json_extract(request_logs.meta, '$.models.provider')
  );
