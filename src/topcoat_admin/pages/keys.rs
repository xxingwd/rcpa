use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};

use crate::topcoat_admin::api::fetch_keys;

#[page("/keys")]
pub async fn keys(cx: &Cx) -> Result {
    let keys = fetch_keys(cx).await;
    
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"RCPA Admin - 密钥管理"</title>
                <script src="https://cdn.tailwindcss.com"></script>
            </head>
            <body class="bg-zinc-50 text-zinc-900">
                <div class="flex min-h-screen">
                    <aside class="w-64 border-r border-zinc-200 bg-white p-4">
                        <h2 class="text-lg font-bold">"RCPA"</h2>
                        <nav class="mt-4 space-y-1">
                            <a href="/" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"仪表盘"</a>
                            <a href="/keys" class="block rounded-lg bg-zinc-100 px-3 py-2 text-sm font-medium">"密钥管理"</a>
                            <a href="/providers" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"供应商"</a>
                            <a href="/logs" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"调用日志"</a>
                            <a href="/config" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"配置"</a>
                        </nav>
                    </aside>
                    <main class="flex-1 p-8">
                        <h1 class="text-2xl font-bold">"密钥管理"</h1>
                        <p class="mt-1 text-sm text-zinc-500">"管理客户端 API 密钥和访问权限"</p>
                        
                        <div class="mt-6 rounded-xl border border-zinc-200 bg-white">
                            <div class="overflow-x-auto">
                                <table class="w-full text-sm">
                                    <thead class="border-b border-zinc-200 bg-zinc-50">
                                        <tr>
                                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"名称"</th>
                                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"密钥"</th>
                                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"状态"</th>
                                            <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"操作"</th>
                                        </tr>
                                    </thead>
                                    <tbody class="divide-y divide-zinc-200">
                                        for key in keys {
                                            <tr class="hover:bg-zinc-50">
                                                <td class="px-4 py-3 font-medium">(key.name.as_deref().unwrap_or("未命名"))</td>
                                                <td class="px-4 py-3">
                                                    <code class="rounded bg-zinc-100 px-1.5 py-0.5 text-xs">(if key.key.len() > 12 { format!("{}...", &key.key[..12]) } else { key.key.clone() })</code>
                                                </td>
                                                <td class="px-4 py-3">
                                                    <span class="inline-flex rounded-full bg-emerald-100 px-2.5 py-0.5 text-xs font-medium text-emerald-700">
                                                        (key.status)
                                                    </span>
                                                </td>
                                                <td class="px-4 py-3">
                                                    <button class="text-zinc-600 hover:text-zinc-900">"编辑"</button>
                                                </td>
                                            </tr>
                                        }
                                    </tbody>
                                </table>
                            </div>
                        </div>
                    </main>
                </div>
            </body>
        </html>
    }
}
