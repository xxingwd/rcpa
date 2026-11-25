use topcoat::{
    context::Cx,
    view::{component, view, View},
    Result,
};

use super::theme::{render_theme_scripts, theme_toggle};

#[component]
async fn nav_link(path: &'static str, label: &'static str, active: bool, child: View) -> Result {
    let class_name = if active {
        "sidebar-link active"
    } else {
        "sidebar-link"
    };

    view! {
        <a href=(path) class=(class_name) data-page=(path) title=(label)>
            (child)
            <span class="sidebar-label">(label)</span>
        </a>
    }
}

/// Shared application chrome composed as Topcoat views.
#[component]
async fn sidebar(current_page: String) -> Result {
    let dashboard_active = current_page == "/" || current_page == "/dashboard";

    view! {
        <header class="mobile-header">
            <a class="brand" href="/dashboard" aria-label="RCPA 仪表盘">
                <span class="brand-mark">"R"</span><span class="brand-name">"RCPA"</span>
            </a>
            <button class="icon-button" type="button" onclick="toggleMobileNav()" aria-label="打开菜单" title="打开菜单">
                <svg class="icon menu-open-icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M4 6h16M4 12h16M4 18h16"/></svg>
                <svg class="icon menu-close-icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M18 6 6 18M6 6l12 12"/></svg>
            </button>
        </header>
        <button id="mobile-nav-scrim" class="mobile-nav-scrim" type="button" onclick="closeMobileNav()" aria-label="关闭菜单"></button>
        <aside id="sidebar" class="sidebar" aria-label="主导航">
            <div class="sidebar-brand-row">
                <a class="brand" href="/dashboard" aria-label="RCPA 仪表盘">
                    <span class="brand-mark">"R"</span><span class="brand-name sidebar-label">"RCPA"</span>
                </a>
                <button class="icon-button collapse-button" type="button" onclick="toggleSidebarCollapse()" aria-label="折叠菜单" title="折叠菜单">
                    <svg class="icon collapse-left-icon" aria-hidden="true" viewBox="0 0 24 24"><path d="m15 18-6-6 6-6"/></svg>
                    <svg class="icon collapse-right-icon" aria-hidden="true" viewBox="0 0 24 24"><path d="m9 18 6-6-6-6"/></svg>
                </button>
            </div>
            <nav class="sidebar-nav">
                nav_link(path: "/dashboard", label: "仪表盘", active: dashboard_active,
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M3 3v18h18M7 16v-5M12 16V7M17 16V4"/></svg>
                )
                nav_link(path: "/keys", label: "密钥管理", active: current_page == "/keys",
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><circle cx="7.5" cy="15.5" r="5.5"/><path d="m21 2-9.6 9.6M15 6l3 3M18 3l3 3"/></svg>
                )
                nav_link(path: "/providers", label: "供应商", active: current_page == "/providers",
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="m12 2 9 5-9 5-9-5 9-5Z"/><path d="m3 12 9 5 9-5M3 17l9 5 9-5"/></svg>
                )
                nav_link(path: "/model-catalog", label: "模型配置", active: current_page == "/model-catalog",
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9h10M7 13h7M7 17h4"/></svg>
                )
                nav_link(path: "/logs", label: "调用日志", active: current_page == "/logs",
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z"/><path d="M14 2v6h6M8 13h8M8 17h8M8 9h2"/></svg>
                )
                nav_link(path: "/config", label: "配置", active: current_page == "/config",
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.38a2 2 0 0 0-.73-2.73l-.15-.09a2 2 0 0 1-1-1.74v-.51a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2Z"/><circle cx="12" cy="12" r="3"/></svg>
                )
            </nav>
            <div class="sidebar-footer">
                theme_toggle(class_name: "sidebar-theme")
                <button class="sidebar-action destructive" type="button" onclick="logoutAdmin()" title="退出登录">
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9"/></svg>
                    <span class="sidebar-label">"退出登录"</span>
                </button>
            </div>
        </aside>
    }
}

pub async fn render_sidebar(cx: &Cx, current_page: &str) -> Result<String> {
    let __cx = cx;
    let rendered: Result = view! { sidebar(current_page: current_page.to_owned()) };
    Ok(rendered?.render(cx))
}

