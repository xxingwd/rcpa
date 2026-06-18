import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { apiFetch } from '../utils/api';
import {
  formatCostCny,
  formatCount,
  formatDuration,
  formatPercent,
  formatTokenCount,
  keyDisplayName,
  modelDisplayName,
} from '../utils/display';
import { REFRESH_INTERVAL_OPTIONS } from '../utils/timeControls';
import { Button } from './ui/button';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from './ui/table';
import { Badge } from './ui/badge';
import { CodeBlock } from './ui/code';
import CopyableHoverText from './CopyableHoverText';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './ui/select';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Eye, FileText, RefreshCw } from 'lucide-react';

const PAGE_SIZE = 20;

function formatJson(value) {
  if (value === null || value === undefined) return '';
  if (typeof value === 'string') return value;
  return JSON.stringify(value, null, 2);
}

function cacheRate(inputTokens, cachedTokens) {
  const input = Number(inputTokens ?? 0);
  return input > 0 ? Number(cachedTokens ?? 0) / input : 0;
}

function generationLatencyMs(latencyMs, firstByteLatencyMs) {
  return Math.max(0, Number(latencyMs ?? 0) - Number(firstByteLatencyMs ?? 0));
}

function formatTps(outputTokens, latencyMs, firstByteLatencyMs) {
  const latencySeconds = generationLatencyMs(latencyMs, firstByteLatencyMs) / 1000;
  if (!latencySeconds) return '0.0';
  return (Number(outputTokens ?? 0) / latencySeconds).toFixed(2);
}

function formatTpot(outputTokens, latencyMs, firstByteLatencyMs) {
  const tokens = Number(outputTokens ?? 0);
  if (!tokens) return '0ms';
  return formatDuration(generationLatencyMs(latencyMs, firstByteLatencyMs) / tokens);
}

