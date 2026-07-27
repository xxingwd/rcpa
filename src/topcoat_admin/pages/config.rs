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
                <script src="https://unpkg.com/htmx.org@2.0.4"></script>
            </head>
            <body class="bg-zinc-50 text-zinc-900">
                <div class="flex min-h-screen">
                    <sidebar />
                    <main class="flex-1 p-8">
                        <div class="mx-auto max-w-7xl">
                            <div class="mb-8 flex items-center justify-between">
                                <div>
                                    <h1 class="text-2xl font-bold">"配置管理"</h1>
                                    <p class="mt-1 text-sm text-zinc-500">"编辑网关 YAML 配置文件"</p>
                                </div>
                                <div class="flex gap-2">
                                    <button class="inline-flex h-9 items-center gap-2 rounded-lg border border-zinc-200 bg-white px-4 text-sm font-medium hover:bg-zinc-50">
                                        "验证"
                                    </button>
                                    <button class="inline-flex h-9 items-center gap-2 rounded-lg bg-zinc-900 px-4 text-sm font-medium text-white hover:bg-zinc-800">
                                        "保存"
                                    </button>
                                </div>
                            </div>
                            
                            <div class="rounded-xl border border-zinc-200 bg-white p-6">
                                <textarea
                                    name="content"
                                    rows="30"
                                    class="h-[600px] w-full rounded-lg border border-zinc-200 bg-white px-3 py-2 font-mono text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-zinc-400"
                                >(yaml_content)</textarea>
                                <div class="mt-4 text-xs text-zinc-500">
                                    "提示：修改配置后点击保存，配置将立即生效。"
                                </div>
                            </div>
                        </div>
                    </main>
                </div>
            </body>
        </html>
    }
}
