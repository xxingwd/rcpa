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
    
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"RCPA Admin"</title>
                <script src="https://cdn.tailwindcss.com"></script>
            </head>
            <body class="bg-zinc-50 text-zinc-900">
                <div class="flex min-h-screen">
                    <aside class="w-64 border-r border-zinc-200 bg-white p-4">
                        <h2 class="text-lg font-bold">"RCPA"</h2>
                        <nav class="mt-4 space-y-1">
                            <a href="/" class="block rounded-lg bg-zinc-100 px-3 py-2 text-sm font-medium">"仪表盘"</a>
                            <a href="/keys" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"密钥管理"</a>
                            <a href="/providers" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"供应商"</a>
                            <a href="/logs" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"调用日志"</a>
                            <a href="/config" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"配置"</a>
                        </nav>
                    </aside>
                    <main class="flex-1 p-8">
                        <h1 class="text-2xl font-bold">"仪表盘"</h1>
                        <p class="mt-1 text-sm text-zinc-500">"RCPA 网关运行状态总览"</p>
                        
                        <div class="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
                            <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                <p class="text-sm font-medium text-zinc-500">"总请求数"</p>
                                <p class="mt-2 text-3xl font-bold">(format!("{}", stats.requests.total))</p>
                            </div>
                            <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                <p class="text-sm font-medium text-zinc-500">"Token 消耗"</p>
                                <p class="mt-2 text-3xl font-bold">(format!("{}", stats.tokens.total))</p>
                            </div>
                            <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                <p class="text-sm font-medium text-zinc-500">"平均延迟"</p>
                                <p class="mt-2 text-3xl font-bold">(format!("{:.0}ms", stats.latency.avg_ms))</p>
                            </div>
                            <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                <p class="text-sm font-medium text-zinc-500">"总成本"</p>
                                <p class="mt-2 text-3xl font-bold">(format!("{:.2}€", stats.cost.total_cents as f64 / 100.0))</p>
                            </div>
                        </div>
                    </main>
                </div>
            </body>
        </html>
    }
}