export default function LogsView({ showToast }) {
  const [searchParams, setSearchParams] = useSearchParams();
  const filtersInitializedRef = useRef(false);
  const fetchInFlightRef = useRef(false);
  const [logs, setLogs] = useState([]);
  const [keysList, setKeysList] = useState([]);
  const [providersList, setProvidersList] = useState([]);
  const [modelCatalog, setModelCatalog] = useState([]);
  const [filterKeyId, setFilterKeyId] = useState(searchParams.get('key') || 'all');
  const [filterModel, setFilterModel] = useState(searchParams.get('model') || 'all');
  const [filterProviderName, setFilterProviderName] = useState(searchParams.get('provider_name') || 'all');
  const [page, setPage] = useState(() => Math.max(1, Number(searchParams.get('page') || 1) || 1));
  const [total, setTotal] = useState(0);
  const [refreshIntervalMs, setRefreshIntervalMs] = useState(() => localStorage.getItem('rcpa_logs_refresh_ms') || '5000');
  const [detailOpen, setDetailOpen] = useState(false);
  const [detail, setDetail] = useState(null);
  const [detailLoading, setDetailLoading] = useState(false);

  const fetchFilters = useCallback(async () => {
    try {
      const [keysRes, providersRes, modelRes] = await Promise.allSettled([
        apiFetch('/v1/admin/keys'),
        apiFetch('/v1/admin/providers'),
        apiFetch('/v1/admin/analytics/model'),
      ]);

      if (keysRes.status === 'fulfilled' && keysRes.value.ok) {
        const data = await keysRes.value.json();
        setKeysList(Array.isArray(data) ? data : []);
      }

      if (providersRes.status === 'fulfilled' && providersRes.value.ok) {
        const data = await providersRes.value.json();
        setProvidersList(Array.isArray(data) ? data : []);
      }

      if (modelRes.status === 'fulfilled' && modelRes.value.ok) {
        const data = await modelRes.value.json();
        const models = Array.isArray(data)
          ? data.map((row) => row?.group_key).filter((value) => typeof value === 'string' && value.length > 0)
          : [];
        setModelCatalog(models);
      }
    } catch { /* ignore */ }
  }, []);

  const fetchLogs = useCallback(async () => {
    if (fetchInFlightRef.current) return;
    fetchInFlightRef.current = true;
    try {
      const params = new URLSearchParams({
        limit: String(PAGE_SIZE),
        offset: String((page - 1) * PAGE_SIZE),
      });
      if (filterKeyId !== 'all') {
        params.set('api_key_id', filterKeyId);
      }
      if (filterModel !== 'all') {
        params.set('model', filterModel);
      }
      if (filterProviderName !== 'all') {
        params.set('provider_name', filterProviderName);
      }
      const res = await apiFetch(`/v1/admin/logs?${params.toString()}`);
      if (res.ok) {
        const data = await res.json();
        const items = Array.isArray(data?.items) ? data.items : [];
        setLogs(items);
        setTotal(Number(data?.total ?? 0));
      }
    } catch {
      showToast('获取审计日志失败', 'error');
    } finally {
      fetchInFlightRef.current = false;
    }
  }, [filterKeyId, filterModel, filterProviderName, page, showToast]);

  useEffect(() => {
    fetchFilters();
  }, [fetchFilters]);

  useEffect(() => {
    fetchLogs();
  }, [fetchLogs]);

  useEffect(() => {
    localStorage.setItem('rcpa_logs_refresh_ms', refreshIntervalMs);
  }, [refreshIntervalMs]);

  useEffect(() => {
    const intervalMs = Number(refreshIntervalMs);
    if (!Number.isFinite(intervalMs) || intervalMs <= 0) return undefined;
    const interval = setInterval(fetchLogs, Math.max(1000, intervalMs));
    return () => clearInterval(interval);
  }, [fetchLogs, refreshIntervalMs]);

  useEffect(() => {
    const next = new URLSearchParams();
    if (filterKeyId !== 'all') next.set('key', filterKeyId);
    if (filterModel !== 'all') next.set('model', filterModel);
    if (filterProviderName !== 'all') next.set('provider_name', filterProviderName);
    if (page > 1) next.set('page', String(page));
    setSearchParams(next, { replace: true });
  }, [filterKeyId, filterModel, filterProviderName, page, setSearchParams]);

  useEffect(() => {
    if (!filtersInitializedRef.current) {
      filtersInitializedRef.current = true;
      return;
    }

    setPage(1);
  }, [filterKeyId, filterModel, filterProviderName]);

  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  useEffect(() => {
    if (page > pageCount) {
      setPage(pageCount);
    }
  }, [page, pageCount]);

  const keyOptions = useMemo(
    () => [
      { value: 'all', label: '全部 Key' },
      ...keysList.map((key) => ({
        value: key.id,
        label: keyDisplayName(key),
      })),
    ],
    [keysList]
  );

  const providerNameOptions = useMemo(() => {
    return [
      { value: 'all', label: '全部供应商' },
      ...providersList.map((provider) => ({ value: provider.name, label: provider.name })),
    ];
  }, [providersList]);

  const modelOptions = useMemo(() => {
    const seen = new Set();
    const values = [];

    modelCatalog.forEach((model) => {
      if (!seen.has(model)) {
        seen.add(model);
        values.push(model);
      }
    });

    logs.forEach((log) => {
      if (log?.model && !seen.has(log.model)) {
        seen.add(log.model);
        values.push(log.model);
      }
    });

    return [
      { value: 'all', label: '全部模型' },
      ...values.map((model) => ({ value: model, label: modelDisplayName(model, 28) })),
    ];
  }, [logs, modelCatalog]);

  const pageButtons = useMemo(() => {
    const windowSize = 5;
    const start = Math.max(1, Math.min(page - 2, pageCount - windowSize + 1));
    const end = Math.min(pageCount, start + windowSize - 1);
    const pages = [];

    for (let value = start; value <= end; value += 1) {
      pages.push(value);
    }

    return pages;
  }, [page, pageCount]);

  const firstRowNumber = total === 0 ? 0 : (page - 1) * PAGE_SIZE + 1;
  const lastRowNumber = Math.min(page * PAGE_SIZE, total);
  const detailMatchedKey = detail ? keysList.find((key) => key.id === detail.api_key_id) : null;

  const openDetail = async (logId) => {
    setDetailOpen(true);
    setDetail(null);
    setDetailLoading(true);
    try {
      const res = await apiFetch(`/v1/admin/logs/${encodeURIComponent(logId)}`);
      if (!res.ok) throw new Error('detail request failed');
      const data = await res.json();
      setDetail(data);
    } catch {
      showToast('获取日志详情失败', 'error');
      setDetailOpen(false);
    } finally {
      setDetailLoading(false);
    }
  };

  return (
    <>
    <Card className="animate-in fade-in duration-500 h-full min-h-0 flex flex-col overflow-hidden">
      <CardHeader className="shrink-0 border-b py-4">
        <div className="flex flex-col gap-4">
          <CardTitle className="flex items-center gap-2">
            <FileText size={18} className="text-primary" />
            调用审计日志
          </CardTitle>

          <div className="flex flex-wrap items-center gap-2">
            <Select value={filterKeyId} onValueChange={setFilterKeyId}>
              <SelectTrigger className="h-8 w-[160px] text-xs">
                <SelectValue placeholder="全部 Key" />
              </SelectTrigger>
              <SelectContent>
                {keyOptions.map((option) => (
                  <SelectItem key={option.value} value={option.value} className="text-xs">
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Select value={filterModel} onValueChange={setFilterModel}>
              <SelectTrigger className="h-8 w-[180px] text-xs">
                <SelectValue placeholder="全部模型" />
              </SelectTrigger>
              <SelectContent>
                {modelOptions.map((option) => (
                  <SelectItem key={option.value} value={option.value} className="text-xs" title={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Select value={filterProviderName} onValueChange={setFilterProviderName}>
              <SelectTrigger className="h-8 w-[160px] text-xs">
                <SelectValue placeholder="全部供应商" />
              </SelectTrigger>
              <SelectContent>
                {providerNameOptions.map((option) => (
                  <SelectItem key={option.value} value={option.value} className="text-xs">
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Select value={refreshIntervalMs} onValueChange={setRefreshIntervalMs}>
              <SelectTrigger className="h-8 w-[92px] text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {REFRESH_INTERVAL_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value} className="text-xs">
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Button
              variant="outline"
              size="sm"
              onClick={fetchLogs}
              className="h-8 px-3 text-xs"
            >
              <RefreshCw size={12} />
              刷新
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="flex-1 min-h-0 p-0">
        <div className="flex h-full min-h-0 flex-col">
          <div className="min-h-0 flex-1 overflow-auto px-6 pt-3">
          <Table className="min-w-[1320px] text-sm [&_th]:h-10 [&_th]:px-3 [&_td]:px-3 [&_td]:py-3">
            <TableHeader>
              <TableRow>
                <TableHead>时间</TableHead>
                <TableHead>请求 ID</TableHead>
                <TableHead>Key</TableHead>
                <TableHead>供应商</TableHead>
                <TableHead>Model</TableHead>
                <TableHead>Input</TableHead>
                <TableHead>Output</TableHead>
                <TableHead>Cache</TableHead>
                <TableHead>CHR</TableHead>
                <TableHead>TPS</TableHead>
                <TableHead>TTFT</TableHead>
                <TableHead>TPOT</TableHead>
                <TableHead>耗时</TableHead>
                <TableHead>价格</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>详情</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {logs.length > 0 ? (
                logs.map((l, index) => {
                  const isErr = l.status_code >= 400;
                  const formattedTime = l.created_at.replace('T', ' ').substring(0, 19);
                  const matchedKey = keysList.find((key) => key.id === l.api_key_id);
                  const keyText = keyDisplayName(matchedKey, l.key_display_name || l.api_key_id);
                  const firstByte = l.first_byte_latency_ms ?? 0;
                  const inputTokens = l.input_tokens ?? 0;
                  const outputTokens = l.output_tokens ?? 0;
                  const cachedTokens = l.cached_tokens ?? 0;
                  const hitRate = cacheRate(inputTokens, cachedTokens);

                  return (
                    <TableRow key={l.id} className={index === 0 ? 'log-row' : ''}>
                      <TableCell className="whitespace-nowrap text-muted-foreground">{formattedTime}</TableCell>
                      <TableCell className="font-mono text-muted-foreground whitespace-nowrap">{l.request_id.substring(0, 8)}</TableCell>
                      <TableCell className="w-[6.5rem] max-w-[6.5rem] pr-1">
                        <CopyableHoverText value={keyText}>
                          <Badge variant="secondary" className="block max-w-full truncate font-mono px-2 py-0.5 text-sm">
                            {keyText}
                          </Badge>
                        </CopyableHoverText>
                      </TableCell>
                      <TableCell className="max-w-[140px] pl-1">
                        <CopyableHoverText value={l.provider_name}>
                          <Badge variant="outline" className="block max-w-full truncate font-mono px-2.5 py-0.5 text-sm">
                            {l.provider_name}
                          </Badge>
                        </CopyableHoverText>
                      </TableCell>
                      <TableCell className="max-w-[160px] pl-1">
                        <CopyableHoverText value={l.model}>
                          <Badge variant="outline" className="block max-w-full truncate font-mono px-2.5 py-0.5 text-sm">
                            {modelDisplayName(l.model, 24)}
                          </Badge>
                        </CopyableHoverText>
                      </TableCell>
                      <TableCell className="font-mono whitespace-nowrap">{formatTokenCount(inputTokens)}</TableCell>
                      <TableCell className="font-mono whitespace-nowrap">{formatTokenCount(outputTokens)}</TableCell>
                      <TableCell className="font-mono whitespace-nowrap">{formatTokenCount(cachedTokens)}</TableCell>
                      <TableCell className="font-mono whitespace-nowrap">{formatPercent(hitRate)}</TableCell>
                      <TableCell className="font-mono whitespace-nowrap">{formatTps(outputTokens, l.latency_ms, firstByte)}</TableCell>
                      <TableCell className="font-mono whitespace-nowrap">{formatDuration(firstByte)}</TableCell>
                      <TableCell className="font-mono whitespace-nowrap">{formatTpot(outputTokens, l.latency_ms, firstByte)}</TableCell>
                      <TableCell className="font-mono whitespace-nowrap">{formatDuration(l.latency_ms)}</TableCell>
                      <TableCell className="font-mono whitespace-nowrap">{formatCostCny(l.cost_cents)}</TableCell>
                      <TableCell>
                        <Badge variant={isErr ? 'destructive' : 'success'} className="font-mono px-3 py-1 text-sm">
                          {l.status_code}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Button
                          type="button"
                          variant="outline"
                          size="icon"
                          className="h-8 w-8"
                          onClick={() => openDetail(l.id)}
                        >
                          <Eye size={15} />
                        </Button>
                      </TableCell>
                    </TableRow>
                  );
                })
              ) : (
                <TableRow>
                  <TableCell colSpan={16} className="text-center text-muted-foreground py-8">暂无接口请求记录</TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          </div>
          <div className="shrink-0 border-t px-6 py-3">
            <div className="flex flex-col gap-3 text-xs text-muted-foreground md:flex-row md:items-center md:justify-between">
              <div>
                第 <span className="font-mono text-foreground">{firstRowNumber}</span>
                {' - '}
                <span className="font-mono text-foreground">{lastRowNumber}</span> 条，共{' '}
                <span className="font-mono text-foreground">{formatCount(total)}</span> 条
              </div>
              <div className="flex items-center gap-1">
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="h-8 w-8"
                  disabled={page <= 1}
                  onClick={() => setPage(1)}
                >
                  <ChevronsLeft size={14} />
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="h-8 w-8"
                  disabled={page <= 1}
                  onClick={() => setPage((current) => Math.max(1, current - 1))}
                >
                  <ChevronLeft size={14} />
                </Button>

                {pageButtons.map((pageNumber) => (
                  <Button
                    key={pageNumber}
                    type="button"
                    variant={pageNumber === page ? 'default' : 'outline'}
                    size="sm"
                    className="h-8 min-w-8 px-2 font-mono"
                    onClick={() => setPage(pageNumber)}
                  >
                    {pageNumber}
                  </Button>
                ))}

                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="h-8 w-8"
                  disabled={page >= pageCount}
                  onClick={() => setPage((current) => Math.min(pageCount, current + 1))}
                >
                  <ChevronRight size={14} />
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="h-8 w-8"
                  disabled={page >= pageCount}
                  onClick={() => setPage(pageCount)}
                >
                  <ChevronsRight size={14} />
                </Button>
              </div>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
    <Dialog open={detailOpen} onOpenChange={setDetailOpen}>
      <DialogContent className="max-w-[980px] max-h-[calc(100vh-2rem)] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FileText size={18} className="text-primary" />
            调用日志详情
          </DialogTitle>
        </DialogHeader>

        {detailLoading ? (
          <div className="text-center text-muted-foreground py-8">加载中...</div>
        ) : detail ? (
          <div className="space-y-5">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">状态码</div>
                <div className={`font-mono text-sm ${detail.status_code >= 400 ? 'text-destructive' : 'text-emerald-600 dark:text-emerald-400'}`}>{detail.status_code}</div>
              </div>
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">耗时</div>
                <div className="font-mono text-sm">{formatDuration(detail.latency_ms)}</div>
              </div>
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">TTFT</div>
                <div className="font-mono text-sm">{formatDuration(detail.first_byte_latency_ms)}</div>
              </div>
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">输入 / 输出</div>
                <div className="font-mono text-sm">{formatTokenCount(detail.input_tokens)} / {formatTokenCount(detail.output_tokens)}</div>
              </div>
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">缓存命中 / 写入</div>
                <div className="font-mono text-sm">{formatTokenCount(detail.cached_tokens)} / {formatTokenCount(detail.cache_write_tokens)}</div>
              </div>
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">总 Tokens</div>
                <div className="font-mono text-sm">{formatTokenCount(detail.total_tokens)}</div>
              </div>
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">CHR</div>
                <div className="font-mono text-sm">{formatPercent(cacheRate(detail.input_tokens, detail.cached_tokens ?? 0))}</div>
              </div>
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">TPS</div>
                <div className="font-mono text-sm">{formatTps(detail.output_tokens, detail.latency_ms, detail.first_byte_latency_ms)}</div>
              </div>
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">TPOT</div>
                <div className="font-mono text-sm">{formatTpot(detail.output_tokens, detail.latency_ms, detail.first_byte_latency_ms)}</div>
              </div>
              <div className="rounded-lg border bg-muted/30 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground mb-1">价格</div>
                <div className="font-mono text-sm">{formatCostCny(detail.cost_cents)}</div>
              </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
              <div><span className="text-muted-foreground">请求 ID：</span><span className="font-mono break-all">{detail.request_id}</span></div>
              <div><span className="text-muted-foreground">日志 ID：</span><span className="font-mono break-all">{detail.id}</span></div>
              <div>
                <span className="text-muted-foreground">模型：</span>
                <CopyableHoverText value={detail.model} triggerClassName="inline-block max-w-full align-middle">
                  <Badge variant="outline" className="font-mono">{modelDisplayName(detail.model, 28)}</Badge>
                </CopyableHoverText>
              </div>
              <div>
                <span className="text-muted-foreground">Key：</span>
                <CopyableHoverText value={keyDisplayName(detailMatchedKey, detail.key_display_name || detail.api_key_id)} triggerClassName="inline-block max-w-full align-middle">
                  <Badge variant="secondary" className="font-mono">{keyDisplayName(detailMatchedKey, detail.key_display_name || detail.api_key_id)}</Badge>
                </CopyableHoverText>
              </div>
              <div><span className="text-muted-foreground">供应商：</span>{detail.provider_name}</div>
              <div><span className="text-muted-foreground">接口：</span>{detail.operation} / {detail.provider}</div>
              <div><span className="text-muted-foreground">时间：</span>{detail.created_at}</div>
            </div>

            {(detail.error_code || detail.error) && (
              <div className="rounded-lg border border-destructive/25 bg-destructive/10 p-3">
                <div className="text-[0.65rem] uppercase tracking-wider text-destructive font-semibold mb-2">错误</div>
                <div className="font-mono text-xs break-all">{detail.error_code || 'unknown'}</div>
                <div className="text-xs text-muted-foreground mt-1 break-all">{detail.error}</div>
              </div>
            )}

            <div>
              <div className="text-[0.65rem] uppercase tracking-wider text-muted-foreground font-semibold mb-2">请求 JSON</div>
              <CodeBlock className="max-h-[420px]">{formatJson(detail.request_body) || '无请求体'}</CodeBlock>
            </div>
          </div>
        ) : (
          <div className="text-center text-muted-foreground py-8">暂无详情</div>
        )}
      </DialogContent>
    </Dialog>
    </>
  );
}
