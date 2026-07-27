use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::View,
    view::view,
};

use crate::topcoat_admin::api::fetch_keys;

#[page("/keys")]
pub async fn keys(_cx: &Cx) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"RCPA Admin - 密钥管理"</title>
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
                            <a href="/keys" class="block rounded-lg bg-zinc-100 px-3 py-2 text-sm font-medium">"密钥管理"</a>
                            <a href="/providers" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"供应商"</a>
                            <a href="/logs" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"调用日志"</a>
                            <a href="/config" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">"配置"</a>
                        </nav>
                    </aside>
                    <main class="flex-1 p-8">
                        <div class="mx-auto max-w-7xl">
                            <div class="mb-8 flex items-center justify-between">
                                <div>
                                    <h1 class="text-2xl font-bold">"密钥管理"</h1>
                                    <p class="mt-1 text-sm text-zinc-500">"管理客户端 API 密钥和访问权限"</p>
                                </div>
                                <button class="inline-flex h-9 items-center gap-2 rounded-lg bg-zinc-900 px-4 text-sm font-medium text-white hover:bg-zinc-800">
                                    "生成新密钥"
                                </button>
                            </div>
                            
                            <div
                                class="rounded-xl border border-zinc-200 bg-white"
                                hx-get="/keys/table"
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

#[page("/keys/table")]
pub async fn keys_table(cx: &Cx) -> Result {
    let all_keys = fetch_keys(cx).await;
    let key_count = all_keys.len();
    
    if key_count == 0 {
        return view! {
            <div class="py-12 text-center">
                <p class="text-sm text-zinc-500">"暂无密钥"</p>
            </div>
        };
    }
    
    // Build table rows as HTML string
    let mut rows = String::new();
    for key in &all_keys {
        let name = key.name.as_deref().unwrap_or("未命名");
        let key_val = &key.key;
        let status_class = if key.status == "active" {
            "bg-emerald-100 text-emerald-700"
        } else {
            "bg-red-100 text-red-700"
        };
        let status_text = if key.status == "active" { "启用" } else { "禁用" };
        let model_count = key.allowed_models.len();
        let provider_count = key.allowed_providers.len();
        let alias_count = key.model_aliases.len();
        let labels = key.labels.as_deref().unwrap_or("—");
        
        rows.push_str(&format!(
            r#"<tr class="hover:bg-zinc-50">
                <td class="px-4 py-3 font-medium">{}</td>
                <td class="px-4 py-3">
                    <code class="select-all rounded bg-zinc-100 px-1.5 py-0.5 text-xs">{}</code>
                </td>
                <td class="px-4 py-3">
                    <span class="inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium {}">{}</span>
                </td>
                <td class="px-4 py-3 text-xs text-zinc-500">{} 个模型</td>
                <td class="px-4 py-3 text-xs text-zinc-500">{}</td>
                <td class="px-4 py-3 text-xs text-zinc-500">{}</td>
                <td class="px-4 py-3 text-xs text-zinc-500">{}</td>
                <td class="px-4 py-3">
                    <div class="flex gap-1.5">
                        <button class="inline-flex h-7 items-center rounded border border-zinc-200 px-2 text-xs text-zinc-600 hover:bg-zinc-50">编辑</button>
                        <button class="inline-flex h-7 items-center rounded border border-zinc-200 px-2 text-xs text-zinc-600 hover:bg-zinc-50">{}</button>
                    </div>
                </td>
            </tr>"#,
            name, key_val, status_class, status_text,
            model_count,
            if provider_count == 0 { "全部".to_string() } else { format!("{} 个供应商", provider_count) },
            if alias_count == 0 { "—".to_string() } else { format!("{} 个别名", alias_count) },
            labels,
            if key.status == "active" { "禁用" } else { "启用" }
        ));
    }
    
    // Use View::unescaped_unchecked with a leaked static string
    let rows_leaked: &'static str = Box::leak(rows.into_boxed_str());
    
    view! {
        <div class="overflow-x-auto">
            <table class="w-full text-sm">
                <thead class="border-b border-zinc-200 bg-zinc-50">
                    <tr>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"名称"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"API 密钥"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"状态"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"允许模型"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"允许供应商"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"模型别名"</th>
                        <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"备注"</th>
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
