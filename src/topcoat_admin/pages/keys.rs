use topcoat::{
    context::Cx,
    router::{page, path_param},
    view::{view, View},
    Result,
};

use crate::topcoat_admin::api::{fetch_keys, fetch_providers};
use crate::topcoat_admin::app::require_admin;
use crate::topcoat_admin::{
    escape_html, escape_inline_js_string, render_dialog, render_list, render_modal_backdrop,
    render_page, render_shared_scripts, render_shared_styles, render_sidebar,
    render_theme_bootstrap, render_toast_container, trusted_html, DialogLayout, ListLayout,
    PageLayout,
};

#[path_param]
struct KeyId(str);

/// Main keys page with table container.
#[page("/keys")]
pub async fn keys(cx: &Cx) -> Result {
    require_admin(cx)?;
    let sidebar = render_sidebar(cx, "/keys").await?;
    let toast_container = render_toast_container();
    let modal_backdrop = render_modal_backdrop();
    let shared_styles = render_shared_styles();
    let shared_scripts = render_shared_scripts();
    let theme_bootstrap = render_theme_bootstrap();
    let list_body: Result = view! { <div class="list-loading">"加载中..."</div> };
    let list_view = render_list(
        cx,
        ListLayout {
            id: "keys-list",
            label: "API 密钥列表",
            endpoint: Some("/keys/table"),
            refresh_event: Some("rcpa-keys-refresh"),
            body: list_body?,
        },
    )
    .await?;
    let page_actions: Result = view! {
        <button class="primary-button" type="button" onclick="openKeyModal()">
            <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M12 5v14M5 12h14"/></svg>
            "生成新密钥"
        </button>
    };
    let page_html = render_page(
        cx,
        PageLayout {
            title: "API 密钥管理",
            description: Some("管理访问凭据、授权模型与供应商范围"),
            class_name: "keys-page",
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
    <title>RCPA Admin - 密钥管理</title>
    {theme_bootstrap}
    <link rel="stylesheet" href="/_topcoat/tailwind.css">
    <script src="/_topcoat/htmx.min.js"></script>
    {}
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
        function openKeyModal() {{
            Modal.load('/keys/new');
        }}

        function openEditModal(id) {{
            Modal.load('/keys/' + id + '/edit');
        }}

        function copyKey(key) {{
            navigator.clipboard.writeText(key).then(() => {{
                Toast.success('密钥已复制到剪贴板');
            }}).catch(() => {{
                Toast.error('复制失败，请手动复制');
            }});
        }}

        function addModelRow() {{
            const container = document.getElementById('model-rows');
            const row = document.createElement('div');
            row.className = 'flex gap-2 items-end bg-zinc-50 border border-zinc-200 p-3 rounded-lg mb-2';
            row.innerHTML = `
                <div class="flex-1 grid grid-cols-12 gap-2">
                    <div class="col-span-5">
                        <label class="block text-[10px] text-zinc-500 mb-1">有效模型名 *</label>
                        <input type="text" class="model-name h-9 w-full rounded-lg border border-zinc-200 px-3 text-sm font-mono" placeholder="gpt-4o-mini">
                    </div>
                    <div class="col-span-4">
                        <label class="block text-[10px] text-zinc-500 mb-1">Key 别名</label>
                        <input type="text" class="model-aliases h-9 w-full rounded-lg border border-zinc-200 px-3 text-sm font-mono" placeholder="fast,quick">
                    </div>
                    <div class="col-span-3">
                        <label class="block text-[10px] text-zinc-500 mb-1">状态</label>
                        <div class="flex h-9 items-center gap-2">
                            <label class="relative inline-flex items-center cursor-pointer">
                                <input type="checkbox" class="model-status sr-only peer" checked>
                                <div class="w-9 h-5 bg-zinc-200 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-zinc-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-emerald-500"></div>
                            </label>
                            <span class="model-status-text text-xs text-emerald-600">启用</span>
                        </div>
                    </div>
                </div>
                <button type="button" class="h-8 w-8 shrink-0 rounded border border-red-200 text-red-600 hover:bg-red-50 flex items-center justify-center" onclick="this.parentElement.remove()">
                    <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                </button>
            `;
            container.appendChild(row);

            const statusInput = row.querySelector('.model-status');
            const statusText = row.querySelector('.model-status-text');
            statusInput.addEventListener('change', () => {{
                if (statusInput.checked) {{
                    statusText.textContent = '启用';
                    statusText.className = 'model-status-text text-xs text-emerald-600';
                }} else {{
                    statusText.textContent = '禁用';
                    statusText.className = 'model-status-text text-xs text-zinc-500';
                }}
            }});
        }}

        function saveKey(e, id) {{
            e.preventDefault();
            const form = e.target;
            const name = form.querySelector('#input-name')?.value || '';
            const labels = form.querySelector('#input-labels')?.value || '';

            const allowedProviders = [];
            form.querySelectorAll('.provider-check:checked').forEach(cb => {{
                allowedProviders.push(cb.value);
            }});

            const modelRows = [];
            form.querySelectorAll('#model-rows > div').forEach(row => {{
                const modelName = row.querySelector('.model-name')?.value?.trim();
                if (!modelName) return;
                const aliases = row.querySelector('.model-aliases')?.value || '';
                const status = row.querySelector('.model-status')?.checked ? 'enabled' : 'disabled';
                modelRows.push({{ name: modelName, aliases: aliases, status: status }});
            }});

            const payload = {{
                name: name || null,
                labels: labels || null,
                allowed_providers: allowedProviders,
                allowed_models: modelRows.length > 0 ? modelRows : null,
                model_aliases: {{}}
            }};

            modelRows.forEach(row => {{
                if (row.aliases) {{
                    row.aliases.split(',').forEach(alias => {{
                        const a = alias.trim();
                        if (a) payload.model_aliases[a] = row.name;
                    }});
                }}
            }});

            const url = id ? '/v1/admin/keys/' + id : '/v1/admin/keys';
            setFormBusy(form, true);
            fetch(url, {{
                method: id ? 'PUT' : 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                credentials: 'include',
                body: JSON.stringify(payload)
            }}).then(response => {{
                if (response.status === 401) {{
                    window.location.href = '/login';
                    return;
                }}
                if (response.ok) {{
                    Toast.success(id ? 'API 密钥配置已更新' : 'API 密钥生成成功');
                    Modal.close();
                    refreshData('keys');
                }} else {{
                    response.json().then(d => Toast.error(d?.error?.message || '保存失败'));
                }}
            }}).catch(() => Toast.error('API 接口连接出错'))
              .finally(() => setFormBusy(form, false));
        }}

        function toggleKeyStatus(id, currentStatus) {{
            const nextStatus = currentStatus === 'enabled' ? 'disabled' : 'enabled';
            fetch('/v1/admin/keys/' + id + '/status', {{
                method: 'PUT',
                headers: {{ 'Content-Type': 'application/json' }},
                credentials: 'include',
                body: JSON.stringify({{ status: nextStatus }})
            }}).then(response => {{
                if (response.status === 401) {{
                    window.location.href = '/login';
                    return;
                }}
                if (response.ok) {{
                    Toast.success('密钥已' + (nextStatus === 'enabled' ? '启用' : '禁用'));
                    refreshData('keys');
                }} else {{
                    Toast.error('更新状态失败');
                }}
            }});
        }}

        function toggleKeyModelStatus(keyId, modelName, currentStatus) {{
            const nextStatus = currentStatus === 'enabled' ? 'disabled' : 'enabled';
            fetch('/v1/admin/keys/' + keyId + '/models/' + encodeURIComponent(modelName) + '/status', {{
                method: 'PUT',
                headers: {{ 'Content-Type': 'application/json' }},
                credentials: 'include',
                body: JSON.stringify({{ status: nextStatus }})
            }}).then(response => {{
                if (response.status === 401) {{
                    window.location.href = '/login';
                    return;
                }}
                if (response.ok) {{
                    Toast.success('模型规则已更新');
                    refreshData('keys');
                }} else {{
                    Toast.error('更新模型规则失败');
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

/// Keys table with full details.
#[page("/keys/table")]
pub async fn keys_table(cx: &Cx) -> Result {
    require_admin(cx)?;
    let all_keys = fetch_keys(cx).await;
    let key_count = all_keys.len();

    if key_count == 0 {
        return Ok(View::unescaped_unchecked(
            r##"<div class="py-12 text-center text-sm text-zinc-500">未找到 API 密钥</div>"##,
        ));
    }

    let mut html = String::new();
    html.push_str(r##"<div class="overflow-x-auto"><table class="w-full text-sm">
        <thead class="border-b border-zinc-200 bg-zinc-50">
            <tr>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">名称</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">API 密钥</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">状态</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">允许模型</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">允许供应商</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">模型别名</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">备注</th>
                <th class="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">操作</th>
            </tr>
        </thead>
        <tbody class="divide-y divide-zinc-200"> "##);

    for key in &all_keys {
        let name = escape_html(key.name.as_deref().unwrap_or("—"));
        let key_val = escape_html(&key.key);
        let key_val_js = escape_inline_js_string(&key.key);
        let status_class = if key.status == "enabled" {
            "bg-emerald-100 text-emerald-700"
        } else {
            "bg-red-100 text-red-700"
        };
        let status_text = if key.status == "enabled" {
            "启用"
        } else {
            "禁用"
        };
        let labels = escape_html(key.labels.as_deref().unwrap_or("—"));
        let id = &key.id;
        let id_js = escape_inline_js_string(id);
        let is_enabled = key.status == "enabled";

        let model_badges = if key.allowed_models.is_empty() {
            r#"<span class="text-xs italic text-zinc-400">全部</span>"#.to_string()
        } else {
            let mut badges = String::new();
            for model in &key.allowed_models {
                let model_name = escape_html(&model.name);
                let model_name_js = escape_inline_js_string(&model.name);
                let model_status = &model.status;
                let model_status_js = escape_inline_js_string(model_status);
                let opacity = if model_status == "enabled" {
                    ""
                } else {
                    " opacity-50 line-through"
                };
                badges.push_str(&format!(
                    r##"<button type="button" onclick="toggleKeyModelStatus({}, {}, {})" class="inline-flex h-6 items-center rounded border border-zinc-200 px-2 font-mono text-xs hover:bg-zinc-50{}" title="{}">{}</button>"##,
                    id_js, model_name_js, model_status_js, opacity, model_name, model_name
                ));
            }
            badges
        };

        let provider_badges = if key.allowed_providers.is_empty() {
            r#"<span class="text-xs italic text-zinc-400">全部</span>"#.to_string()
        } else {
            let mut badges = String::new();
            for provider in &key.allowed_providers {
                badges.push_str(&format!(
                    r##"<span class="inline-flex rounded-full bg-zinc-100 px-2 py-0.5 font-mono text-xs">{}</span>"##,
                    escape_html(provider)
                ));
            }
            badges
        };

        let alias_badges = if key.model_aliases.is_empty() {
            r#"<span class="text-xs italic text-zinc-400">—</span>"#.to_string()
        } else {
            let mut badges = String::new();
            for (alias, target) in &key.model_aliases {
                badges.push_str(&format!(
                    r##"<span class="inline-flex rounded-full bg-zinc-100 px-2 py-0.5 font-mono text-xs" title="{} -> {}">{} → {}</span>"##,
                    escape_html(alias), escape_html(target), escape_html(alias), escape_html(target)
                ));
            }
            badges
        };

        let toggle_text = if is_enabled { "禁用" } else { "启用" };

        html.push_str(&format!(
            r##"<tr class="hover:bg-zinc-50">
                <td class="px-4 py-3 font-medium">{}</td>
                <td class="px-4 py-3">
                    <div class="flex items-center gap-1.5">
                        <code class="select-all rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-xs">{}</code>
                        <button type="button" onclick="copyKey({})" class="inline-flex h-7 w-7 items-center justify-center rounded border border-zinc-200 text-zinc-600 hover:bg-zinc-50" title="复制密钥">
                            <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"/></svg>
                        </button>
                    </div>
                </td>
                <td class="px-4 py-3"><span class="inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium {}">{}</span></td>
                <td class="px-4 py-3">
                    <div class="flex flex-wrap gap-1">{}</div>
                </td>
                <td class="px-4 py-3">
                    <div class="flex flex-wrap gap-1">{}</div>
                </td>
                <td class="px-4 py-3">
                    <div class="flex flex-wrap gap-1">{}</div>
                </td>
                <td class="px-4 py-3 text-xs text-zinc-500">{}</td>
                <td class="px-4 py-3">
                    <div class="flex gap-1.5">
                        <button type="button" onclick="openEditModal({})" class="inline-flex h-7 w-7 items-center justify-center rounded border border-zinc-200 text-zinc-600 hover:bg-zinc-50">
                            <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"/></svg>
                        </button>
                        <button type="button" onclick="toggleKeyStatus({}, {})" class="inline-flex h-7 items-center rounded border border-zinc-200 px-2 text-xs text-zinc-600 hover:bg-zinc-50">{}</button>
                    </div>
                </td>
            </tr>"##,
            name, key_val, key_val_js, status_class, status_text,
            model_badges, provider_badges, alias_badges, labels,
            id_js, id_js, escape_inline_js_string(&key.status), toggle_text
        ));
    }

    html.push_str(r##"</tbody></table></div>"##);

    Ok(trusted_html(html))
}

/// New key modal content.
#[page("/keys/new")]
pub async fn keys_new(cx: &Cx) -> Result {
    require_admin(cx)?;
    let providers = fetch_providers(cx).await;
    let provider_options = if providers.is_empty() {
        r#"<div class="text-xs text-zinc-500">暂无可选供应商</div>"#.to_string()
    } else {
        let mut opts = String::new();
        for provider in &providers {
            let name = escape_html(&provider.name);
            let endpoint = escape_html(
                provider
                    .endpoints
                    .first()
                    .map(|endpoint| endpoint.base_url.as_str())
                    .unwrap_or("未声明端点"),
            );
            opts.push_str(&format!(
                r##"<label class="flex items-start gap-2 rounded-md border border-transparent px-2 py-1.5 hover:border-zinc-200 cursor-pointer">
                    <input type="checkbox" value="{}" class="provider-check mt-0.5 h-4 w-4 rounded border-zinc-300">
                    <div class="min-w-0">
                        <div class="font-mono text-sm">{}</div>
                        <div class="text-[11px] text-zinc-500">{}</div>
                    </div>
                </label>"##,
                name, name, endpoint
            ));
        }
        opts
    };

    let body_html = format!(
        r##"<form onsubmit="saveKey(event, '')" class="dialog-form">
        <div>
            <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">名称</label>
            <input type="text" id="input-name" placeholder="生产环境密钥" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
        </div>

        <div>
            <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">允许供应商</label>
            <div class="rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
                <div class="grid gap-2 sm:grid-cols-2">{}</div>
                <div class="mt-2 text-[11px] text-zinc-500">不勾选表示允许全部供应商</div>
            </div>
        </div>

        <div class="border-t border-zinc-200 pt-4">
            <div class="mb-3 flex items-center justify-between">
                <h4 class="text-sm font-semibold">模型 / Key 别名</h4>
                <button type="button" onclick="addModelRow()" class="inline-flex h-7 items-center gap-1 rounded border border-zinc-200 px-2 text-xs hover:bg-zinc-50">
                    <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/></svg>
                    添加模型
                </button>
            </div>
            <div id="model-rows"></div>
            <div class="mt-2 text-xs text-zinc-500">点击"添加模型"开始配置；为空时默认允许全部模型</div>
        </div>

        <div>
            <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">备注</label>
            <input type="text" id="input-labels" placeholder="可选备注" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
        </div>

        <div class="dialog-footer">
            <button type="button" onclick="closeModal()" class="outline-button">取消</button>
            <button type="submit" class="primary-button">生成密钥</button>
        </div>
    </form>"##,
        provider_options
    );

    let html = render_dialog(
        cx,
        DialogLayout {
            title: "生成 API 密钥",
            description: Some("设置供应商和模型范围；留空表示不限制"),
            class_name: "key-dialog",
            body: trusted_html(body_html),
        },
    )
    .await?;

    Ok(html)
}

/// Edit key modal content.
#[page("/keys/{key_id}/edit")]
pub async fn keys_edit(cx: &Cx) -> Result {
    require_admin(cx)?;
    let id = path_param::<KeyId>(cx);
    let all_keys = fetch_keys(cx).await;
    let providers = fetch_providers(cx).await;
    let Some(key) = all_keys.iter().find(|key| key.id == id) else {
        let body_html = format!(
            r#"<div role="alert" class="rounded-md border border-red-200 bg-red-50 p-4 text-sm text-red-700">API 密钥 <span class="font-mono">{}</span> 不存在或已被删除，请关闭后刷新列表。</div>"#,
            escape_html(id)
        );
        let html = render_dialog(
            cx,
            DialogLayout {
                title: "编辑 API 密钥",
                description: Some("无法读取密钥配置"),
                class_name: "key-dialog",
                body: trusted_html(body_html),
            },
        )
        .await?;
        return Ok(html);
    };

    let key_val = key.key.as_str();
    let key_name = key.name.as_deref().unwrap_or("");
    let key_labels = key.labels.as_deref().unwrap_or("");

    let provider_checkboxes = if providers.is_empty() {
        r#"<div class="text-xs text-zinc-500">暂无可选供应商</div>"#.to_string()
    } else {
        let mut opts = String::new();
        let allowed_providers: Vec<&str> =
            key.allowed_providers.iter().map(String::as_str).collect();
        for provider in &providers {
            let checked = if allowed_providers.contains(&provider.name.as_str()) {
                "checked"
            } else {
                ""
            };
            let name = escape_html(&provider.name);
            let endpoint = escape_html(
                provider
                    .endpoints
                    .first()
                    .map(|endpoint| endpoint.base_url.as_str())
                    .unwrap_or("未声明端点"),
            );
            opts.push_str(&format!(
                r##"<label class="flex items-start gap-2 rounded-md border border-transparent px-2 py-1.5 hover:border-zinc-200 cursor-pointer">
                    <input type="checkbox" value="{}" class="provider-check mt-0.5 h-4 w-4 rounded border-zinc-300" {}>
                    <div class="min-w-0">
                        <div class="font-mono text-sm">{}</div>
                        <div class="text-[11px] text-zinc-500">{}</div>
                    </div>
                </label>"##,
                name, checked, name, endpoint
            ));
        }
        opts
    };

    let model_rows_html = {
        let mut rows = String::new();
        for model in &key.allowed_models {
            let model_name = escape_html(&model.name);
            let model_status = &model.status;
            let aliases = key
                .model_aliases
                .iter()
                .filter_map(|(alias, target)| (target == &model.name).then_some(alias.as_str()))
                .collect::<Vec<_>>()
                .join(", ");
            let aliases = escape_html(&aliases);
            let checked = if model_status == "enabled" {
                "checked"
            } else {
                ""
            };
            let status_text = if model_status == "enabled" {
                "启用"
            } else {
                "禁用"
            };
            let status_color = if model_status == "enabled" {
                "text-emerald-600"
            } else {
                "text-zinc-500"
            };
            rows.push_str(&format!(
                r##"<div class="flex gap-2 items-end bg-zinc-50 border border-zinc-200 p-3 rounded-lg mb-2">
                    <div class="flex-1 grid grid-cols-12 gap-2">
                        <div class="col-span-5">
                            <label class="block text-[10px] text-zinc-500 mb-1">有效模型名 *</label>
                            <input type="text" class="model-name h-9 w-full rounded-lg border border-zinc-200 px-3 text-sm font-mono" value="{}">
                        </div>
                        <div class="col-span-4">
                            <label class="block text-[10px] text-zinc-500 mb-1">Key 别名</label>
                            <input type="text" class="model-aliases h-9 w-full rounded-lg border border-zinc-200 px-3 text-sm font-mono" value="{}">
                        </div>
                        <div class="col-span-3">
                            <label class="block text-[10px] text-zinc-500 mb-1">状态</label>
                            <div class="flex h-9 items-center gap-2">
                                <label class="relative inline-flex items-center cursor-pointer">
                                    <input type="checkbox" class="model-status sr-only peer" {}>
                                    <div class="w-9 h-5 bg-zinc-200 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-zinc-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-emerald-500"></div>
                                </label>
                                <span class="model-status-text text-xs {}">{}</span>
                            </div>
                        </div>
                    </div>
                    <button type="button" class="h-8 w-8 shrink-0 rounded border border-red-200 text-red-600 hover:bg-red-50 flex items-center justify-center" onclick="this.parentElement.remove()">
                        <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                    </button>
                </div>"##,
                model_name, aliases, checked, status_color, status_text
            ));
        }
        rows
    };

    let body_html = format!(
        r##"<form onsubmit="saveKey(event, {})" class="dialog-form">
        <div>
            <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">API 密钥</label>
            <input type="text" value="{}" readonly class="h-9 w-full rounded-lg border border-zinc-200 bg-zinc-50 px-3 font-mono text-sm">
        </div>

        <div>
            <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">名称</label>
            <input type="text" id="input-name" value="{}" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
        </div>

        <div>
            <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">允许供应商</label>
            <div class="rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
                <div class="grid gap-2 sm:grid-cols-2">{}</div>
                <div class="mt-2 text-[11px] text-zinc-500">不勾选表示允许全部供应商</div>
            </div>
        </div>

        <div class="border-t border-zinc-200 pt-4">
            <div class="mb-3 flex items-center justify-between">
                <h4 class="text-sm font-semibold">模型 / Key 别名</h4>
                <button type="button" onclick="addModelRow()" class="inline-flex h-7 items-center gap-1 rounded border border-zinc-200 px-2 text-xs hover:bg-zinc-50">
                    <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/></svg>
                    添加模型
                </button>
            </div>
            <div id="model-rows">{}</div>
            <div class="mt-2 text-xs text-zinc-500">点击"添加模型"开始配置；为空时默认允许全部模型</div>
        </div>

        <div>
            <label class="mb-1.5 block text-xs font-medium uppercase tracking-wider text-zinc-500">备注</label>
            <input type="text" id="input-labels" value="{}" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 text-sm">
        </div>

        <div class="dialog-footer">
            <button type="button" onclick="closeModal()" class="outline-button">取消</button>
            <button type="submit" class="primary-button">保存配置</button>
        </div>
    </form>"##,
        escape_inline_js_string(id),
        escape_html(key_val),
        escape_html(key_name),
        provider_checkboxes,
        model_rows_html,
        escape_html(key_labels)
    );

    let html = render_dialog(
        cx,
        DialogLayout {
            title: "编辑 API 密钥",
            description: Some("更新名称、供应商范围和 Key 私有模型别名"),
            class_name: "key-dialog",
            body: trusted_html(body_html),
        },
    )
    .await?;

    Ok(html)
}
