use indexmap::IndexMap;

use topcoat::{
    context::Cx,
    router::{page, path_param},
    view::{view, View},
    Result,
};

use crate::config::{
    join_provider_endpoint, EndpointConfig, ModelRule, PlatformModel, ProviderProtocol,
};
use crate::config_service::ProviderView;
use crate::topcoat_admin::api::fetch_providers;
use crate::topcoat_admin::app::require_admin;
use crate::topcoat_admin::{
    escape_html, escape_inline_js_string, render_dialog, render_list, render_modal_backdrop,
    render_page, render_shared_scripts, render_shared_styles, render_sidebar,
    render_sidebar_bootstrap, render_theme_bootstrap, render_toast_container, trusted_html,
    DialogLayout, ListLayout, PageLayout,
};

#[path_param]
struct ProviderName(str);

#[page("/providers")]
pub async fn providers(cx: &Cx) -> Result {
    render_providers(cx, None).await
}

async fn render_providers(cx: &Cx, detail: Option<(&str, View)>) -> Result {
    require_admin(cx)?;
    let sidebar = render_sidebar(cx, "/providers").await?;
    let toast_container = render_toast_container();
    let modal_backdrop = render_modal_backdrop();
    let shared_styles = render_shared_styles();
    let shared_scripts = render_shared_scripts();
    let theme_bootstrap = render_theme_bootstrap();
    let sidebar_bootstrap = render_sidebar_bootstrap();
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
    let is_detail = detail.is_some();
    let (title, body) = detail.unwrap_or(("供应商", list_view));
    let page_html = render_page(
        cx,
        PageLayout {
            title,
            description: Some("管理上游供应商、端点和模型映射"),
            class_name: "providers-page",
            actions: Some(if is_detail {
                trusted_html(r#"<a class="outline-button" href="/providers">返回列表</a>"#.into())
            } else {
                page_actions?
            }),
            body,
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
    {sidebar_bootstrap}
    <link rel="stylesheet" href="/_topcoat/tailwind.css">
    <script src="/_topcoat/htmx.min.js"></script>
    {}
    <style>
        .provider-detail {{ min-height: 0; height: 100%; flex: 1 1 auto; border: 1px solid var(--border); border-radius: var(--radius); background: var(--card); overflow: hidden; }}
        .provider-detail .provider-dialog {{ height: 100%; }}
        .provider-detail .dialog-header {{ display: none; }}
        #providers-list {{ min-height: 0; }}
        .provider-list-table {{ width: 100%; text-align: left; }}
        .provider-list-table th {{ font-size: .75rem; font-weight: 500; padding: .5rem .75rem; }}
        .provider-list-table td {{ padding: .5rem .75rem; border-top: 1px solid var(--border); }}
        .provider-list-name {{ font-weight: 600; color: var(--foreground); text-decoration: none; }}
        .provider-list-name:hover {{ color: var(--primary); }}
        .provider-list-url {{ display: block; max-width: 28rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: ui-monospace, monospace; font-size: .8125rem; color: var(--muted-foreground); }}
        .provider-list-actions {{ display: flex; justify-content: flex-end; gap: .25rem; }}
        .provider-list-secondary {{ color: var(--muted-foreground); font-size: .75rem; }}
        .upstream-preview {{ margin-bottom: .75rem; padding: .75rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--muted); }}
        .upstream-preview-tools {{ display: flex; flex-wrap: wrap; gap: .5rem; align-items: center; margin-bottom: .5rem; }}
        .upstream-preview-tools input {{ flex: 1; min-width: 8rem !important; margin: 0 !important; }}
        .upstream-preview-list {{ display: grid; max-height: 15rem; overflow: auto; }}
        .upstream-preview-list label {{ display: flex; align-items: center; min-height: 2rem; gap: .5rem; padding: .25rem; font-weight: 400; }}
        @media(max-width:639px) {{
            .provider-list-table thead {{ display: none; }}
            .provider-list-table tr {{ display: grid; grid-template-columns: 2.75rem minmax(0,1fr) auto; padding: .5rem; border-top: 1px solid var(--border); }}
            .provider-list-table td {{ min-width: 0; padding: .25rem; border: 0; }}
            .provider-list-table td:first-child {{ grid-row: 1 / 3; }}
            .provider-list-table td:nth-child(3) {{ grid-column: 2 / -1; }}
            .provider-list-table td:nth-child(4) {{ display: none; }}
            .provider-list-table td:last-child {{ grid-column: 2 / -1; }}
            .upstream-preview-list label {{ min-height: 2.75rem; }}
        }}
        .model-badge {{ display: inline-flex; align-items: center; height: 1.5rem; padding: 0 0.375rem; border-radius: 9999px; border: 1px solid; font-family: ui-monospace, monospace; font-size: 0.75rem; transition: all 0.15s; }}
        .model-badge.enabled {{ background: color-mix(in srgb, var(--success) 8%, var(--card)); border-color: color-mix(in srgb, var(--success) 35%, var(--border)); color: var(--success); }}
        .model-badge.disabled {{ background: var(--muted); border-color: var(--border); color: var(--muted-foreground); }}
        .model-badge-dot {{ width: 0.375rem; height: 0.375rem; border-radius: 50%; margin-right: 0.25rem; }}
        .model-badge.enabled .model-badge-dot {{ background: var(--success); }}
        .model-badge.disabled .model-badge-dot {{ background: rgba(113, 113, 122, 0.5); }}
        .modal-content:has(.provider-dialog) {{ width: min(70rem, calc(100vw - 2rem)); overflow: hidden; }}
        .provider-dialog {{ display: flex; height: min(48rem, calc(100dvh - 2rem)); flex-direction: column; }}
        .provider-dialog .dialog-header {{ position: static; flex: none; }}
        .provider-dialog .dialog-body {{ flex: 1; min-height: 0; padding: 0; overflow: hidden; }}
        .provider-form {{ display: grid; height: 100%; min-height: 0; grid-template-rows: minmax(0, 1fr) auto; }}
        .provider-form-layout {{ display: grid; min-height: 0; grid-template-columns: 9.5rem minmax(0, 1fr); }}
        .provider-form-nav {{ display: flex; flex-direction: column; gap: .25rem; padding: .75rem .5rem; border-right: 1px solid var(--border); background: var(--muted); }}
        .provider-form-nav button {{ display: flex; align-items: center; justify-content: space-between; min-height: 2.25rem; padding: .375rem .625rem; border: 0; border-radius: 6px; color: var(--muted-foreground); background: transparent; text-align: left; font-size: .8125rem; font-weight: 600; }}
        .provider-form-nav button:hover, .provider-form-nav button.is-active {{ color: var(--foreground); background: var(--card); }}
        .provider-form-nav button.is-active {{ box-shadow: inset 3px 0 var(--primary); }}
        .provider-form-nav span {{ min-width: 1.5rem; padding: .125rem .375rem; border-radius: 4px; background: var(--muted); text-align: center; font-size: .75rem; }}
        .provider-form-main {{ min-width: 0; min-height: 0; padding: 1rem; overflow-y: auto; }}
        .provider-panel[hidden], .provider-model-row[hidden], .provider-model-details[hidden], .provider-protocol-row [hidden], .provider-model-empty[hidden], .provider-match-badge[hidden] {{ display: none; }}
        .provider-panel-heading {{ margin-bottom: 1rem; }}
        .provider-panel h3 {{ margin: 0; font-size: 1rem; font-weight: 650; line-height: 1.5rem; }}
        .provider-panel h4 {{ margin: 0; font-size: .875rem; font-weight: 650; }}
        .provider-form label {{ min-width: 0; color: var(--muted-foreground); font-size: .75rem; font-weight: 600; }}
        .provider-form input:not([type="checkbox"]), .provider-form select {{ display: block; width: 100%; min-width: 0; height: var(--control-height); margin-top: .25rem; padding: 0 .625rem; border: 1px solid var(--border); border-radius: 6px; outline: none; background: var(--card); color: var(--foreground); font-size: .8125rem; font-weight: 400; }}
        .provider-form input:not([type="checkbox"]):focus-visible, .provider-form select:focus-visible {{ background: var(--muted) !important; outline: none; box-shadow: none; }}
        .provider-form input[readonly] {{ background: var(--muted); color: var(--muted-foreground); }}
        .provider-form input[type="checkbox"] {{ width: 1rem; height: 1rem; margin: 0; accent-color: var(--primary); }}
        .provider-field-grid {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .75rem; margin-bottom: 1rem; }}
        .provider-section-block {{ margin-bottom: 1rem; }}
        .provider-section-heading {{ margin-bottom: .625rem; }}
        .provider-base-field {{ display: block; margin-bottom: .75rem; }}
        .provider-protocol-list {{ border: 1px solid var(--border); border-radius: 6px; overflow: hidden; }}
        .provider-protocol-row {{ padding: .625rem .75rem; border-top: 1px solid var(--border); }}
        .provider-protocol-row:first-child {{ border-top: 0; }}
        .provider-protocol-row.is-disabled {{ background: color-mix(in srgb, var(--muted) 45%, var(--card)); }}
        .provider-protocol-main {{ display: flex; align-items: center; gap: .75rem; min-width: 0; }}
        .provider-protocol-name, .provider-protocol-custom-label {{ display: inline-flex; align-items: center; gap: .5rem; white-space: nowrap; cursor: pointer; }}
        .provider-protocol-name {{ min-width: 10.25rem !important; color: var(--foreground) !important; font-size: .8125rem !important; }}
        .provider-protocol-main code {{ min-width: 0; overflow: hidden; color: var(--muted-foreground); font-size: .75rem; text-overflow: ellipsis; white-space: nowrap; }}
        .provider-protocol-custom-label {{ margin-left: auto; }}
        .provider-protocol-url {{ margin: .25rem 0 0 1.5rem; overflow: hidden; color: var(--muted-foreground); font-family: ui-monospace, monospace; font-size: .75rem; text-overflow: ellipsis; white-space: nowrap; }}
        .provider-protocol-row.is-disabled .provider-protocol-url {{ display: none; }}
        .provider-protocol-row input[data-protocol-custom-url] {{ margin: .5rem 0 0 1.5rem; width: calc(100% - 1.5rem); }}
        .provider-passthrough {{ display: flex; align-items: flex-start; gap: .625rem; padding: .75rem; border: 1px solid var(--border); border-radius: 6px; cursor: pointer; }}
        .provider-passthrough input {{ margin-top: .125rem !important; }}
        .provider-passthrough span, .provider-passthrough small {{ display: block; }}
        .provider-passthrough strong {{ color: var(--foreground); font-size: .8125rem; }}
        .provider-passthrough small {{ margin-top: .25rem; font-size: .75rem; font-weight: 400; }}
        .provider-headers {{ border-top: 1px solid var(--border); padding-top: 1rem; }}
        .provider-headers summary {{ display: flex; align-items: center; gap: .5rem; cursor: pointer; color: var(--foreground); font-size: .8125rem; font-weight: 600; }}
        .provider-headers summary span {{ color: var(--muted-foreground); font-size: .75rem; }}
        .provider-headers-body {{ padding-top: .75rem; }}
        .provider-headers-body > button {{ margin-bottom: .75rem; }}
        .provider-form-footer {{ display: flex; justify-content: flex-end; gap: .5rem; padding: .75rem 1rem; border-top: 1px solid var(--border); background: var(--card); }}
        .provider-model-toolbar {{ display: flex; align-items: center; justify-content: space-between; gap: .75rem; margin-bottom: .75rem; }}
        .provider-model-toolbar > div:first-child {{ min-width: 0; }}
        .provider-model-toolbar span {{ color: var(--muted-foreground); font-size: .75rem; }}
        .provider-model-toolbar-actions {{ display: flex; gap: .5rem; }}
        .provider-model-filters {{ display: grid; grid-template-columns: minmax(0, 1fr) 9rem; gap: .5rem; margin-bottom: .75rem; }}
        .provider-model-filters input, .provider-model-filters select {{ margin-top: 0 !important; }}
        .provider-model-columns {{ display: grid; grid-template-columns: 2.75rem minmax(8rem, 1.15fr) minmax(8rem, 1fr) 5rem 2.5rem 2.5rem; gap: .375rem; padding: 0 .375rem .375rem; color: var(--muted-foreground); font-size: .75rem; font-weight: 600; }}
        .provider-model-list {{ display: grid; gap: 0; border-top: 1px solid var(--border); }}
        .provider-model-row {{ display: grid; grid-template-columns: 2.75rem minmax(8rem, 1.15fr) minmax(8rem, 1fr) 5rem 2.5rem 2.5rem; grid-template-areas: 'drag identity mapping status expand delete' 'stats stats stats stats stats stats' 'details details details details details details'; align-items: center; gap: .375rem; min-width: 0; padding: .375rem; border: 0; border-bottom: 1px solid var(--border); border-radius: 0; }}
        .provider-model-row:hover {{ background: color-mix(in srgb, var(--muted) 45%, var(--card)); }}
        .provider-model-row > .drag-handle {{ grid-area: drag; }}
        .provider-model-identity {{ grid-area: identity; min-width: 0; }}
        .provider-model-mapping {{ grid-area: mapping; min-width: 0; }}
        .provider-model-status {{ grid-area: status; }}
        .provider-model-expand {{ grid-area: expand; }}
        .provider-row-delete {{ grid-area: delete; }}
        .provider-recent-controls {{ display: flex; flex-wrap: wrap; align-items: center; gap: .5rem .75rem; margin-bottom: .75rem; }}
        .provider-recent-controls label {{ display: flex; align-items: center; gap: .5rem; }}
        .provider-recent-controls select {{ width: auto; margin: 0; }}
        .provider-recent-status {{ color: var(--muted-foreground); font-size: .75rem; }}
        .provider-model-stats {{ grid-area: stats; display: flex; flex-wrap: wrap; gap: .375rem 1.25rem; padding: .25rem .375rem .375rem 3.125rem; color: var(--muted-foreground); font-size: .75rem; font-variant-numeric: tabular-nums; }}
        .provider-model-stats strong {{ color: var(--foreground); font-weight: 500; }}
        .provider-recent-note {{ color: var(--muted-foreground); font-size: .75rem; margin: 0 0 .75rem; }}
        .provider-model-details {{ grid-area: details; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: .75rem; margin-top: .25rem; padding: .75rem; border-top: 1px solid var(--border); }}
        .provider-model-aliases {{ min-width: 0; overflow-wrap: anywhere; color: var(--muted-foreground); font-size: .75rem; }}
        .provider-model-identity label, .provider-model-mapping label {{ display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
        .provider-model-identity input, .provider-model-mapping select {{ margin-top: .25rem !important; }}
        .provider-model-identity input {{ font-family: ui-monospace, monospace; }}
        .provider-model-status {{ margin-top: .9rem !important; padding: 0 .25rem !important; }}
        .provider-row-action {{ display: inline-flex; width: 2.5rem; height: 2.5rem; align-items: center; justify-content: center; margin-top: .9rem; border: 0; border-radius: 6px; background: transparent; color: var(--muted-foreground); }}
        .provider-row-action:hover {{ background: var(--muted); color: var(--foreground); }}
        .provider-row-delete:hover {{ color: var(--destructive); }}
        .provider-model-expand[aria-expanded="true"] .icon {{ transform: rotate(180deg); }}
        .provider-match-badge {{ display: inline-block; max-width: 100%; margin-top: .25rem; padding: .125rem .375rem; border-radius: 4px; background: var(--muted); color: var(--muted-foreground); font-size: .75rem; white-space: nowrap; }}
        .provider-match-badge[data-kind="exact"] {{ background: color-mix(in srgb, var(--success) 13%, var(--card)); color: var(--success); }}
        .provider-match-badge[data-kind="manual"] {{ background: color-mix(in srgb, var(--primary) 13%, var(--card)); color: var(--primary); }}
        .provider-match-badge[data-kind="unmatched"] {{ background: var(--muted); color: var(--muted-foreground); }}
        .provider-model-empty {{ padding: 2rem; text-align: center; color: var(--muted-foreground); font-size: .8125rem; }}
        @media (min-width: 901px) {{
            .provider-model-row {{ padding: .25rem .375rem; }}
            .provider-model-identity {{ display: flex; align-items: center; gap: .375rem; }}
            .provider-model-identity label {{ flex: 1; min-width: 0; font-size: 0; }}
            .provider-model-mapping label {{ font-size: 0; }}
            .provider-model-identity input, .provider-model-mapping select, .provider-model-status {{ margin-top: 0 !important; }}
            .provider-match-badge {{ flex: none; margin-top: 0; }}
            .provider-row-action {{ margin-top: 0; }}
        }}
        .drag-handle {{ display: inline-flex; width: 44px; min-width: 44px; height: 44px; align-items: center; justify-content: center; border: 0; border-radius: 0.375rem; color: var(--muted-foreground); cursor: grab; touch-action: none; transition: background-color 150ms, color 150ms, opacity 150ms; }}
        .drag-handle:hover:not(:disabled) {{ background: var(--muted); color: var(--foreground); }}
        .drag-handle:focus-visible {{ outline: 2px solid var(--ring); outline-offset: 2px; }}
        .drag-handle:disabled {{ cursor: not-allowed; opacity: 0.32; }}
        [data-sortable-row].is-dragging {{ border-color: var(--ring); box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1); opacity: 0.72; }}
        [data-sortable-row].is-dragging .drag-handle {{ cursor: grabbing; }}
        @media (max-width: 900px) {{
            .provider-form-layout {{ grid-template-columns: 9rem minmax(0, 1fr); }}
            .provider-model-columns {{ display: none; }}
            .provider-model-row {{ grid-template-columns: 2.75rem minmax(0, 1fr) 5rem 2.5rem 2.5rem; grid-template-areas: 'drag identity identity expand delete' '. mapping status status status' 'stats stats stats stats stats' 'details details details details details'; }}
            .provider-model-status, .provider-row-action {{ margin-top: 0 !important; }}
            .provider-model-mapping label {{ white-space: normal; }}
        }}
        @media (max-width: 639px) {{
            .modal-content:has(.provider-dialog) {{ width: calc(100vw - 1rem); }}
            .provider-dialog {{ height: calc(100dvh - 2rem); }}
            .provider-form-layout {{ grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); }}
            .provider-form-nav {{ flex-direction: row; padding: .375rem; border-right: 0; border-bottom: 1px solid var(--border); }}
            .provider-form-nav button {{ flex: 1; justify-content: center; gap: .5rem; }}
            .provider-form-nav button.is-active {{ box-shadow: inset 0 -3px var(--primary); }}
            .provider-form-main {{ padding: 1rem .75rem; }}
            .provider-field-grid {{ grid-template-columns: minmax(0, 1fr); gap: .75rem; }}
            .provider-protocol-main {{ flex-wrap: wrap; gap: .25rem .5rem; }}
            .provider-protocol-name {{ min-width: 0 !important; flex: 1; }}
            .provider-protocol-main code {{ order: 3; width: 100%; margin-left: 1.5rem; }}
            .provider-model-toolbar {{ align-items: stretch; flex-direction: column; }}
            .provider-model-toolbar-actions button {{ flex: 1; }}
            .provider-model-filters {{ grid-template-columns: minmax(0, 1fr) 7.5rem; }}
            .provider-model-row {{ grid-template-columns: 2.75rem minmax(0, 1fr) 2.75rem 2.75rem; grid-template-areas: 'drag identity expand delete' '. mapping mapping mapping' '. status status status' 'stats stats stats stats' 'details details details details'; }}
            .provider-model-details {{ grid-template-columns: minmax(0, 1fr); }}
            .provider-row-action {{ width: 2.75rem; }}
            .provider-model-status {{ max-width: 8rem; }}
        }}
        @media (prefers-reduced-motion: reduce) {{ .drag-handle {{ transition: none; }} }}
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
            window.location.href = '/providers/create';
        }}

        function openEditModal(name) {{
            window.location.href = '/providers/' + encodeURIComponent(name) + '/settings';
        }}

        function openCopyModal(name) {{
            window.location.href = '/providers/' + encodeURIComponent(name) + '/duplicate';
        }}

        let orderSaving = false;
        let sortablePointer = null;

        function saveOrder(url, ids, successMessage) {{
            if (orderSaving) return;
            orderSaving = true;
            const orderHandles = Array.from(document.querySelectorAll('.drag-handle[data-persist-order]'));
            orderHandles.forEach(handle => handle.disabled = true);
            fetch(url, {{
                method: 'PUT',
                headers: {{ 'Content-Type': 'application/json' }},
                credentials: 'include',
                body: JSON.stringify({{ ids }})
            }})
            .then(async response => {{
                if (response.status === 401) {{
                    window.location.href = '/login';
                    return null;
                }}
                const result = await response.json();
                if (!response.ok) throw new Error(result?.error?.message || '调整顺序失败');
                return result;
            }})
            .then(result => {{
                if (!result) return;
                Toast.success(successMessage);
                refreshData('providers');
            }})
            .catch(error => {{
                Toast.error(error.message);
                refreshData('providers');
            }})
            .finally(() => {{
                orderSaving = false;
                orderHandles.forEach(handle => handle.disabled = false);
            }});
        }}

        function commitSortableOrder(kind) {{
            if (kind !== 'providers') return;
            const ids = Array.from(document.querySelectorAll('[data-provider-order-id]'))
                .map(row => row.dataset.providerOrderId);
            saveOrder('/v1/admin/providers/order', ids, '供应商顺序已更新');
        }}

        function beginSortablePointer(event, handle, kind) {{
            if (event.button !== 0 || handle.disabled || orderSaving) return;
            const row = handle.closest('[data-sortable-row]');
            if (!row) return;
            sortablePointer = {{
                pointerId: event.pointerId,
                handle,
                row,
                container: row.parentElement,
                startY: event.clientY,
                moved: false,
                kind
            }};
            handle.setPointerCapture(event.pointerId);
        }}

        function moveSortablePointer(event) {{
            const state = sortablePointer;
            if (!state || event.pointerId !== state.pointerId) return;
            if (!state.moved && Math.abs(event.clientY - state.startY) < 6) return;
            event.preventDefault();
            state.moved = true;
            state.row.classList.add('is-dragging');
            const target = document.elementFromPoint(event.clientX, event.clientY)?.closest('[data-sortable-row]');
            if (!target || target === state.row || target.parentElement !== state.container) return;
            const bounds = target.getBoundingClientRect();
            state.container.insertBefore(
                state.row,
                event.clientY < bounds.top + bounds.height / 2 ? target : target.nextSibling
            );
        }}

        function endSortablePointer(event) {{
            const state = sortablePointer;
            if (!state || event.pointerId !== state.pointerId) return;
            sortablePointer = null;
            state.row.classList.remove('is-dragging');
            if (state.handle.hasPointerCapture(event.pointerId)) {{
                state.handle.releasePointerCapture(event.pointerId);
            }}
            if (state.moved) commitSortableOrder(state.kind);
        }}

        function moveSortableWithKeyboard(event, handle, kind) {{
            if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
            event.preventDefault();
            const row = handle.closest('[data-sortable-row]');
            if (!row) return;
            const rows = Array.from(row.parentElement.children)
                .filter(candidate => candidate.matches('[data-sortable-row]'));
            const index = rows.indexOf(row);
            const targetIndex = index + (event.key === 'ArrowUp' ? -1 : 1);
            if (index < 0 || targetIndex < 0 || targetIndex >= rows.length) return;
            if (targetIndex < index) row.parentElement.insertBefore(row, rows[targetIndex]);
            else row.parentElement.insertBefore(row, rows[targetIndex].nextSibling);
            handle.focus();
            commitSortableOrder(kind);
        }}

        function removeFormRow(button) {{
            const row = button.closest('[data-order-row]');
            if (!row) return;
            const modelRow = row.matches('[data-model-row]');
            row.remove();
            if (modelRow) filterProviderModelRows();
        }}

        function prepareListContainer(container) {{
            container.querySelectorAll('[data-empty-state]').forEach(node => node.remove());
        }}

        function renumberFormRows(containerId, prefix) {{
            const container = document.getElementById(containerId);
            Array.from(container.querySelectorAll(':scope > [data-order-row]')).forEach((row, index) => {{
                row.querySelectorAll('[name]').forEach(field => {{
                    field.name = field.name.replace(
                        new RegExp('^' + prefix + '\\[\\d+\\]'),
                        prefix + '[' + index + ']'
                    );
                }});
            }});
        }}

        function prepareProviderFormOrder() {{
            renumberFormRows('headers-container', 'headers');
            renumberFormRows('models-container', 'models');
        }}

        function showProviderSection(section) {{
            document.querySelectorAll('[data-provider-section]').forEach(button => {{
                const active = button.dataset.providerSection === section;
                button.classList.toggle('is-active', active);
                if (active) button.setAttribute('aria-current', 'page');
                else button.removeAttribute('aria-current');
            }});
            document.getElementById('provider-basic-panel').hidden = section !== 'basic';
            document.getElementById('provider-models-panel').hidden = section !== 'models';
            document.querySelector('.provider-form-main')?.scrollTo(0, 0);
            if (section === 'models') loadProviderRecentStats();
        }}

        function providerBaseValue() {{
            return document.querySelector('.provider-form [name="endpoint_base_url"]')?.value.trim().replace(/\/+$/, '') || '';
        }}

        function joinProviderEndpoint(base, path) {{
            if (!base) return '';
            if (base.endsWith(path)) return base;
            return base + (base.endsWith('/v1') ? path.slice(3) : path);
        }}

        function normalizeProviderBaseUrl() {{
            const input = document.querySelector('.provider-form [name="endpoint_base_url"]');
            if (!input) return;
            const value = providerBaseValue();
            const paths = Array.from(document.querySelectorAll('[data-provider-protocol]'))
                .map(row => row.dataset.defaultPath).sort((a, b) => b.length - a.length);
            const path = paths.find(candidate => value.endsWith(candidate));
            input.value = path ? value.slice(0, -path.length) + '/v1' : value;
            refreshProviderProtocolRows();
        }}

        function refreshProviderProtocolRows() {{
            const base = providerBaseValue();
            document.querySelectorAll('[data-provider-protocol]').forEach(row => {{
                const enabled = row.querySelector('[data-protocol-enabled]').checked;
                const custom = row.querySelector('[data-protocol-custom-toggle]').checked;
                const input = row.querySelector('[data-protocol-custom-url]');
                const generated = joinProviderEndpoint(base, row.dataset.defaultPath);
                row.classList.toggle('is-disabled', !enabled);
                row.querySelector('[data-protocol-custom-toggle]').disabled = !enabled;
                row.querySelector('[data-protocol-preview]').textContent = custom ? (input.value.trim() || '填写自定义地址') : (generated || '填写 Base URL 后自动生成');
                input.hidden = !custom || !enabled;
                input.disabled = !custom || !enabled;
            }});
        }}

        function updateProviderProtocolRow(control) {{
            const row = control.closest('[data-provider-protocol]');
            const custom = row.querySelector('[data-protocol-custom-toggle]');
            const input = row.querySelector('[data-protocol-custom-url]');
            if (control === custom && custom.checked && !input.value.trim()) {{
                input.value = joinProviderEndpoint(providerBaseValue(), row.dataset.defaultPath);
            }}
            refreshProviderProtocolRows();
            if (control === custom && custom.checked) input.focus();
        }}

        function collectProviderEndpoints() {{
            const base = providerBaseValue();
            return Array.from(document.querySelectorAll('[data-provider-protocol]'))
                .filter(row => row.querySelector('[data-protocol-enabled]').checked)
                .map(row => ({{
                    protocol: row.dataset.providerProtocol,
                    base_url: row.querySelector('[data-protocol-custom-toggle]').checked
                        ? row.querySelector('[data-protocol-custom-url]').value.trim()
                        : joinProviderEndpoint(base, row.dataset.defaultPath)
                }}));
        }}

        function updateProviderModelMatch(control, refresh = true) {{
            const row = control.closest('[data-model-row]');
            if (!row) return;
            const badge = row.querySelector('[data-model-match]');
            const binding = row.querySelector('[name$="[platform_model]"]');
            const id = row.querySelector('[name$="[name]"]').value.trim();
            if (!binding || !id) {{
                row.dataset.match = 'unmatched';
                badge.hidden = true;
            }} else {{
                const manual = Boolean(binding.value);
                const exact = Array.from(binding.options).some(option => option.value === id);
                row.dataset.match = manual || exact ? 'matched' : 'unmatched';
                badge.textContent = manual ? '手动匹配' : (exact ? '同名匹配' : '未匹配');
                badge.dataset.kind = manual ? 'manual' : (exact ? 'exact' : 'unmatched');
                badge.hidden = false;
            }}
            renderProviderModelStats(row);
            if (refresh) filterProviderModelRows();
        }}

        function filterProviderModelRows() {{
            const search = document.getElementById('provider-model-search')?.value.trim().toLocaleLowerCase() || '';
            const filter = document.getElementById('provider-model-filter')?.value || 'all';
            const rows = Array.from(document.querySelectorAll('#models-container [data-model-row]'));
            let visible = 0;
            let matched = 0;
            rows.forEach(row => {{
                const name = row.querySelector('[name$="[name]"]')?.value.toLocaleLowerCase() || '';
                const binding = row.querySelector('[name$="[platform_model]"]')?.value.toLocaleLowerCase() || '';
                const isMatched = row.dataset.match === 'matched';
                if (isMatched) matched++;
                row.hidden = Boolean((search && !name.includes(search) && !binding.includes(search)) || (filter !== 'all' && filter !== row.dataset.match));
                if (!row.hidden) visible++;
            }});
            document.getElementById('provider-model-summary').textContent = `${{rows.length}} 个模型 · ${{matched}} 个已匹配`;
            document.getElementById('provider-model-nav-count').textContent = rows.length;
            document.getElementById('provider-model-empty').hidden = visible !== 0;
        }}

        function toggleProviderModelDetails(button) {{
            const details = button.closest('[data-model-row]').querySelector('.provider-model-details');
            details.hidden = !details.hidden;
            button.setAttribute('aria-expanded', String(!details.hidden));
            button.setAttribute('aria-label', details.hidden ? '展开模型设置' : '收起模型设置');
        }}

        function submitProviderForm(event, method, url) {{
            event.preventDefault();
            const form = event.target;
            normalizeProviderBaseUrl();
            for (const field of [form.elements.name, form.elements.api_key, form.elements.endpoint_base_url]) {{
                if (!field.checkValidity()) {{
                    showProviderSection('basic');
                    field.reportValidity();
                    return;
                }}
            }}
            const endpoints = collectProviderEndpoints();
            if (!endpoints.length) {{
                showProviderSection('basic');
                Toast.warning('至少启用一个协议');
                return;
            }}
            for (const row of document.querySelectorAll('[data-provider-protocol]')) {{
                const input = row.querySelector('[data-protocol-custom-url]');
                if (!input.disabled && (!input.value.trim() || !input.checkValidity())) {{
                    showProviderSection('basic');
                    input.focus();
                    input.reportValidity();
                    return;
                }}
            }}
            for (const row of form.querySelectorAll('[data-model-row]')) {{
                const invalid = Array.from(row.querySelectorAll('input, select')).find(field => !field.checkValidity());
                if (invalid) {{
                    showProviderSection('models');
                    row.hidden = false;
                    if (invalid.closest('.provider-model-details')) {{
                        row.querySelector('.provider-model-details').hidden = false;
                    }}
                    invalid.reportValidity();
                    return;
                }}
            }}
            prepareProviderFormOrder();
            const formData = new FormData(form);
            const data = {{}};

            const models = [];
            const headers = [];

            for (let [key, value] of formData.entries()) {{
                if (key.startsWith('models[')) {{
                    const match = key.match(/models\[(\d+)\]\[(\w+)\]/);
                    if (match) {{
                        if (!models[match[1]]) models[match[1]] = {{}};
                        models[match[1]][match[2]] = value;
                    }}
                }} else if (key.startsWith('headers[')) {{
                    const match = key.match(/headers\[(\d+)\]\[(\w+)\]/);
                    if (match) {{
                        if (!headers[match[1]]) headers[match[1]] = {{}};
                        headers[match[1]][match[2]] = value;
                    }}
                }} else {{
                    data[key] = value;
                }}
            }}

            delete data.endpoint_base_url;
            data.endpoints = endpoints;
            const priority = Number.parseInt(data.priority, 10);
            data.priority = Number.isFinite(priority) ? priority : 0;
            data.model_passthrough = data.model_passthrough === 'true';
            data.models = models.filter(Boolean).map(m => {{
                const hasPricing = m.input_price !== '' || m.output_price !== '';
                return {{
                    name: m.name,
                    status: m.status,
                    aliases: m.aliases ? m.aliases.split(',').map(s => s.trim()).filter(Boolean) : [],
                    platform_model: m.platform_model || null,
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
                return response.text().then(text => {{
                    try {{
                        return JSON.parse(text);
                    }} catch (_) {{
                        throw new Error(text || ('HTTP ' + response.status));
                    }}
                }});
            }})
            .then(result => {{
                if (!result) return;
                if (result.error) {{
                    Toast.error(result.error.message || '保存失败');
                }} else {{
                    Toast.success(method === 'POST' ? '供应商注册成功' : '供应商更新成功');
                    if (document.querySelector('.provider-detail')) {{
                        form.dataset.saved = 'true';
                        window.location.href = '/providers';
                    }} else {{ Modal.close(); refreshData('providers'); }}
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

        async function toggleProviderStatus(name, currentStatus, button) {{
            if(button?.getAttribute('aria-busy')==='true')return;
            const nextStatus=currentStatus==='enabled'?'disabled':'enabled';
            if(button)button.setAttribute('aria-busy','true');
            try {{
                const response=await fetch('/v1/admin/providers/'+encodeURIComponent(name)+'/status',{{method:'PUT',headers:{{'Content-Type':'application/json'}},credentials:'include',body:JSON.stringify({{status:nextStatus}})}});
                if(response.status===401)return redirectToLogin();
                const result=await response.json();
                if(!response.ok)throw new Error(result.error?.message||'更新状态失败');
                if(button) {{
                    button.textContent=nextStatus==='enabled'?'启用':'停用';
                    button.setAttribute('aria-label',(nextStatus==='enabled'?'停用':'启用')+'供应商 '+name);
                    button.onclick=()=>toggleProviderStatus(name,nextStatus,button);
                }}
                Toast.success('供应商状态已更新');
            }}catch(error){{Toast.error(error.message);}}
            finally{{if(button)button.setAttribute('aria-busy','false');}}
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

    let mut html = String::from(
        r#"<table class="provider-list-table"><thead><tr><th><span class="sr-only">排序</span></th><th>供应商</th><th>Base URL</th><th>模型 / 优先级</th><th>状态</th><th style="text-align:right">操作</th></tr></thead><tbody>"#,
    );
    for provider in &all_providers {
        let name = escape_html(&provider.name);
        let name_js = escape_inline_js_string(&provider.name);
        let status_js = escape_inline_js_string(&provider.status);
        let enabled = provider.status == "enabled";
        let base = provider
            .endpoints
            .first()
            .map(|endpoint| {
                endpoint
                    .base_url
                    .strip_suffix(endpoint.protocol.default_path())
                    .map(|prefix| format!("{prefix}/v1"))
                    .unwrap_or_else(|| endpoint.base_url.clone())
            })
            .unwrap_or_default();
        let base = escape_html(&base);
        let status = if enabled { "启用" } else { "停用" };
        let toggle = if enabled { "停用" } else { "启用" };
        let models = provider.models.len();
        let priority = provider.priority;
        html.push_str(&format!(r#"<tr data-sortable-row data-provider-order-id="{name}" data-row-action="openEditModal" data-row-value="{name}" tabindex="0" aria-label="查看供应商 {name}">
            <td><button type="button" class="drag-handle" data-persist-order title="拖动调整顺序" aria-label="拖动调整供应商 {name}" onpointerdown="beginSortablePointer(event, this, 'providers')" onpointermove="moveSortablePointer(event)" onpointerup="endSortablePointer(event)" onpointercancel="endSortablePointer(event)" onkeydown="moveSortableWithKeyboard(event, this, 'providers')"><svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M9 5h.01M15 5h.01M9 12h.01M15 12h.01M9 19h.01M15 19h.01" stroke-width="3"/></svg></button></td>
            <td><button type="button" class="provider-list-name" style="border:0;background:transparent;padding:0" onclick="openEditModal({name_js})">{name}</button></td>
            <td><span class="provider-list-url" title="{base}">{base}</span></td>
            <td class="provider-list-secondary">{models} 个模型 · P{priority}</td>
            <td><button type="button" class="outline-button" onclick="toggleProviderStatus({name_js}, {status_js}, this)" aria-label="{toggle}供应商 {name}">{status}</button></td>
            <td><div class="provider-list-actions">
                <button type="button" class="icon-button" onclick="openEditModal({name_js})" title="编辑供应商" aria-label="编辑供应商 {name}"><svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="m16 3 5 5-12 12-6 1 1-6L16 3ZM14 5l5 5"/></svg></button>
                <button type="button" class="icon-button" onclick="openCopyModal({name_js})" title="复制供应商" aria-label="复制供应商 {name}"><svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V4H4v12h4"/></svg></button>
                <button type="button" class="icon-button" onclick="deleteProvider({name_js})" title="删除供应商" aria-label="删除供应商 {name}"><svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7"/></svg></button>
            </div></td></tr>"#));
    }
    html.push_str("</tbody></table>");

    Ok(trusted_html(html))
}

#[page("/providers/create")]
pub async fn provider_create_page(cx: &Cx) -> Result {
    let form = render_provider_new(cx).await?;
    render_provider_detail(cx, "新增供应商", form).await
}

#[page("/providers/{provider_name}/settings")]
pub async fn provider_settings_page(cx: &Cx) -> Result {
    let name = path_param::<ProviderName>(cx);
    let form = render_provider_edit(cx).await?;
    render_provider_detail(cx, &format!("供应商 · {name}"), form).await
}

#[page("/providers/{provider_name}/duplicate")]
pub async fn provider_duplicate_page(cx: &Cx) -> Result {
    let form = render_provider_copy(cx).await?;
    render_provider_detail(cx, "复制供应商", form).await
}

async fn render_provider_detail(cx: &Cx, title: &str, form: View) -> Result {
    let body = trusted_html(format!(
        "<div class=\"provider-detail\">{}</div>",
        form.render(cx)
    ));
    render_providers(cx, Some((title, body))).await
}

#[page("/providers/new")]
pub async fn providers_new(cx: &Cx) -> Result {
    render_provider_new(cx).await
}

async fn render_provider_new(cx: &Cx) -> Result {
    require_admin(cx)?;
    let snapshot = crate::topcoat_admin::app::app_state(cx)
        .config_service
        .snapshot();
    let empty_headers = IndexMap::new();
    let html = render_provider_form(
        ProviderForm {
            name: "",
            api_key: "",
            status: "enabled",
            priority: 0,
            model_passthrough: false,
            endpoints: &[],
            models: &[],
            headers: &empty_headers,
            method: "POST",
            url: "/v1/admin/providers",
            name_readonly: false,
        },
        &snapshot.raw_config.model_catalog,
        snapshot.raw_config.model_catalog_enabled,
    );
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
    render_provider_edit(cx).await
}

async fn render_provider_edit(cx: &Cx) -> Result {
    require_admin(cx)?;
    let snapshot = crate::topcoat_admin::app::app_state(cx)
        .config_service
        .snapshot();
    let name = path_param::<ProviderName>(cx);
    let all_providers = fetch_providers(cx).await;
    let Some(provider) = all_providers.iter().find(|provider| provider.name == name) else {
        return render_missing_provider(cx, "编辑供应商", name).await;
    };
    let update_url = format!("/v1/admin/providers/{}", name);
    let html = render_provider_form(
        ProviderForm::from_provider(provider, &provider.name, "PUT", &update_url, true),
        &snapshot.raw_config.model_catalog,
        snapshot.raw_config.model_catalog_enabled,
    );
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
    render_provider_copy(cx).await
}

async fn render_provider_copy(cx: &Cx) -> Result {
    require_admin(cx)?;
    let snapshot = crate::topcoat_admin::app::app_state(cx)
        .config_service
        .snapshot();
    let name = path_param::<ProviderName>(cx);
    let all_providers = fetch_providers(cx).await;
    let Some(provider) = all_providers.iter().find(|provider| provider.name == name) else {
        return render_missing_provider(cx, "复制供应商", name).await;
    };
    let html = render_provider_form(
        ProviderForm::from_provider(provider, "", "POST", "/v1/admin/providers", false),
        &snapshot.raw_config.model_catalog,
        snapshot.raw_config.model_catalog_enabled,
    );
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
    model_passthrough: bool,
    endpoints: &'a [EndpointConfig],
    models: &'a [ModelRule],
    headers: &'a IndexMap<String, String>,
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
            model_passthrough: provider.model_passthrough,
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

fn render_model_binding_input(
    index: &str,
    selected: Option<&str>,
    catalog: &[PlatformModel],
) -> String {
    if catalog.is_empty() {
        return String::new();
    }
    let mut options = String::from(r#"<option value="">按模型 ID 自动匹配</option>"#);
    for model in catalog {
        options.push_str(&format!(
            r#"<option value="{}" {}>{}</option>"#,
            escape_html(&model.name),
            if selected == Some(model.name.as_str()) {
                "selected"
            } else {
                ""
            },
            escape_html(&model.name),
        ));
    }
    format!(
        r#"<label>平台模型<select name="models[{index}][platform_model]" onchange="updateProviderModelMatch(this)">{options}</select></label>"#,
    )
}

fn infer_provider_base_url(endpoints: &[EndpointConfig]) -> String {
    for endpoint in endpoints {
        if let Some(prefix) = endpoint
            .base_url
            .trim_end_matches('/')
            .strip_suffix(endpoint.protocol.default_path())
        {
            return format!("{prefix}/v1");
        }
    }
    endpoints
        .first()
        .and_then(|endpoint| reqwest::Url::parse(&endpoint.base_url).ok())
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_default()
}

fn render_protocol_rows(endpoints: &[EndpointConfig], base_url: &str) -> String {
    let mut protocols: Vec<ProviderProtocol> = endpoints.iter().map(|ep| ep.protocol).collect();
    for protocol in ProviderProtocol::ALL {
        if !protocols.contains(&protocol) {
            protocols.push(protocol);
        }
    }
    protocols
        .into_iter()
        .map(|protocol| {
            let endpoint = endpoints.iter().find(|ep| ep.protocol == protocol);
            let generated = join_provider_endpoint(base_url, protocol);
            let custom = endpoint.is_some_and(|ep| ep.base_url != generated);
            let enabled = endpoint.is_some()
                || (endpoints.is_empty() && protocol == ProviderProtocol::Completions);
            format!(
                r#"<div class="provider-protocol-row" data-provider-protocol="{protocol}" data-default-path="{default_path}">
                    <div class="provider-protocol-main"><label class="provider-protocol-name"><input type="checkbox" data-protocol-enabled onchange="updateProviderProtocolRow(this)" {enabled}><span>{protocol}</span></label><code>{default_path}</code><label class="provider-protocol-custom-label"><input type="checkbox" data-protocol-custom-toggle onchange="updateProviderProtocolRow(this)" {custom}>自定义地址</label></div>
                    <div class="provider-protocol-url" data-protocol-preview>{generated}</div>
                    <input type="url" data-protocol-custom-url value="{actual_url}" placeholder="https://api.example.com/v1/{protocol}" aria-label="{protocol} 自定义地址" oninput="refreshProviderProtocolRows()" {custom_hidden}>
                </div>"#,
                default_path = protocol.default_path(),
                enabled = if enabled { "checked" } else { "" },
                custom = if custom { "checked" } else { "" },
                generated = escape_html(&generated),
                actual_url = endpoint.map(|ep| escape_html(&ep.base_url)).unwrap_or_default(),
                custom_hidden = if custom { "" } else { "hidden" },
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_model_row(
    index: &str,
    model: Option<&ModelRule>,
    catalog: &[PlatformModel],
    catalog_enabled: bool,
) -> String {
    let name = model.map(|model| model.name.as_str()).unwrap_or_default();
    let aliases = model
        .map(|model| model.aliases.join(", "))
        .unwrap_or_default();
    let pricing = model.and_then(|model| model.pricing.as_ref());
    let input_price = pricing
        .map(|pricing| pricing.input_per_1k.to_string())
        .unwrap_or_default();
    let output_price = pricing
        .map(|pricing| pricing.output_per_1k.to_string())
        .unwrap_or_default();
    let binding = render_model_binding_input(
        index,
        model.and_then(|model| model.platform_model.as_deref()),
        catalog,
    );
    let alias_input = if catalog_enabled {
        format!(
            r#"<div class="provider-model-aliases">当前别名: {}</div><input type="hidden" name="models[{index}][aliases]" value="{}">"#,
            if aliases.is_empty() {
                "无".to_string()
            } else {
                escape_html(&aliases)
            },
            escape_html(&aliases)
        )
    } else {
        format!(
            r#"<label>别名<input type="text" name="models[{index}][aliases]" value="{}" placeholder="多个别名以逗号分隔"></label>"#,
            escape_html(&aliases)
        )
    };
    let status = model
        .map(|model| model.status.as_str())
        .unwrap_or("enabled");
    format!(
        r#"<div class="provider-model-row" data-order-row data-sortable-row data-model-row data-observed-model="{name}" data-row-action="toggleProviderModelRow" tabindex="0" aria-label="查看模型设置">
            <button type="button" class="drag-handle" title="拖动调整顺序" aria-label="拖动调整模型" onpointerdown="beginSortablePointer(event, this, 'form')" onpointermove="moveSortablePointer(event)" onpointerup="endSortablePointer(event)" onpointercancel="endSortablePointer(event)" onkeydown="moveSortableWithKeyboard(event, this, 'form')"><svg class="h-5 w-5" aria-hidden="true" viewBox="0 0 24 24" fill="currentColor"><circle cx="9" cy="5" r="1.5"/><circle cx="15" cy="5" r="1.5"/><circle cx="9" cy="12" r="1.5"/><circle cx="15" cy="12" r="1.5"/><circle cx="9" cy="19" r="1.5"/><circle cx="15" cy="19" r="1.5"/></svg></button>
            <div class="provider-model-identity"><label>上游模型 ID<input type="text" name="models[{index}][name]" value="{name}" placeholder="gpt-4o-mini" required oninput="updateProviderModelMatch(this)"></label><span class="provider-match-badge" data-model-match></span></div>
            <div class="provider-model-mapping">{binding}</div>
            <select class="provider-model-status" name="models[{index}][status]" aria-label="模型状态"><option value="enabled" {enabled_selected}>启用</option><option value="disabled" {disabled_selected}>禁用</option></select>
            <button type="button" class="provider-row-action provider-model-expand" onclick="toggleProviderModelDetails(this)" aria-label="展开模型设置" title="价格与别名" aria-expanded="false"><svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg></button>
            <button type="button" class="provider-row-action provider-row-delete" onclick="removeFormRow(this)" aria-label="删除模型" title="删除模型"><svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M3 6h18M8 6V4h8v2m3 0-1 14H6L5 6m5 4v7m4-7v7"/></svg></button>
            <div class="provider-model-stats" data-model-stats><span data-stat-empty>选择模型页后加载近期统计</span></div>
            <div class="provider-model-details" hidden>{alias_input}<label>输入价格 / 1K<input type="number" min="0" step="any" name="models[{index}][input_price]" value="{input_price}" placeholder="0.005"></label><label>输出价格 / 1K<input type="number" min="0" step="any" name="models[{index}][output_price]" value="{output_price}" placeholder="0.015"></label></div>
        </div>"#,
        name = escape_html(name),
        enabled_selected = if status == "enabled" { "selected" } else { "" },
        disabled_selected = if status == "enabled" { "" } else { "selected" },
    )
}

fn render_provider_form(
    form: ProviderForm<'_>,
    catalog: &[PlatformModel],
    catalog_enabled: bool,
) -> String {
    let ProviderForm {
        name,
        api_key,
        status,
        priority,
        model_passthrough,
        endpoints,
        models,
        headers,
        method,
        url,
        name_readonly,
    } = form;

    let base_url = infer_provider_base_url(endpoints);
    let endpoints_html = render_protocol_rows(endpoints, &base_url);
    let models_html = models
        .iter()
        .enumerate()
        .map(|(index, model)| {
            render_model_row(&index.to_string(), Some(model), catalog, catalog_enabled)
        })
        .collect::<Vec<_>>()
        .join("");
    let new_model_row_html = render_model_row("__INDEX__", None, catalog, catalog_enabled);

    let headers_html = if headers.is_empty() {
        r#"<div data-empty-state class="rounded-lg border border-dashed border-zinc-300 py-5 text-center text-xs text-zinc-500">未配置自定义 Header</div>"#.to_string()
    } else {
        headers.iter().enumerate().map(|(i, (k, v))| {
            format!(r#"<div data-order-row data-sortable-row class="flex gap-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
                <button type="button" class="drag-handle self-center" title="拖动调整顺序" aria-label="拖动调整 Header" onpointerdown="beginSortablePointer(event, this, 'form')" onpointermove="moveSortablePointer(event)" onpointerup="endSortablePointer(event)" onpointercancel="endSortablePointer(event)" onkeydown="moveSortableWithKeyboard(event, this, 'form')">
                    <svg class="h-5 w-5" aria-hidden="true" viewBox="0 0 24 24" fill="currentColor"><circle cx="9" cy="5" r="1.5"/><circle cx="15" cy="5" r="1.5"/><circle cx="9" cy="12" r="1.5"/><circle cx="15" cy="12" r="1.5"/><circle cx="9" cy="19" r="1.5"/><circle cx="15" cy="19" r="1.5"/></svg>
                </button>
                <div class="grid min-w-0 flex-1 grid-cols-12 gap-2">
                    <div class="col-span-4">
                        <label class="mb-1 block text-[10px] text-zinc-500">Header 名称</label>
                        <input type="text" name="headers[{}][name]" value="{}" placeholder="Authorization" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-sm">
                    </div>
                    <div class="col-span-8">
                        <label class="mb-1 block text-[10px] text-zinc-500">Header 值</label>
                        <input type="text" name="headers[{}][value]" value="{}" placeholder="Bearer ..." class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-sm">
                    </div>
                </div>
                <button type="button" onclick="removeFormRow(this)" class="inline-flex min-h-[44px] min-w-[44px] self-end items-center justify-center rounded border border-red-200 text-red-600 hover:bg-red-50" title="删除 Header" aria-label="删除 Header">
                    <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                </button>
            </div>"#, i, escape_html(k), i, escape_html(v))
        }).collect::<Vec<_>>().join("")
    };

    format!(
        r##"<form onsubmit="submitProviderForm(event, {method_js}, {url_js})" class="provider-form" data-stats-provider="{stats_provider}" novalidate>
        <template id="new-model-row">{new_model_row_html}</template>
        <div class="provider-form-layout">
            <nav class="provider-form-nav" aria-label="供应商设置">
                <button type="button" class="is-active" data-provider-section="basic" onclick="showProviderSection('basic')">基础信息</button>
                <button type="button" data-provider-section="models" onclick="showProviderSection('models')">模型配置 <span id="provider-model-nav-count">{model_count}</span></button>
            </nav>
            <div class="provider-form-main">
                <section id="provider-basic-panel" class="provider-panel">
                    <div class="provider-panel-heading"><h3>基础信息</h3></div>
                    <div class="provider-field-grid">
                        <label>供应商名称 *<input type="text" name="name" value="{name_html}" placeholder="midcode" required maxlength="64" pattern="[\p{{L}}\p{{N}}][\p{{L}}\p{{N}}._\-]*" title="以字母或数字开头，可包含中文、字母、数字、点、下划线和连字符（最多 64 字符）" {name_readonly}></label>
                        <label>API Key *<input type="password" name="api_key" value="{api_key_html}" placeholder="sk-..." required autocomplete="off"></label>
                    </div>
                    <div class="provider-section-block">
                        <div class="provider-section-heading"><h4>协议地址</h4></div>
                        <label class="provider-base-field">Base URL *<input type="url" name="endpoint_base_url" value="{base_url_html}" placeholder="https://api.example.com/v1" required oninput="refreshProviderProtocolRows()" onblur="normalizeProviderBaseUrl()"></label>
                        <div id="endpoints-container" class="provider-protocol-list">{endpoints_html}</div>
                    </div>
                    <div class="provider-field-grid">
                        <label>优先级<input type="number" name="priority" value="{priority}"></label>
                        <label>供应商状态<select name="status"><option value="enabled" {enabled_selected}>启用</option><option value="disabled" {disabled_selected}>禁用</option></select></label>
                    </div>
                    <div class="provider-section-block">
                        <input type="hidden" name="model_passthrough" value="false">
                        <label class="provider-passthrough"><input type="checkbox" name="model_passthrough" value="true" {model_passthrough_checked} {model_passthrough_disabled}><span><strong>允许模型透传</strong><small>{model_passthrough_description}</small></span></label>
                    </div>
                    <details class="provider-headers"><summary>自定义 Headers <span>{header_count}</span></summary>
                        <div class="provider-headers-body"><button type="button" class="outline-button" onclick="addHeaderRow()">添加 Header</button><div id="headers-container" class="space-y-2">{headers_html}</div></div>
                    </details>
                </section>
                <section id="provider-models-panel" class="provider-panel" hidden>
                    <div class="provider-model-toolbar"><div><h3>模型配置</h3><span id="provider-model-summary"></span></div><div class="provider-model-toolbar-actions"><button type="button" class="outline-button" onclick="discoverProviderModels()">获取上游模型</button><button type="button" class="outline-button" onclick="addModelRow()">添加模型</button></div></div>
                    <div class="provider-model-filters"><input id="provider-model-search" type="search" placeholder="搜索模型 ID 或平台模型" aria-label="搜索模型" oninput="filterProviderModelRows()"><select id="provider-model-filter" aria-label="筛选匹配状态" onchange="filterProviderModelRows()"><option value="all">全部模型</option><option value="matched">已匹配</option><option value="unmatched">未匹配</option></select></div>
                    <div class="provider-recent-controls"><label for="provider-recent-window">近期统计 <select id="provider-recent-window" onchange="loadProviderRecentStats(true)"><option value="5">最近 5 分钟</option><option value="15" selected>最近 15 分钟</option><option value="60">最近 1 小时</option><option value="1440">最近 24 小时</option></select></label><button id="provider-recent-refresh" type="button" class="outline-button" onclick="loadProviderRecentStats()">刷新统计</button><span id="provider-recent-status" class="provider-recent-status" role="status"></span></div>
                    <p class="provider-recent-note">按本供应商的上游模型统计，每 30 秒刷新。首包显示 P50 / P95；TPS 显示 P50 / P10，是有效流式请求的估算值。</p>
                    <div id="upstream-preview" class="upstream-preview" hidden></div>
                    <div class="provider-model-columns" aria-hidden="true"><span></span><span>上游模型 ID / 匹配</span><span>平台模型</span><span>状态</span></div>
                    <div id="models-container" class="provider-model-list">{models_html}</div>
                    <div id="provider-model-empty" class="provider-model-empty" hidden>没有匹配的模型</div>
                </section>
            </div>
        </div>
        <div class="provider-form-footer"><button type="button" class="outline-button" onclick="leaveProviderForm()">取消</button><button type="submit" class="primary-button">保存配置</button></div>
    </form>

    <script>
        {recent_scripts}
        var modelIndex = 100;
        var headerIndex = 100;

        async function discoverProviderModels() {{
            const form = document.querySelector('.provider-form');
            const endpoint = collectProviderEndpoints()[0]?.base_url;
            const apiKey = form.elements.api_key.value.trim();
            if (!endpoint || !apiKey) return Toast.warning('先填写 Base URL、启用协议并填写 API Key');
            const headers = {{}};
            form.querySelectorAll('#headers-container [data-order-row]').forEach(row => {{
                const name = row.querySelector('[name$="[name]"]')?.value.trim();
                const value = row.querySelector('[name$="[value]"]')?.value.trim();
                if (name && value) headers[name] = value;
            }});
            try {{
                const response = await fetch('/v1/admin/providers/upstream-models', {{
                    method: 'POST', credentials: 'include',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ origin: endpoint, api_key: apiKey, headers }})
                }});
                const result = await response.json();
                if (!response.ok) throw new Error(result.error?.message || '获取上游模型失败');
                showUpstreamPreview(result.models);
            }} catch (error) {{ Toast.error(error.message); }}
        }}

        var upstreamModels = [];
        var upstreamSelected = new Set();
        function showUpstreamPreview(models) {{
            const existing = new Set(Array.from(document.querySelectorAll('#models-container [name$="[name]"]')).map(input=>input.value));
            upstreamModels = models.map(id=>({{id,existing:existing.has(id)}}));
            upstreamSelected = new Set();
            const preview = document.getElementById('upstream-preview');
            preview.hidden = false;
            preview.innerHTML = '<div class="upstream-preview-tools"><input type="search" id="upstream-search" placeholder="搜索上游模型" aria-label="搜索上游模型" oninput="renderUpstreamChoices()"><select id="upstream-scope" aria-label="筛选上游模型" onchange="renderUpstreamChoices()"><option value="new">只看新增</option><option value="unmatched">只看未匹配</option><option value="all">全部模型</option></select></div><div class="upstream-preview-tools"><button type="button" class="outline-button" onclick="selectUpstreamVisible()">选择搜索结果</button><button type="button" class="outline-button" onclick="upstreamSelected.clear();renderUpstreamChoices()">清空</button><button type="button" class="primary-button" id="import-upstream" onclick="importUpstreamSelected()">添加所选</button><button type="button" class="outline-button" onclick="document.getElementById(\'upstream-preview\').hidden=true">关闭</button></div><div id="upstream-choice-list" class="upstream-preview-list"></div>';
            renderUpstreamChoices();
        }}
        function visibleUpstreamModels() {{
            const search=document.getElementById('upstream-search').value.toLowerCase();
            const scope=document.getElementById('upstream-scope').value;
            return upstreamModels.filter(model=>model.id.toLowerCase().includes(search) && (scope==='all'||!model.existing) && (scope!=='unmatched'||!Array.from(document.querySelectorAll('[name$=\"[platform_model]\"] option')).some(item=>item.value===model.id)));
        }}
        function renderUpstreamChoices() {{
            const list=document.getElementById('upstream-choice-list');
            list.replaceChildren();
            for(const model of visibleUpstreamModels()) {{
                const label=document.createElement('label');
                const check=document.createElement('input');check.type='checkbox';check.value=model.id;check.disabled=model.existing;check.checked=upstreamSelected.has(model.id);
                check.onchange=()=>{{check.checked?upstreamSelected.add(model.id):upstreamSelected.delete(model.id);updateUpstreamCount();}};
                const name=document.createElement('span');name.textContent=model.id+(model.existing?' · 已存在':'');
                label.append(check,name);list.append(label);
            }}
            if(!list.children.length) list.textContent='没有符合条件的模型';
            updateUpstreamCount();
        }}
        function updateUpstreamCount() {{const button=document.getElementById('import-upstream');button.textContent='添加所选 ('+upstreamSelected.size+')';button.disabled=!upstreamSelected.size;}}
        function selectUpstreamVisible() {{for(const model of visibleUpstreamModels())if(!model.existing)upstreamSelected.add(model.id);renderUpstreamChoices();}}
        function importUpstreamSelected() {{
            for(const id of upstreamSelected) {{const row=addModelRow(false);row.querySelector('[name$="[name]"]').value=id;updateProviderModelMatch(row.querySelector('[name$="[name]"]'),false);}}
            document.getElementById('upstream-preview').hidden=true;
            document.querySelector('.provider-form').dataset.dirty='true';
            document.getElementById('provider-model-search').value='';document.getElementById('provider-model-filter').value='all';
            filterProviderModelRows();Toast.success('已添加 '+upstreamSelected.size+' 个模型，保存配置后生效');
        }}
        function providerFormIsDirty() {{
            const form=document.querySelector('.provider-form');
            return form && form.dataset.saved!=='true' && JSON.stringify(Array.from(new FormData(form)))!==form.dataset.initialValues;
        }}
        function leaveProviderForm() {{
            if(document.querySelector('.provider-detail')) {{
                const form=document.querySelector('.provider-form');
                if(providerFormIsDirty()&&!confirm('放弃未保存的供应商修改？'))return;
                form.dataset.saved='true';window.location.href='/providers';
            }} else closeModal();
        }}

        function addModelRow(focus = true) {{
            const container = document.getElementById('models-container');
            const html = document.getElementById('new-model-row').innerHTML.replaceAll('__INDEX__', String(modelIndex++));
            const template = document.createElement('template');
            template.innerHTML = html.trim();
            const row = template.content.firstElementChild;
            container.appendChild(row);
            if (focus) {{
                document.getElementById('provider-model-search').value = '';
                document.getElementById('provider-model-filter').value = 'all';
            }}
            updateProviderModelMatch(row.querySelector('[name$="[name]"]'), focus);
            if (focus) row.querySelector('[name$="[name]"]').focus();
            return row;
        }}

        function addHeaderRow() {{
            const container = document.getElementById('headers-container');
            prepareListContainer(container);
            const div = document.createElement('div');
            div.dataset.orderRow = '';
            div.dataset.sortableRow = '';
            div.className = 'flex gap-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3';
            div.innerHTML = `
                <button type="button" class="drag-handle self-center" title="拖动调整顺序" aria-label="拖动调整 Header" onpointerdown="beginSortablePointer(event, this, 'form')" onpointermove="moveSortablePointer(event)" onpointerup="endSortablePointer(event)" onpointercancel="endSortablePointer(event)" onkeydown="moveSortableWithKeyboard(event, this, 'form')">
                    <svg class="h-5 w-5" aria-hidden="true" viewBox="0 0 24 24" fill="currentColor"><circle cx="9" cy="5" r="1.5"/><circle cx="15" cy="5" r="1.5"/><circle cx="9" cy="12" r="1.5"/><circle cx="15" cy="12" r="1.5"/><circle cx="9" cy="19" r="1.5"/><circle cx="15" cy="19" r="1.5"/></svg>
                </button>
                <div class="grid min-w-0 flex-1 grid-cols-12 gap-2">
                    <div class="col-span-4">
                        <label class="mb-1 block text-[10px] text-zinc-500">Header 名称</label>
                        <input type="text" name="headers[${{headerIndex}}][name]" placeholder="Authorization" class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-sm">
                    </div>
                    <div class="col-span-8">
                        <label class="mb-1 block text-[10px] text-zinc-500">Header 值</label>
                        <input type="text" name="headers[${{headerIndex}}][value]" placeholder="Bearer ..." class="h-9 w-full rounded-lg border border-zinc-200 bg-white px-3 font-mono text-sm">
                    </div>
                </div>
                <button type="button" onclick="removeFormRow(this)" class="inline-flex min-h-[44px] min-w-[44px] self-end items-center justify-center rounded border border-red-200 text-red-600 hover:bg-red-50" title="删除 Header" aria-label="删除 Header">
                    <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
                </button>
            `;
            container.appendChild(div);
            headerIndex++;
        }}

        function initializeProviderForm() {{
        document.querySelector('.provider-form').addEventListener('input',event=>event.currentTarget.dataset.dirty='true');
        document.querySelector('.provider-form').addEventListener('change',event=>event.currentTarget.dataset.dirty='true');
        window.addEventListener('beforeunload',event=>{{if(providerFormIsDirty()){{event.preventDefault();event.returnValue='';}}}});
        initializeProviderRecentStats();
        refreshProviderProtocolRows();
        document.querySelectorAll('#models-container [data-model-row]').forEach(row =>
            updateProviderModelMatch(row.querySelector('[name$="[name]"]'), false)
        );
        filterProviderModelRows();
        const form=document.querySelector('.provider-form');
        form.dataset.initialValues=JSON.stringify(Array.from(new FormData(form)));
        }}
        if(document.readyState==='loading') document.addEventListener('DOMContentLoaded',initializeProviderForm,{{once:true}});
        else initializeProviderForm();
    </script>
"##,
        recent_scripts = include_str!("../../../static/admin/provider-recent.js"),
        stats_provider = if name_readonly {
            escape_html(name)
        } else {
            String::new()
        },
        method_js = escape_inline_js_string(method),
        url_js = escape_inline_js_string(url),
        name_html = escape_html(name),
        name_readonly = if name_readonly { "readonly" } else { "" },
        api_key_html = escape_html(api_key),
        base_url_html = escape_html(&base_url),
        endpoints_html = endpoints_html,
        priority = priority,
        model_passthrough_checked = if model_passthrough && !catalog_enabled {
            "checked"
        } else {
            ""
        },
        model_passthrough_disabled = if catalog_enabled { "disabled" } else { "" },
        model_passthrough_description = if catalog_enabled {
            "模型目录已启用，未绑定的上游模型不会透传。"
        } else {
            "启用后，未单独配置的模型名会原样转发给此供应商。"
        },
        enabled_selected = if status == "enabled" { "selected" } else { "" },
        disabled_selected = if status == "enabled" { "" } else { "selected" },
        headers_html = headers_html,
        header_count = headers.len(),
        models_html = models_html,
        new_model_row_html = new_model_row_html,
        model_count = models.len(),
    )
}
