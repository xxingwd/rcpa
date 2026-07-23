export function compactNumber(value, decimals = 2) {
  const number = Number(value ?? 0);
  if (!Number.isFinite(number)) return '0';

  const sign = number < 0 ? '-' : '';
  const abs = Math.abs(number);
  const units = [
    { value: 1_000_000_000, suffix: 'B' },
    { value: 1_000_000, suffix: 'M' },
    { value: 1_000, suffix: 'K' },
  ];
  const unit = units.find((item) => abs >= item.value);

  if (!unit) {
    return `${sign}${Math.round(abs).toLocaleString()}`;
  }

  return `${sign}${(abs / unit.value).toFixed(decimals)}${unit.suffix}`;
}

export function formatTokenCount(value) {
  return compactNumber(value, 2);
}

export function formatCount(value) {
  return Number(value ?? 0).toLocaleString();
}

export function formatPercent(value, decimals = 2) {
  return `${((Number(value ?? 0)) * 100).toFixed(decimals)}%`;
}

export function formatDuration(ms) {
  const value = Number(ms ?? 0);
  if (!Number.isFinite(value) || value <= 0) return '0ms';
  if (value < 1000) return `${Math.round(value)}ms`;
  return `${(value / 1000).toFixed(2)}s`;
}

const localDateTimeFormatter = new Intl.DateTimeFormat('zh-CN', {
  year: 'numeric',
  month: '2-digit',
  day: '2-digit',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  hourCycle: 'h23',
});

export function formatLocalDateTime(value) {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);

  const parts = Object.fromEntries(
    localDateTimeFormatter
      .formatToParts(date)
      .filter((part) => part.type !== 'literal')
      .map((part) => [part.type, part.value])
  );
  return `${parts.year}-${parts.month}-${parts.day} ${parts.hour}:${parts.minute}:${parts.second}`;
}

export function formatCostCny(cents) {
  return `¥${((Number(cents ?? 0)) / 100).toFixed(2)}`;
}

export function keyDisplayName(key, fallback = '') {
  if (key?.name && key.name.trim()) return key.name;
  if (key?.id && key.id.trim()) return key.id;
  if (typeof fallback === 'string' && fallback.trim()) return fallback;
  return '';
}

export function compactText(value, maxLength = 24) {
  const text = String(value ?? '');
  if (text.length <= maxLength) return text;
  if (maxLength <= 4) return text.slice(0, maxLength);

  const head = Math.ceil((maxLength - 3) * 0.58);
  const tail = Math.max(1, maxLength - 3 - head);
  return `${text.slice(0, head)}...${text.slice(-tail)}`;
}

export function modelDisplayName(value, maxLength = 24) {
  return compactText(value, maxLength);
}