pub fn render_toast_container() -> &'static str {
    r#"<div id="toast-container" class="toast-container" aria-live="polite" aria-atomic="true"></div>"#
}

pub fn render_modal_backdrop() -> &'static str {
    r#"<div id="modal-backdrop" class="modal-overlay" hidden><section class="modal-content" role="dialog" aria-modal="true" aria-labelledby="dialog-title"><div id="modal-body"></div></section></div>"#
}

pub fn render_sidebar_bootstrap() -> &'static str {
    r#"<script>
    (() => {
        document.documentElement.classList.toggle(
            'sidebar-collapsed',
            localStorage.getItem('rcpa_sidebar_collapsed') === 'true'
        );
    })();
    </script>"#
}

pub fn render_shared_scripts() -> String {
    [
        render_theme_scripts(),
        "<script>",
        include_str!("../../../static/admin/filters.js"),
        include_str!("../../../static/admin/rows.js"),
        "</script>",
        r#"<script>
    const SidebarManager = {
        key: 'rcpa_sidebar_collapsed',
        toggle() {
            const collapsed = !document.documentElement.classList.contains('sidebar-collapsed');
            document.documentElement.classList.toggle('sidebar-collapsed', collapsed);
            localStorage.setItem(this.key, String(collapsed));
        }
    };

    const Toast = {
        show(message, type = 'info', duration = 4000) {
            const container = document.getElementById('toast-container');
            if (!container) return null;
            const toast = document.createElement('div');
            toast.className = 'toast toast-' + type;
            const text = document.createElement('span');
            text.textContent = message;
            const close = document.createElement('button');
            close.type = 'button';
            close.className = 'toast-close';
            close.setAttribute('aria-label', '关闭通知');
            close.textContent = '×';
            close.onclick = () => toast.remove();
            toast.append(text, close);
            container.appendChild(toast);
            if (duration > 0) setTimeout(() => toast.remove(), duration);
            return toast;
        },
        success(message, duration) { return this.show(message, 'success', duration); },
        error(message, duration) { return this.show(message, 'error', duration); },
        warning(message, duration) { return this.show(message, 'warning', duration); },
        info(message, duration) { return this.show(message, 'info', duration); }
    };

    const Modal = {
        returnFocus: null,
        beforeClose: null,
        loadGeneration: 0,
        open(html, options = {}) {
            this.returnFocus = document.activeElement;
            this.beforeClose = null;
            const backdrop = document.getElementById('modal-backdrop');
            const body = document.getElementById('modal-body');
            backdrop.classList.toggle('drawer-overlay', !!options.drawer);
            body.innerHTML = html;
            body.querySelectorAll('script').forEach((script) => {
                const executable = document.createElement('script');
                for (const attribute of script.attributes) executable.setAttribute(attribute.name, attribute.value);
                executable.textContent = script.textContent;
                script.replaceWith(executable);
            });
            backdrop.hidden = false;
            document.body.classList.add('modal-open');
            backdrop.querySelector('input, select, textarea, button')?.focus();
        },
        close(force = false) {
            const backdrop = document.getElementById('modal-backdrop');
            if (!backdrop || backdrop.hidden) return;
            if (!force && this.beforeClose && !this.beforeClose()) return;
            this.loadGeneration++;
            this.beforeClose = null;
            backdrop.hidden = true;
            document.getElementById('modal-body').innerHTML = '';
            document.body.classList.remove('modal-open');
            backdrop.classList.remove('drawer-overlay');
            if (this.returnFocus?.isConnected) this.returnFocus.focus();
        },
        load(url) {
            const generation = ++this.loadGeneration;
            fetch(url, { credentials: 'include' })
                .then((response) => {
                    if (response.status === 401) return redirectToLogin();
                    if (!response.ok) throw new Error('加载失败');
                    return response.text();
                })
                .then((html) => { if (html && generation === this.loadGeneration) this.open(html); })
                .catch((error) => Toast.error(error.message));
        }
    };

    const RcpaData = {
        refresh(scope) {
            document.body.dispatchEvent(new CustomEvent('rcpa-' + scope + '-refresh'));
            window.dispatchEvent(new CustomEvent('rcpa:data-change', { detail: { scope } }));
        }
    };

    function redirectToLogin() {
        const next = encodeURIComponent(window.location.pathname + window.location.search);
        window.location.replace('/login?next=' + next);
    }
    function verifyAdminSession() {
        fetch('/v1/admin/session', { credentials: 'include' })
            .then((response) => { if (!response.ok) redirectToLogin(); })
            .catch(() => redirectToLogin());
    }
    function logoutAdmin() {
        fetch('/v1/admin/logout', { method: 'POST', credentials: 'include' })
            .finally(() => window.location.replace('/login'));
    }
    function toggleSidebarCollapse() { SidebarManager.toggle(); }
    function toggleMobileNav() { document.documentElement.classList.toggle('mobile-nav-open'); }
    function closeMobileNav() { document.documentElement.classList.remove('mobile-nav-open'); }
    function showToast(message, type) { Toast.show(message, type); }
    function openModal(html) { Modal.open(html); }
    function closeModal() { Modal.close(); }
    function loadModal(url) { Modal.load(url); }
    function refreshData(scope) { RcpaData.refresh(scope); }
    function setFormBusy(form, busy) {
        const submit = form?.querySelector('[type="submit"]');
        if (!submit) return;
        if (busy) submit.dataset.idleLabel = submit.textContent;
        submit.disabled = busy;
        submit.setAttribute('aria-busy', String(busy));
        submit.textContent = busy ? '保存中...' : (submit.dataset.idleLabel || submit.textContent);
    }

    document.addEventListener('keydown', (event) => {
        const overlay = document.getElementById('modal-backdrop');
        if (event.key === 'Escape') {
            if (document.querySelector('.multi-filter[open]')) return;
            if (overlay && !overlay.hidden) { event.preventDefault(); Modal.close(); }
            else closeMobileNav();
        }
        if(event.key === 'Tab' && overlay && !overlay.hidden) {
            const controls = Array.from(overlay.querySelectorAll('button:not(:disabled), a[href], input:not(:disabled):not([type="hidden"]), select:not(:disabled), textarea:not(:disabled), [tabindex="0"]')).filter(node=>node.getClientRects().length);
            const first=controls[0], last=controls.at(-1);
            if(event.shiftKey && document.activeElement===first){event.preventDefault();last?.focus();}
            else if(!event.shiftKey && document.activeElement===last){event.preventDefault();first?.focus();}
        }
    });
    document.addEventListener('DOMContentLoaded', () => {
        verifyAdminSession();
    });
    </script>"#,
    ]
    .concat()
}

