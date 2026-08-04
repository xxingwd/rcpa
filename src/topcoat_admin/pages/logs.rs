use topcoat::{
    context::Cx,
    router::{page, uri},
    Result,
};

use crate::store::models::RequestLogFilter;
use crate::topcoat_admin::app::{app_state, require_admin};
use crate::topcoat_admin::{
    escape_html, escape_inline_js_string, format_duration_ms, format_shanghai_time_full,
    format_shanghai_time_short, render_list, render_page, render_shared_scripts,
    render_shared_styles, render_sidebar, render_sidebar_bootstrap, render_theme_bootstrap,
    render_toast_container, trusted_html, ListLayout, PageLayout,
};

const PAGE_SIZE: i64 = 20;

/// Helper to parse query parameters from URI.
fn parse_query_params(uri_str: &str) -> Vec<(String, String)> {
    uri_str
        .split('?')
        .nth(1)
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(k), Some(v)) => Some((url_decode(k), url_decode(v))),
                _ => None,
            }
        })
        .collect()
}

/// Simple URL decoder.
fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes().peekable();
    while let Some(b) = chars.next() {
        match b {
            b'+' => result.push(' '),
            b'%' => {
                let h = chars.next().unwrap_or(0);
                let l = chars.next().unwrap_or(0);
                let byte = (hex_val(h) << 4) | hex_val(l);
                result.push(byte as char);
            }
            _ => result.push(b as char),
        }
    }
    result
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

/// Get query parameter value.
fn get_param(params: &[(String, String)], key: &str) -> Option<String> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .filter(|v| !v.is_empty())
}

fn logs_actions() -> String {
    r##"<select id="filter-key" class="filter-select w-40">
        <option value="all">全部 Key</option>
    </select>
    <select id="filter-model" class="filter-select w-44">
        <option value="all">全部上游模型</option>
    </select>
    <select id="filter-provider" class="filter-select w-36">
        <option value="all">全部供应商</option>
    </select>
    <select id="filter-protocol" class="filter-select w-28">
        <option value="all">全部协议</option>
    </select>
    <select id="filter-status" class="filter-select w-28">
        <option value="all">全部状态</option>
        <option value="success">成功</option>
        <option value="failed">失败</option>
        <option value="running">处理中</option>
        <option value="interrupted">中断</option>
    </select>
    <select id="filter-refresh" class="filter-select w-24">
        <option value="1000">1秒刷新</option>
        <option value="5000">5秒刷新</option>
        <option value="10000">10秒刷新</option>
        <option value="30000">30秒刷新</option>
        <option value="60000">1分钟刷新</option>
        <option value="0">手动</option>
    </select>
    <button class="outline-button h-8 px-3 text-xs" type="button" onclick="loadPage(1)">
        <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M21 12a9 9 0 0 1-15.2 6.5L3 16M3 16v5m0-5h5M3 12A9 9 0 0 1 18.2 5.5L21 8M21 8V3m0 5h-5"/></svg>
        刷新
    </button>"##
        .to_string()
}

fn logs_list_body() -> String {
    r##"<div class="logs-list-content flex min-h-0 min-w-0 flex-1 flex-col">
        <div class="logs-table-wrap min-h-0 min-w-0 flex-1">
            <table class="w-full text-sm min-w-[1440px]">
                <thead class="border-b border-zinc-200 bg-zinc-50">
                    <tr>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">时间</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">请求ID</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">Key</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">供应商</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">协议</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">上游模型</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">Input</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">Output</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">Cache</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">CHR</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">TPS</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">TTFT</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">TPOT</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">耗时</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">价格</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">重试</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">状态</th>
                        <th class="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">详情</th>
                    </tr>
                </thead>
                <tbody id="logs-tbody">
                    <tr><td colspan="18" class="px-4 py-8 text-center text-sm text-zinc-500">加载中...</td></tr>
                </tbody>
            </table>
        </div>
        <div class="logs-pagination-row border-t border-zinc-200 px-4 py-3 flex items-center justify-between text-xs text-zinc-500">
            <div id="logs-info">第 0 - 0 条，共 0 条</div>
            <div id="logs-pagination" class="flex items-center gap-1">
                <button class="pagination-btn" disabled>&laquo;</button>
                <button class="pagination-btn" disabled>&lsaquo;</button>
                <button class="pagination-btn" disabled>&rsaquo;</button>
                <button class="pagination-btn" disabled>&raquo;</button>
            </div>
        </div>
    </div>"##
        .to_string()
}

