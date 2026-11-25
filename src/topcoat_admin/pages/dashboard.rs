use topcoat::{
    context::Cx,
    router::page,
    view::{view, View},
    Result,
};

use crate::topcoat_admin::app::require_admin;
use crate::topcoat_admin::{
    render_modal_backdrop, render_page, render_shared_scripts, render_shared_styles,
    render_sidebar, render_sidebar_bootstrap, render_theme_bootstrap, render_toast_container,
    trusted_html, PageLayout,
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
    let sidebar = render_sidebar(cx, "/").await?;
    let toast_container = render_toast_container();
    let shared_styles = render_shared_styles();
    let shared_scripts = render_shared_scripts();
    let theme_bootstrap = render_theme_bootstrap();
    let sidebar_bootstrap = render_sidebar_bootstrap();
    let __cx = cx;
    let page_actions: Result = view! {
        <select id="time-range" class="h-8 rounded-lg border border-zinc-200 bg-white px-3 text-xs">
            <option value="5m">"5分钟"</option><option value="15m">"15分钟"</option><option value="custom">"自定义时间"</option><option value="1h">"1小时"</option><option value="6h">"6小时"</option><option value="12h">"12小时"</option>
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
            description: None,
            class_name: "dashboard-page",
            actions: Some(page_actions?),
            body: View::unescaped_unchecked("<!-- dashboard-body -->"),
        },
    )
    .await?;
    let (page_start, page_end) = page_shell
        .split_once("<!-- dashboard-body -->")
        .expect("dashboard page component must preserve its body marker");

    let modal_backdrop = render_modal_backdrop();
    let home_styles = include_str!("../../../static/admin/dashboard-home.css");
    let home_scripts = include_str!("../../../static/admin/dashboard-home.js");
    let html = format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RCPA Admin - 仪表盘</title>
    {theme_bootstrap}
    {sidebar_bootstrap}
    <link rel="stylesheet" href="/_topcoat/tailwind.css">
    <script src="/_topcoat/htmx.min.js"></script>
    <script src="/_topcoat/chart.umd.min.js"></script>
    {}
    <style>{home_styles}    </style>
