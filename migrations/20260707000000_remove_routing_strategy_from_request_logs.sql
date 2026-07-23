-- routing.strategy was removed from runtime configuration. Historical request
-- log metadata must be normalized instead of keeping compatibility for it.
UPDATE request_logs
SET metadata_json = json_remove(metadata_json, '$.routing.strategy')
WHERE json_valid(metadata_json)
  AND json_type(metadata_json, '$.routing.strategy') IS NOT NULL;
