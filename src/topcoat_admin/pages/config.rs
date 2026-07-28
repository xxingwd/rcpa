use topcoat::{context::Cx, router::page, view::view, Result};

use crate::topcoat_admin::api::fetch_config_yaml;
use crate::topcoat_admin::app::{app_state, require_admin};
use crate::topcoat_admin::{
    render_page, render_shared_scripts, render_shared_styles, render_sidebar,
    render_theme_bootstrap, render_toast_container, trusted_html, PageLayout,
};

#[page("/config")]
pub async fn config(cx: &Cx) -> Result {
    require_admin(cx)?;
    let yaml_content = fetch_config_yaml(cx).await;
    let config_path = app_state(cx).config_service.path_display();
    let sidebar = render_sidebar(cx, "/config").await?;
    let toast_container = render_toast_container();
    let shared_styles = render_shared_styles();
    let shared_scripts = render_shared_scripts();
    let theme_bootstrap = render_theme_bootstrap();
    let actions: Result = view! {
        <span id="config-path" class="config-path" title=(config_path.as_str())>(config_path.as_str())</span>
        <span id="dirty-badge" class="dirty-badge" hidden="">"已修改"</span>
        <button id="reload-btn" class="outline-button h-8 px-3 text-xs" type="button" onclick="reloadConfig()">
            <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M21 12a9 9 0 0 1-15.2 6.5L3 16M3 16v5m0-5h5M3 12A9 9 0 0 1 18.2 5.5L21 8M21 8V3m0 5h-5"/></svg>
            "重载"
        </button>
        <button id="save-btn" class="primary-button h-8 px-3 text-xs" type="button" onclick="saveConfig()" disabled="">
            <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2Z"/><path d="M17 21v-8H7v8M7 3v5h8"/></svg>
            "保存"
        </button>
    };
    let editor_body: Result = view! {
        <div class="config-editor-shell">
            <textarea id="config-editor" aria-label="YAML 配置">(yaml_content)</textarea>
        </div>
    };
    let page_html = render_page(
        cx,
        PageLayout {
            title: "配置",
            description: Some("直接编辑运行时 YAML 配置"),
            class_name: "config-page",
            actions: Some(actions?),
            body: editor_body?,
        },
    )
    .await?;

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RCPA Admin - 配置</title>
    {theme_bootstrap}
    <link rel="stylesheet" href="/_topcoat/tailwind.css">
    <link rel="stylesheet" href="/_topcoat/codemirror/codemirror.min.css">
    <link rel="stylesheet" href="/_topcoat/codemirror/dracula.min.css">
    <script src="/_topcoat/codemirror/codemirror.min.js"></script>
    <script src="/_topcoat/codemirror/yaml.min.js"></script>
    {}
    <style>
        .config-path, .dirty-badge {{ display: inline-flex; max-width: min(32rem, calc(100vw - 18rem)); align-items: center; padding: .125rem .625rem; border: 1px solid var(--border); border-radius: 6px; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: .75rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}
        .dirty-badge {{ max-width: none; border-color: transparent; background: color-mix(in oklch, #f59e0b 15%, transparent); color: #f59e0b; font-family: inherit; font-weight: 500; }}
        .config-page .page-body {{ overflow: hidden; border: 1px solid var(--border); border-radius: 6px; }}
        .config-editor-shell {{ width: 100%; height: 100%; min-height: 0; overflow: hidden; }}
        .CodeMirror, .CodeMirror.cm-s-dracula {{ width: 100%; height: 100%; background: var(--card) !important; color: var(--foreground); font-size: .75rem; line-height: 1.25rem; }}
        .CodeMirror-gutters, .cm-s-dracula .CodeMirror-gutters {{ border-right: 1px solid var(--border); background: color-mix(in oklch, var(--muted) 55%, transparent) !important; color: var(--muted-foreground); }}
        .CodeMirror-linenumber {{ color: var(--muted-foreground); }}
        .CodeMirror-cursor {{ border-left-color: var(--foreground); }}
        .CodeMirror-selected, .CodeMirror-focused .CodeMirror-selected {{ background: color-mix(in oklch, var(--primary) 24%, transparent) !important; }}
        .CodeMirror-activeline-background {{ background: color-mix(in oklch, var(--accent) 65%, transparent); }}
        @media (max-width: 1023px) {{
            .config-path {{ max-width: 100%; flex: 1 1 14rem; }}
            .config-editor-shell {{ min-height: 36rem; }}
        }}
    </style>
</head>
<body>
    {}
    <div class="flex min-h-screen">
        {}
        <main class="admin-main">
            <div class="admin-content">{}</div>
        </main>
    </div>
    <script>
        let editor;
        let savedContent = '';
        let loading = false;

        function setBusy(busy) {{
            loading = busy;
            document.getElementById('reload-btn').disabled = busy;
            updateDirtyState();
        }}

        function updateDirtyState() {{
            const dirty = editor && editor.getValue() !== savedContent;
            document.getElementById('dirty-badge').hidden = !dirty;
            document.getElementById('save-btn').disabled = loading || !dirty;
        }}

        function applyEditorTheme(theme) {{
            if (editor) editor.setOption('theme', theme === 'dark' ? 'dracula' : 'default');
        }}

        function reloadConfig() {{
            if (editor && editor.getValue() !== savedContent && !confirm('当前修改尚未保存，确定重载吗？')) return;
            setBusy(true);
            fetch('/v1/admin/config-file', {{ credentials: 'include' }})
                .then((response) => {{
                    if (response.status === 401) return redirectToLogin();
                    if (!response.ok) throw new Error('读取配置失败');
                    return response.json();
                }})
                .then((data) => {{
                    if (!data) return;
                    savedContent = data.content || '';
                    editor.setValue(savedContent);
                    if (data.path) {{
                        const path = document.getElementById('config-path');
                        path.textContent = data.path;
                        path.title = data.path;
                    }}
                    Toast.success('配置已重载');
                }})
                .catch((error) => Toast.error(error.message))
                .finally(() => setBusy(false));
        }}

        function saveConfig() {{
            if (!editor || editor.getValue() === savedContent) return;
            const content = editor.getValue();
            setBusy(true);
            fetch('/v1/admin/config-file', {{
                method: 'PUT',
                headers: {{ 'Content-Type': 'application/json' }},
                credentials: 'include',
                body: JSON.stringify({{ content }})
            }})
                .then((response) => {{
                    if (response.status === 401) return redirectToLogin();
                    return response.json().then((data) => ({{ ok: response.ok, data }}));
                }})
                .then((result) => {{
                    if (!result) return;
                    if (!result.ok) throw new Error(result.data?.error?.message || '保存配置失败');
                    savedContent = content;
                    if (result.data.path) {{
                        const path = document.getElementById('config-path');
                        path.textContent = result.data.path;
                        path.title = result.data.path;
                    }}
                    Toast.success('配置已保存');
                    refreshData('config');
                }})
                .catch((error) => Toast.error(error.message))
                .finally(() => setBusy(false));
        }}

        document.addEventListener('DOMContentLoaded', () => {{
            const textarea = document.getElementById('config-editor');
            savedContent = textarea.value;
            editor = CodeMirror.fromTextArea(textarea, {{
                mode: 'yaml',
                theme: document.documentElement.dataset.theme === 'dark' ? 'dracula' : 'default',
                lineNumbers: true,
                lineWrapping: true,
                indentUnit: 2,
                tabSize: 2,
                indentWithTabs: false,
                styleActiveLine: true,
                extraKeys: {{
                    'Ctrl-S': () => saveConfig(),
                    'Cmd-S': () => saveConfig()
                }}
            }});
            editor.on('change', updateDirtyState);
            window.addEventListener('rcpa:theme-change', (event) => applyEditorTheme(event.detail.theme));
            window.addEventListener('beforeunload', (event) => {{
                if (editor.getValue() !== savedContent) {{
                    event.preventDefault();
                    event.returnValue = '';
                }}
            }});
            updateDirtyState();
        }});
    </script>
    {}
</body>
</html>"##,
        shared_styles, toast_container, sidebar, page_html, shared_scripts
    );

    Ok(trusted_html(html))
}
