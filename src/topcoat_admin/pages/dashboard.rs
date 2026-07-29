use topcoat::{
    context::Cx,
    router::page,
    view::{view, View},
    Result,
};

use crate::topcoat_admin::app::{app_state, require_admin};
use crate::topcoat_admin::{
    format_duration_ms, render_page, render_shared_scripts, render_shared_styles, render_sidebar,
    render_theme_bootstrap, render_toast_container, trusted_html, PageLayout,
};

#[page("/")]
pub async fn dashboard() -> Result {
    Err(topcoat::router::error::redirect("/dashboard").into())
}

#[page("/dashboard")]
pub async fn dashboard_page(cx: &Cx) -> Result {
    render_dashboard(cx).await
}

async fn render_dashboard(cx: &Cx) -> Result {
    require_admin(cx)?;
    let state = app_state(cx);
    let from = "1970-01-01T00:00:00Z";
    let to = "9999-12-31T23:59:59Z";

    let stats = match state.store.dashboard_stats(from, to).await {
        Ok(s) => s,
        Err(_) => {
            return Ok(View::unescaped_unchecked(
                r##"<div class='p-8 text-center text-zinc-500'>Dashboard unavailable</div>"##,
            ))
        }
    };

    let total_requests = stats.requests.total;
    let total_tokens = stats.tokens.total;
    let avg_latency = stats.latency.avg_ms;
    let success_rate = stats.requests.success_rate;
    let input_tokens = stats.tokens.input;
    let output_tokens = stats.tokens.output;
    let cached_tokens = stats.tokens.cached;
    let cache_write_tokens = stats.tokens.cache_write;
    let cache_hit_rate = stats.tokens.cache_hit_rate;
    let avg_first_byte = stats.latency.first_byte_avg_ms;
    let avg_tokens_per_req = stats.tokens.avg_per_request;

    let sidebar = render_sidebar(cx, "/").await?;
    let toast_container = render_toast_container();
    let shared_styles = render_shared_styles();
    let shared_scripts = render_shared_scripts();
    let theme_bootstrap = render_theme_bootstrap();
    let __cx = cx;
    let page_actions: Result = view! {
        <select id="time-range" class="h-8 rounded-lg border border-zinc-200 bg-white px-3 text-xs">
            <option value="1h">"1小时"</option><option value="6h">"6小时"</option><option value="12h">"12小时"</option>
            <option value="today">"今天"</option><option value="yesterday">"昨天"</option>
            <option value="this_week">"本周"</option><option value="last_week">"上周"</option>
            <option value="this_month">"本月"</option><option value="last_month">"上月"</option><option value="all">"全部"</option>
        </select>
        <select id="refresh-interval" class="h-8 rounded-lg border border-zinc-200 bg-white px-3 text-xs">
            <option value="1000">"1s"</option><option value="5000">"5s"</option><option value="10000">"10s"</option>
            <option value="30000">"30s"</option><option value="60000">"60s"</option><option value="0">"关闭"</option>
        </select>
        <button type="button" onclick="refreshDashboard()" class="icon-button border border-zinc-200 bg-white" aria-label="刷新仪表盘" title="刷新仪表盘">
            <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M21 12a9 9 0 0 1-15.2 6.5L3 16M3 16v5m0-5h5M3 12A9 9 0 0 1 18.2 5.5L21 8M21 8V3m0 5h-5"/></svg>
        </button>
        <div class="status-badge"><div class="pulse-dot"></div><span id="uptime-label">"在线"</span></div>
    };
    let page_shell = render_page(
        cx,
        PageLayout {
            title: "仪表盘",
            description: Some("网关流量、Token、延迟与费用概览"),
            class_name: "dashboard-page",
            actions: Some(page_actions?),
            body: View::unescaped_unchecked("<!-- dashboard-body -->"),
        },
    )
    .await?;
    let (page_start, page_end) = page_shell
        .split_once("<!-- dashboard-body -->")
        .expect("dashboard page component must preserve its body marker");

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RCPA Admin - 仪表盘</title>
    {theme_bootstrap}
    <link rel="stylesheet" href="/_topcoat/tailwind.css">
    <script src="/_topcoat/htmx.min.js"></script>
    <script src="/_topcoat/chart.umd.min.js"></script>
    {}
    <style>
        .stat-card {{ padding: .75rem !important; transition: border-color 0.2s; }}
        .stat-card:hover {{ border-color: color-mix(in oklch, var(--foreground) 24%, var(--border)); }}
        .stat-card > .mb-3 {{ margin-bottom: .5rem; }}
        .stat-card .pt-3 {{ padding-top: .5rem; }}
        .chart-container {{ position: relative; height: 224px; width: 100%; }}
        .dimension-switch {{ display: inline-flex; align-items: center; border: 1px solid var(--border); background: color-mix(in oklch, var(--muted) 40%, transparent); padding: 2px; border-radius: 6px; }}
        .dimension-btn {{ height: 24px; padding: 0 8px; font-size: 11px; font-weight: 500; border-radius: 4px; transition: all 0.15s; }}
        .dimension-btn.active {{ background: var(--background); color: var(--foreground); box-shadow: 0 1px 2px rgb(0 0 0 / 0.1); }}
        .dimension-btn:not(.active) {{ color: var(--muted-foreground); }}
        .dimension-btn:not(.active):hover {{ color: var(--foreground); }}
        .status-badge {{ display: flex; align-items: center; gap: 8px; padding: 4px 12px; border-radius: 6px; font-size: 12px; font-weight: 500; border: 1px solid var(--border); background: color-mix(in oklch, var(--muted) 50%, transparent); color: var(--muted-foreground); }}
        .pulse-dot {{ width: 8px; height: 8px; border-radius: 50%; background: rgb(16 185 129); animation: pulse 2s infinite; }}
        @keyframes pulse {{ 0%, 100% {{ opacity: 1; }} 50% {{ opacity: 0.5; }} }}
        .dashboard-page .page-body {{ min-height: 0; overflow: hidden; }}
        .dashboard-content {{ display: grid; height: 100%; min-height: 0; gap: .75rem; }}
        .dashboard-content > .mt-4 {{ margin-top: 0; }}
        .dashboard-chart-card {{ display: flex; flex-direction: column; }}
        .dashboard-chart-card .chart-container {{ min-height: 14rem; flex: 1 1 auto; }}
        .dashboard-table-card {{ display: flex; min-height: 0; flex-direction: column; }}
        .dashboard-table-scroll {{ min-height: 0; flex: 1 1 auto; overflow: auto; overscroll-behavior: contain; scrollbar-gutter: stable; }}
        .dashboard-table-scroll thead th {{ position: sticky; top: 0; z-index: 1; background: var(--card); }}
        .dashboard-table-scroll:focus-visible {{ outline: 2px solid var(--ring); outline-offset: 2px; }}
        @media (min-width: 1024px) {{
            .dashboard-content {{ grid-template-rows: minmax(0, 1fr) minmax(0, 2fr) minmax(0, 2fr); overflow: hidden; }}
            .dashboard-content > .grid {{ min-height: 0; gap: .75rem; }}
            .dashboard-content > .grid > div {{ min-height: 0; overflow: hidden; }}
            .dashboard-chart-card .chart-container {{ height: auto; min-height: 0; }}
        }}
        @media (max-width: 639px) {{
            .dashboard-toolbar {{ align-items: stretch; }}
            .dashboard-toolbar .status-badge {{ min-width: 6.9rem; }}
            .stat-card .grid-cols-5 {{ grid-template-columns: repeat(2, minmax(0, 1fr)); row-gap: 1rem; }}
            .stat-card .grid-cols-4 {{ grid-template-columns: repeat(2, minmax(0, 1fr)); row-gap: 1rem; }}
            .chart-container {{ height: 240px; }}
        }}
    </style>
</head>
<body class="bg-zinc-50 text-zinc-900">
    {}
    <div class="flex min-h-screen">
        {}
        <main class="admin-main">
            <div class="admin-content">
                {}
                <div class="dashboard-content">
                <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
                    <div class="stat-card rounded-xl border border-zinc-200 bg-white p-4">
                        <div class="mb-3 flex items-center gap-2">
                            <svg class="h-4 w-4 text-blue-600" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z"/></svg>
                            <h3 class="text-sm font-semibold">Token 用量</h3>
                        </div>
                        <div class="mb-3">
                            <div class="text-xs uppercase tracking-wider text-zinc-500 mb-1">全部 Tokens</div>
                            <div id="metric-total-tokens" class="font-mono text-xl font-semibold">{}</div>
                        </div>
                        <div class="grid grid-cols-5 gap-4 border-t border-zinc-100 pt-3">
                            <div>
                                <div class="text-[0.68rem] uppercase tracking-wider text-zinc-500 mb-1">输入</div>
                                <div id="metric-input-tokens" class="font-mono text-sm font-semibold">{}</div>
                            </div>
                            <div>
                                <div class="text-[0.68rem] uppercase tracking-wider text-zinc-500 mb-1">输出</div>
                                <div id="metric-output-tokens" class="font-mono text-sm font-semibold">{}</div>
                            </div>
                            <div>
                                <div class="text-[0.68rem] uppercase tracking-wider text-zinc-500 mb-1">命中</div>
                                <div id="metric-cached-tokens" class="font-mono text-sm font-semibold">{}</div>
                            </div>
                            <div>
                                <div class="text-[0.68rem] uppercase tracking-wider text-zinc-500 mb-1">写入</div>
                                <div id="metric-cache-write-tokens" class="font-mono text-sm font-semibold">{}</div>
                            </div>
                            <div>
                                <div class="text-[0.68rem] uppercase tracking-wider text-zinc-500 mb-1">命中率</div>
                                <div id="metric-cache-rate" class="font-mono text-sm font-semibold">{:.1}%</div>
                            </div>
                        </div>
                    </div>

                    <div class="stat-card rounded-xl border border-zinc-200 bg-white p-4">
                        <div class="mb-3 flex items-center gap-2">
                            <svg class="h-4 w-4 text-blue-600" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"/></svg>
                            <h3 class="text-sm font-semibold">API 调用</h3>
                        </div>
                        <div class="mb-3">
                            <div class="text-xs uppercase tracking-wider text-zinc-500 mb-1">调用总量</div>
                            <div id="metric-total-requests" class="font-mono text-xl font-semibold">{}</div>
                        </div>
                        <div class="grid grid-cols-4 gap-4 border-t border-zinc-100 pt-3">
                            <div>
                                <div class="text-[0.68rem] uppercase tracking-wider text-zinc-500 mb-1">成功率</div>
                                <div id="metric-success-rate" class="font-mono text-sm font-semibold">{:.1}%</div>
                            </div>
                            <div>
                                <div class="text-[0.68rem] uppercase tracking-wider text-zinc-500 mb-1">平均首字节</div>
                                <div id="metric-first-byte" class="font-mono text-sm font-semibold">{}</div>
                            </div>
                            <div>
                                <div class="text-[0.68rem] uppercase tracking-wider text-zinc-500 mb-1">平均延迟</div>
                                <div id="metric-latency" class="font-mono text-sm font-semibold">{}</div>
                            </div>
                            <div>
                                <div class="text-[0.68rem] uppercase tracking-wider text-zinc-500 mb-1">平均 Tokens</div>
                                <div id="metric-avg-tokens" class="font-mono text-sm font-semibold">{:.0}</div>
                            </div>
                        </div>
                    </div>
                </div>

                <div class="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2">
                    <div class="dashboard-chart-card rounded-xl border border-zinc-200 bg-white p-4">
                        <div class="mb-3 flex items-center gap-2">
                            <svg class="h-4 w-4 text-blue-600" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"/></svg>
                            <h3 class="text-sm font-semibold">Token / 缓存趋势</h3>
                        </div>
                        <div class="chart-container">
                            <canvas id="tokenTrendChart"></canvas>
                        </div>
                    </div>
                    <div class="dashboard-chart-card rounded-xl border border-zinc-200 bg-white p-4">
                        <div class="mb-3 flex items-center gap-2">
                            <svg class="h-4 w-4 text-blue-600" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M12 5l7 7-7 7"/></svg>
                            <h3 class="text-sm font-semibold">请求趋势</h3>
                        </div>
                        <div class="chart-container">
                            <canvas id="requestTrendChart"></canvas>
                        </div>
                    </div>
                </div>

                <div class="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2">
                    <div class="dashboard-table-card rounded-xl border border-zinc-200 bg-white p-4">
                        <div class="mb-3 flex items-center justify-between">
                            <h3 class="text-sm font-semibold" id="operations-title">API Key 用量</h3>
                            <div class="dimension-switch">
                                <button class="dimension-btn active" data-dim="key" onclick="switchOperationDim('key')">Key</button>
                                <button class="dimension-btn" data-dim="provider" onclick="switchOperationDim('provider')">供应商</button>
                            </div>
                        </div>
                        <div class="dashboard-table-scroll" role="region" aria-label="API Key Token 用量排行榜" tabindex="0">
                            <table class="w-full text-xs min-w-[640px]">
                                <thead class="border-b border-zinc-200 bg-zinc-50">
                                    <tr>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">API 密钥</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">请求数</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">成功率</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500" aria-sort="descending" title="按 Token 用量降序">Tokens</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">缓存</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">CHR</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">延迟</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">费用</th>
                                    </tr>
                                </thead>
                                <tbody id="operations-table-body">
                                    <tr><td colspan="8" class="px-3 py-6 text-center text-zinc-500">加载中...</td></tr>
                                </tbody>
                            </table>
                        </div>
                    </div>

                    <div class="dashboard-table-card rounded-xl border border-zinc-200 bg-white p-4">
                        <div class="mb-3 flex items-center justify-between">
                            <h3 class="text-sm font-semibold" id="traffic-title">上游模型用量</h3>
                            <div class="dimension-switch">
                                <button class="dimension-btn active" data-dim="model" onclick="switchTrafficDim('model')">模型</button>
                                <button class="dimension-btn" data-dim="protocol" onclick="switchTrafficDim('protocol')">协议</button>
                                <button class="dimension-btn" data-dim="status_code" onclick="switchTrafficDim('status_code')">状态码</button>
                            </div>
                        </div>
                        <div class="dashboard-table-scroll" role="region" aria-label="上游模型 Token 用量排行榜" tabindex="0">
                            <table class="w-full text-xs min-w-[640px]">
                                <thead class="border-b border-zinc-200 bg-zinc-50">
                                    <tr>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">上游模型</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">请求数</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">成功率</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500" aria-sort="descending" title="按 Token 用量降序">Tokens</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">缓存</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">CHR</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">延迟</th>
                                        <th class="px-3 py-2 text-left font-medium text-zinc-500">费用</th>
                                    </tr>
                                </thead>
                                <tbody id="traffic-table-body">
                                    <tr><td colspan="8" class="px-3 py-6 text-center text-zinc-500">加载中...</td></tr>
                                </tbody>
                            </table>
                        </div>
                    </div>
                </div>
                </div>
                {}
            </div>
        </main>
    </div>

    {}
    <script>
        let tokenChart = null;
        let requestChart = null;
        let currentOpDim = 'key';
        let currentTrafficDim = 'model';
        let analyticsData = null;
        let analyticsLoading = false;
        let chartDataSignature = null;
        const DAY_MS = 24 * 60 * 60 * 1000;
        const SHANGHAI_OFFSET_MS = 8 * 60 * 60 * 1000;

        function formatTokens(n) {{
            n = parseInt(n) || 0;
            if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
            if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
            return n.toString();
        }}

        function formatPercent(v) {{
            return (parseFloat(v) * 100).toFixed(1) + '%';
        }}

        function formatDuration(ms) {{
            ms = Number(ms);
            if (!Number.isFinite(ms) || ms < 0) ms = 0;
            if (ms < 1000) return Math.min(999, Math.round(ms)) + 'ms';
            return (ms / 1000).toFixed(2).replace(/\.?0+$/, '') + 's';
        }}

        function formatCost(cents) {{
            return '¥' + (parseInt(cents) / 100).toFixed(4);
        }}

        function cacheRate(input, cached) {{
            input = parseInt(input);
            return input > 0 ? parseInt(cached) / input : 0;
        }}

        function escapeHtml(value) {{
            const element = document.createElement('span');
            element.textContent = String(value ?? '');
            return element.innerHTML;
        }}

        function formatTimelineLabel(value) {{
            const label = String(value || '');
            if (/^\d{{4}}-\d{{2}}-\d{{2}} \d{{2}}:00$/.test(label)) return label.slice(5);
            if (/^\d{{4}}-\d{{2}}-\d{{2}}$/.test(label)) return label.slice(5);
            return label;
        }}

        function formatSeconds(seconds) {{
            seconds = parseInt(seconds) || 0;
            if (seconds < 60) return seconds + 's';
            const minutes = Math.floor(seconds / 60);
            if (minutes < 60) return minutes + 'm ' + (seconds % 60) + 's';
            return Math.floor(minutes / 60) + 'h ' + (minutes % 60) + 'm';
        }}

        function toBackendIso(date, milliseconds = false) {{
            const iso = date.toISOString();
            return milliseconds ? iso.replace('Z', '+00:00') : iso.replace(/\.\d{{3}}Z$/, '+00:00');
        }}

        function shanghaiDateParts(date) {{
            const shifted = new Date(date.getTime() + SHANGHAI_OFFSET_MS);
            return {{
                year: shifted.getUTCFullYear(),
                month: shifted.getUTCMonth(),
                day: shifted.getUTCDate(),
                weekday: shifted.getUTCDay() || 7
            }};
        }}

        function shanghaiDate(year, month, day) {{
            return new Date(Date.UTC(year, month, day) - SHANGHAI_OFFSET_MS);
        }}

        function startOfDay(date) {{
            const parts = shanghaiDateParts(date);
            return shanghaiDate(parts.year, parts.month, parts.day);
        }}

        function startOfWeek(date) {{
            const start = startOfDay(date);
            const weekday = shanghaiDateParts(date).weekday;
            return new Date(start.getTime() - (weekday - 1) * DAY_MS);
        }}

        function startOfMonth(date) {{
            const parts = shanghaiDateParts(date);
            return shanghaiDate(parts.year, parts.month, 1);
        }}

        function analyticsParams(rangeName) {{
            const now = new Date();
            let from = null;
            let to = now;
            if (rangeName === '1h') from = new Date(now.getTime() - 60 * 60 * 1000);
            if (rangeName === '6h') from = new Date(now.getTime() - 6 * 60 * 60 * 1000);
            if (rangeName === '12h') from = new Date(now.getTime() - 12 * 60 * 60 * 1000);
            if (rangeName === 'today') from = startOfDay(now);
            if (rangeName === 'yesterday') {{
                to = new Date(startOfDay(now).getTime() - 1);
                from = startOfDay(to);
            }}
            if (rangeName === 'this_week') from = startOfWeek(now);
            if (rangeName === 'last_week') {{
                to = new Date(startOfWeek(now).getTime() - 1);
                from = new Date(startOfWeek(now).getTime() - 7 * DAY_MS);
            }}
            if (rangeName === 'this_month') from = startOfMonth(now);
            if (rangeName === 'last_month') {{
                to = new Date(startOfMonth(now).getTime() - 1);
                const parts = shanghaiDateParts(now);
                from = shanghaiDate(parts.year, parts.month - 1, 1);
            }}
            const params = new URLSearchParams({{ bucket: rangeName === 'all' ? 'day' : 'hour' }});
            if (from) {{
                params.set('from', toBackendIso(from));
                params.set('to', toBackendIso(to, true));
            }}
            return params;
        }}

        function setMetric(id, value) {{
            const element = document.getElementById(id);
            if (element) element.textContent = value;
        }}

        function renderSummary(total) {{
            total = total || {{}};
            setMetric('metric-total-tokens', formatTokens(total.total_tokens));
            setMetric('metric-input-tokens', formatTokens(total.total_input_tokens));
            setMetric('metric-output-tokens', formatTokens(total.total_output_tokens));
            setMetric('metric-cached-tokens', formatTokens(total.total_cached_tokens));
            setMetric('metric-cache-write-tokens', formatTokens(total.total_cache_write_tokens));
            setMetric('metric-cache-rate', formatPercent(total.cache_hit_rate || 0));
            setMetric('metric-total-requests', formatTokens(total.request_count));
            setMetric('metric-success-rate', formatPercent(total.success_rate || 0));
            setMetric('metric-first-byte', formatDuration(total.avg_first_byte_latency_ms));
            setMetric('metric-latency', formatDuration(total.avg_latency_ms));
            setMetric('metric-avg-tokens', formatTokens(total.avg_tokens_per_request));
        }}

        function loadAnalytics() {{
            if (analyticsLoading) return Promise.resolve();
            analyticsLoading = true;
            const range = document.getElementById('time-range')?.value || 'today';
            const analyticsUrl = '/v1/admin/analytics/dashboard?' + analyticsParams(range).toString();
            return Promise.all([
                fetch(analyticsUrl, {{ credentials: 'include' }}),
                fetch('/health', {{ credentials: 'include' }})
            ])
                .then(async ([response, healthResponse]) => {{
                    if (response.status === 401) return redirectToLogin();
                    if (!response.ok) throw new Error('统计数据加载失败');
                    const data = await response.json();
                    if (healthResponse.ok) {{
                        const health = await healthResponse.json();
                        setMetric('uptime-label', '在线 ' + formatSeconds(health.uptime_secs));
                    }}
                    return data;
                }})
                .then(data => {{
                    if (!data) return;
                    analyticsData = data;
                    renderSummary(data.total);
                    renderCharts(data.timeline);
                    renderOperationsTable();
                    renderTrafficTable();
                }})
                .catch(err => console.error('Failed to load analytics:', err))
                .finally(() => {{ analyticsLoading = false; }});
        }}

        function renderCharts(timeline) {{
            if (!timeline || timeline.length === 0) {{
                timeline = [{{ label: '无数据', request_count: 0, success_count: 0, error_count: 0, total_input_tokens: 0, total_output_tokens: 0, total_cached_tokens: 0, total_cache_write_tokens: 0, total_tokens: 0 }}];
            }}

            const labels = timeline.map(b => formatTimelineLabel(b.label || b.group_key));
            const tokenCtx = document.getElementById('tokenTrendChart').getContext('2d');
            const requestCtx = document.getElementById('requestTrendChart').getContext('2d');
            const styles = getComputedStyle(document.documentElement);
            const textColor = styles.getPropertyValue('--muted-foreground').trim();
            const gridColor = styles.getPropertyValue('--border').trim();
            const foreground = styles.getPropertyValue('--foreground').trim();
            const options = (tokenChartOptions) => ({{
                responsive: true,
                maintainAspectRatio: false,
                animation: false,
                normalized: true,
                interaction: {{ mode: 'index', intersect: false }},
                scales: {{
                    x: {{ stacked: true, grid: {{ display: false }}, ticks: {{ color: textColor, maxRotation: 0, autoSkip: true, font: {{ size: 10 }} }}, border: {{ display: false }} }},
                    y: {{ stacked: true, beginAtZero: true, grid: {{ color: gridColor }}, ticks: {{ color: textColor, font: {{ size: 10 }}, callback: value => tokenChartOptions ? formatTokens(value) : value }}, border: {{ display: false }} }}
                }},
                plugins: {{
                    legend: {{ position: 'bottom', labels: {{ color: textColor, boxWidth: 9, boxHeight: 9, font: {{ size: 10 }} }} }},
                    tooltip: {{ titleColor: foreground, bodyColor: foreground }}
                }}
            }});

            const tokenData = [
                timeline.map(b => Math.max(0, b.total_input_tokens - b.total_cached_tokens)),
                timeline.map(b => b.total_cached_tokens),
                timeline.map(b => b.total_cache_write_tokens),
                timeline.map(b => b.total_output_tokens)
            ];
            const requestData = [
                timeline.map(b => b.success_count),
                timeline.map(b => b.error_count)
            ];
            const nextSignature = JSON.stringify({{
                labels,
                tokenData,
                requestData,
                theme: document.documentElement.dataset.theme || 'light'
            }});
            if (tokenChart && requestChart && chartDataSignature === nextSignature) return;

            if (!tokenChart) {{
                tokenChart = new Chart(tokenCtx, {{
                    type: 'bar',
                    data: {{
                        labels: labels,
                        datasets: [
                            {{ label: '非缓存输入', data: tokenData[0], backgroundColor: 'rgba(59, 130, 246, 0.78)', stack: 'tokens' }},
                            {{ label: '缓存命中', data: tokenData[1], backgroundColor: 'rgba(16, 185, 129, 0.82)', stack: 'tokens' }},
                            {{ label: '缓存写入', data: tokenData[2], backgroundColor: 'rgba(245, 158, 11, 0.82)', stack: 'tokens' }},
                            {{ label: '输出', data: tokenData[3], backgroundColor: 'rgba(139, 92, 246, 0.72)', stack: 'tokens' }}
                        ]
                    }},
                    options: options(true)
                }});
            }} else {{
                tokenChart.data.labels = labels;
                tokenChart.data.datasets.forEach((dataset, index) => {{ dataset.data = tokenData[index]; }});
                tokenChart.options = options(true);
                tokenChart.update('none');
            }}

            if (!requestChart) {{
                requestChart = new Chart(requestCtx, {{
                    type: 'bar',
                    data: {{
                        labels: labels,
                        datasets: [
                            {{ label: '成功', data: requestData[0], backgroundColor: 'rgba(16, 185, 129, 0.78)', stack: 'requests' }},
                            {{ label: '失败', data: requestData[1], backgroundColor: 'rgba(239, 68, 68, 0.78)', stack: 'requests' }}
                        ]
                    }},
                    options: options(false)
                }});
            }} else {{
                requestChart.data.labels = labels;
                requestChart.data.datasets.forEach((dataset, index) => {{ dataset.data = requestData[index]; }});
                requestChart.options = options(false);
                requestChart.update('none');
            }}
            chartDataSignature = nextSignature;
        }}

        function renderOperationsTable() {{
            if (!analyticsData) return;
            const rows = currentOpDim === 'key' ? (analyticsData.by_key || []) : (analyticsData.by_provider || []);
            const tbody = document.getElementById('operations-table-body');
            const title = document.getElementById('operations-title');
            title.textContent = currentOpDim === 'key' ? 'API Key 用量' : '供应商表现';

            if (!rows || rows.length === 0) {{
                tbody.innerHTML = '<tr><td colspan="8" class="px-3 py-6 text-center text-zinc-500">暂无统计记录</td></tr>';
                return;
            }}

            tbody.innerHTML = rows.map(r => {{
                const chr = cacheRate(r.total_input_tokens, r.total_cached_tokens);
                const groupKey = escapeHtml(r.group_key);
                return '<tr class="border-b border-zinc-100 hover:bg-zinc-50">' +
                    '<td class="px-3 py-2 font-mono text-xs truncate max-w-[120px]" title="' + groupKey + '">' + groupKey + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + r.request_count + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatPercent(r.success_rate) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatTokens(r.total_tokens) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatTokens(r.total_cached_tokens) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatPercent(chr) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatDuration(r.avg_latency_ms) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatCost(r.total_cost_cents) + '</td>' +
                '</tr>';
            }}).join('');
        }}

        function renderTrafficTable() {{
            if (!analyticsData) return;
            const dimMap = {{ model: analyticsData.by_model, protocol: analyticsData.by_protocol, status_code: analyticsData.by_status_code }};
            const rows = dimMap[currentTrafficDim] || [];
            const tbody = document.getElementById('traffic-table-body');
            const title = document.getElementById('traffic-title');
            const labels = {{ model: '上游模型用量', protocol: '协议分布', status_code: '状态码分布' }};
            title.textContent = labels[currentTrafficDim];

            if (!rows || rows.length === 0) {{
                tbody.innerHTML = '<tr><td colspan="8" class="px-3 py-6 text-center text-zinc-500">暂无统计记录</td></tr>';
                return;
            }}

            tbody.innerHTML = rows.map(r => {{
                const chr = cacheRate(r.total_input_tokens, r.total_cached_tokens);
                const groupKey = escapeHtml(r.group_key);
                return '<tr class="border-b border-zinc-100 hover:bg-zinc-50">' +
                    '<td class="px-3 py-2 font-mono text-xs truncate max-w-[120px]" title="' + groupKey + '">' + groupKey + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + r.request_count + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatPercent(r.success_rate) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatTokens(r.total_tokens) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatTokens(r.total_cached_tokens) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatPercent(chr) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatDuration(r.avg_latency_ms) + '</td>' +
                    '<td class="px-3 py-2 font-mono">' + formatCost(r.total_cost_cents) + '</td>' +
                '</tr>';
            }}).join('');
        }}

        function switchOperationDim(dim) {{
            currentOpDim = dim;
            document.querySelectorAll('#operations-title + .dimension-switch .dimension-btn').forEach(btn => {{
                btn.classList.toggle('active', btn.dataset.dim === dim);
            }});
            renderOperationsTable();
        }}

        function switchTrafficDim(dim) {{
            currentTrafficDim = dim;
            document.querySelectorAll('#traffic-title + .dimension-switch .dimension-btn').forEach(btn => {{
                btn.classList.toggle('active', btn.dataset.dim === dim);
            }});
            renderTrafficTable();
        }}

        function refreshDashboard() {{
            loadAnalytics();
        }}

        document.addEventListener('DOMContentLoaded', function() {{
            const timeRange = document.getElementById('time-range');
            const refreshSelect = document.getElementById('refresh-interval');
            timeRange.value = localStorage.getItem('rcpa_dashboard_time_range') || 'today';
            refreshSelect.value = localStorage.getItem('rcpa_dashboard_refresh_ms') || '5000';
            let interval = null;
            const resetInterval = () => {{
                if (interval) clearInterval(interval);
                const ms = parseInt(refreshSelect.value);
                if (ms > 0) interval = setInterval(loadAnalytics, Math.max(1000, ms));
            }};
            timeRange.addEventListener('change', () => {{
                localStorage.setItem('rcpa_dashboard_time_range', timeRange.value);
                loadAnalytics();
            }});
            refreshSelect.addEventListener('change', () => {{
                localStorage.setItem('rcpa_dashboard_refresh_ms', refreshSelect.value);
                resetInterval();
            }});
            window.addEventListener('rcpa:theme-change', () => {{
                if (analyticsData) renderCharts(analyticsData.timeline);
            }});
            window.addEventListener('rcpa:data-change', loadAnalytics);
            loadAnalytics();
            resetInterval();
        }});
    </script>
</body>
</html>"##,
        shared_styles,
        toast_container,
        sidebar,
        page_start,
        format_tokens(total_tokens),
        format_tokens(input_tokens),
        format_tokens(output_tokens),
        format_tokens(cached_tokens),
        format_tokens(cache_write_tokens),
        cache_hit_rate * 100.0,
        format_tokens(total_requests),
        success_rate * 100.0,
        format_duration_ms(avg_first_byte),
        format_duration_ms(avg_latency),
        avg_tokens_per_req,
        page_end,
        shared_scripts
    );

    Ok(trusted_html(html))
}

fn format_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
