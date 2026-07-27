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
    let key_count = keys.len();
    
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
                    <sidebar />
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
                            
                            <div class="rounded-xl border border-zinc-200 bg-white">
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
                                            if key_count == 0 {
                                                <tr>
                                                    <td colspan="8" class="px-4 py-8 text-center text-sm text-zinc-500">
                                                        "暂无密钥"
                                                    </td>
                                                </tr>
                                            }
                                        </tbody>
                                    </table>
                                </div>
                            </div>
                        </div>
                    </main>
                </div>
            </body>
        </html>
    }
}