#[page("/logs")]
pub async fn logs(cx: &Cx) -> Result {
    require_admin(cx)?;
    let sidebar = render_sidebar(cx, "/logs").await?;
    let toast_container = render_toast_container();
    let shared_styles = render_shared_styles();
    let shared_scripts = render_shared_scripts();
    let theme_bootstrap = render_theme_bootstrap();
    let sidebar_bootstrap = render_sidebar_bootstrap();
    let list_view = render_list(
        cx,
        ListLayout {
            id: "logs-list",
            label: "调用审计日志列表",
            endpoint: None,
            refresh_event: None,
            body: trusted_html(logs_list_body()),
        },
    )
    .await?;
    let page_html = render_page(
        cx,
        PageLayout {
            title: "调用审计日志",
            description: Some("查看请求、Token、延迟、重试与费用明细"),
            class_name: "logs-page",
            actions: Some(trusted_html(logs_actions())),
            body: list_view,
        },
    )
    .await?;

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RCPA Admin - 调用日志</title>
    {theme_bootstrap}
    {sidebar_bootstrap}
    <link rel="stylesheet" href="/_topcoat/tailwind.css">
    <script src="/_topcoat/htmx.min.js"></script>
    {}
    <style>
        .log-row:hover {{ background-color: color-mix(in oklch, var(--muted) 50%, transparent); }}
        .filter-select {{ height: 2rem; padding: 0 0.5rem; font-size: 0.75rem; border: 1px solid var(--border); border-radius: 0.375rem; background: var(--card); }}
        .pagination-btn {{ height: 2rem; width: 2rem; display: inline-flex; align-items: center; justify-content: center; border: 1px solid var(--border); border-radius: 0.375rem; background: var(--card); color: var(--foreground); font-size: 0.75rem; }}
        .pagination-btn:hover {{ background: var(--accent); }}
        .pagination-btn.active {{ background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }}
        .pagination-btn:disabled {{ opacity: 0.5; cursor: not-allowed; }}
        .badge-success {{ background: color-mix(in oklch, #10b981 15%, transparent); color: #10b981; padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        .badge-error {{ background: var(--destructive); color: white; padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        .badge-running {{ background: color-mix(in oklch, #d97706 15%, transparent); color: #b45309; padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        .badge-outline {{ border: 1px solid var(--border); padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        .badge-secondary {{ background: var(--muted); color: var(--foreground); padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        .logs-page .page-header {{ align-items: flex-start; }}
        .logs-page .page-actions {{ flex: 1 1 auto; }}
        .logs-page {{ min-width: 0; overflow: hidden; }}
        .logs-page .page-body, .logs-page .data-list {{ display: flex; width: 100%; min-width: 0; min-height: 0; flex: 1 1 auto; overflow: hidden; }}
        .logs-list-content {{ width: 100%; max-width: 100%; overflow: hidden; }}
        .logs-table-wrap {{ width: 100%; max-width: 100%; min-height: 0; overflow: auto; overscroll-behavior: contain; }}
        .logs-table-wrap thead {{ position: sticky; top: 0; z-index: 1; }}
        @media (max-width: 639px) {{
            .filter-select {{ width: calc(50% - .25rem) !important; min-width: 0; }}
            .logs-pagination-row {{ align-items: flex-start; flex-direction: column; gap: .75rem; }}
        }}
    </style>
</head>
<body class="bg-zinc-50 text-zinc-900">
    {}
    <div class="flex min-h-screen">
        {}
        <main class="admin-main">
            <div class="admin-content">{}</div>
        </main>
    </div>

    <!-- Detail Dialog -->
    <div id="detail-dialog" class="modal-overlay hidden" onclick="if(event.target===this) closeDetailDialog()">
        <section class="modal-content" role="dialog" aria-modal="true" aria-labelledby="log-dialog-title" style="width: min(60rem, calc(100vw - 2rem)); max-width: 60rem;">
            <article class="dialog-shell log-dialog">
                <header class="dialog-header">
                    <div class="dialog-heading">
                        <h2 id="log-dialog-title" class="dialog-title">调用日志详情</h2>
                        <p class="dialog-description">请求、响应、Token、延迟与重试信息</p>
                    </div>
                    <button type="button" onclick="closeDetailDialog()" class="icon-button dialog-close" aria-label="关闭" title="关闭">
                        <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M18 6 6 18M6 6l12 12"/></svg>
                    </button>
                </header>
                <div id="detail-body" class="dialog-body space-y-4">
                <div class="text-center text-zinc-500 py-8">加载中...</div>
                </div>
            </article>
        </section>
    </div>

    <script>
        let currentPage = 1;
        let totalPages = 1;
        let totalCount = 0;
        let refreshTimer = null;
        let logsRequest = null;
        const shanghaiTimeFormatter = new Intl.DateTimeFormat('zh-CN', {{
            timeZone: 'Asia/Shanghai',
            year: 'numeric', month: '2-digit', day: '2-digit',
            hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23'
        }});

        function formatTokens(n) {{
            n = parseInt(n) || 0;
            if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
            if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
            return n.toString();
        }}

        function formatDuration(ms) {{
            ms = Number(ms);
            if (!Number.isFinite(ms) || ms < 0) ms = 0;
            if (ms < 1000) return Math.min(999, Math.round(ms)) + 'ms';
            return (ms / 1000).toFixed(2).replace(/\.?0+$/, '') + 's';
        }}

        function formatPercent(v) {{
            return (parseFloat(v) * 100).toFixed(1) + '%';
        }}

        function formatCost(cents) {{
            return '¥' + (parseInt(cents) / 100).toFixed(4);
        }}

        function formatTps(outputTokens, latencyMs, firstByteMs) {{
            const genTime = Math.max(0, parseInt(latencyMs) - parseInt(firstByteMs)) / 1000;
            if (!genTime) return '0.0';
            return (parseInt(outputTokens) / genTime).toFixed(1);
        }}

        function formatTpot(outputTokens, latencyMs, firstByteMs) {{
            outputTokens = parseInt(outputTokens);
            if (!outputTokens) return '0ms';
            return formatDuration(Math.max(0, parseInt(latencyMs) - parseInt(firstByteMs)) / outputTokens);
        }}

        function formatTime(iso) {{
            const d = new Date(iso);
            if (Number.isNaN(d.getTime())) return String(iso || '');
            return shanghaiTimeFormatter.format(d).replaceAll('/', '-') + ' UTC+8';
        }}

        function cacheRate(input, cached) {{
            input = parseInt(input);
            return input > 0 ? parseInt(cached) / input : 0;
        }}

        function escapeHtml(str) {{
            const div = document.createElement('div');
            div.textContent = str;
            return div.innerHTML;
        }}

        function loadPage(page) {{
            currentPage = page;
            if (refreshTimer) {{
                clearTimeout(refreshTimer);
                refreshTimer = null;
            }}
            if (logsRequest) logsRequest.abort();
            const request = new AbortController();
            logsRequest = request;
            const key = document.getElementById('filter-key').value;
            const model = document.getElementById('filter-model').value;
            const provider = document.getElementById('filter-provider').value;
            const protocol = document.getElementById('filter-protocol').value;
            const status = document.getElementById('filter-status').value;
            const params = new URLSearchParams({{
                page: page.toString(),
                limit: '20',
                api_key_id: key === 'all' ? '' : key,
                model: model === 'all' ? '' : model,
                provider_name: provider === 'all' ? '' : provider,
                protocol: protocol === 'all' ? '' : protocol,
                status: status === 'all' ? '' : status,
            }});
            return fetch('/logs/table?' + params.toString(), {{ signal: request.signal }})
                .then(r => {{
                    if (r.status === 401) return redirectToLogin();
                    return r.text();
                }})
                .then(html => {{
                    if (!html) return;
                    const pagination = html.match(/<!-- pagination-data data-total="(\d+)" data-limit="(\d+)" data-offset="(\d+)" -->/);
                    const limit = pagination ? parseInt(pagination[2]) : 20;
                    const offset = pagination ? parseInt(pagination[3]) : (page - 1) * limit;
                    totalCount = pagination ? parseInt(pagination[1]) : 0;
                    totalPages = Math.max(1, Math.ceil(totalCount / limit));
                    document.getElementById('logs-tbody').innerHTML = html;
                    const firstRow = totalCount === 0 ? 0 : offset + 1;
                    const lastRow = Math.min(offset + limit, totalCount);
                    document.getElementById('logs-info').textContent = `第 ${{firstRow}} - ${{lastRow}} 条，共 ${{totalCount}} 条`;
                    updatePagination(page, totalPages);
                }})
                .catch(error => {{
                    if (error.name !== 'AbortError') console.error('Failed to load logs:', error);
                }})
                .finally(() => {{
                    if (logsRequest === request) {{
                        logsRequest = null;
                        scheduleRefresh();
                    }}
                }});
        }}

        function addFilterOption(selectId, value, label) {{
            const option = document.createElement('option');
            option.value = value;
            option.textContent = label;
            document.getElementById(selectId).appendChild(option);
        }}

        async function loadFilterOptions() {{
            const responses = await Promise.allSettled([
                fetch('/v1/admin/keys', {{ credentials: 'include' }}),
                fetch('/v1/admin/providers', {{ credentials: 'include' }}),
                fetch('/v1/admin/analytics/model', {{ credentials: 'include' }})
            ]);
            const readJson = async (result) => result.status === 'fulfilled' && result.value.ok ? result.value.json() : [];
            const [keys, providers, models] = await Promise.all(responses.map(readJson));
            (Array.isArray(keys) ? keys : []).forEach((key) => addFilterOption('filter-key', key.id, key.name || key.id));
            (Array.isArray(providers) ? providers : []).forEach((provider) => addFilterOption('filter-provider', provider.name, provider.name));
            const protocols = new Set();
            (Array.isArray(providers) ? providers : []).forEach((provider) => (provider.endpoints || []).forEach((endpoint) => {{
                if (endpoint.protocol) protocols.add(endpoint.protocol);
            }}));
            Array.from(protocols).sort().forEach((protocol) => addFilterOption('filter-protocol', protocol, protocol));
            (Array.isArray(models) ? models : []).forEach((model) => {{
                if (model.group_key) addFilterOption('filter-model', model.group_key, model.group_key);
            }});
        }}

        function openDetail(logId) {{
            document.getElementById('detail-dialog').classList.remove('hidden');
            document.getElementById('detail-body').innerHTML = '<div class="text-center text-zinc-500 py-8">加载中...</div>';
            fetch('/v1/admin/logs/' + encodeURIComponent(logId), {{ credentials: 'include' }})
                .then(response => {{
                    if (response.status === 401) {{
                        window.location.href = '/login';
                        return;
                    }}
                    return response.json();
                }})
                .then(data => {{ if (data) renderDetail(data); }})
                .catch(() => {{ document.getElementById('detail-body').innerHTML = '<div class="text-center text-red-500 py-8">加载失败</div>'; }});
        }}

        function closeDetailDialog() {{
            document.getElementById('detail-dialog').classList.add('hidden');
            document.getElementById('detail-body').innerHTML = '';
        }}

        function renderDetail(d) {{
            const log = d.log || d;
            const isRunning = log.status === 'running';
            const isErr = !isRunning && log.status !== 'success';
            let statusDisplay;
            if (isRunning) {{
                statusDisplay = log.retry_count > 0 ? '重试中' : '处理中';
            }} else if (isErr && log.status_code < 400) {{
                statusDisplay = (log.status === 'interrupted' ? '中断' : '失败') + ' (' + log.status_code + ')';
            }} else {{
                statusDisplay = log.status_code;
            }}
            const firstByte = log.first_byte_latency_ms || 0;
            const inputTokens = log.input_tokens || 0;
            const outputTokens = log.output_tokens || 0;
            const cachedTokens = log.cached_tokens || 0;
            const hitRate = cacheRate(inputTokens, cachedTokens);

            let meta = {{}};
            try {{ meta = JSON.parse(log.meta || '{{}}'); }} catch(e) {{}}

            let requestBody = d.request_body;
            let responseBody = d.response_body;
            if (typeof requestBody === 'string') {{ try {{ requestBody = JSON.stringify(JSON.parse(requestBody), null, 2); }} catch(e) {{}} }}
            if (typeof responseBody === 'string') {{ try {{ responseBody = JSON.stringify(JSON.parse(responseBody), null, 2); }} catch(e) {{}} }}

            const html = `
                <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">状态码</div>
                        <div class="font-mono text-sm ${{isRunning ? 'text-amber-700' : (isErr ? 'text-red-600' : 'text-emerald-600')}}">${{statusDisplay}}</div>
                    </div>
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">耗时</div>
                        <div class="font-mono text-sm">${{formatDuration(log.latency_ms)}}</div>
                    </div>
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">TTFT</div>
                        <div class="font-mono text-sm">${{isRunning ? '—' : formatDuration(firstByte)}}</div>
                    </div>
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">输入 / 输出</div>
                        <div class="font-mono text-sm">${{isRunning ? '—' : formatTokens(inputTokens) + ' / ' + formatTokens(outputTokens)}}</div>
                    </div>
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">缓存命中 / 写入</div>
                        <div class="font-mono text-sm">${{isRunning ? '—' : formatTokens(cachedTokens) + ' / ' + formatTokens(log.cache_write_tokens || 0)}}</div>
                    </div>
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">总 Tokens</div>
                        <div class="font-mono text-sm">${{isRunning ? '—' : formatTokens(log.total_tokens || 0)}}</div>
                    </div>
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">CHR</div>
                        <div class="font-mono text-sm">${{isRunning ? '—' : formatPercent(hitRate)}}</div>
                    </div>
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">TPS</div>
                        <div class="font-mono text-sm">${{isRunning ? '—' : formatTps(outputTokens, log.latency_ms, firstByte)}}</div>
                    </div>
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">TPOT</div>
                        <div class="font-mono text-sm">${{isRunning ? '—' : formatTpot(outputTokens, log.latency_ms, firstByte)}}</div>
                    </div>
                    <div class="rounded-lg border bg-zinc-50 p-3">
                        <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 mb-1">价格</div>
                        <div class="font-mono text-sm">${{isRunning ? '—' : formatCost(log.cost_cents)}}</div>
                    </div>
                </div>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
                    <div><span class="text-zinc-500">请求 ID：</span><span class="font-mono break-all">${{escapeHtml(log.request_id)}}</span></div>
                    <div><span class="text-zinc-500">日志 ID：</span><span class="font-mono break-all">${{escapeHtml(log.id)}}</span></div>
                    <div><span class="text-zinc-500">上游模型：</span><span class="font-mono">${{escapeHtml(log.model)}}</span></div>
                    <div><span class="text-zinc-500">Key：</span><span class="font-mono">${{escapeHtml(log.key_display_name || log.api_key_id)}}</span></div>
                    <div><span class="text-zinc-500">供应商：</span><span class="font-mono">${{escapeHtml(log.provider_name)}}</span></div>
                    <div><span class="text-zinc-500">接口：</span><span class="font-mono">${{escapeHtml(log.operation)}} / ${{escapeHtml(log.protocol)}}</span></div>
                    <div><span class="text-zinc-500">时间：</span><span class="font-mono">${{formatTime(log.created_at)}}</span></div>
                </div>
                ${{meta.retry ? `
                <div class="rounded-lg border bg-zinc-50 p-3">
                    <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 font-semibold mb-2">重试</div>
                    <div class="flex flex-wrap gap-2 text-xs">
                        <span class="badge-secondary font-mono">retry=${{meta.retry.retry_count || 0}}</span>
                        <span class="badge-outline font-mono">attempts=${{meta.retry.attempt_count || 0}}</span>
                        <span class="badge-outline font-mono">backoff=${{formatDuration(meta.retry.total_backoff_ms || 0)}}</span>
                    </div>
                    ${{Array.isArray(meta.retry.attempts) && meta.retry.attempts.length > 0 ? `
                    <div class="mt-2 space-y-2">
                        ${{meta.retry.attempts.map(a => `
                        <div class="rounded-md border bg-white p-2">
                            <div class="flex flex-wrap items-center gap-2 text-xs">
                                <span class="badge-outline font-mono">#${{a.attempt}}</span>
                                <span class="${{a.retryable ? 'badge-error' : 'badge-success'}} font-mono">${{a.status_code}}</span>
                                <span class="font-mono">${{escapeHtml(a.provider_name)}}</span>
                                <span class="text-zinc-500">${{escapeHtml(a.protocol)}}</span>
                                <span class="text-zinc-500">backoff=${{formatDuration(a.backoff_ms_before_next || 0)}}</span>
                            </div>
                            ${{a.error_code ? `<div class="mt-1 text-xs text-zinc-500 break-all">${{escapeHtml(a.error_code)}}${{a.error_message ? ': ' + escapeHtml(a.error_message) : ''}}</div>` : ''}}
                        </div>
                        `).join('')}}
                    </div>` : '<div class="text-xs text-zinc-500 mt-2">无重试</div>'}}
                </div>` : ''}}
                ${{log.error_code || log.error ? `
                <div class="rounded-lg border border-red-200 bg-red-50 p-3">
                    <div class="text-[0.65rem] uppercase tracking-wider text-red-600 font-semibold mb-2">错误</div>
                    <div class="font-mono text-xs break-all">${{escapeHtml(log.error_code || 'unknown')}}</div>
                    <div class="text-xs text-zinc-500 mt-1 break-all">${{escapeHtml(log.error || '')}}</div>
                </div>` : ''}}
                <div>
                    <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 font-semibold mb-2">元数据 JSON</div>
                    <pre class="rounded-lg border bg-zinc-50 p-3 text-xs overflow-x-auto max-h-64 overflow-y-auto"><code>${{escapeHtml(JSON.stringify(meta, null, 2))}}</code></pre>
                </div>
                ${{requestBody ? `
                <div>
                    <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 font-semibold mb-2">请求体</div>
                    <pre class="rounded-lg border bg-zinc-50 p-3 text-xs overflow-x-auto max-h-64 overflow-y-auto"><code>${{escapeHtml(typeof requestBody === 'string' ? requestBody : JSON.stringify(requestBody, null, 2))}}</code></pre>
                </div>` : ''}}
                ${{responseBody ? `
                <div>
                    <div class="text-[0.65rem] uppercase tracking-wider text-zinc-500 font-semibold mb-2">响应体</div>
                    <pre class="rounded-lg border bg-zinc-50 p-3 text-xs overflow-x-auto max-h-64 overflow-y-auto"><code>${{escapeHtml(typeof responseBody === 'string' ? responseBody : JSON.stringify(responseBody, null, 2))}}</code></pre>
                </div>` : ''}}
            `;
            document.getElementById('detail-body').innerHTML = html;
        }}

        function updatePagination(page, total) {{
            const container = document.getElementById('logs-pagination');
            let html = '';
            html += `<button class="pagination-btn" ${{page <= 1 ? 'disabled' : ''}} onclick="loadPage(1)">&laquo;</button>`;
            html += `<button class="pagination-btn" ${{page <= 1 ? 'disabled' : ''}} onclick="loadPage(${{page - 1}})">&lsaquo;</button>`;
            const start = Math.max(1, page - 2);
            const end = Math.min(total, start + 4);
            for (let i = start; i <= end; i++) {{
                html += `<button class="pagination-btn ${{i === page ? 'active' : ''}}" onclick="loadPage(${{i}})">${{i}}</button>`;
            }}
            html += `<button class="pagination-btn" ${{page >= total ? 'disabled' : ''}} onclick="loadPage(${{page + 1}})">&rsaquo;</button>`;
            html += `<button class="pagination-btn" ${{page >= total ? 'disabled' : ''}} onclick="loadPage(${{total}})">&raquo;</button>`;
            container.innerHTML = html;
        }}

        function scheduleRefresh() {{
            if (refreshTimer) clearTimeout(refreshTimer);
            refreshTimer = null;
            const refreshSelect = document.getElementById('filter-refresh');
            const ms = parseInt(refreshSelect.value);
            if (ms > 0 && !document.hidden) {{
                refreshTimer = setTimeout(() => loadPage(currentPage), Math.max(1000, ms));
            }}
        }}

        // Initialize
        document.addEventListener('DOMContentLoaded', function() {{
            const query = new URLSearchParams(window.location.search);
            const refreshSelect = document.getElementById('filter-refresh');
            refreshSelect.value = localStorage.getItem('rcpa_logs_refresh_ms') || '1000';
            refreshSelect.addEventListener('change', () => {{
                localStorage.setItem('rcpa_logs_refresh_ms', refreshSelect.value);
                scheduleRefresh();
            }});
            document.addEventListener('visibilitychange', () => {{
                if (document.hidden) {{
                    if (refreshTimer) clearTimeout(refreshTimer);
                    refreshTimer = null;
                }} else {{
                    loadPage(currentPage);
                }}
            }});
            ['filter-key', 'filter-model', 'filter-provider', 'filter-protocol', 'filter-status'].forEach((id) => {{
                document.getElementById(id).addEventListener('change', () => loadPage(1));
            }});
            loadFilterOptions().finally(() => {{
                document.getElementById('filter-key').value = query.get('key') || 'all';
                document.getElementById('filter-model').value = query.get('model') || 'all';
                document.getElementById('filter-provider').value = query.get('provider_name') || 'all';
                document.getElementById('filter-protocol').value = query.get('protocol') || 'all';
                document.getElementById('filter-status').value = query.get('status') || 'all';
                loadPage(Math.max(1, parseInt(query.get('page')) || 1));
            }});
        }});
    </script>
    {}
</body>
</html>"##,
        shared_styles, toast_container, sidebar, page_html, shared_scripts
    );

    Ok(trusted_html(html))
}

#[page("/logs/table")]
pub async fn logs_table(cx: &Cx) -> Result {
    require_admin(cx)?;
    let uri_str = uri(cx).to_string();
    let params = parse_query_params(&uri_str);

    let page: i64 = get_param(&params, "page")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let limit: i64 = get_param(&params, "limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(PAGE_SIZE);
    let offset = (page - 1) * limit;

    let state = app_state(cx);
    let filter = RequestLogFilter {
        from: None,
        to: None,
        api_key_id: get_param(&params, "api_key_id"),
        session_hash: None,
        model: get_param(&params, "model"),
        provider_name: get_param(&params, "provider_name"),
        protocol: get_param(&params, "protocol"),
        status: get_param(&params, "status"),
        status_code: None,
        success: None,
        limit: Some(limit),
        offset: Some(offset),
    };

    let log_entries = state
        .store
        .query_request_logs(&filter)
        .await
        .unwrap_or_default();
    let total = state.store.count_request_logs(&filter).await.unwrap_or(0);
    let snapshot = state.config_service.snapshot();

    let mut html = String::new();

    if log_entries.is_empty() {
        html.push_str(
            r##"<tr><td colspan="18" class="px-4 py-8 text-center text-sm text-zinc-500">暂无接口请求记录</td></tr>"##,
        );
    } else {
        for log in &log_entries {
            let is_running = log.status == "running";
            let is_err = !is_running && log.status != "success";
            let first_byte = log.first_byte_latency_ms;
            let input_tokens = log.input_tokens;
            let output_tokens = log.output_tokens;
            let cached_tokens = log.cached_tokens;
            let hit_rate = if input_tokens > 0 {
                cached_tokens as f64 / input_tokens as f64
            } else {
                0.0
            };
            let gen_time_ms = std::cmp::max(0, log.latency_ms - first_byte);
            let tps = if gen_time_ms > 0 {
                output_tokens as f64 / (gen_time_ms as f64 / 1000.0)
            } else {
                0.0
            };
            let tpot = if output_tokens > 0 {
                gen_time_ms as f64 / output_tokens as f64
            } else {
                0.0
            };

            let (status_class, status_label) = if is_running {
                (
                    "badge-running",
                    if log.retry_count > 0 {
                        "重试中".to_string()
                    } else {
                        "处理中".to_string()
                    },
                )
            } else if is_err {
                // A failure with a 2xx/3xx code is not a real HTTP error; show the
                // failure reason instead of a misleading success-range code.
                let label = if log.status_code < 400 {
                    if log.status == "interrupted" {
                        "中断".to_string()
                    } else {
                        "失败".to_string()
                    }
                } else {
                    log.status_code.to_string()
                };
                ("badge-error", label)
            } else {
                ("badge-success", log.status_code.to_string())
            };
            let input_display = if is_running {
                "—".to_string()
            } else {
                format_tokens(input_tokens)
            };
            let output_display = if is_running {
                "—".to_string()
            } else {
                format_tokens(output_tokens)
            };
            let cached_display = if is_running {
                "—".to_string()
            } else {
                format_tokens(cached_tokens)
            };
            let hit_rate_display = if is_running {
                "—".to_string()
            } else {
                format!("{:.1}%", hit_rate * 100.0)
            };
            let tps_display = if is_running {
                "—".to_string()
            } else {
                format!("{tps:.1}")
            };
            let ttft_display = if is_running {
                "—".to_string()
            } else {
                format_duration_ms(first_byte as f64)
            };
            let tpot_display = if is_running {
                "—".to_string()
            } else {
                format_duration_ms(tpot)
            };
            let cost_display = if is_running {
                "—".to_string()
            } else {
                format!("¥{:.4}", log.cost_cents as f64 / 100.0)
            };
            let time_short = escape_html(&format_shanghai_time_short(&log.created_at));
            let time_full = escape_html(&format_shanghai_time_full(&log.created_at));
            let request_id_short = if log.request_id.len() > 8 {
                &log.request_id[..8]
            } else {
                &log.request_id
            };
            let request_id_short = escape_html(request_id_short);
            let key_display_name = escape_html(snapshot.auth_key_display_name(&log.api_key_id));
            let provider_name = escape_html(&log.provider_name);
            let protocol = escape_html(&log.protocol);
            let operation = escape_html(&log.operation);
            let model = escape_html(&log.model);
            let log_id_js = escape_inline_js_string(&log.id);

            html.push_str(&format!(
                r##"<tr class="log-row border-b border-zinc-100">
                    <td class="px-3 py-2.5 whitespace-nowrap font-mono text-xs text-zinc-500" title="{}">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs text-zinc-500 whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5"><span class="badge-secondary font-mono truncate block max-w-24" title="{}">{}</span></td>
                    <td class="px-3 py-2.5"><span class="badge-outline font-mono truncate block max-w-28">{}</span></td>
                    <td class="px-3 py-2.5"><div class="flex flex-col gap-0.5"><span class="badge-outline font-mono truncate block max-w-24">{}</span><span class="truncate font-mono text-[10px] text-zinc-400">{}</span></div></td>
                    <td class="px-3 py-2.5"><span class="badge-outline font-mono truncate block max-w-32" title="{}">{}</span></td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5"><span class="{} font-mono px-2 py-0.5 text-xs">{}</span></td>
                    <td class="px-3 py-2.5"><button class="inline-flex h-7 w-7 items-center justify-center rounded border border-zinc-200 hover:bg-zinc-50" onclick="openDetail({})"><svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/></svg></button></td>
                </tr>"##,
                time_full,
                time_short,
                request_id_short,
                key_display_name,
                key_display_name,
                provider_name,
                protocol,
                operation,
                model,
                model,
                input_display,
                output_display,
                cached_display,
                hit_rate_display,
                tps_display,
                ttft_display,
                tpot_display,
                format_duration_ms(log.latency_ms as f64),
                cost_display,
                log.retry_count,
                status_class,
                status_label,
                log_id_js
            ));
        }
    }

    let full_html = format!(
        r##"{}<!-- pagination-data data-total="{}" data-limit="{}" data-offset="{}" -->"##,
        html, total, limit, offset
    );

    Ok(trusted_html(full_html))
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
