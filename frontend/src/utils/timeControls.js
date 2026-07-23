export const TIME_RANGE_OPTIONS = [
  { value: '1h', label: '1小时' },
  { value: '6h', label: '6小时' },
  { value: '12h', label: '12小时' },
  { value: 'today', label: '今天' },
  { value: 'yesterday', label: '昨天' },
  { value: 'this_week', label: '本周' },
  { value: 'last_week', label: '上周' },
  { value: 'this_month', label: '本月' },
  { value: 'last_month', label: '上月' },
  { value: 'all', label: '全部' },
];

export const REFRESH_INTERVAL_OPTIONS = [
  { value: '1000', label: '1s' },
  { value: '5000', label: '5s' },
  { value: '10000', label: '10s' },
  { value: '30000', label: '30s' },
  { value: '60000', label: '60s' },
  { value: '0', label: '关闭' },
];

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

function toBackendIso(date, includeMilliseconds = false) {
  const iso = date.toISOString();
  if (includeMilliseconds) return iso.replace('Z', '+00:00');
  return iso.replace(/\.\d{3}Z$/, '+00:00');
}

function range(from, to) {
  return {
    from: toBackendIso(from),
    to: toBackendIso(to, true),
  };
}

export function getTimeRange(value, now = new Date()) {
  switch (value) {
    case '1h':
      return range(new Date(now.getTime() - 60 * 60 * 1000), now);
    case '6h':
      return range(new Date(now.getTime() - 6 * 60 * 60 * 1000), now);
    case '12h':
      return range(new Date(now.getTime() - 12 * 60 * 60 * 1000), now);
    case 'today':
      return range(startOfLocalDay(now), now);
    case 'yesterday': {
      const today = startOfLocalDay(now);
      const yesterday = new Date(today);
      yesterday.setDate(yesterday.getDate() - 1);
      const end = new Date(today.getTime() - 1);
      return range(yesterday, end);
    }
    case 'this_week':
      return range(startOfLocalWeek(now), now);
    case 'last_week': {
      const thisWeek = startOfLocalWeek(now);
      const lastWeek = new Date(thisWeek);
      lastWeek.setDate(lastWeek.getDate() - 7);
      const end = new Date(thisWeek.getTime() - 1);
      return range(lastWeek, end);
    }
    case 'this_month':
      return range(startOfLocalMonth(now), now);
    case 'last_month': {
      const thisMonth = startOfLocalMonth(now);
      const lastMonth = new Date(thisMonth.getFullYear(), thisMonth.getMonth() - 1, 1);
      const end = new Date(thisMonth.getTime() - 1);
      return range(lastMonth, end);
    }
    case 'all':
    default:
      return null;
  }
}

export function appendTimeRangeParams(params, timeRangeValue) {
  const timeRange = getTimeRange(timeRangeValue);
  if (!timeRange) return params;
  params.set('from', timeRange.from);
  params.set('to', timeRange.to);
  return params;
}

export function refreshIntervalLabel(value) {
  return REFRESH_INTERVAL_OPTIONS.find((option) => option.value === String(value))?.label || `${value}ms`;
}

export function timeRangeLabel(value) {
  return TIME_RANGE_OPTIONS.find((option) => option.value === value)?.label || '全部';
}
