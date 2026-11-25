use topcoat::{context::Cx, router::page, Result};

use crate::topcoat_admin::app::require_admin;
use crate::topcoat_admin::{
    render_modal_backdrop, render_page, render_shared_scripts, render_shared_styles,
    render_sidebar, render_sidebar_bootstrap, render_theme_bootstrap, render_toast_container,
    trusted_html, PageLayout,
};

#[page("/model-catalog")]
pub async fn model_catalog(cx: &Cx) -> Result {
    require_admin(cx)?;
    let sidebar = render_sidebar(cx, "/model-catalog").await?;
    let actions = trusted_html(
        r#"<div class="catalog-page-actions"><button class="outline-button" type="button" onclick="openSourcePicker()">从 models.dev 导入</button><button class="primary-button" type="button" onclick="openModelEditor()"><svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M12 5v14M5 12h14"/></svg>新增模型</button></div>"#.to_string(),
    );
    let body = trusted_html(
        r#"<div class="catalog-workspace">
            <div class="catalog-controls">
                <div class="catalog-list-heading"><h2>平台模型</h2><span id="catalog-count"></span></div>
                <label class="catalog-switch" for="catalog-enabled"><input id="catalog-enabled" type="checkbox" onchange="toggleCatalog(this)"><span>启用模型目录</span></label>
            </div>
            <div id="catalog-list" class="data-list" aria-live="polite"><div class="list-loading">加载中...</div></div>
        </div>
        <template id="catalog-editor-template"><div class="dialog-shell catalog-dialog">
            <header class="dialog-header"><div class="dialog-heading"><h2 id="dialog-title" class="dialog-title">模型配置</h2></div><button class="icon-button dialog-close" type="button" onclick="closeModal()" aria-label="关闭" title="关闭"><svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M18 6 6 18M6 6l12 12"/></svg></button></header>
            <div class="dialog-body"><form id="catalog-editor" class="catalog-form" onsubmit="savePlatformModel(event)">
                <label>模型本名<input name="name" required autocomplete="off" placeholder="gpt-4o"></label>
                <label>显示名称<input name="display_name" autocomplete="off" placeholder="GPT-4o"></label>
                <label class="catalog-field-wide">公开别名<input name="aliases" autocomplete="off" placeholder="逗号分隔；有别名时只公开别名"></label>
                <label>路由到<select name="route_to"><option value="">按本模型绑定路由</option></select></label>
                <label>上下文长度<input name="context_window" type="number" min="1" step="1" placeholder="可留空"></label>
                <label class="catalog-field-wide">Logo 图片地址<div class="catalog-logo-input"><span id="catalog-logo-preview"></span><input name="logo_url" type="url" placeholder="https://example.com/logo.svg" autocomplete="off"></div><span class="catalog-meta">支持彩色 SVG、PNG、WebP，按图片原色显示</span></label>
                <label class="catalog-field-wide">描述<textarea name="description" rows="2"></textarea></label>
                <div class="catalog-form-grid"><label>输入价格 / 1K Token<input name="input_price" type="number" min="0" step="any" placeholder="可留空"></label><label>输出价格 / 1K Token<input name="output_price" type="number" min="0" step="any" placeholder="可留空"></label></div>
                <label class="catalog-check"><input name="enabled" type="checkbox" checked><span>启用模型</span></label>
                <p id="catalog-form-error" class="catalog-error" role="alert" hidden></p>
                <div class="dialog-footer"><button class="outline-button" type="button" onclick="closeModal()">取消</button><button class="primary-button" type="submit">保存模型</button></div>
            </form></div>
        </div></template>
        <template id="catalog-picker-template"><div class="dialog-shell catalog-picker">
            <header class="dialog-header"><div class="dialog-heading"><h2 id="dialog-title" class="dialog-title">从 models.dev 导入</h2></div><div class="catalog-picker-actions"><button class="outline-button" type="button" onclick="refreshSourceCatalog()">刷新目录</button><button class="icon-button dialog-close" type="button" onclick="closeModal()" aria-label="关闭" title="关闭"><svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M18 6 6 18M6 6l12 12"/></svg></button></div></header>
            <div class="dialog-body catalog-picker-body">
                <aside class="catalog-source-providers"><label for="source-provider-filter">供应商<input id="source-provider-filter" type="search" placeholder="搜索供应商" autocomplete="off" oninput="filterSourceCatalog()"></label><div id="source-provider-list"></div></aside>
                <div class="catalog-picker-main"><div class="catalog-picker-filters">
                    <label for="source-model-filter">模型<input id="source-model-filter" type="search" placeholder="模型名称或 ID" autocomplete="off" oninput="filterSourceCatalog()"></label>
                </div>
                <div class="catalog-picker-summary"><span id="source-summary" aria-live="polite">加载中...</span><span id="source-picked-count"></span></div>
                <div id="source-viewport" class="catalog-source-viewport" tabindex="0" aria-label="models.dev 模型列表" onscroll="renderSourceRows()"><div id="source-items" class="catalog-source-items"></div></div>
                <div class="catalog-import-footer"><button type="button" class="outline-button" onclick="selectFilteredSources()">选择筛选结果</button><button type="button" class="outline-button" onclick="clearSourceSelection()">清空</button><button id="source-import-selected" type="button" class="primary-button" onclick="importSelectedSources()" disabled>导入所选</button></div>
                </div>
            </div>
        </div></template>"#.to_string(),
    );
    let page = render_page(
        cx,
        PageLayout {
            title: "模型配置",
            description: Some("平台公开名称、供应商映射与路由"),
            class_name: "catalog-page",
            actions: Some(actions),
            body,
        },
    )
    .await?;
    let html = format!(
        r#"<!DOCTYPE html><html lang="zh-CN"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>RCPA Admin - 模型配置</title>{theme_bootstrap}{sidebar_bootstrap}<link rel="stylesheet" href="/_topcoat/tailwind.css"><link rel="stylesheet" href="/_topcoat/model-catalog.css?v={catalog_css_hash}">{shared_styles}</head><body><div class="flex min-h-screen">{sidebar}<main class="admin-main"><div class="admin-content">{page}</div></main></div>{modal}{toast}{shared_scripts}<script src="/_topcoat/model-catalog.js?v={catalog_js_hash}"></script></body></html>"#,
        theme_bootstrap = render_theme_bootstrap(),
        sidebar_bootstrap = render_sidebar_bootstrap(),
        catalog_css_hash = env!("RCPA_MODEL_CATALOG_CSS_HASH"),
        catalog_js_hash = env!("RCPA_MODEL_CATALOG_JS_HASH"),
        shared_styles = render_shared_styles(),
        sidebar = sidebar,
        page = page,
        modal = render_modal_backdrop(),
        toast = render_toast_container(),
        shared_scripts = render_shared_scripts(),
    );
    Ok(trusted_html(html))
}
