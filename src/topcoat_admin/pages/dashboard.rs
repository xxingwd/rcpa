use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};

use crate::topcoat_admin::api::fetch_dashboard_stats;

#[page("/")]
pub async fn dashboard(cx: &Cx) -> Result {
    let stats = fetch_dashboard_stats(cx).await;
    let total_requests = stats.requests.total;
    let total_tokens = stats.tokens.total;
    let avg_latency = stats.latency.avg_ms;
    let total_cost = stats.cost.total_cents;
    let success_rate = stats.requests.success_rate;
    let input_tokens = stats.tokens.input;
    let output_tokens = stats.tokens.output;
    let cached_tokens = stats.tokens.cached;
    let cache_hit_rate = stats.tokens.cache_hit_rate;
    let avg_first_byte = stats.latency.first_byte_avg_ms;
    let avg_tokens_per_req = stats.tokens.avg_per_request;
    let _error_count = stats.requests.errors;
    let _max_latency = stats.latency.max_ms;
    let _max_first_byte = stats.latency.first_byte_max_ms;
    
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"RCPA Admin - 仪表盘"</title>
                <script src="https://cdn.tailwindcss.com"></script>
                <script src="https://unpkg.com/htmx.org@2.0.4"></script>
            </head>
            <body class="bg-zinc-50 text-zinc-900">
                <div class="flex min-h-screen">
                    <aside class="flex h-screen w-64 flex-col border-r border-zinc-200 bg-white">
                        <div class="border-b border-zinc-200 px-6 py-4">
                            <h2 class="text-lg font-bold text-zinc-900">"RCPA"</h2>
                            <p class="text-xs text-zinc-500">"LLM 网关管理"</p>
                        </div>
                        <nav class="flex-1 space-y-1 px-3 py-4">
                            <a href="/" class="block rounded-lg bg-zinc-100 px-3 py-2 text-sm font-medium">"仪表盘"</a>
                            <a href="/keys" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"密钥管理"</a>
                            <a href="/providers" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"供应商"</a>
                            <a href="/logs" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"调用日志"</a>
                            <a href="/config" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"配置"</a>
                        </nav>
                    </aside>
                    <main class="flex-1 p-8">
                        <div class="mx-auto max-w-7xl">
                            <header class="mb-8 flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
                                <h1 class="text-2xl font-bold">"仪表盘"</h1>
                                <div class="flex flex-wrap items-center gap-2">
                                    <select class="h-8 rounded-lg border border-zinc-200 bg-white px-3 text-xs">
                                        <option>"今天"</option>
                                        <option>"昨天"</option>
                                        <option>"本周"</option>
                                        <option>"上周"</option>
                                        <option>"本月"</option>
                                        <option>"全部"</option>
                                    </select>
                                    <select class="h-8 rounded-lg border border-zinc-200 bg-white px-3 text-xs">
                                        <option>"5 秒刷新"</option>
                                        <option>"10 秒刷新"</option>
                                        <option>"30 秒刷新"</option>
                                        <option>"1 分钟刷新"</option>
                                        <option>"手动"</option>
                                    </select>
                                    <div class="flex items-center gap-2 rounded-md border bg-muted px-3 py-1.5 text-xs font-medium text-muted-foreground">
                                        <div class="h-2 w-2 animate-pulse rounded-full bg-emerald-500"></div>
                                        <span>"在线"</span>
                                    </div>
                                </div>
                            </header>

                            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
                                <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                    <p class="text-sm font-medium text-zinc-500">"总请求数"</p>
                                    <p class="mt-2 text-3xl font-bold">(format!("{}", total_requests))</p>
                                    <p class="mt-2 text-sm text-zinc-500">(format!("成功率 {:.1}%", success_rate * 100.0))</p>
                                </div>
                                <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                    <p class="text-sm font-medium text-zinc-500">"Token 消耗"</p>
                                    <p class="mt-2 text-3xl font-bold">(format!("{}", total_tokens))</p>
                                    <p class="mt-2 text-sm text-zinc-500">(format!("入 {} / 出 {}", input_tokens, output_tokens))</p>
                                </div>
                                <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                    <p class="text-sm font-medium text-zinc-500">"平均延迟"</p>
                                    <p class="mt-2 text-3xl font-bold">(format!("{:.0}ms", avg_latency))</p>
                                    <p class="mt-2 text-sm text-zinc-500">(format!("首字节 {:.0}ms", avg_first_byte))</p>
                                </div>
                                <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                    <p class="text-sm font-medium text-zinc-500">"总成本"</p>
                                    <p class="mt-2 text-3xl font-bold">(format!("{:.2}€", total_cost as f64 / 100.0))</p>
                                    <p class="mt-2 text-sm text-zinc-500">"累计"</p>
                                </div>
                            </div>

                            <div class="mt-8 grid grid-cols-1 gap-6 lg:grid-cols-2">
                                <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                    <h3 class="mb-4 text-sm font-semibold">"Token 用量详情"</h3>
                                    <div class="grid grid-cols-5 gap-4 text-xs">
                                        <div>
                                            <p class="text-zinc-500">"输入"</p>
                                            <p class="mt-1 font-mono font-semibold">(format!("{}", input_tokens))</p>
                                        </div>
                                        <div>
                                            <p class="text-zinc-500">"输出"</p>
                                            <p class="mt-1 font-mono font-semibold">(format!("{}", output_tokens))</p>
                                        </div>
                                        <div>
                                            <p class="text-zinc-500">"命中"</p>
                                            <p class="mt-1 font-mono font-semibold">(format!("{}", cached_tokens))</p>
                                        </div>
                                        <div>
                                            <p class="text-zinc-500">"命中率"</p>
                                            <p class="mt-1 font-mono font-semibold">(format!("{:.1}%", cache_hit_rate * 100.0))</p>
                                        </div>
                                        <div>
                                            <p class="text-zinc-500">"平均/请求"</p>
                                            <p class="mt-1 font-mono font-semibold">(format!("{:.0}", avg_tokens_per_req))</p>
                                        </div>
                                    </div>
                                </div>
                                <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                    <h3 class="mb-4 text-sm font-semibold">"API 调用详情"</h3>
                                    <div class="grid grid-cols-4 gap-4 text-xs">
                                        <div>
                                            <p class="text-zinc-500">"成功率"</p>
                                            <p class="mt-1 font-mono font-semibold">(format!("{:.1}%", success_rate * 100.0))</p>
                                        </div>
                                        <div>
                                            <p class="text-zinc-500">"平均首字节"</p>
                                            <p class="mt-1 font-mono font-semibold">(format!("{:.0}ms", avg_first_byte))</p>
                                        </div>
                                        <div>
                                            <p class="text-zinc-500">"平均延迟"</p>
                                            <p class="mt-1 font-mono font-semibold">(format!("{:.0}ms", avg_latency))</p>
                                        </div>
                                        <div>
                                            <p class="text-zinc-500">"平均 Tokens"</p>
                                            <p class="mt-1 font-mono font-semibold">(format!("{:.0}", avg_tokens_per_req))</p>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </main>
                </div>
            </body>
        </html>
    }
}
