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
    reqwest::Url::parse(&format!("http://localhost{uri_str}"))
        .map(|url| url.query_pairs().into_owned().collect())
        .unwrap_or_default()
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
    r##"<div id="filter-key"></div><div id="filter-model"></div><div id="filter-provider"></div><div id="filter-protocol"></div>
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
                    <tr><td colspan="18" class="px-4 py-8 text-sm text-zinc-500"><span class="logs-empty-message">加载中...</span></td></tr>
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
        {log_detail_css}
        .log-row:hover {{ background-color: color-mix(in oklch, var(--muted) 50%, transparent); }}
        .filter-select {{ height: var(--control-height); padding: 0 0.5rem; font-size: 0.8125rem; border: 1px solid var(--border); border-radius: 0.375rem; background: var(--card); }}
        .pagination-btn {{ height: var(--control-height); width: 2.375rem; display: inline-flex; align-items: center; justify-content: center; border: 1px solid var(--border); border-radius: 0.375rem; background: var(--card); color: var(--foreground); font-size: 0.75rem; }}
        .pagination-btn:hover {{ background: var(--accent); }}
        .pagination-btn.active {{ background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }}
        .pagination-btn:disabled {{ opacity: 0.5; cursor: not-allowed; }}
        .badge-success {{ background: color-mix(in srgb, var(--success) 12%, var(--card)); color: var(--success); padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        .badge-error {{ background: color-mix(in srgb, var(--destructive) 12%, var(--card)); color: var(--destructive); padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        .badge-running {{ background: color-mix(in srgb, #b66d16 12%, var(--card)); color: #a46212; padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        html[data-theme="dark"] .badge-running {{ color: #f0be79; }}
        .badge-outline {{ border: 1px solid var(--border); padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        .badge-secondary {{ background: var(--muted); color: var(--foreground); padding: 0.125rem 0.5rem; border-radius: 6px; font-size: 0.75rem; }}
        .logs-page .page-header {{ align-items: flex-start; }}
        .logs-page .page-actions {{ flex: 1 1 auto; }}
        .logs-page {{ min-width: 0; }}
        .logs-page .page-body, .logs-page .data-list {{ display: flex; width: 100%; min-width: 0; min-height: 0; flex: 1 1 auto; overflow: hidden; }}
        .logs-list-content {{ width: 100%; max-width: 100%; overflow: hidden; }}
        .logs-table-wrap {{ width: 100%; max-width: 100%; min-height: 0; overflow: auto; overscroll-behavior: contain; }}
        .logs-table-wrap thead {{ position: sticky; top: 0; z-index: 1; }}
        .logs-empty-message {{ position: sticky; left: 0; display: grid; width: calc(100vw - 19rem); min-height: 3.5rem; place-items: center; }}
        @media (max-width: 1023px) {{ .logs-empty-message {{ width: calc(100vw - 4rem); }} }}
        @media (max-width: 639px) {{
            .filter-select {{ width: calc(50% - .25rem) !important; min-width: 0; }}
            .pagination-btn {{ width: 2.75rem; height: 2.75rem; }}
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

    <div id="detail-dialog" class="modal-overlay log-detail-overlay hidden" onclick="if(event.target===this) closeDetailDialog()">
        <section class="modal-content log-detail-panel" role="dialog" aria-modal="true" aria-labelledby="log-dialog-title">
            <header class="log-detail-header"><div><h2 id="log-dialog-title">调用详情</h2><p id="log-dialog-subtitle">请求与响应</p></div><div class="log-header-actions"><button type="button" onclick="refreshLogDetail()" class="icon-button" aria-label="刷新详情" title="刷新详情"><svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M20 7v5h-5M4 17v-5h5M6 7a7 7 0 0 1 12-1l2 3M4 15l2 3a7 7 0 0 0 12-1"/></svg></button><button type="button" onclick="closeDetailDialog()" class="icon-button dialog-close" aria-label="关闭详情" title="关闭"><svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="m6 6 12 12M18 6 6 18"/></svg></button></div></header>
            <div id="detail-body" class="log-detail-body" aria-busy="false"></div>
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
            return new Intl.NumberFormat('zh-CN', {{style:'currency',currency:AdminFilters.currency||'USD',maximumFractionDigits:4}}).format((Number(cents)||0)/100);
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
            if (!document.getElementById('detail-dialog').classList.contains('hidden')) return Promise.resolve();
            currentPage = page;
            if (refreshTimer) {{
                clearTimeout(refreshTimer);
                refreshTimer = null;
            }}
            if (logsRequest) logsRequest.abort();
            const request = new AbortController();
            logsRequest = request;
            const status = document.getElementById('filter-status').value;
            const params = AdminFilters.append(new URLSearchParams({{
                page: page.toString(), limit: '20', status: status === 'all' ? '' : status
            }}));
            return fetch('/logs/table?' + params.toString(), {{ signal: request.signal }})
                .then(r => {{
                    if (r.status === 401) return redirectToLogin();
                    return r.text();
                }})
                .then(html => {{
                    if (!html || logsRequest !== request || !document.getElementById('detail-dialog').classList.contains('hidden')) return;
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

        async function loadFilterOptions() {{
            await AdminFilters.mount([
                ['filter-key', 'keys', 'Key'], ['filter-model', 'models', '模型'],
                ['filter-provider', 'providers', '供应商'], ['filter-protocol', 'protocols', '协议']
            ], () => loadPage(1));
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
            if (ms > 0 && !document.hidden && document.getElementById('detail-dialog').classList.contains('hidden')) {{
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
            ['filter-status'].forEach((id) => {{
                document.getElementById(id).addEventListener('change', () => loadPage(1));
            }});
            loadFilterOptions().finally(() => {{
                AdminFilters.restore('filter-key', query.has('key') ? [query.get('key')] : []);
                AdminFilters.restore('filter-model', query.has('model') ? [query.get('model')] : []);
                AdminFilters.restore('filter-provider', query.has('provider_name') ? [query.get('provider_name')] : []);
                AdminFilters.restore('filter-protocol', query.has('protocol') ? [query.get('protocol')] : []);
                document.getElementById('filter-status').value = query.get('status') || 'all';
                loadPage(Math.max(1, parseInt(query.get('page')) || 1));
            }});
        }});
    </script>
    {}
    <script>{log_detail_js}</script>
</body>
</html>"##,
        shared_styles,
        toast_container,
        sidebar,
        page_html,
        shared_scripts,
        log_detail_css = include_str!("../../../static/admin/log-detail.css"),
        log_detail_js = include_str!("../../../static/admin/log-detail.js")
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
    let selection = crate::admin::SelectionQuery {
        models: get_param(&params, "models"),
        providers: get_param(&params, "providers"),
        keys: get_param(&params, "keys"),
        protocols: get_param(&params, "protocols"),
    }
    .resolve(&state.config_service.snapshot())
    .map_err(|error| topcoat::Error::from(std::io::Error::other(error.to_string())))?;
    let filter = RequestLogFilter {
        selection,
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
            r##"<tr><td colspan="18" class="px-4 py-8 text-sm text-zinc-500"><span class="logs-empty-message">暂无接口请求记录</span></td></tr>"##,
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
            let log_id_html = escape_html(&log.id);
            let logo = snapshot.model_logo_url(&log.provider_name, &log.model).map(|url| format!(r#"<img src="{}" alt="" loading="lazy" referrerpolicy="no-referrer" onerror="this.remove()">"#, escape_html(url))).unwrap_or_default();

            html.push_str(&format!(
                r##"<tr class="log-row border-b border-zinc-100" data-row-action="openDetail" data-row-value="{log_id_html}" tabindex="0" aria-label="查看调用 {request_id_short}">
                    <td class="px-3 py-2.5 whitespace-nowrap font-mono text-xs text-zinc-500" title="{}">{}</td>
                    <td class="px-3 py-2.5 font-mono text-xs text-zinc-500 whitespace-nowrap">{}</td>
                    <td class="px-3 py-2.5"><span class="badge-secondary font-mono truncate block max-w-24" title="{}">{}</span></td>
                    <td class="px-3 py-2.5"><span class="badge-outline font-mono truncate block max-w-28">{}</span></td>
                    <td class="px-3 py-2.5"><div class="flex flex-col gap-0.5"><span class="badge-outline font-mono truncate block max-w-24">{}</span><span class="truncate font-mono text-[10px] text-zinc-400">{}</span></div></td>
                    <td class="px-3 py-2.5"><span class="model-mark">{logo}<span class="badge-outline font-mono truncate block max-w-32" title="{}">{}</span></span></td>
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
                    <td class="px-3 py-2.5"><button class="inline-flex h-7 w-7 items-center justify-center rounded border border-zinc-200 hover:bg-zinc-50" onclick="openDetail({})" aria-label="查看调用详情" title="查看调用详情"><svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/></svg></button></td>
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
