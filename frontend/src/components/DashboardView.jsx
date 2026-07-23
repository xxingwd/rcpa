import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { apiFetch } from '../utils/api';
import {
  compactNumber,
  compactText,
  formatCostCny,
  formatCount,
  formatDuration,
  formatPercent,
  formatTokenCount,
  keyDisplayName,
  modelDisplayName,
} from '../utils/display';
import {
  appendTimeRangeParams,
  REFRESH_INTERVAL_OPTIONS,
  TIME_RANGE_OPTIONS,
} from '../utils/timeControls';
import { Button } from './ui/button';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from './ui/table';
import { Badge } from './ui/badge';
import CopyableHoverText from './CopyableHoverText';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './ui/select';
import { Bar } from 'react-chartjs-2';
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  BarElement,
  Tooltip,
  Legend,
} from 'chart.js';
import { Activity, Cpu, RefreshCw, Server, Workflow } from 'lucide-react';

ChartJS.register(CategoryScale, LinearScale, BarElement, Tooltip, Legend);

const emptyStats = {
  request_count: 0,
  success_count: 0,
  success_rate: 0,
  total_input_tokens: 0,
  total_output_tokens: 0,
  total_cached_tokens: 0,
  total_cache_write_tokens: 0,
  cache_hit_rate: 0,
  total_tokens: 0,
  avg_tokens_per_request: 0,
  total_cost_cents: 0,
  avg_latency_ms: 0,
  max_latency_ms: 0,
  avg_first_byte_latency_ms: 0,
  max_first_byte_latency_ms: 0,
  error_count: 0,
};

const DEFAULT_TIME_RANGE = 'today';
const DEFAULT_REFRESH_INTERVAL_MS = '5000';
const OPERATIONS_DIMENSIONS = [
  { value: 'key', label: 'Key' },
  { value: 'provider', label: '供应商' },
];
const TRAFFIC_DIMENSIONS = [
  { value: 'model', label: '模型' },
  { value: 'protocol', label: '协议' },
  { value: 'status_code', label: '状态码' },
];

function cacheRate(inputTokens, cachedTokens) {
  return Number(inputTokens ?? 0) > 0 ? Number(cachedTokens ?? 0) / Number(inputTokens ?? 0) : 0;
}

function aggregateSuccessRate(row) {
  const value = Number(row?.success_rate);
  if (Number.isFinite(value)) return value;
  const requests = Number(row?.request_count ?? 0);
  return requests > 0 ? Number(row?.success_count ?? 0) / requests : 0;
}

