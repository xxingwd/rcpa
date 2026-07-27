use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::View,
    view::view,
};

use crate::topcoat_admin::api::fetch_providers;

#[page("/providers")]
pub async fn providers(_cx: &Cx) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"RCPA Admin - 供应商"</title>
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
                            <a href="/" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"仪表盘"</a>
                            <a href="/keys" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"密钥管理"</a>
                            <a href="/providers" class="block rounded-lg bg-zinc-100 px-3 py-2 text-sm font-medium">"供应商"</a>
                            <a href="/logs" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"调用日志"</a>
                            <a href="/config" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"配置"</a>
                        </nav>
                    </aside>
                    <main class="flex-1 p-8">
                        <div class="mx-auto max-w-7xl">
                            <div class="mb-8 flex items-center justify-between">
                                <div>
                                    <h1 class="text-2xl font-bold">"供应商管理"</h1>
                                    <p class="mt-1 text-sm text-zinc-500">"管理上游 LLM 供应商和端点配置"</p>
                                </div>
                                <button class="inline-flex h-9 items-center gap-2 rounded-lg bg-zinc-900 px-4 text-sm font-medium text-white hover:bg-zinc-800">
                                    "添加供应商"
                                </button>
                            </div>
                            
                            <div
                                class="rounded-xl border border-zinc-200 bg-white"
                                hx-get="/providers/table"
                                hx-trigger="load"
                            >
                                <div class="py-12 text-center">
                                    <p class="text-sm text-zinc-500">"加载中..."</p>
                                </div>
                            </div>
                        </div>
                    </main>
                </div>
            </body>
        </html>
    }
}

#[page("/providers/table")]
pub async fn providers_table(cx: &Cx) -> Result {
    let all_providers = fetch_providers(cx).await;
    let provider_count = all_providers.len();
    
    if provider_count == 0 {
        return view! {
            <div class="py-12 text-center">
                <p class="text-sm text-zinc-500">"暂无供应商"</p>
            </div>
        };
    }
    
    let mut rows = String::new();
    for provider in &all_providers {
        let name = &provider.name;
        let status_class = if provider.status == "active" {
            "bg-emerald-100 text-emerald-700"
        } else {
            "bg-red-100 text-red-700"
        };
        let status_text = if provider.status == "active" { "启用" } else { "禁用" };
        let model_count = provider.models.len();
        let endpoint_count = provider.endpoints.len();
        let priority = provider.priority;
        
        rows.push_str(&format!(
            r#"<tr class="hover:bg-zinc-50">
                <td class="px-4 py-3 font-medium">{}</td>
                <td class="px-4 py-3">
                    <span class="inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium {}">{}</span>
                </td>
                <td class="px-4 py-3 text-zinc-500">{}</td>
                <td class="px-4 py-3 text-zinc-500">{} 个模型</td>
                <td class="px-4 py-3 text-zinc-500">{} 个端点</td>
                <td class="px-4 py-3">
                    <div class="flex gap-1.5">
                        <button class="inline-flex h-7 items-center rounded border border-zinc-200 px-2 text-xs text-zinc-600 hover:bg-zinc-50">编辑</button>
                        <button class="inline-flex h-7 items-center rounded border border-zinc-200 px-2 text-xs text-zinc-600 hover:bg-zinc-50">{}</button>
                        <button class="inline-flex h-7 items-center rounded border border-red-200 px-2 text-xs text-red-600 hover:bg-red-50">删除</button>
                    </div>
                </td>
            </tr>"#,
            name, status_class, status_text, priority, model_count, endpoint_count,
            if provider.status == "active" { "禁用" } else { "启用" }
        ));
    }
    
    let rows_leaked: &'static str = Box::leak(rows.into_boxed_str());
    
    view! {
        <div class="overflow-x-auto">
            <table class="w-full text-sm">
                <thead class="border-b border-zinc-200 bg-zinc-50">
                    <tr>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"名称"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"状态"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"优先级"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"模型数"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"端点数"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"操作"</th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-zinc-200">
                    (View::unescaped_unchecked(rows_leaked))
                </tbody>
            </table>
        </div>
    }
}