pub fn render_shared_styles() -> &'static str {
    r#"<style>
    :root {
        --radius: 6px;
        --background: #fafafa;
        --foreground: #202124;
        --card: #ffffff;
        --card-foreground: #202124;
        --muted: #f4f4f5;
        --muted-foreground: #64646d;
        --accent: #eef4f1;
        --accent-foreground: #215941;
        --primary: #08765e;
        --primary-foreground: #ffffff;
        --destructive: #b33b46;
        --success: #176547;
        --border: #e4e4e7;
        --ring: #08765e;
        --sidebar: #f5f5f6;
        --sidebar-foreground: #202124;
        --sidebar-muted: #64646d;
        --sidebar-accent: #e8eeeb;
        --sidebar-border: #e4e4e7;
        --workspace-gutter: 1rem;
        --control-height: 2rem;
        color-scheme: light;
    }
    html[data-theme="dark"] {
        --background: #141416;
        --foreground: #ededf0;
        --card: #1c1c1f;
        --card-foreground: #ededf0;
        --muted: #242427;
        --muted-foreground: #a5a5af;
        --accent: #26352e;
        --accent-foreground: #b5e5cb;
        --primary: #6bd4ac;
        --primary-foreground: #0d2e20;
        --destructive: #ff9198;
        --success: #78d8a9;
        --border: #333338;
        --ring: #77d9b2;
        --sidebar: #19191c;
        --sidebar-foreground: #ededf0;
        --sidebar-muted: #a5a5af;
        --sidebar-accent: #28372f;
        --sidebar-border: #303034;
        color-scheme: dark;
    }
    @font-face {
        font-family: "RCPA Sans";
        src: url("/_topcoat/fonts/noto-sans-cjk-sc.otf") format("opentype");
        font-style: normal;
        font-weight: 400;
        font-display: swap;
    }
    * { box-sizing: border-box; border-color: var(--border); }
    [hidden] { display: none !important; }
    html, body { min-height: 100%; }
    body {
        margin: 0;
        background: var(--background) !important;
        color: var(--foreground) !important;
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "RCPA Sans", "PingFang SC", "Microsoft YaHei", sans-serif;
        font-size: .875rem;
        line-height: 1.5;
        letter-spacing: 0;
        -webkit-font-smoothing: antialiased;
    }
    button, input, select, textarea { font: inherit; }
    button, a { touch-action: manipulation; }
    button { cursor: pointer; }
    button:disabled { cursor: not-allowed; opacity: .5; }
    [data-row-action] { cursor: pointer; }
    [data-row-action]:hover, [data-row-action]:focus-visible { background: var(--muted) !important; }
    .model-mark { display: inline-flex; align-items: center; gap: .375rem; min-width: 0; }
    .model-mark img { width: 1.125rem; height: 1.125rem; object-fit: contain; flex: none; border-radius: 3px; background: #fff; }
    :focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
    .tracking-wider { letter-spacing: 0 !important; }
    .font-mono { font-variant-numeric: tabular-nums; }
    ::-webkit-scrollbar { width: 5px; height: 5px; }
    ::-webkit-scrollbar-track { background: transparent; }
    ::-webkit-scrollbar-thumb { background: color-mix(in oklch, var(--muted-foreground) 28%, transparent); border-radius: 999px; }

    .admin-tooltip { position: fixed; z-index: 1100; width: max-content; max-width: min(18rem, calc(100vw - 1rem)); padding: .375rem .625rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--card); color: var(--card-foreground); box-shadow: 0 4px 12px rgb(0 0 0 / .12); font-size: .75rem; line-height: 1.5; overflow-wrap: anywhere; }
    .icon { width: 1rem; height: 1rem; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.8; }
    .icon-button { display: inline-flex; width: var(--control-height); height: var(--control-height); align-items: center; justify-content: center; border: 0; border-radius: 6px; color: var(--muted-foreground); background: transparent; transition: color 150ms ease, background 150ms ease; }
    .icon-button:hover { color: var(--foreground); background: var(--accent); }
    .brand { display: flex; min-width: 0; align-items: center; gap: .625rem; color: var(--sidebar-foreground); text-decoration: none; }
    .brand-mark { display: flex; width: 2rem; height: 2rem; flex: 0 0 2rem; align-items: center; justify-content: center; border: 0; border-radius: 6px; background: var(--primary); color: var(--primary-foreground); font-size: .9375rem; font-weight: 700; }
    .brand-name { font-size: .9375rem; font-weight: 700; }

    .sidebar { position: sticky; top: 0; z-index: 30; display: flex; width: 13rem; height: 100dvh; flex: 0 0 13rem; flex-direction: column; padding: .75rem .5rem; border-right: 1px solid var(--sidebar-border); background: var(--sidebar); transition: width 200ms ease, flex-basis 200ms ease, transform 200ms ease; }
    .sidebar-brand-row { display: flex; min-height: 2.25rem; align-items: center; justify-content: space-between; gap: .625rem; margin: 0 .375rem .75rem; padding-bottom: .75rem; border-bottom: 1px solid var(--sidebar-border); }
    .sidebar-nav { display: flex; min-height: 0; flex-direction: column; gap: .125rem; overflow-y: auto; overscroll-behavior: contain; }
    .sidebar-brand-row, .sidebar-footer { flex-shrink: 0; }
    .sidebar-link { position: relative; display: flex; height: 2.25rem; align-items: center; gap: .625rem; padding: 0 .75rem; border-radius: 6px; color: var(--sidebar-muted); font-size: .875rem; font-weight: 500; text-decoration: none; transition: color 150ms ease, background 150ms ease; }
    .sidebar-link:hover { color: var(--sidebar-foreground); background: color-mix(in srgb, var(--sidebar-foreground) 10%, transparent); }
    .sidebar-link.active { color: var(--sidebar-foreground); background: var(--sidebar-accent); font-weight: 600; }
    .sidebar-link.active::before { position: absolute; left: 0; width: 3px; height: 1.25rem; border-radius: 0 3px 3px 0; background: var(--primary); content: ""; }
    .sidebar-link.active .icon { stroke-width: 2.2; }
    .sidebar-footer { display: flex; flex-direction: column; gap: .25rem; margin-top: auto; padding-top: .75rem; border-top: 1px solid var(--sidebar-border); }
    .sidebar-action { display: flex; width: 100%; height: 2.25rem; align-items: center; justify-content: flex-start; gap: .625rem; padding: 0 .75rem; border: 0; border-radius: 6px; background: transparent; color: var(--sidebar-muted); font-size: .8125rem; font-weight: 500; transition: color 150ms ease, background 150ms ease; }
    .sidebar-action:hover { color: var(--sidebar-foreground); background: var(--sidebar-accent); }
    .sidebar-action.destructive:hover { color: var(--destructive); }
    .sidebar .icon-button { color: var(--sidebar-muted); }
    .theme-toggle .theme-mode-icon { display: none; flex: 0 0 1rem; }
    html[data-theme-mode="system"] .theme-toggle .theme-mode-icon-system,
    html[data-theme-mode="light"] .theme-toggle .theme-mode-icon-light,
    html[data-theme-mode="dark"] .theme-toggle .theme-mode-icon-dark { display: block; }
    .collapse-right-icon { display: none; }

    html.sidebar-collapsed .sidebar { width: 4.25rem; flex-basis: 4.25rem; }
    html.sidebar-collapsed .sidebar-label { display: none; }
    html.sidebar-collapsed .sidebar-brand-row { justify-content: center; flex-wrap: wrap; margin: 0 0 .75rem; padding-bottom: .75rem; }
    html.sidebar-collapsed .collapse-button { width: 100%; }
    html.sidebar-collapsed .collapse-left-icon { display: none; }
    html.sidebar-collapsed .collapse-right-icon { display: block; }
    html.sidebar-collapsed .sidebar-link { justify-content: center; padding: 0; }
    html.sidebar-collapsed .sidebar-action { justify-content: center; padding: 0; }

    .mobile-header, .mobile-nav-scrim { display: none; }
    .menu-close-icon { display: none; }

    body:has(.admin-main) { height: 100dvh; overflow: hidden; }
    body > .flex.min-h-screen { height: 100dvh; min-height: 0; overflow: hidden; }
    .admin-main { min-width: 0; min-height: 0; height: 100%; flex: 1 1 auto; overflow: hidden; padding: var(--workspace-gutter); background: var(--background); }
    .admin-content { width: 100%; max-width: none; height: 100%; min-width: 0; margin: 0 auto; }
    .page { display: flex; width: 100%; height: 100%; min-height: 0; flex-direction: column; gap: .875rem; }
    .page-header { display: flex; min-height: 3.25rem; flex: 0 0 auto; align-items: center; justify-content: space-between; gap: 1rem; padding: 0 0 .75rem; border-bottom: 1px solid var(--border); }
    .page-heading { min-width: 0; }
    .page-title { margin: 0; font-size: 1.125rem; line-height: 1.5rem; font-weight: 650; letter-spacing: 0; }
    .page-description { margin: .25rem 0 0; color: var(--muted-foreground); font-size: .8125rem; line-height: 1.25rem; }
    .page-actions { display: flex; min-width: 0; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: .5rem; }
    .page-body { display: flex; min-width: 0; min-height: 0; flex: 1 1 auto; flex-direction: column; overflow: auto; overscroll-behavior: contain; }
    .data-list { width: 100%; min-width: 0; min-height: 0; flex: 1 1 auto; overflow: auto; overscroll-behavior: contain; border: 1px solid var(--border); border-radius: var(--radius); background: var(--card); }
    .data-list thead th { position: sticky; top: 0; z-index: 1; background: var(--card); }
    .data-table-scroll { height: 100%; min-height: 0; overflow: auto; overscroll-behavior: contain; }
    .data-list:has(> .data-table-scroll) { overflow: hidden; }
    .data-list.htmx-request { opacity: .72; }
    .data-list > .py-12 { display: grid; min-height: 12rem; place-items: center; padding: 2rem; color: var(--muted-foreground); }
    .list-loading { display: grid; min-height: 10rem; place-items: center; color: var(--muted-foreground); font-size: .875rem; }
    .primary-button, .outline-button { display: inline-flex; min-height: var(--control-height); align-items: center; justify-content: center; gap: .375rem; padding: .3125rem .75rem; border-radius: 6px; font-size: .8125rem; font-weight: 600; white-space: nowrap; transition: background 150ms ease, color 150ms ease, border-color 150ms ease, box-shadow 150ms ease; }
    .primary-button { border: 1px solid transparent; background: var(--primary); color: var(--primary-foreground); }
    .primary-button:hover { background: color-mix(in srgb, var(--primary) 88%, var(--foreground));  }
    .outline-button { border: 1px solid var(--border); background: var(--card); color: var(--foreground); }
    .outline-button:hover { border-color: var(--border); background: var(--muted); }

    .filter-bar { display: flex; align-items: center; flex-wrap: wrap; gap: .5rem; min-width: 0; }
    .multi-filter { position: relative; min-width: 7rem; font-size: .8125rem; }
    .multi-filter summary { display: flex; min-height: var(--control-height); align-items: center; justify-content: space-between; gap: .75rem; padding: .3125rem .625rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--card); cursor: pointer; list-style: none; }
    .multi-filter summary::-webkit-details-marker { display: none; }
    .multi-filter summary.has-selection { color: var(--primary); background: var(--accent); }
    .multi-filter summary .icon { width: .75rem; height: .75rem; }
    .multi-filter-panel { position: fixed; display: flex; flex-direction: column; z-index: 45; overflow: hidden; width: min(19rem, calc(100vw - 2rem)); padding: .5rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--card); box-shadow: 0 8px 24px rgb(0 0 0 / .12); }
    .multi-filter-panel input[type="search"] { width: 100%; padding: .375rem .5rem; border: 1px solid var(--border); }
    .multi-filter-tools { display: flex; align-items: center; justify-content: space-between; padding: .375rem 0; }
    .multi-filter-tools button { border: 0; padding: .25rem; background: transparent; color: var(--muted-foreground); font-size: .75rem; }
    .multi-filter-tools button:hover { color: var(--primary); }
    .multi-filter-list { min-height: 0; max-height: 16rem; overflow: auto; overscroll-behavior: contain; }
    .multi-filter-panel > input, .multi-filter-tools { flex-shrink: 0; }
    .multi-filter-list label { display: flex; align-items: center; gap: .5rem; min-height: 2rem; padding: .25rem .375rem; border-radius: 4px; cursor: pointer; }
    .multi-filter-list label:hover { background: var(--muted); }
    .multi-filter-list input { flex: none; accent-color: var(--primary); }
    .multi-filter-list span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .multi-filter-list p { padding: .5rem; color: var(--muted-foreground); }
    @media (max-width: 639px) {
        .multi-filter { flex: 1 1 calc(50% - .5rem); min-width: 0; }
        .multi-filter-list label, .multi-filter-tools button { min-height: 2.75rem; }
    }

    /* Existing page utilities are mapped onto the semantic palette. */
    .bg-white { background-color: var(--card) !important; }
    .bg-zinc-50 { background-color: var(--muted) !important; }
    .bg-zinc-50\/50 { background-color: color-mix(in oklch, var(--muted) 38%, transparent) !important; }
    .bg-zinc-100 { background-color: var(--muted) !important; }
    .bg-zinc-900 { background-color: var(--primary) !important; }
    .hover\:bg-zinc-800:hover { background-color: color-mix(in oklch, var(--primary) 95%, var(--background)) !important; }
    .text-zinc-900, .text-zinc-800, .text-zinc-700 { color: var(--foreground) !important; }
    .text-zinc-600, .text-zinc-500, .text-zinc-400 { color: var(--muted-foreground) !important; }
    .text-blue-600 { color: var(--primary) !important; }
    .bg-emerald-100 { background-color: color-mix(in oklch, #10b981 15%, transparent) !important; }
    .text-emerald-700, .text-emerald-600 { color: var(--success) !important; }
    .bg-red-100, .bg-red-50 { background-color: color-mix(in oklch, var(--destructive) 12%, transparent) !important; }
    .text-red-700, .text-red-600, .text-red-500 { color: var(--destructive) !important; }
    .border-red-200 { border-color: color-mix(in oklch, var(--destructive) 30%, transparent) !important; }
    .border-zinc-100, .border-zinc-200, .border-zinc-300, .divide-zinc-200 > :not(:last-child) { border-color: var(--border) !important; }
    .hover\:bg-zinc-50:hover, .hover\:bg-zinc-100:hover { background-color: var(--accent) !important; }
    select, input, textarea { border-color: var(--border) !important; border-radius: 6px; background: var(--card) !important; color: var(--foreground) !important; }
    select:focus-visible, input:not([type="checkbox"]):not([type="radio"]):focus-visible, textarea:focus-visible { background: var(--muted) !important; outline: none; box-shadow: none; }
    select { min-height: var(--control-height); color-scheme: inherit; }
    input:not([type="checkbox"]):not([type="radio"]) { min-height: var(--control-height); }
    table { border-collapse: collapse; }
    thead { background: var(--muted); color: var(--muted-foreground); }
    tbody tr:hover { background: color-mix(in srgb, var(--accent) 55%, var(--card)) !important; }
    .rounded-xl { border-radius: var(--radius) !important; }

    .toast-container { position: fixed; right: 1rem; bottom: 1rem; z-index: 1000; display: flex; max-width: calc(100vw - 2rem); flex-direction: column; gap: .5rem; pointer-events: none; }
    .toast { display: flex; min-width: 17rem; max-width: 25rem; align-items: center; justify-content: space-between; gap: .75rem; padding: .75rem 1rem; border: 1px solid var(--border); border-radius: 6px; background: var(--card); color: var(--foreground); box-shadow: 0 10px 30px rgb(0 0 0 / .14); pointer-events: auto; animation: toast-in 180ms ease-out; }
    .toast-success { border-color: color-mix(in oklch, #10b981 45%, var(--border)); }
    .toast-error { border-color: color-mix(in oklch, var(--destructive) 55%, var(--border)); }
    .toast-warning { border-color: color-mix(in oklch, #f59e0b 55%, var(--border)); }
    .toast-close { width: 1.75rem; height: 1.75rem; border: 0; background: transparent; color: inherit; font-size: 1.25rem; }
    @keyframes toast-in { from { opacity: 0; transform: translateY(.5rem); } to { opacity: 1; transform: translateY(0); } }

    .modal-open { overflow: hidden; }
    .modal-overlay { position: fixed; inset: 0; z-index: 100; display: grid; place-items: center; padding: 1rem; background: rgb(0 0 0 / .48); }
    .modal-overlay[hidden], .modal-overlay.hidden { display: none; }
    .modal-content { position: relative; width: min(48rem, calc(100vw - 2rem)); max-height: calc(100dvh - 2rem); overflow: auto; padding: 0; border: 1px solid var(--border); border-radius: var(--radius); background: var(--card); color: var(--card-foreground); box-shadow: 0 24px 60px rgb(0 0 0 / .2); }
    .drawer-overlay { justify-items: end; padding: 0; }
    .drawer-overlay .modal-content { width: min(36rem, 100vw); height: 100dvh; max-height: 100dvh; border-radius: 0; border-width: 0 0 0 1px; }
    .drawer-overlay .dialog-shell { display: flex; min-height: 100%; flex-direction: column; }
    .drawer-overlay .dialog-body { flex: 1; }
    .drawer-overlay .catalog-form { min-height: calc(100dvh - 5.5rem); grid-template-rows: repeat(6, min-content) 1fr min-content; }
    .drawer-overlay .dialog-footer { align-self: end; }
    .dialog-shell { min-width: 0; }
    .dialog-header { position: sticky; top: 0; z-index: 3; display: flex; min-height: 3.25rem; align-items: center; justify-content: space-between; gap: .75rem; padding: .75rem 1rem; border-bottom: 1px solid var(--border); background: var(--card); }
    .dialog-heading { min-width: 0; }
    .dialog-title { margin: 0; font-size: 1rem; line-height: 1.5rem; font-weight: 600; letter-spacing: 0; }
    .dialog-description { margin: .125rem 0 0; color: var(--muted-foreground); font-size: .75rem; line-height: 1rem; }
    .dialog-close { flex: 0 0 2.25rem; margin: -.25rem -.25rem -.25rem 0; }
    .dialog-body { min-width: 0; padding: 1rem; }
    .dialog-form { display: flex; flex-direction: column; gap: 1rem; }
    .dialog-footer { position: sticky; bottom: -1rem; z-index: 2; display: flex; justify-content: flex-end; gap: .5rem; margin: 0 -1rem -1rem; padding: .75rem 1rem; border-top: 1px solid var(--border); background: var(--card); }

    @media (max-width: 1023px) {
        .mobile-header { position: sticky; top: 0; z-index: 40; display: flex; width: 100%; height: 3.5rem; flex: 0 0 3.5rem; align-items: center; justify-content: space-between; padding: 0 1rem; border-bottom: 1px solid var(--border); background: var(--card); }
        .mobile-header .brand { color: var(--foreground); }
        body > .flex.min-h-screen { flex-direction: column; }
        .sidebar { position: fixed; top: 0; right: 0; z-index: 60; width: 18rem; max-width: 84vw; height: 100dvh; flex-basis: auto; transform: translateX(100%); }
        .collapse-button { display: none; }
        .mobile-nav-scrim { position: fixed; inset: 0; z-index: 50; border: 0; background: rgb(0 0 0 / .45); }
        html.mobile-nav-open .sidebar { transform: translateX(0); box-shadow: -12px 0 32px rgb(0 0 0 / .18); }
        html.mobile-nav-open .mobile-nav-scrim { display: block; }
        html.mobile-nav-open .menu-open-icon { display: none; }
        html.mobile-nav-open .menu-close-icon { display: block; }
        html.sidebar-collapsed .sidebar { width: 18rem; }
        html.sidebar-collapsed .sidebar-label { display: inline; }
        html.sidebar-collapsed .sidebar-brand-row { justify-content: space-between; flex-wrap: nowrap; margin: 0 .375rem 1rem; }
        html.sidebar-collapsed .sidebar-link { justify-content: flex-start; padding: 0 .75rem; }
        .admin-main { height: auto; padding: 1rem; }
        .page-header { align-items: flex-start; flex-direction: column; }
        .page-actions { width: 100%; justify-content: flex-start; }
        .modal-content input:not([type="checkbox"]):not([type="radio"]), .modal-content select, .modal-content textarea, .modal-content button { min-height: 2.75rem; }
        .theme-toggle { min-height: 2.75rem; }
    }
    @media (max-width: 639px) {
        :root { --control-height: 2.75rem; }
        input:not([type="checkbox"]):not([type="radio"]), select, textarea { font-size: 1rem !important; }
        .sidebar-action { min-height: 2.75rem; }
        .admin-main { padding: .875rem; }
        .modal-overlay { padding: .5rem; }
        .page-header { gap: .75rem; }
        .icon-button { width: 2.75rem; height: 2.75rem; }
        .sidebar-link { height: 2.75rem; }
        select, input:not([type="checkbox"]):not([type="radio"]) { min-height: 2.75rem; }
        .dialog-body { padding: .75rem; }
        .dialog-footer { bottom: -.75rem; margin: 0 -.75rem -.75rem; padding: .75rem; }
        .modal-content .grid-cols-2 { grid-template-columns: minmax(0, 1fr) !important; }
        .modal-content .grid-cols-12 > * { grid-column: 1 / -1 !important; }
        .modal-content .flex.items-end { align-items: stretch; flex-direction: column; }
        .modal-content .flex.items-end > button { width: 100%; }
        .primary-button, .outline-button { min-height: 2.75rem; }
    }
    @media (forced-colors: active) {
        input:focus-visible, select:focus-visible, textarea:focus-visible { outline: 2px solid Highlight !important; outline-offset: 2px; }
        .catalog-switch input { appearance: auto; }
    }
    @media (prefers-reduced-motion: reduce) {
        *, *::before, *::after { scroll-behavior: auto !important; animation-duration: .01ms !important; animation-iteration-count: 1 !important; transition-duration: .01ms !important; }
    }
    </style>"#
}