function DimensionSwitch({ value, onChange, options }) {
  return (
    <div role="tablist" className="inline-flex shrink-0 items-center rounded-md border bg-muted/40 p-0.5">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="tab"
          aria-selected={value === option.value}
          className={`h-6 rounded px-2 text-[11px] font-medium transition-colors ${
            value === option.value
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'
          }`}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function AnalyticsTable({ rows, dimension, keysList }) {
  const firstColumn = {
    key: 'API 密钥',
    provider: '供应商',
    model: '上游模型',
    protocol: '协议',
    status_code: '状态码',
  }[dimension];
  const showFirstByte = dimension === 'model';

  const groupValue = (row) => {
    if (dimension === 'key') {
      const matchedKey = keysList.find((key) => key.id === row.group_key);
      return keyDisplayName(matchedKey, row.key_display_name || row.group_key);
    }
    return row.group_key;
  };

  const badgeVariant = (row) => {
    if (dimension === 'key') return 'secondary';
    if (dimension === 'status_code' && Number(row.group_key) >= 400) return 'destructive';
    return 'outline';
  };

  return (
    <Table className="min-w-[640px] text-xs">
      <TableHeader>
        <TableRow>
          <TableHead>{firstColumn}</TableHead>
          <TableHead>请求数</TableHead>
          <TableHead>成功率</TableHead>
          <TableHead>Tokens</TableHead>
          <TableHead>缓存</TableHead>
          <TableHead>CHR</TableHead>
          <TableHead>{showFirstByte ? '首字节' : '平均延迟'}</TableHead>
          <TableHead>费用</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.length > 0 ? (
          rows.map((row) => {
            const value = groupValue(row);
            const display = dimension === 'model'
              ? modelDisplayName(value, 22)
              : compactText(value, 22);
            const latency = showFirstByte ? row.avg_first_byte_latency_ms : row.avg_latency_ms;
            return (
              <TableRow key={row.group_key}>
                <TableCell className="w-[8rem] max-w-[10rem]">
                  <CopyableHoverText value={value}>
                    <Badge variant={badgeVariant(row)} className="block max-w-full truncate px-2 py-0 font-mono text-xs">
                      {display}
                    </Badge>
                  </CopyableHoverText>
                </TableCell>
                <TableCell>{formatCount(row.request_count)}</TableCell>
                <TableCell>{formatPercent(aggregateSuccessRate(row))}</TableCell>
                <TableCell>{formatTokenCount(row.total_tokens)}</TableCell>
                <TableCell>{formatTokenCount(row.total_cached_tokens)}</TableCell>
                <TableCell>{formatPercent(cacheRate(row.total_input_tokens, row.total_cached_tokens))}</TableCell>
                <TableCell className="font-mono">{formatDuration(latency)}</TableCell>
                <TableCell className="font-mono">{formatCostCny(row.total_cost_cents)}</TableCell>
              </TableRow>
            );
          })
        ) : (
          <TableRow>
            <TableCell colSpan={8} className="py-8 text-center text-muted-foreground">暂无统计记录</TableCell>
          </TableRow>
        )}
      </TableBody>
    </Table>
  );
}

function MetricItem({ label, value }) {
  return (
    <div className="min-w-0">
      <div className="text-[0.68rem] uppercase tracking-wider text-muted-foreground mb-1">{label}</div>
      <div className="font-mono text-sm font-semibold truncate">{value}</div>
    </div>
  );
}

function startOfLocalDay(date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function startOfLocalWeek(date) {
  const start = startOfLocalDay(date);
  const day = start.getDay() || 7;
  start.setDate(start.getDate() - day + 1);
  return start;
}

function startOfLocalMonth(date) {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function addHours(date, hours) {
  return new Date(date.getTime() + hours * 60 * 60 * 1000);
}

function addDays(date, days) {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
}

function parseHourBucket(value) {
  const date = new Date(`${value}:00:00+00:00`);
  return Number.isNaN(date.getTime()) ? null : date;
}

function parseDayBucket(value) {
  const date = new Date(`${value}T00:00:00+00:00`);
  return Number.isNaN(date.getTime()) ? null : date;
}

function dayLabel(date) {
  return new Intl.DateTimeFormat('zh-CN', { month: '2-digit', day: '2-digit' }).format(date);
}

function hourLabel(date, hours) {
  const start = date.getHours().toString().padStart(2, '0');
  const end = ((date.getHours() + hours) % 24).toString().padStart(2, '0');
  return `${start}-${end}`;
}

function addAggregate(target, row) {
  target.request_count += Number(row.request_count ?? 0);
  target.success_count += Number(row.success_count ?? 0);
  target.error_count += Number(row.error_count ?? 0);
  target.total_input_tokens += Number(row.total_input_tokens ?? 0);
  target.total_output_tokens += Number(row.total_output_tokens ?? 0);
  target.total_cached_tokens += Number(row.total_cached_tokens ?? 0);
  target.total_cache_write_tokens += Number(row.total_cache_write_tokens ?? 0);
  target.total_tokens += Number(row.total_tokens ?? 0);
}

function emptyBucket(label, start = null, end = null) {
  return {
    label,
    start,
    end,
    request_count: 0,
    success_count: 0,
    error_count: 0,
    total_input_tokens: 0,
    total_output_tokens: 0,
    total_cached_tokens: 0,
    total_cache_write_tokens: 0,
    total_tokens: 0,
  };
}

function makeTimeBuckets(timeRange, rows) {
  const now = new Date();

  if (timeRange === 'all') {
    return [...rows]
      .sort((a, b) => String(a.group_key).localeCompare(String(b.group_key)))
      .map((row) => {
        const date = parseDayBucket(row.group_key);
        const bucket = emptyBucket(date ? dayLabel(date) : row.group_key);
        addAggregate(bucket, row);
        return bucket;
      });
  }

  let start;
  let count;
  let stepHours;
  let labeler;

  switch (timeRange) {
    case '1h':
      start = addHours(now, -1);
      count = 1;
      stepHours = 1;
      labeler = (date) => hourLabel(date, 1);
      break;
    case '6h':
      start = addHours(now, -6);
      count = 6;
      stepHours = 1;
      labeler = (date) => hourLabel(date, 1);
      break;
    case '12h':
      start = addHours(now, -12);
      count = 6;
      stepHours = 2;
      labeler = (date) => hourLabel(date, 2);
      break;
    case 'yesterday': {
      const today = startOfLocalDay(now);
      start = addDays(today, -1);
      count = 12;
      stepHours = 2;
      labeler = (date) => hourLabel(date, 2);
      break;
    }
    case 'this_week':
      start = startOfLocalWeek(now);
      count = 7;
      stepHours = 24;
      labeler = dayLabel;
      break;
    case 'last_week':
      start = addDays(startOfLocalWeek(now), -7);
      count = 7;
      stepHours = 24;
      labeler = dayLabel;
      break;
    case 'this_month':
      start = startOfLocalMonth(now);
      count = now.getDate();
      stepHours = 24;
      labeler = dayLabel;
      break;
    case 'last_month': {
      start = new Date(now.getFullYear(), now.getMonth() - 1, 1);
      const nextMonth = new Date(now.getFullYear(), now.getMonth(), 1);
      count = Math.round((nextMonth.getTime() - start.getTime()) / (24 * 60 * 60 * 1000));
      stepHours = 24;
      labeler = dayLabel;
      break;
    }
    case 'today':
    default:
      start = startOfLocalDay(now);
      count = 12;
      stepHours = 2;
      labeler = (date) => hourLabel(date, 2);
      break;
  }

  const buckets = Array.from({ length: count }, (_, index) => {
    const bucketStart = addHours(start, index * stepHours);
    return emptyBucket(labeler(bucketStart), bucketStart, addHours(bucketStart, stepHours));
  });

  rows.forEach((row) => {
    const rowDate = parseHourBucket(row.group_key);
    if (!rowDate) return;
    const bucket = buckets.find((item) => rowDate >= item.start && rowDate < item.end);
    if (bucket) addAggregate(bucket, row);
  });

  return buckets;
}

export default function DashboardView() {
  const [uptime, setUptime] = useState(0);
  const [stats, setStats] = useState(emptyStats);

  const [keysList, setKeysList] = useState([]);
  const [modelAnalytics, setModelAnalytics] = useState([]);
  const [keyAnalytics, setKeyAnalytics] = useState([]);
  const [providerAnalytics, setProviderAnalytics] = useState([]);
  const [protocolAnalytics, setProtocolAnalytics] = useState([]);
  const [statusCodeAnalytics, setStatusCodeAnalytics] = useState([]);
  const [timeAnalytics, setTimeAnalytics] = useState([]);
  const [operationsDimension, setOperationsDimension] = useState('key');
  const [trafficDimension, setTrafficDimension] = useState('model');
  const [timeRange, setTimeRange] = useState(
    () => localStorage.getItem('rcpa_dashboard_time_range') || DEFAULT_TIME_RANGE
  );
  const [refreshIntervalMs, setRefreshIntervalMs] = useState(
    () => localStorage.getItem('rcpa_dashboard_refresh_ms') || DEFAULT_REFRESH_INTERVAL_MS
  );
  const mountedRef = useRef(true);
  const fetchInFlightRef = useRef(false);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    localStorage.setItem('rcpa_dashboard_time_range', timeRange);
  }, [timeRange]);

  useEffect(() => {
    localStorage.setItem('rcpa_dashboard_refresh_ms', refreshIntervalMs);
  }, [refreshIntervalMs]);

  const fetchAll = useCallback(async () => {
    if (fetchInFlightRef.current) return;
    fetchInFlightRef.current = true;
    const dashboardParams = appendTimeRangeParams(
      new URLSearchParams({ bucket: timeRange === 'all' ? 'day' : 'hour' }),
      timeRange
    );

    try {
      const [resHealth, resKeys, resAnalytics] = await Promise.all([
        apiFetch('/health'),
        apiFetch('/v1/admin/keys'),
        apiFetch(`/v1/admin/analytics/dashboard?${dashboardParams.toString()}`),
      ]);

      if (resHealth.ok) {
        const healthData = await resHealth.json();
        if (mountedRef.current) setUptime(healthData?.uptime_secs ?? 0);
      }
      if (resKeys.ok) {
        const keysData = await resKeys.json();
        if (mountedRef.current) setKeysList(Array.isArray(keysData) ? keysData : []);
      }
      if (resAnalytics.ok) {
        const analyticsData = await resAnalytics.json();
        if (mountedRef.current) {
          setStats({ ...emptyStats, ...(analyticsData?.total || {}) });
          setModelAnalytics(Array.isArray(analyticsData?.by_model) ? analyticsData.by_model : []);
          setKeyAnalytics(Array.isArray(analyticsData?.by_key) ? analyticsData.by_key : []);
          setProviderAnalytics(Array.isArray(analyticsData?.by_provider) ? analyticsData.by_provider : []);
          setProtocolAnalytics(Array.isArray(analyticsData?.by_protocol) ? analyticsData.by_protocol : []);
          setStatusCodeAnalytics(Array.isArray(analyticsData?.by_status_code) ? analyticsData.by_status_code : []);
          setTimeAnalytics(Array.isArray(analyticsData?.timeline) ? analyticsData.timeline : []);
        }
      }
    } catch (err) {
      console.error("Dashboard data fetching failed:", err);
    } finally {
      fetchInFlightRef.current = false;
    }
  }, [timeRange]);

  useEffect(() => {
    fetchAll();
    const intervalMs = Number(refreshIntervalMs);
    if (!Number.isFinite(intervalMs) || intervalMs <= 0) return undefined;
    const interval = setInterval(fetchAll, Math.max(1000, intervalMs));
    return () => clearInterval(interval);
  }, [fetchAll, refreshIntervalMs]);

  const formatSeconds = (secs) => {
    if (secs < 60) return `${secs}s`;
    const mins = Math.floor(secs / 60);
    if (mins < 60) return `${mins}m ${secs % 60}s`;
    const hrs = Math.floor(mins / 60);
    return `${hrs}h ${mins % 60}m`;
  };

  const sortByRequests = (rows) =>
    [...rows].sort((a, b) => (b.request_count ?? 0) - (a.request_count ?? 0));
  const operationsAnalytics = useMemo(
    () => sortByRequests(operationsDimension === 'key' ? keyAnalytics : providerAnalytics),
    [keyAnalytics, operationsDimension, providerAnalytics]
  );
  const trafficAnalytics = useMemo(() => {
    const rows = {
      model: modelAnalytics,
      protocol: protocolAnalytics,
      status_code: statusCodeAnalytics,
    }[trafficDimension];
    return sortByRequests(rows || []);
  }, [modelAnalytics, protocolAnalytics, statusCodeAnalytics, trafficDimension]);
  const timeBuckets = useMemo(() => makeTimeBuckets(timeRange, timeAnalytics), [timeAnalytics, timeRange]);
  const tokenTrendData = useMemo(() => ({
    labels: timeBuckets.map((bucket) => bucket.label),
    datasets: [
      {
        label: '非缓存输入',
        data: timeBuckets.map((bucket) => Math.max(0, bucket.total_input_tokens - bucket.total_cached_tokens)),
        backgroundColor: 'rgba(59, 130, 246, 0.78)',
        stack: 'tokens',
      },
      {
        label: '缓存命中',
        data: timeBuckets.map((bucket) => bucket.total_cached_tokens),
        backgroundColor: 'rgba(16, 185, 129, 0.82)',
        stack: 'tokens',
      },
      {
        label: '缓存写入',
        data: timeBuckets.map((bucket) => bucket.total_cache_write_tokens),
        backgroundColor: 'rgba(245, 158, 11, 0.82)',
        stack: 'tokens',
      },
      {
        label: '输出',
        data: timeBuckets.map((bucket) => bucket.total_output_tokens),
        backgroundColor: 'rgba(139, 92, 246, 0.72)',
        stack: 'tokens',
      },
    ],
  }), [timeBuckets]);
  const requestTrendData = useMemo(() => ({
    labels: timeBuckets.map((bucket) => bucket.label),
    datasets: [
      {
        label: '成功',
        data: timeBuckets.map((bucket) => bucket.success_count),
        backgroundColor: 'rgba(16, 185, 129, 0.78)',
        stack: 'requests',
      },
      {
        label: '失败',
        data: timeBuckets.map((bucket) => bucket.error_count),
        backgroundColor: 'rgba(239, 68, 68, 0.78)',
        stack: 'requests',
      },
    ],
  }), [timeBuckets]);
  const trendOptions = useMemo(() => ({
    responsive: true,
    maintainAspectRatio: false,
    interaction: { mode: 'index', intersect: false },
    scales: {
      x: {
        stacked: true,
        grid: { display: false },
        ticks: { maxRotation: 0, autoSkip: true, font: { size: 10 } },
        border: { display: false },
      },
      y: {
        stacked: true,
        beginAtZero: true,
        ticks: {
          font: { size: 10 },
          callback: (value) => compactNumber(value, 1),
        },
        border: { display: false },
      },
    },
    plugins: {
      legend: {
        position: 'bottom',
        labels: { boxWidth: 9, boxHeight: 9, font: { size: 10 } },
      },
      tooltip: {
        callbacks: {
          label: (ctx) => `${ctx.dataset.label}: ${compactNumber(ctx.parsed.y, 3)}`,
        },
      },
    },
  }), []);

  return (
    <div className="grid min-h-0 grid-rows-none gap-3 animate-in fade-in duration-500 lg:h-full lg:grid-rows-[auto_minmax(0,1fr)_minmax(0,2fr)_minmax(0,2fr)] lg:overflow-hidden">
      <header className="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
        <h1 className="text-2xl font-semibold">仪表盘</h1>
        <div className="flex flex-wrap items-center gap-2 xl:justify-end">
          <Select value={timeRange} onValueChange={setTimeRange}>
            <SelectTrigger className="h-8 w-[112px] text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {TIME_RANGE_OPTIONS.map((option) => (
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
            type="button"
            variant="outline"
            size="icon"
            className="h-8 w-8"
            onClick={fetchAll}
            title="刷新"
          >
            <RefreshCw size={13} />
          </Button>
          <div className="flex items-center gap-2 bg-muted text-muted-foreground px-3 py-1.5 rounded-md text-xs font-medium border">
            <div className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
            <span>在线 {formatSeconds(uptime)}</span>
          </div>
        </div>
      </header>

      <div className="grid min-h-0 grid-cols-1 gap-3 lg:grid-cols-2">
        <Card className="flex min-h-0 flex-col overflow-hidden transition-colors hover:border-primary/30">
          <CardHeader className="shrink-0 p-3 pb-1">
            <CardTitle className="text-sm font-semibold flex items-center gap-2">
              <Cpu size={16} className="text-primary" />
              Token 用量
            </CardTitle>
          </CardHeader>
          <CardContent className="min-h-0 flex-1 space-y-2 overflow-hidden p-3 pt-0">
            <div>
              <div className="text-xs uppercase tracking-wider text-muted-foreground mb-1">全部 Tokens</div>
              <div className="font-mono text-xl font-semibold">{formatTokenCount(stats.total_tokens)}</div>
            </div>
            <div className="grid grid-cols-2 md:grid-cols-5 gap-x-5 gap-y-2 border-t pt-2">
              <MetricItem label="输入" value={formatTokenCount(stats.total_input_tokens)} />
              <MetricItem label="输出" value={formatTokenCount(stats.total_output_tokens)} />
              <MetricItem label="命中" value={formatTokenCount(stats.total_cached_tokens)} />
              <MetricItem label="写入" value={formatTokenCount(stats.total_cache_write_tokens)} />
              <MetricItem label="命中率" value={formatPercent(stats.cache_hit_rate)} />
            </div>
          </CardContent>
        </Card>

        <Card className="flex min-h-0 flex-col overflow-hidden transition-colors hover:border-primary/30">
          <CardHeader className="shrink-0 p-3 pb-1">
            <CardTitle className="text-sm font-semibold flex items-center gap-2">
              <Activity size={16} className="text-primary" />
              API 调用
            </CardTitle>
          </CardHeader>
          <CardContent className="min-h-0 flex-1 space-y-2 overflow-hidden p-3 pt-0">
            <div>
              <div className="text-xs uppercase tracking-wider text-muted-foreground mb-1">调用总量</div>
              <div className="font-mono text-xl font-semibold">{formatCount(stats.request_count)}</div>
            </div>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-x-5 gap-y-2 border-t pt-2">
              <MetricItem label="成功率" value={formatPercent(stats.success_rate)} />
              <MetricItem label="平均首字节" value={formatDuration(stats.avg_first_byte_latency_ms)} />
              <MetricItem label="平均延迟" value={formatDuration(stats.avg_latency_ms)} />
              <MetricItem label="平均 Tokens" value={formatTokenCount(stats.avg_tokens_per_request)} />
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid min-h-0 grid-cols-1 gap-3 lg:grid-cols-2">
        <Card className="flex min-h-[18rem] flex-col overflow-hidden lg:min-h-0">
          <CardHeader className="shrink-0 p-4 pb-2">
            <CardTitle className="text-sm font-semibold flex items-center gap-2">
              <Workflow size={15} className="text-primary" />
              Token / 缓存趋势
            </CardTitle>
          </CardHeader>
          <CardContent className="min-h-0 flex-1 p-4 pt-0">
            <div className="h-56 min-h-0 lg:h-full">
              <Bar data={tokenTrendData} options={trendOptions} />
            </div>
          </CardContent>
        </Card>

        <Card className="flex min-h-[18rem] flex-col overflow-hidden lg:min-h-0">
          <CardHeader className="shrink-0 p-4 pb-2">
            <CardTitle className="text-sm font-semibold flex items-center gap-2">
              <Server size={15} className="text-primary" />
              请求趋势
            </CardTitle>
          </CardHeader>
          <CardContent className="min-h-0 flex-1 p-4 pt-0">
            <div className="h-56 min-h-0 lg:h-full">
              <Bar data={requestTrendData} options={trendOptions} />
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid min-h-0 grid-cols-1 items-stretch gap-3 lg:grid-cols-2">
        <Card className="flex min-h-0 flex-col overflow-hidden">
          <CardHeader className="flex shrink-0 flex-row items-center justify-between gap-3 p-4 pb-2">
            <CardTitle className="text-sm font-semibold">
              {operationsDimension === 'key' ? 'API Key 用量' : '供应商表现'}
            </CardTitle>
            <DimensionSwitch
              value={operationsDimension}
              onChange={setOperationsDimension}
              options={OPERATIONS_DIMENSIONS}
            />
          </CardHeader>
          <CardContent className="min-h-0 flex-1 overflow-auto p-4 pt-0">
            <AnalyticsTable
              rows={operationsAnalytics}
              dimension={operationsDimension}
              keysList={keysList}
            />
          </CardContent>
        </Card>

        <Card className="flex min-h-0 flex-col overflow-hidden">
          <CardHeader className="flex shrink-0 flex-row items-center justify-between gap-3 p-4 pb-2">
            <CardTitle className="text-sm font-semibold">
              {{ model: '上游模型用量', protocol: '协议分布', status_code: '状态码分布' }[trafficDimension]}
            </CardTitle>
            <DimensionSwitch
              value={trafficDimension}
              onChange={setTrafficDimension}
              options={TRAFFIC_DIMENSIONS}
            />
          </CardHeader>
          <CardContent className="min-h-0 flex-1 overflow-auto p-4 pt-0">
            <AnalyticsTable
              rows={trafficAnalytics}
              dimension={trafficDimension}
              keysList={keysList}
            />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
