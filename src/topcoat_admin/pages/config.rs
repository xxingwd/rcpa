use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};

use crate::topcoat_admin::api::fetch_config_yaml;

#[page("/config")]
pub async fn config(cx: &Cx) -> Result {
    let yaml_content = fetch_config_yaml(cx).await;
    
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"RCPA Admin - 配置"</title>
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
                            <a href="/logs" class="block rounded-lg px-3 py-2 text-sm font-medium text-zinc-600 hover:bg-zinc-100">"调用日志"</a>
                            <a href="/config" class="block rounded-lg bg-zinc-100 px-3 py-2 text-sm font-medium">"配置"</a>
                        </nav>
                    </aside>
                    <main class="flex-1 p-8">
                        <h1 class="text-2xl font-bold">"配置管理"</h1>
                        <p class="mt-1 text-sm text-zinc-500">"编辑网关 YAML 配置文件"</p>
                        
                        <div class="mt-6 rounded-xl border border-zinc-200 bg-white p-6">
                            <textarea
                                rows="30"
                                class="h-96 w-full rounded-lg border border-zinc-200 bg-white px-3 py-2 font-mono text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-zinc-400"
                            >(yaml_content)</textarea>
                            <div class="mt-4 flex gap-2">
                                <button class="h-9 rounded-lg bg-zinc-100 px-4 text-sm font-medium text-zinc-900 hover:bg-zinc-200">"验证"</button>
                                <button class="h-9 rounded-lg bg-zinc-900 px-4 text-sm font-medium text-white hover:bg-zinc-800">"保存"</button>
                            </div>
                        </div>
                    </main>
                </div>
            </body>
        </html>
    }
}
