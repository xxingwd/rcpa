use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};


#[page("/logs")]
pub async fn logs(_cx: &Cx) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"RCPA Admin - 调用日志"</title>
                <script src="https://cdn.tailwindcss.com"></script>
                <script src="https://unpkg.com/htmx.org@2.0.4"></script>
            </head>
            <body class="bg-zinc-50 text-zinc-900">
                <div class="flex min-h-screen">
                    <sidebar />
                    <main class="flex-1 p-8">
                        <div class="mx-auto max-w-7xl">
                            <div class="mb-8">
                                <h1 class="text-2xl font-bold">"调用日志"</h1>
                                <p class="mt-1 text-sm text-zinc-500">"查看 API 请求和响应记录"</p>
                            </div>
                            
                            <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                <h3 class="mb-4 text-sm font-semibold">"过滤器"</h3>
                                <div class="grid grid-cols-1 gap-4 md:grid-cols-4">
                                    <div>
                                        <label class="mb-1.5 block text-xs font-medium text-zinc-700">"时间范围"</label>
                                        <input type="text" placeholder="最近 24 小时" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
                                    </div>
                                    <div>
                                        <label class="mb-1.5 block text-xs font-medium text-zinc-700">"密钥"</label>
                                        <input type="text" placeholder="全部密钥" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
                                    </div>
                                    <div>
                                        <label class="mb-1.5 block text-xs font-medium text-zinc-700">"模型"</label>
                                        <input type="text" placeholder="全部模型" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
                                    </div>
                                    <div>
                                        <label class="mb-1.5 block text-xs font-medium text-zinc-700">"状态"</label>
                                        <input type="text" placeholder="全部" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
                                    </div>
                                </div>
                            </div>
                            
                            <div class="mt-6 rounded-xl border border-zinc-200 bg-white">
                                <div class="overflow-x-auto">
                                    <table class="w-full text-sm">
                                        <thead class="border-b border-zinc-200 bg-zinc-50">
                                            <tr>
                                                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"时间"</th>
                                                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"密钥"</th>
                                                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"模型"</th>
                                                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"状态"</th>
                                                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"延迟"</th>
                                                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"Token"</th>
                                                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">"操作"</th>
                                            </tr>
                                        </thead>
                                        <tbody class="divide-y divide-zinc-200">
                                            <tr>
                                                <td colspan="7" class="px-4 py-8 text-center text-sm text-zinc-500">
                                                    "暂无日志记录"
                                                </td>
                                            </tr>
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
