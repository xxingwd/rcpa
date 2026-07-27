//! Admin UI pages.

pub mod config;
pub mod dashboard;
pub mod keys;
pub mod logs;
pub mod providers;

use topcoat::{
    Result,
    view::{component, view},
};

/// The sidebar navigation component.
#[component]
pub async fn sidebar() -> Result {
    view! {
        <aside class="flex h-screen w-64 flex-col border-r border-zinc-200 bg-white">
            <div class="border-b border-zinc-200 px-6 py-4">
                <h2 class="text-lg font-bold text-zinc-900">"RCPA"</h2>
                <p class="text-xs text-zinc-500">"LLM 网关管理"</p>
            </div>
            <nav class="flex-1 space-y-1 px-3 py-4">
                <a href="/" class="flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">
                    "仪表盘"
                </a>
                <a href="/keys" class="flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">
                    "密钥管理"
                </a>
                <a href="/providers" class="flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">
                    "供应商"
                </a>
                <a href="/logs" class="flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">
                    "调用日志"
                </a>
                <a href="/config" class="flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-100">
                    "配置"
                </a>
            </nav>
        </aside>
    }
}
