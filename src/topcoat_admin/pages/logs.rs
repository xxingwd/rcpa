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
            </head>
            <body class="bg-zinc-50 text-zinc-900">
                <div class="flex min-h-screen">
                    <aside class="w-64 border-r border-zinc-200 bg-white p-4">
                        <h2 class="text-lg font-bold">"RCPA"</h2>
                        <nav class="mt-4 space-y-1">
                            <a href="/" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"仪表盘"</a>
                            <a href="/keys" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"密钥管理"</a>
                            <a href="/providers" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"供应商"</a>
                            <a href="/logs" class="block rounded-lg bg-zinc-100 px-3 py-2 text-sm font-medium">"调用日志"</a>
                            <a href="/config" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"配置"</a>
                        </nav>
                    </aside>
                    <main class="flex-1 p-8">
                        <h1 class="text-2xl font-bold">"调用日志"</h1>
                        <p class="mt-1 text-sm text-zinc-500">"查看 API 请求和响应记录"</p>
                        
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
                                        </tr>
                                    </thead>
                                    <tbody class="divide-y divide-zinc-200">
                                        <tr>
                                            <td colspan="5" class="px-4 py-8 text-center text-sm text-zinc-500">
                                                "暂无日志记录"
                                            </td>
                                        </tr>
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
