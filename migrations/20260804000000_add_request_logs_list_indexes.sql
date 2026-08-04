-- 日志列表慢查询修复：为列表查询的常见过滤列增加 (列, created_at) 复合索引。
--
-- 之前的单列索引无法同时满足 WHERE col = ? 和 ORDER BY created_at DESC，
-- SQLite 只能扫描全部匹配行并做临时 B-Tree 排序；表体量增大（含请求/响应
-- body BLOB）后单次 LIMIT 20 的列表查询可达数十秒。
--
-- 复合索引让 SQLite 直接按 created_at 逆序遍历索引并提前停止，无需排序。

CREATE INDEX idx_request_logs_model_created_at ON request_logs(model, created_at);
CREATE INDEX idx_request_logs_api_key_created_at ON request_logs(api_key_id, created_at);
CREATE INDEX idx_request_logs_provider_created_at ON request_logs(provider_name, created_at);
CREATE INDEX idx_request_logs_protocol_created_at ON request_logs(protocol, created_at);

-- 状态列已有 (status, created_at) 的 body GC 索引（20260723020000），
-- 可直接覆盖状态筛选，无需新建；删除单列状态索引避免冗余。

DROP INDEX idx_request_logs_status;

-- 以下单列索引已成为新复合索引的前缀，删除以控制写放大和索引体积：
DROP INDEX idx_request_logs_model;
DROP INDEX idx_request_logs_api_key_id;
DROP INDEX idx_request_logs_provider_name;
DROP INDEX idx_request_logs_protocol;
