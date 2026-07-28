use std::collections::HashMap;

use topcoat::{
    context::Cx,
    router::{page, path_param},
    view::{view, View},
    Result,
};

use crate::config::{EndpointConfig, ModelRule};
use crate::config_service::ProviderView;
use crate::topcoat_admin::api::fetch_providers;
use crate::topcoat_admin::app::require_admin;
use crate::topcoat_admin::{
    escape_html, escape_inline_js_string, render_dialog, render_list, render_modal_backdrop,
    render_page, render_shared_scripts, render_shared_styles, render_sidebar,
    render_theme_bootstrap, render_toast_container, trusted_html, DialogLayout, ListLayout,
    PageLayout,
};

#[path_param]
struct ProviderName(str);

#[page("/providers")]
pub async fn providers(cx: &Cx) -> Result {
    require_admin(cx)?;
    let sidebar = render_sidebar(cx, "/providers").await?;
    let toast_container = render_toast_container();
    let modal_backdrop = render_modal_backdrop();
    let shared_styles = render_shared_styles();
    let shared_scripts = render_shared_scripts();
    let theme_bootstrap = render_theme_bootstrap();
    let list_body: Result = view! { <div class="list-loading">"加载中..."</div> };
    let list_view = render_list(
        cx,
        ListLayout {
            id: "providers-list",
            label: "供应商与模型列表",
            endpoint: Some("/providers/table"),
            refresh_event: Some("rcpa-providers-refresh"),
            body: list_body?,
        },
    )
    .await?;
    let page_actions: Result = view! {
        <button class="primary-button" type="button" onclick="openCreateModal()">
            <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M12 5v14M5 12h14"/></svg>
            "注册供应商"
        </button>
    };
    let page_html = render_page(
        cx,
        PageLayout {
            title: "供应商与模型",
            description: Some("管理上游供应商、端点和模型映射"),
            class_name: "providers-page",
            actions: Some(page_actions?),
            body: list_view,
        },
    )
    .await?;

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RCPA Admin - 供应商</title>
    {theme_bootstrap}
    <link rel="stylesheet" href="/_topcoat/tailwind.css">
    <script src="/_topcoat/htmx.min.js"></script>
    {}
    <style>
        .model-badge {{ display: inline-flex; align-items: center; height: 1.5rem; padding: 0 0.375rem; border-radius: 9999px; border: 1px solid; font-family: ui-monospace, monospace; font-size: 0.75rem; transition: all 0.15s; }}
        .model-badge.enabled {{ background: rgba(16, 185, 129, 0.06); border-color: rgba(16, 185, 129, 0.4); color: #059669; }}
        .model-badge.disabled {{ background: var(--muted); border-color: var(--border); color: var(--muted-foreground); }}
        .model-badge-dot {{ width: 0.375rem; height: 0.375rem; border-radius: 50%; margin-right: 0.25rem; }}
        .model-badge.enabled .model-badge-dot {{ background: #10b981; }}
        .model-badge.disabled .model-badge-dot {{ background: rgba(113, 113, 122, 0.5); }}
        .modal-content:has(.provider-dialog) {{ width: min(63.75rem, calc(100vw - 2rem)); }}
    </style>
</head>
<body class="bg-zinc-50 text-zinc-900">
    {}
    <div class="flex min-h-screen">
        {}
        <main class="admin-main">
            <div class="admin-content">{}</div>
        </main>
    </div>
    {}
    <script>
        function openCreateModal() {{
            Modal.load('/providers/new');
        }}

        function openEditModal(name) {{
            Modal.load('/providers/' + encodeURIComponent(name) + '/edit');
        }}

        function openCopyModal(name) {{
            Modal.load('/providers/' + encodeURIComponent(name) + '/copy');
        }}

        function submitProviderForm(event, method, url) {{
            event.preventDefault();
            const form = event.target;
            const formData = new FormData(form);
            const data = {{}};

            const endpoints = [];
            const models = [];
            const headers = [];

            const endpointIndices = new Set();
            const modelIndices = new Set();
            const headerIndices = new Set();

            for (let [key, value] of formData.entries()) {{
                if (key.startsWith('endpoints[')) {{
                    const match = key.match(/endpoints\[(\d+)\]\[(\w+)\]/);
                    if (match) {{
                        endpointIndices.add(parseInt(match[1]));
                        if (!endpoints[match[1]]) endpoints[match[1]] = {{}};
                        endpoints[match[1]][match[2]] = value;
                    }}
                }} else if (key.startsWith('models[')) {{
                    const match = key.match(/models\[(\d+)\]\[(\w+)\]/);
                    if (match) {{
                        modelIndices.add(parseInt(match[1]));
                        if (!models[match[1]]) models[match[1]] = {{}};
                        models[match[1]][match[2]] = value;
                    }}
                }} else if (key.startsWith('headers[')) {{
                    const match = key.match(/headers\[(\d+)\]\[(\w+)\]/);
                    if (match) {{
                        headerIndices.add(parseInt(match[1]));
                        if (!headers[match[1]]) headers[match[1]] = {{}};
                        headers[match[1]][match[2]] = value;
                    }}
                }} else {{
                    data[key] = value;
                }}
            }}

            data.endpoints = endpoints.filter(Boolean);
            const priority = Number.parseInt(data.priority, 10);
            data.priority = Number.isFinite(priority) ? priority : 0;
            data.models = models.filter(Boolean).map(m => {{
                const hasPricing = m.input_price !== '' || m.output_price !== '';
                return {{
                    name: m.name,
                    status: m.status,
                    aliases: m.aliases ? m.aliases.split(',').map(s => s.trim()).filter(Boolean) : [],
                    pricing: hasPricing ? {{
                        input_per_1k: Number.parseFloat(m.input_price || '0'),
                        output_per_1k: Number.parseFloat(m.output_price || '0')
                    }} : null
                }};
            }});
            data.headers = Object.fromEntries(
                headers.filter(Boolean).map(h => [h.name, h.value])
            );

            setFormBusy(form, true);
            fetch(url, {{
                method: method,
                headers: {{ 'Content-Type': 'application/json' }},
                credentials: 'include',
                body: JSON.stringify(data)
            }})
            .then(response => {{
                if (response.status === 401) {{
                    window.location.href = '/login';
                    return;
                }}
                return response.json();
            }})
            .then(result => {{
                if (!result) return;
                if (result.error) {{
                    Toast.error(result.error.message || '保存失败');
                }} else {{
                    Toast.success(method === 'POST' ? '供应商注册成功' : '供应商更新成功');
                    Modal.close();
                    refreshData('providers');
                }}
            }})
            .catch(err => Toast.error('保存失败: ' + err.message))
            .finally(() => setFormBusy(form, false));
        }}

        function deleteProvider(name) {{
            if (!confirm('确定删除供应商 ' + name + '？')) return;
            fetch('/v1/admin/providers/' + encodeURIComponent(name), {{ method: 'DELETE', credentials: 'include' }})
                .then(response => {{
                    if (response.status === 401) {{
                        window.location.href = '/login';
                        return;
                    }}
                    return response.json();
                }})
                .then(result => {{
                    if (!result) return;
                    if (result.error) {{
                        Toast.error(result.error.message || '删除失败');
                    }} else {{
                        Toast.success('供应商 ' + name + ' 已删除');
                        refreshData('providers');
                    }}
                }});
        }}

        function toggleProviderStatus(name, currentStatus) {{
            const nextStatus = currentStatus === 'enabled' ? 'disabled' : 'enabled';
            fetch('/v1/admin/providers/' + encodeURIComponent(name) + '/status', {{
                method: 'PUT',
                headers: {{ 'Content-Type': 'application/json' }},
                credentials: 'include',
                body: JSON.stringify({{ status: nextStatus }})
            }})
            .then(response => {{
                if (response.status === 401) {{
                    window.location.href = '/login';
                    return;
                }}
                return response.json();
            }})
            .then(result => {{
                if (!result) return;
                if (result.error) {{
                    Toast.error('更新状态失败');
                }} else {{
                    Toast.success('供应商 ' + name + ' 已' + (nextStatus === 'enabled' ? '启用' : '禁用'));
                    refreshData('providers');
                }}
            }});
        }}

        function toggleModelStatus(providerName, modelName, currentStatus) {{
            const nextStatus = currentStatus === 'enabled' ? 'disabled' : 'enabled';
            fetch('/v1/admin/providers/' + encodeURIComponent(providerName) + '/models/' + encodeURIComponent(modelName) + '/status', {{
                method: 'PUT',
                headers: {{ 'Content-Type': 'application/json' }},
                credentials: 'include',
                body: JSON.stringify({{ status: nextStatus }})
            }})
            .then(response => {{
                if (response.status === 401) {{
                    window.location.href = '/login';
                    return;
                }}
                return response.json();
            }})
            .then(result => {{
                if (!result) return;
                if (!result.error) {{
                    refreshData('providers');
                }}
            }});
        }}
    </script>
    {}
</body>
</html>"##,
        shared_styles, toast_container, sidebar, page_html, modal_backdrop, shared_scripts
    );

    Ok(trusted_html(html))
}

#[page("/providers/table")]
pub async fn providers_table(cx: &Cx) -> Result {
    require_admin(cx)?;
    let all_providers = fetch_providers(cx).await;
    let provider_count = all_providers.len();

    if provider_count == 0 {
        return Ok(View::unescaped_unchecked(
            r##"<div class="py-12 text-center text-sm text-zinc-500">暂无配置的供应商</div>"##,
        ));
    }

    let mut html = String::new();
    html.push_str(r##"<div class="overflow-x-auto"><table class="w-full min-w-[1040px] text-sm">
        <thead class="border-b border-zinc-200 bg-zinc-50">
            <tr>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">供应商名称</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">Base URL</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">支持协议</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">优先级</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">状态</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">模型目录</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">操作</th>
            </tr>
        </thead>
        <tbody class="divide-y divide-zinc-200"> "##);

    for provider in &all_providers {
        let name = &provider.name;
        let name_html = escape_html(name);
        let name_js = escape_inline_js_string(name);
        let status_class = if provider.status == "enabled" {
            "bg-emerald-100 text-emerald-700"
        } else {
            "bg-red-100 text-red-700"
        };
        let status_text = if provider.status == "enabled" {
            "启用"
        } else {
            "禁用"
        };
        let priority = provider.priority;
        let toggle_text = if provider.status == "enabled" {
            "禁用"
        } else {
            "启用"
        };
        let _endpoints_json = serde_json::to_string(&provider.endpoints).unwrap_or_default();
        let _models_json = serde_json::to_string(&provider.models).unwrap_or_default();
        let _headers_json = serde_json::to_string(&provider.headers).unwrap_or_default();

        html.push_str(&format!(
            r##"<tr class="hover:bg-zinc-50">
                <td class="max-w-[150px] px-4 py-3 font-mono text-xs font-medium" title="{}">
                    <span class="block truncate">{}</span>
                </td>
                <td class="max-w-[260px] px-4 py-3">
                    <div class="space-y-1">
                        {}                    </div>
                </td>
                <td class="px-4 py-3">
                    <div class="flex flex-wrap gap-1">
                        {}
                    </div>
                </td>
                <td class="px-4 py-3 font-mono text-xs text-zinc-500">{}</td>
                <td class="px-4 py-3"><span class="inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium {}">{}</span></td>
                <td class="max-w-[260px] px-4 py-3">
                    <div class="flex flex-wrap gap-1.5">
                        {}
                    </div>
                </td>
                <td class="px-4 py-3">
                    <div class="flex gap-1.5">
                        <button class="inline-flex h-7 w-7 items-center justify-center rounded border border-zinc-200 text-zinc-600 hover:bg-zinc-50"
                            onclick="openEditModal({})" title="编辑">
                            <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"/></svg>
                        </button>
                        <button class="inline-flex h-7 w-7 items-center justify-center rounded border border-zinc-200 text-zinc-600 hover:bg-zinc-50"
                            onclick="openCopyModal({})" title="复制">
                            <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"/></svg>
                        </button>
                        <button class="inline-flex h-7 items-center rounded border border-zinc-200 px-2 text-xs text-zinc-600 hover:bg-zinc-50"
                            onclick="toggleProviderStatus({}, {})">{}</button>
                        <button class="inline-flex h-7 items-center rounded border border-red-200 px-2 text-xs text-red-600 hover:bg-red-50"
                            onclick="deleteProvider({})">删除</button>
                    </div>
                </td>
            </tr>"##,
            name_html, name_html,
            render_endpoints(&provider.endpoints),
            render_protocol_badges(&provider.endpoints),
            priority,
            status_class, status_text,
            render_model_badges(name, &provider.models),
            name_js, name_js, name_js, escape_inline_js_string(&provider.status), toggle_text, name_js
        ));
    }

    html.push_str(r##"</tbody></table></div>"##);

    Ok(trusted_html(html))
}

fn render_endpoints(endpoints: &[crate::config::EndpointConfig]) -> String {
    endpoints
        .iter()
        .map(|e| {
            format!(
                r#"<div class="font-mono text-xs break-all" title="{}">{}</div>"#,
                escape_html(&e.base_url),
                escape_html(&e.base_url)
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_protocol_badges(endpoints: &[crate::config::EndpointConfig]) -> String {
    endpoints.iter()
        .map(|e| format!(r#"<span class="inline-flex rounded-full bg-zinc-100 px-2 py-0.5 font-mono text-[10px] text-zinc-600">{}</span>"#, escape_html(&e.protocol.to_string())))
        .collect::<Vec<_>>()
        .join("")
}

fn render_model_badges(provider_name: &str, models: &[crate::config::ModelRule]) -> String {
    models.iter()
        .map(|m| {
            let status = if m.status == "enabled" { "enabled" } else { "disabled" };
            format!(
                r#"<button type="button" onclick="toggleModelStatus({}, {}, {})" class="model-badge {}" title="{}">
                    <span class="model-badge-dot"></span>{}
                </button>"#,
                escape_inline_js_string(provider_name),
                escape_inline_js_string(&m.name),
                escape_inline_js_string(status),
                status,
                escape_html(&m.name),
                escape_html(&m.name)
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

#[page("/providers/new")]
pub async fn providers_new(cx: &Cx) -> Result {
    require_admin(cx)?;
    let empty_headers = HashMap::new();
    let html = render_provider_form(ProviderForm {
        name: "",
        api_key: "",
        status: "enabled",
        priority: 0,
        endpoints: &[],
        models: &[],
        headers: &empty_headers,
        method: "POST",
        url: "/v1/admin/providers",
        name_readonly: false,
    });
    let html = render_dialog(
        cx,
        DialogLayout {
            title: "注册新供应商",
            description: Some("配置协议端点、鉴权请求头和模型映射"),
            class_name: "provider-dialog",
            body: trusted_html(html),
        },
    )
    .await?;
    Ok(html)
}

#[page("/providers/{provider_name}/edit")]
pub async fn providers_edit(cx: &Cx) -> Result {
    require_admin(cx)?;
    let name = path_param::<ProviderName>(cx);
    let all_providers = fetch_providers(cx).await;
    let Some(provider) = all_providers.iter().find(|provider| provider.name == name) else {
        return render_missing_provider(cx, "编辑供应商", name).await;
    };
    let update_url = format!("/v1/admin/providers/{}", name);
    let html = render_provider_form(ProviderForm::from_provider(
        provider,
        &provider.name,
        "PUT",
        &update_url,
        true,
    ));
    let html = render_dialog(
        cx,
        DialogLayout {
            title: "编辑供应商",
            description: Some("更新端点、请求头、模型费率和公开别名"),
            class_name: "provider-dialog",
            body: trusted_html(html),
        },
    )
    .await?;
    Ok(html)
}

#[page("/providers/{provider_name}/copy")]
pub async fn providers_copy(cx: &Cx) -> Result {
    require_admin(cx)?;
    let name = path_param::<ProviderName>(cx);
    let all_providers = fetch_providers(cx).await;
    let Some(provider) = all_providers.iter().find(|provider| provider.name == name) else {
        return render_missing_provider(cx, "复制供应商", name).await;
    };
    let html = render_provider_form(ProviderForm::from_provider(
        provider,
        "",
        "POST",
        "/v1/admin/providers",
        false,
    ));
    let html = render_dialog(
        cx,
        DialogLayout {
            title: "复制供应商",
            description: Some("基于现有配置创建一个新的供应商"),
            class_name: "provider-dialog",
            body: trusted_html(html),
        },
    )
    .await?;
    Ok(html)
}

struct ProviderForm<'a> {
    name: &'a str,
    api_key: &'a str,
    status: &'a str,
    priority: i64,
    endpoints: &'a [EndpointConfig],
    models: &'a [ModelRule],
    headers: &'a HashMap<String, String>,
    method: &'a str,
    url: &'a str,
    name_readonly: bool,
}

impl<'a> ProviderForm<'a> {
    fn from_provider(
        provider: &'a ProviderView,
        name: &'a str,
        method: &'a str,
        url: &'a str,
        name_readonly: bool,
    ) -> Self {
        Self {
            name,
            api_key: &provider.api_key,
            status: &provider.status,
            priority: provider.priority,
            endpoints: &provider.endpoints,
            models: &provider.models,
            headers: &provider.headers,
            method,
            url,
            name_readonly,
        }
    }
}

async fn render_missing_provider(cx: &Cx, title: &'static str, name: &str) -> Result {
    let body_html = format!(
        r#"<div role="alert" class="rounded-md border border-red-200 bg-red-50 p-4 text-sm text-red-700">供应商 <span class="font-mono">{}</span> 不存在或已被删除，请关闭后刷新列表。</div>"#,
        escape_html(name)
    );
    let html = render_dialog(
        cx,
        DialogLayout {
            title,
            description: Some("无法读取供应商配置"),
            class_name: "provider-dialog",
            body: trusted_html(body_html),
        },
    )
    .await?;
    Ok(html)
}

fn render_provider_form(form: ProviderForm<'_>) -> String {
    let ProviderForm {
        name,
        api_key,
        status,
        priority,
        endpoints,
        models,
        headers,
        method,
        url,
        name_readonly,
    } = form;

    let endpoints_html = if endpoints.is_empty() {
        r#"<div class="rounded-lg border border-dashed border-zinc-300 py-5 text-center text-xs text-zinc-500">点击"添加 Endpoint"开始配置</div>"#.to_string()
    } else {
        endpoints.iter().enumerate().map(|(i, ep)| {
            let protocol = ep.protocol.to_string();
            format!(r#"<div class="grid grid-cols-12 gap-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
                <div class="col-span-4">
                    <label class="mb-1 block text-[10px] text-zinc-500">协议</label>
                    <select name="endpoints[{}][protocol]" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-2 text-sm">
                        <option value="completions" {}>completions</option>
                        <option value="responses" {}>responses</option>
                        <option value="messages" {}>messages</option>
                    </select>
                </div>
                <div class="col-span-7">
                    <label class="mb-1 block text-[10px] text-zinc-500">Base URL</label>
                    <input type="text" name="endpoints[{}][base_url]" value="{}" placeholder="https://api.openai.com" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
                </div>
                <div class="col-span-1 flex items-end">
                    <button type="button" onclick="this.closest('.grid').remove()" class="inline-flex h-9 w-9 items-center justify-center rounded border border-red-200 text-red-600 hover:bg-red-50">
                        <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                    </button>
                </div>
            </div>"#,
                i, if protocol == "completions" { "selected" } else { "" },
                if protocol == "responses" { "selected" } else { "" },
                if protocol == "messages" { "selected" } else { "" },
                i, escape_html(&ep.base_url)
            )
        }).collect::<Vec<_>>().join("")
    };

    let models_html = if models.is_empty() {
        r#"<div class="rounded-lg border border-dashed border-zinc-300 py-5 text-center text-xs text-zinc-500">点击"添加模型"开始配置</div>"#.to_string()
    } else {
        models.iter().enumerate().map(|(i, m)| {
            let input_price = m.pricing.as_ref().map(|pricing| pricing.input_per_1k.to_string()).unwrap_or_default();
            let output_price = m.pricing.as_ref().map(|pricing| pricing.output_per_1k.to_string()).unwrap_or_default();
            let aliases = m.aliases.join(", ");
            format!(r#"<div class="flex items-end gap-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
                <div class="grid flex-1 grid-cols-12 gap-2">
                    <div class="col-span-3">
                        <label class="mb-1 block text-[10px] text-zinc-500">模型名称 *</label>
                        <input type="text" name="models[{}][name]" value="{}" placeholder="gpt-4o-mini" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-xs">
                    </div>
                    <div class="col-span-3">
                        <label class="mb-1 block text-[10px] text-zinc-500">别名</label>
                        <input type="text" name="models[{}][aliases]" value="{}" placeholder="gpt4,my-gpt" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-xs">
                    </div>
                    <div class="col-span-2">
                        <label class="mb-1 block text-[10px] text-zinc-500">输入/1K</label>
                        <input type="number" step="0.000001" name="models[{}][input_price]" value="{}" placeholder="0.005" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-xs">
                    </div>
                    <div class="col-span-2">
                        <label class="mb-1 block text-[10px] text-zinc-500">输出/1K</label>
                        <input type="number" step="0.000001" name="models[{}][output_price]" value="{}" placeholder="0.015" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-xs">
                    </div>
                    <div class="col-span-2">
                        <label class="mb-1 block text-[10px] text-zinc-500">状态</label>
                        <select name="models[{}][status]" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-2 text-xs">
                            <option value="enabled" {}>启用</option>
                            <option value="disabled" {}>禁用</option>
                        </select>
                    </div>
                </div>
                <button type="button" onclick="this.closest('.flex').remove()" class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded border border-red-200 text-red-600 hover:bg-red-50">
                    <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                </button>
            </div>"#,
                i, escape_html(&m.name), i, escape_html(&aliases), i, input_price, i, output_price, i,
                if m.status == "enabled" { "selected" } else { "" },
                if m.status == "enabled" { "" } else { "selected" }
            )
        }).collect::<Vec<_>>().join("")
    };

    let headers_html = if headers.is_empty() {
        r#"<div class="rounded-lg border border-dashed border-zinc-300 py-5 text-center text-xs text-zinc-500">未配置自定义 Header</div>"#.to_string()
    } else {
        headers.iter().enumerate().map(|(i, (k, v))| {
            format!(r#"<div class="grid grid-cols-12 gap-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
                <div class="col-span-4">
                    <label class="mb-1 block text-[10px] text-zinc-500">Header 名称</label>
                    <input type="text" name="headers[{}][name]" value="{}" placeholder="Authorization" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-sm">
                </div>
                <div class="col-span-7">
                    <label class="mb-1 block text-[10px] text-zinc-500">Header 值</label>
                    <input type="text" name="headers[{}][value]" value="{}" placeholder="Bearer ..." class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-sm">
                </div>
                <div class="col-span-1 flex items-end">
                    <button type="button" onclick="this.closest('.grid').remove()" class="inline-flex h-9 w-9 items-center justify-center rounded border border-red-200 text-red-600 hover:bg-red-50">
                        <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                    </button>
                </div>
            </div>"#, i, escape_html(k), i, escape_html(v))
        }).collect::<Vec<_>>().join("")
    };

    format!(
        r##"<form onsubmit="submitProviderForm(event, {method_js}, {url_js})" class="provider-form dialog-form">
        <div class="grid grid-cols-2 gap-4">
            <div>
                <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">供应商名称 *</label>
                <input type="text" name="name" value="{name_html}" placeholder="openai" required class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm" {name_readonly}>
            </div>
            <div>
                <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">API Key *</label>
                <input type="password" name="api_key" value="{api_key_html}" placeholder="sk-..." required class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-sm">
            </div>
        </div>

        <div class="border-t border-zinc-200 pt-4">
            <div class="mb-3 flex items-center justify-between">
                <label class="text-xs font-medium uppercase tracking-wider text-zinc-500">Endpoints *</label>
                <button type="button" onclick="addEndpointRow()" class="inline-flex h-7 items-center gap-1 rounded border border-zinc-200 px-2 text-xs hover:bg-zinc-50">
                    <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/></svg>
                    添加 Endpoint
                </button>
            </div>
            <div id="endpoints-container" class="space-y-2">
                {endpoints_html}
            </div>
        </div>

        <div class="grid grid-cols-2 gap-4 border-t border-zinc-200 pt-4">
            <div>
                <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">优先级 (数字越小越优先)</label>
                <input type="number" name="priority" value="{priority}" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
            </div>
            <div>
                <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">状态</label>
                <select name="status" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
                    <option value="enabled" {enabled_selected}>启用</option>
                    <option value="disabled" {disabled_selected}>禁用</option>
                </select>
            </div>
        </div>

        <div class="border-t border-zinc-200 pt-4">
            <div class="mb-3 flex items-center justify-between">
                <div>
                    <h4 class="text-sm font-semibold">自定义 Headers</h4>
                    <p class="mt-1 text-xs text-zinc-500">随供应商请求发送，可用于 Authorization、Anthropic Beta 等上游专用请求头。</p>
                </div>
                <button type="button" onclick="addHeaderRow()" class="inline-flex h-7 items-center gap-1 rounded border border-zinc-200 px-2 text-xs hover:bg-zinc-50">
                    <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/></svg>
                    添加 Header
                </button>
            </div>
            <div id="headers-container" class="space-y-2">
                {headers_html}
            </div>
        </div>

        <div class="border-t border-zinc-200 pt-4">
            <div class="mb-3 flex items-center justify-between">
                <h4 class="text-sm font-semibold">模型 / 费率 / 别名</h4>
                <button type="button" onclick="addModelRow()" class="inline-flex h-7 items-center gap-1 rounded border border-zinc-200 px-2 text-xs hover:bg-zinc-50">
                    <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/></svg>
                    添加模型
                </button>
            </div>
            <div id="models-container" class="max-h-[300px] space-y-2 overflow-y-auto pr-1">
                {models_html}
            </div>
        </div>

        <div class="dialog-footer">
            <button type="button" onclick="closeModal()" class="outline-button">取消</button>
            <button type="submit" class="primary-button">保存配置</button>
        </div>
    </form>

    <script>
        var endpointIndex = 100;
        var modelIndex = 100;
        var headerIndex = 100;

        function addEndpointRow() {{
            const container = document.getElementById('endpoints-container');
            const div = document.createElement('div');
            div.className = 'grid grid-cols-12 gap-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3';
            div.innerHTML = `
                <div class="col-span-4">
                    <label class="mb-1 block text-[10px] text-zinc-500">协议</label>
                    <select name="endpoints[${{endpointIndex}}][protocol]" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-2 text-sm">
                        <option value="completions">completions</option>
                        <option value="responses">responses</option>
                        <option value="messages">messages</option>
                    </select>
                </div>
                <div class="col-span-7">
                    <label class="mb-1 block text-[10px] text-zinc-500">Base URL</label>
                    <input type="text" name="endpoints[${{endpointIndex}}][base_url]" placeholder="https://api.openai.com" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
                </div>
                <div class="col-span-1 flex items-end">
                    <button type="button" onclick="this.closest('.grid').remove()" class="inline-flex h-9 w-9 items-center justify-center rounded border border-red-200 text-red-600 hover:bg-red-50">
                        <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                    </button>
                </div>
            `;
            container.appendChild(div);
            endpointIndex++;
        }}

        function addModelRow() {{
            const container = document.getElementById('models-container');
            const div = document.createElement('div');
            div.className = 'flex items-end gap-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3';
            div.innerHTML = `
                <div class="grid flex-1 grid-cols-12 gap-2">
                    <div class="col-span-3">
                        <label class="mb-1 block text-[10px] text-zinc-500">模型名称 *</label>
                        <input type="text" name="models[${{modelIndex}}][name]" placeholder="gpt-4o-mini" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-xs">
                    </div>
                    <div class="col-span-3">
                        <label class="mb-1 block text-[10px] text-zinc-500">别名</label>
                        <input type="text" name="models[${{modelIndex}}][aliases]" placeholder="gpt4,my-gpt" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-xs">
                    </div>
                    <div class="col-span-2">
                        <label class="mb-1 block text-[10px] text-zinc-500">输入/1K</label>
                        <input type="number" step="0.000001" name="models[${{modelIndex}}][input_price]" placeholder="0.005" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-xs">
                    </div>
                    <div class="col-span-2">
                        <label class="mb-1 block text-[10px] text-zinc-500">输出/1K</label>
                        <input type="number" step="0.000001" name="models[${{modelIndex}}][output_price]" placeholder="0.015" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-xs">
                    </div>
                    <div class="col-span-2">
                        <label class="mb-1 block text-[10px] text-zinc-500">状态</label>
                        <select name="models[${{modelIndex}}][status]" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-2 text-xs">
                            <option value="enabled">启用</option>
                            <option value="disabled">禁用</option>
                        </select>
                    </div>
                </div>
                <button type="button" onclick="this.closest('.flex').remove()" class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded border border-red-200 text-red-600 hover:bg-red-50">
                    <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                </button>
            `;
            container.appendChild(div);
            modelIndex++;
        }}

        function addHeaderRow() {{
            const container = document.getElementById('headers-container');
            const div = document.createElement('div');
            div.className = 'grid grid-cols-12 gap-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3';
            div.innerHTML = `
                <div class="col-span-4">
                    <label class="mb-1 block text-[10px] text-zinc-500">Header 名称</label>
                    <input type="text" name="headers[${{headerIndex}}][name]" placeholder="Authorization" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-sm">
                </div>
                <div class="col-span-7">
                    <label class="mb-1 block text-[10px] text-zinc-500">Header 值</label>
                    <input type="text" name="headers[${{headerIndex}}][value]" placeholder="Bearer ..." class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-sm">
                </div>
                <div class="col-span-1 flex items-end">
                    <button type="button" onclick="this.closest('.grid').remove()" class="inline-flex h-9 w-9 items-center justify-center rounded border border-red-200 text-red-600 hover:bg-red-50">
                        <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                    </button>
                </div>
            `;
            container.appendChild(div);
            headerIndex++;
        }}
    </script>
"##,
        method_js = escape_inline_js_string(method),
        url_js = escape_inline_js_string(url),
        name_html = escape_html(name),
        name_readonly = if name_readonly { "readonly" } else { "" },
        api_key_html = escape_html(api_key),
        endpoints_html = endpoints_html,
        priority = priority,
        enabled_selected = if status == "enabled" { "selected" } else { "" },
        disabled_selected = if status == "enabled" { "" } else { "selected" },
        headers_html = headers_html,
        models_html = models_html,
    )
}