</head>
<body class="bg-zinc-50 text-zinc-900">
    {}
    <div class="flex min-h-screen">
        {}
        <main class="admin-main">
            <div class="admin-content">
                {}
                <div class="dashboard-content">
                <div class="filter-bar dashboard-filters" aria-label="分析筛选">
                    <div id="dashboard-models"></div><div id="dashboard-providers"></div><div id="dashboard-keys"></div><div id="dashboard-protocols"></div>
                    <button type="button" class="outline-button" onclick="AdminFilters.clear();loadAnalytics()">重置</button>
                    <span id="analytics-feedback" role="status" class="analytics-feedback"></span>
                </div>
                <form id="dashboard-custom-range" class="dashboard-custom-range" hidden>
                    <label>开始（上海时间）<input id="dashboard-from" type="datetime-local" required></label>
                    <label>结束<input id="dashboard-to" type="datetime-local" required></label>
                    <button class="outline-button" type="submit">应用时间</button><span id="dashboard-time-error" role="alert"></span>
                </form>
                <section class="dashboard-kpis" aria-label="关键指标">
                    <div><span>请求</span><strong id="metric-total-requests">—</strong><small id="metric-rpm">—</small></div>
                    <div><span title="已结束且无记录错误；HTTP 200 的流内错误也计入失败">成功率</span><strong id="metric-success-rate">—</strong><small id="analysis-errors">—</small></div>
                    <div><span title="网关收到首个网络数据块的时间，不等同于首个有效Token">首包 · P50</span><strong id="metric-first-byte">—</strong><small id="metric-first-byte-p95">—</small></div>
                    <div><span title="成功流式请求的输出Token / (总耗时−首包耗时)，逐请求取中位数；缺少流式标记的旧记录不参与">估算 TPS · P50</span><strong id="metric-tps">—</strong><small id="metric-tps-samples">—</small></div>
                    <div><span>Token</span><strong id="metric-total-tokens">—</strong><small>输入 <b id="metric-input-tokens">—</b> / 输出 <b id="metric-output-tokens">—</b></small></div>
                </section>
                <div class="dashboard-summary" aria-label="补充指标">
                    <span>总耗时 P50 / P95 / P99 <b id="analysis-p50">—</b> / <b id="analysis-p95">—</b> / <b id="analysis-p99">—</b></span>
                    <span>重试 <b id="analysis-retries">—</b></span><span>缓存命中 <b id="metric-cache-rate">—</b></span><span>模型 / 供应商 <b id="analysis-active">—</b></span><span>费用 <b id="analysis-cost">—</b></span>
                </div>
                <div class="dashboard-board">
                <section class="dashboard-channel-card" aria-labelledby="channel-title">
                    <div class="dashboard-section-header"><h3 id="channel-title">模型性能</h3><span id="channel-count"></span><div class="channel-windows" role="group" aria-label="性能统计时间范围"><button type="button" data-window="5m" aria-pressed="false">近 5 分钟</button><button type="button" data-window="15m" aria-pressed="false">15 分钟</button><button type="button" data-window="1h" aria-pressed="false">1 小时</button></div><label class="channel-state-label">状态 <select id="channel-state" aria-label="模型观测状态"></select></label><label>排序 <select id="channel-sort"><option value="requests">按请求量</option><option value="errors">优先有错误</option><option value="speed">按估算 TPS</option><option value="first_byte">按首包延迟</option></select></label><button id="channel-clear" class="outline-button" type="button" hidden>全部状态</button></div>
                    <div class="dashboard-table-scroll" role="region" aria-label="模型供应商性能比较" tabindex="0"><table class="dashboard-channel-table"><thead><tr><th scope="col">模型 / 供应商</th><th scope="col">请求 / RPM</th><th scope="col">成功率 / 错误</th><th scope="col">首包 P50 / P95</th><th scope="col">估算 TPS · P50 / P10</th><th scope="col">耗时 P95 / 重试</th></tr></thead><tbody id="channel-table-body"></tbody></table></div>
                </section>
                <aside class="dashboard-trends" aria-label="流量与性能趋势">
                    <section class="dashboard-chart-card"><h3>请求</h3><div class="chart-container"><canvas id="requestTrendChart"></canvas></div></section>
                    <section class="dashboard-chart-card"><h3>响应时间</h3><div class="chart-container"><canvas id="latencyTrendChart"></canvas></div></section>
                    <section class="dashboard-chart-card"><h3>Token / 缓存</h3><div class="chart-container"><canvas id="tokenTrendChart"></canvas></div></section>
                </aside>
                <div class="dashboard-breakdowns">
                    <section class="dashboard-table-card" aria-labelledby="operations-title">
                        <div class="dashboard-section-header"><h3 id="operations-title">Key 用量</h3></div>
                        <div class="dashboard-table-scroll" role="region" aria-label="API Key Token 用量排行榜" tabindex="0">
                            <table class="dashboard-usage-table"><thead><tr><th scope="col">密钥</th><th scope="col">请求</th><th scope="col" aria-sort="descending">Token</th><th scope="col">缓存命中</th></tr></thead><tbody id="operations-table-body"></tbody></table>
                        </div>
                    </section>
                    <section class="dashboard-table-card" aria-labelledby="traffic-title">
                        <div class="dashboard-section-header"><h3 id="traffic-title">供应商用量</h3></div>
                        <div class="dashboard-table-scroll" role="region" aria-label="供应商 Token 用量排行榜" tabindex="0">
                            <table class="dashboard-usage-table"><thead><tr><th scope="col">供应商</th><th scope="col">请求</th><th scope="col" aria-sort="descending">Token</th><th scope="col">缓存命中</th></tr></thead><tbody id="traffic-table-body"></tbody></table>
                        </div>
                        <div class="dashboard-distributions"><div><span>协议</span><div id="protocol-distribution"></div></div><div><span>状态码</span><div id="status-distribution"></div></div></div>
                    </section>
                </div>
                </div>
                </div>
                {}
            </div>
        </main>
    </div>

    {}
    {modal_backdrop}
    <script>
        let tokenChart = null;
        let requestChart = null;
        let analyticsData = null;
        let analyticsRequest = null;
        const extraCharts = new Map();
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
            const value = (Number(cents) || 0) / 100;
            return new Intl.NumberFormat('zh-CN', {{style:'currency',currency:AdminFilters.currency || 'USD',maximumFractionDigits:4}}).format(value);
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
            if (rangeName === '5m') from = new Date(now.getTime() - 5 * 60 * 1000);
            if (rangeName === '15m') from = new Date(now.getTime() - 15 * 60 * 1000);
            if (rangeName === 'custom') {{from = dashboardCustomRange.from;to = dashboardCustomRange.to;}}
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
            const params = new URLSearchParams({{ bucket: rangeName === 'all' ? 'day' : from && to-from <= 6*60*60*1000 ? 'minute' : 'hour' }});
            if (from) {{
                params.set('from', toBackendIso(from));
                params.set('to', toBackendIso(to, true));
            }}
            return AdminFilters.append(params);
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
            setMetric('analysis-cost', formatCost(total.total_cost_cents));
            setMetric('analysis-unit-cost', formatCost(total.request_count ? total.total_cost_cents / total.request_count : 0));

        }}

        function loadAnalytics() {{
            if (dashboardDetailOpen) return Promise.resolve();
            if (analyticsRequest) analyticsRequest.abort();
            const request = new AbortController();
            analyticsRequest = request;
            setMetric('analytics-feedback', '更新中…');
            const range = document.getElementById('time-range')?.value || 'today';
            const analyticsUrl = '/v1/admin/analytics/dashboard?' + analyticsParams(range).toString();
            return Promise.all([
                fetch(analyticsUrl, {{ credentials: 'include', signal: request.signal }}),
                fetch('/health', {{ credentials: 'include', signal: request.signal }})
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
                    if (!data || analyticsRequest !== request) return;
                    analyticsData = data;
                    renderSummary(data.total);
                    renderDashboardHome(data);
                    setMetric('analysis-p50', formatDuration(data.performance.latency_p50_ms));
                    setMetric('analysis-p95', formatDuration(data.performance.latency_p95_ms));
                    setMetric('analysis-p99', formatDuration(data.performance.latency_p99_ms));
                    setMetric('analysis-retries', formatPercent(data.total.request_count ? data.performance.retried_requests / data.total.request_count : 0));
                    setMetric('analysis-active', data.by_model.length + ' / ' + data.by_provider.length);
                    setMetric('analytics-feedback', '已更新 ' + new Date().toLocaleTimeString('zh-CN'));
                    renderCharts(data.timeline);
                    renderUsageBreakdowns(data);
                }})
                .catch(err => {{ if(err.name !== 'AbortError') setMetric('analytics-feedback', '加载失败，请重试'); }})
                .finally(() => {{ if(analyticsRequest === request) analyticsRequest = null; }});
        }}

        function renderCharts(timeline) {{
            if (!timeline || timeline.length === 0) {{
                timeline = [{{ label: '无数据', request_count: 0, success_count: 0, error_count: 0, total_input_tokens: 0, total_output_tokens: 0, total_cached_tokens: 0, total_cache_write_tokens: 0, total_tokens: 0 }}];
            }}

            renderExtraCharts(timeline);
            const labels = timeline.map(b => formatTimelineLabel(b.label || b.group_key));
            const tokenCtx = document.getElementById('tokenTrendChart').getContext('2d');
            const requestCtx = document.getElementById('requestTrendChart').getContext('2d');
            const styles = getComputedStyle(document.documentElement);
            const textColor = styles.getPropertyValue('--muted-foreground').trim();
            const gridColor = styles.getPropertyValue('--border').trim();
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
                    tooltip: chartTooltipOptions()
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

        function chartTooltipOptions() {{
            const styles = getComputedStyle(document.documentElement);
            const color = styles.getPropertyValue('--card-foreground').trim();
            return {{
                backgroundColor: styles.getPropertyValue('--card').trim(),
                borderColor: styles.getPropertyValue('--border').trim(),
                borderWidth: 1,
                titleColor: color,
                bodyColor: color,
                footerColor: color,
                cornerRadius: 6
            }};
        }}

        function renderExtraCharts(timeline) {{
            const style = getComputedStyle(document.documentElement);
            const color = style.getPropertyValue('--muted-foreground').trim();
            const grid = style.getPropertyValue('--border').trim();
            for (const [id, fields] of [
                ['latencyTrendChart', [['平均耗时', 'avg_latency_ms'], ['平均首字节', 'avg_first_byte_latency_ms']]]
            ]) {{
                const data = {{labels:timeline.map(row=>formatTimelineLabel(row.group_key || row.label)), datasets:fields.map(([label,field],index)=>({{label,data:timeline.map(row=>(row[field] || 0)),borderColor:index?'#818cf8':style.getPropertyValue('--primary').trim(),borderWidth:1.5,pointRadius:timeline.length===1?3:0,pointHoverRadius:4,tension:.25}}))}};
                const options = {{responsive:true,maintainAspectRatio:false,animation:false,interaction:{{mode:'index',intersect:false}},plugins:{{legend:{{display:fields.length>1,position:'bottom',labels:{{color,boxWidth:8,font:{{size:11}}}}}},tooltip:{{...chartTooltipOptions(),callbacks:{{label:context=>context.dataset.label+': '+formatDuration(context.parsed.y)}}}}}},scales:{{x:{{grid:{{display:false}},ticks:{{color,maxRotation:0,autoSkip:true,font:{{size:10}}}}}},y:{{beginAtZero:true,grid:{{color:grid}},ticks:{{color,font:{{size:10}}}}}}}}}};
                if(extraCharts.has(id)) {{const chart=extraCharts.get(id);chart.data=data;chart.options=options;chart.update('none');}}
                else extraCharts.set(id,new Chart(document.getElementById(id),{{type:'line',data,options}}));
            }}
        }}

        function refreshDashboard() {{
            loadAnalytics();
        }}

        {home_scripts}

        document.addEventListener('DOMContentLoaded', async function() {{
            const timeRange = document.getElementById('time-range');
            const refreshSelect = document.getElementById('refresh-interval');
            timeRange.value = localStorage.getItem('rcpa_dashboard_time_range') || '1h';
            if(timeRange.value==='custom'||!timeRange.value)timeRange.value='1h';
            refreshSelect.value = localStorage.getItem('rcpa_dashboard_refresh_ms') || '5000';
            let interval = null;
            const resetInterval = () => {{
                if (interval) clearInterval(interval);
                const ms = parseInt(refreshSelect.value);
                if (ms > 0) interval = setInterval(() => {{
                    // Slow queries must finish; only user changes supersede an active request.
                    if (!analyticsRequest) loadAnalytics();
                }}, Math.max(1000, ms));
            }};
            timeRange.addEventListener('change', () => {{
                localStorage.setItem('rcpa_dashboard_time_range', timeRange.value);
                document.getElementById('dashboard-custom-range').hidden=timeRange.value!=='custom';
                if(timeRange.value!=='custom')loadAnalytics();
            }});
            refreshSelect.addEventListener('change', () => {{
                localStorage.setItem('rcpa_dashboard_refresh_ms', refreshSelect.value);
                resetInterval();
            }});
            window.addEventListener('rcpa:theme-change', () => {{
                if (analyticsData) renderCharts(analyticsData.timeline);
            }});
            window.addEventListener('rcpa:data-change', loadAnalytics);
            try {{
                await AdminFilters.mount([
                    ['dashboard-models','models','模型'],['dashboard-providers','providers','供应商'],
                    ['dashboard-keys','keys','Key'],['dashboard-protocols','protocols','协议']
                ], loadAnalytics);
                loadAnalytics(); resetInterval();
            }} catch(error) {{setMetric('analytics-feedback',error.message);}}
        }});
    </script>
</body>
</html>"##,
        shared_styles, toast_container, sidebar, page_start, page_end, shared_scripts
    );

    Ok(trusted_html(html))
}
