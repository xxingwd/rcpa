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

pub fn render_shared_scripts() -> String {
    [
        render_theme_scripts(),
        r#"<script>
    const SidebarManager = {
        key: 'rcpa_sidebar_collapsed',
        init() {
            document.documentElement.classList.toggle('sidebar-collapsed', localStorage.getItem(this.key) === 'true');
        },
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
        open(html) {
            const backdrop = document.getElementById('modal-backdrop');
            const body = document.getElementById('modal-body');
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
        close() {
            const backdrop = document.getElementById('modal-backdrop');
            if (!backdrop) return;
            backdrop.hidden = true;
            document.getElementById('modal-body').innerHTML = '';
            document.body.classList.remove('modal-open');
        },
        load(url) {
            fetch(url, { credentials: 'include' })
                .then((response) => {
                    if (response.status === 401) return redirectToLogin();
                    if (!response.ok) throw new Error('加载失败');
                    return response.text();
                })
                .then((html) => { if (html) this.open(html); })
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
        if (event.key === 'Escape') {
            closeMobileNav();
            Modal.close();
        }
    });
    document.addEventListener('DOMContentLoaded', () => {
        SidebarManager.init();
        verifyAdminSession();
    });
    </script>"#,
    ]
    .concat()
}

pub fn render_shared_styles() -> &'static str {
    r#"<style>
    :root {
        --radius: 8px;
        --background: oklch(0.982 0.003 250);
        --foreground: oklch(0.22 0.008 250);
        --card: oklch(1 0 0);
        --card-foreground: oklch(0.22 0.008 250);
        --muted: oklch(0.955 0.004 250);
        --muted-foreground: oklch(0.52 0.01 250);
        --accent: oklch(0.94 0.006 250);
        --accent-foreground: oklch(0.25 0.01 250);
        --primary: oklch(0.42 0.075 250);
        --primary-foreground: oklch(0.985 0.003 250);
        --destructive: oklch(0.55 0.16 25);
        --border: oklch(0.895 0.006 250);
        --ring: oklch(0.52 0.08 250);
        --sidebar: oklch(0.968 0.003 250);
        --sidebar-foreground: oklch(0.24 0.008 250);
        --sidebar-accent: oklch(0.925 0.006 250);
        --sidebar-border: oklch(0.895 0.006 250);
        --workspace-gutter: .5rem;
        color-scheme: light;
    }
    html[data-theme="dark"] {
        --background: oklch(0.17 0.004 250);
        --foreground: oklch(0.9 0.004 250);
        --card: oklch(0.205 0.005 250);
        --card-foreground: oklch(0.9 0.004 250);
        --muted: oklch(0.255 0.005 250);
        --muted-foreground: oklch(0.66 0.006 250);
        --accent: oklch(0.285 0.006 250);
        --accent-foreground: oklch(0.9 0.004 250);
        --primary: oklch(0.7 0.08 245);
        --primary-foreground: oklch(0.17 0.004 250);
        --destructive: oklch(0.66 0.15 25);
        --border: oklch(0.32 0.006 250);
        --ring: oklch(0.7 0.08 245);
        --sidebar: oklch(0.185 0.004 250);
        --sidebar-foreground: oklch(0.9 0.004 250);
        --sidebar-accent: oklch(0.27 0.006 250);
        --sidebar-border: oklch(0.32 0.006 250);
        color-scheme: dark;
    }
    * { box-sizing: border-box; border-color: var(--border); }
    [hidden] { display: none !important; }
    html, body { min-height: 100%; }
    body {
        margin: 0;
        background: var(--background) !important;
        color: var(--foreground) !important;
        font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Noto Sans SC", sans-serif;
        -webkit-font-smoothing: antialiased;
    }
    button, input, select, textarea { font: inherit; }
    button, a { touch-action: manipulation; }
    button { cursor: pointer; }
    button:disabled { cursor: not-allowed; opacity: .5; }
    :focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
    ::-webkit-scrollbar { width: 5px; height: 5px; }
    ::-webkit-scrollbar-track { background: transparent; }
    ::-webkit-scrollbar-thumb { background: color-mix(in oklch, var(--muted-foreground) 28%, transparent); border-radius: 999px; }

    .icon { width: 1rem; height: 1rem; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.8; }
    .icon-button { display: inline-flex; width: 2.25rem; height: 2.25rem; align-items: center; justify-content: center; border: 0; border-radius: 6px; color: var(--muted-foreground); background: transparent; }
    .icon-button:hover { color: var(--foreground); background: var(--accent); }
    .brand { display: flex; min-width: 0; align-items: center; gap: .625rem; color: var(--sidebar-foreground); text-decoration: none; }
    .brand-mark { display: flex; width: 2rem; height: 2rem; flex: 0 0 2rem; align-items: center; justify-content: center; border: 1px solid var(--sidebar-border); border-radius: 6px; background: var(--card); font-size: .875rem; font-weight: 600; }
    .brand-name { font-size: 1rem; font-weight: 600; }

    .sidebar { position: sticky; top: 0; z-index: 30; display: flex; width: 13rem; height: 100dvh; flex: 0 0 13rem; flex-direction: column; padding: .75rem; border-right: 1px solid var(--sidebar-border); background: var(--sidebar); transition: width 200ms ease, flex-basis 200ms ease, transform 200ms ease; }
    .sidebar-brand-row { display: flex; min-height: 2rem; align-items: center; justify-content: space-between; gap: .625rem; margin: 0 .25rem 1.5rem; }
    .sidebar-nav { display: flex; flex-direction: column; gap: .25rem; }
    .sidebar-link { display: flex; height: 2.25rem; align-items: center; gap: .625rem; padding: 0 .75rem; border-radius: 6px; color: var(--muted-foreground); font-size: .875rem; font-weight: 500; text-decoration: none; transition: color 150ms ease, background 150ms ease; }
    .sidebar-link:hover { color: var(--sidebar-foreground); background: var(--accent); }
    .sidebar-link.active { color: var(--sidebar-foreground); background: var(--sidebar-accent); }
    .sidebar-link.active .icon { stroke-width: 2.2; }
    .sidebar-footer { display: flex; flex-direction: column; gap: .5rem; margin-top: auto; padding-top: .75rem; border-top: 1px solid var(--sidebar-border); }
    .sidebar-action { display: flex; width: 100%; height: 2rem; align-items: center; justify-content: center; gap: .5rem; border: 1px solid var(--border); border-radius: 6px; background: var(--card); color: var(--muted-foreground); font-size: .75rem; font-weight: 500; }
    .sidebar-action:hover { color: var(--foreground); background: var(--accent); }
    .sidebar-action.destructive { color: var(--destructive); border-color: color-mix(in oklch, var(--destructive) 25%, transparent); }
    .theme-toggle .theme-mode-icon { display: none; flex: 0 0 1rem; }
    html[data-theme-mode="system"] .theme-toggle .theme-mode-icon-system,
    html[data-theme-mode="light"] .theme-toggle .theme-mode-icon-light,
    html[data-theme-mode="dark"] .theme-toggle .theme-mode-icon-dark { display: block; }
    .collapse-right-icon { display: none; }

    html.sidebar-collapsed .sidebar { width: 4.25rem; flex-basis: 4.25rem; }
    html.sidebar-collapsed .sidebar-label { display: none; }
    html.sidebar-collapsed .sidebar-brand-row { justify-content: center; flex-wrap: wrap; margin-bottom: .75rem; }
    html.sidebar-collapsed .collapse-button { width: 100%; }
    html.sidebar-collapsed .collapse-left-icon { display: none; }
    html.sidebar-collapsed .collapse-right-icon { display: block; }
    html.sidebar-collapsed .sidebar-link { justify-content: center; padding: 0; }

    .mobile-header, .mobile-nav-scrim { display: none; }
    .menu-close-icon { display: none; }

    .admin-main { min-width: 0; height: 100dvh; flex: 1 1 auto; overflow: auto; padding: var(--workspace-gutter); }
    .admin-content { width: 100%; height: 100%; min-width: 0; margin: 0; }
    .page { display: flex; width: 100%; height: 100%; min-height: 0; flex-direction: column; gap: .75rem; }
    .page-header { display: flex; min-height: 2.75rem; flex: 0 0 auto; align-items: center; justify-content: space-between; gap: 1rem; padding: .25rem .25rem .625rem; border-bottom: 1px solid var(--border); }
    .page-heading { min-width: 0; }
    .page-title { margin: 0; font-size: 1.125rem; line-height: 1.5rem; font-weight: 600; letter-spacing: 0; }
    .page-description { margin: .125rem 0 0; color: var(--muted-foreground); font-size: .75rem; line-height: 1rem; }
    .page-actions { display: flex; min-width: 0; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: .5rem; }
    .page-body { min-width: 0; min-height: 0; flex: 1 1 auto; }
    .data-list { width: 100%; min-width: 0; min-height: 0; overflow: hidden; border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); background: color-mix(in oklch, var(--card) 72%, transparent); }
    .data-list.htmx-request { opacity: .72; }
    .list-loading { display: grid; min-height: 10rem; place-items: center; color: var(--muted-foreground); font-size: .875rem; }
    .primary-button, .outline-button { display: inline-flex; min-height: 2.25rem; align-items: center; justify-content: center; gap: .5rem; padding: .5rem 1rem; border-radius: 6px; font-size: .875rem; font-weight: 500; white-space: nowrap; transition: background 150ms ease, color 150ms ease; }
    .primary-button { border: 1px solid transparent; background: var(--primary); color: var(--primary-foreground); }
    .primary-button:hover { filter: brightness(.96); }
    .outline-button { border: 1px solid var(--border); background: var(--card); color: var(--foreground); }
    .outline-button:hover { background: var(--accent); }

    /* Existing page utilities are mapped onto the semantic palette. */
    .bg-white, .bg-zinc-50 { background-color: var(--card) !important; }
    .bg-zinc-50\/50 { background-color: color-mix(in oklch, var(--muted) 38%, transparent) !important; }
    .bg-zinc-100 { background-color: var(--muted) !important; }
    .bg-zinc-900 { background-color: var(--primary) !important; }
    .hover\:bg-zinc-800:hover { background-color: color-mix(in oklch, var(--primary) 95%, var(--background)) !important; }
    .text-zinc-900, .text-zinc-800, .text-zinc-700 { color: var(--foreground) !important; }
    .text-zinc-600, .text-zinc-500, .text-zinc-400 { color: var(--muted-foreground) !important; }
    .text-blue-600 { color: var(--primary) !important; }
    .bg-emerald-100 { background-color: color-mix(in oklch, #10b981 15%, transparent) !important; }
    .text-emerald-700, .text-emerald-600 { color: #10b981 !important; }
    .bg-red-100, .bg-red-50 { background-color: color-mix(in oklch, var(--destructive) 12%, transparent) !important; }
    .text-red-700, .text-red-600, .text-red-500 { color: var(--destructive) !important; }
    .border-red-200 { border-color: color-mix(in oklch, var(--destructive) 30%, transparent) !important; }
    .border-zinc-100, .border-zinc-200, .border-zinc-300, .divide-zinc-200 > :not(:last-child) { border-color: var(--border) !important; }
    .hover\:bg-zinc-50:hover, .hover\:bg-zinc-100:hover { background-color: var(--accent) !important; }
    select, input, textarea { border-color: var(--border) !important; background: var(--card) !important; color: var(--foreground) !important; }
    select { color-scheme: inherit; }
    table { border-collapse: collapse; }
    thead { color: var(--muted-foreground); }
    tbody tr:hover { background: color-mix(in oklch, var(--muted) 50%, transparent) !important; }
    .rounded-xl { border-radius: var(--radius) !important; }

    .toast-container { position: fixed; right: 1rem; bottom: 1rem; z-index: 1000; display: flex; max-width: calc(100vw - 2rem); flex-direction: column; gap: .5rem; pointer-events: none; }
    .toast { display: flex; min-width: 17rem; max-width: 25rem; align-items: center; justify-content: space-between; gap: .75rem; padding: .75rem 1rem; border: 1px solid var(--border); border-radius: 6px; background: var(--card); color: var(--foreground); box-shadow: 0 10px 30px rgb(0 0 0 / .14); pointer-events: auto; animation: toast-in 180ms ease-out; }
    .toast-success { border-color: color-mix(in oklch, #10b981 45%, var(--border)); }
    .toast-error { border-color: color-mix(in oklch, var(--destructive) 55%, var(--border)); }
    .toast-warning { border-color: color-mix(in oklch, #f59e0b 55%, var(--border)); }
    .toast-close { width: 1.75rem; height: 1.75rem; border: 0; background: transparent; color: inherit; font-size: 1.25rem; }
    @keyframes toast-in { from { opacity: 0; transform: translateY(.5rem); } to { opacity: 1; transform: translateY(0); } }

    .modal-open { overflow: hidden; }
    .modal-overlay { position: fixed; inset: 0; z-index: 100; display: grid; place-items: center; padding: 1rem; background: color-mix(in oklch, var(--background) 70%, transparent); backdrop-filter: blur(4px); }
    .modal-overlay[hidden], .modal-overlay.hidden { display: none; }
    .modal-content { position: relative; width: min(48rem, calc(100vw - 2rem)); max-height: calc(100dvh - 2rem); overflow: auto; padding: 0; border: 1px solid var(--border); border-radius: var(--radius); background: var(--card); color: var(--card-foreground); box-shadow: 0 24px 60px rgb(0 0 0 / .2); }
    .dialog-shell { min-width: 0; }
    .dialog-header { position: sticky; top: 0; z-index: 3; display: flex; min-height: 4rem; align-items: flex-start; justify-content: space-between; gap: 1rem; padding: 1rem; border-bottom: 1px solid var(--border); background: var(--card); }
    .dialog-heading { min-width: 0; }
    .dialog-title { margin: 0; font-size: 1rem; line-height: 1.5rem; font-weight: 600; letter-spacing: 0; }
    .dialog-description { margin: .125rem 0 0; color: var(--muted-foreground); font-size: .75rem; line-height: 1rem; }
    .dialog-close { flex: 0 0 2.25rem; margin: -.375rem -.375rem 0 0; }
    .dialog-body { min-width: 0; padding: 1rem; }
    .dialog-form { display: flex; flex-direction: column; gap: 1rem; }
    .dialog-footer { position: sticky; bottom: -1rem; z-index: 2; display: flex; justify-content: flex-end; gap: .5rem; margin: 0 -1rem -1rem; padding: 1rem; border-top: 1px solid var(--border); background: var(--card); }

    @media (max-width: 1023px) {
        .mobile-header { position: sticky; top: 0; z-index: 40; display: flex; width: 100%; height: 3.5rem; flex: 0 0 3.5rem; align-items: center; justify-content: space-between; padding: 0 .75rem; border-bottom: 1px solid var(--sidebar-border); background: var(--sidebar); }
        body > .flex.min-h-screen { min-height: 100dvh; flex-direction: column; }
        .sidebar { position: fixed; top: 0; right: 0; z-index: 60; width: 18rem; max-width: 84vw; height: 100dvh; flex-basis: auto; transform: translateX(100%); box-shadow: -12px 0 32px rgb(0 0 0 / .18); }
        .collapse-button { display: none; }
        .mobile-nav-scrim { position: fixed; inset: 0; z-index: 50; border: 0; background: rgb(0 0 0 / .45); }
        html.mobile-nav-open .sidebar { transform: translateX(0); }
        html.mobile-nav-open .mobile-nav-scrim { display: block; }
        html.mobile-nav-open .menu-open-icon { display: none; }
        html.mobile-nav-open .menu-close-icon { display: block; }
        html.sidebar-collapsed .sidebar { width: 18rem; }
        html.sidebar-collapsed .sidebar-label { display: inline; }
        html.sidebar-collapsed .sidebar-brand-row { justify-content: space-between; flex-wrap: nowrap; margin-bottom: 1.5rem; }
        html.sidebar-collapsed .sidebar-link { justify-content: flex-start; padding: 0 .75rem; }
        .admin-main { height: auto; min-height: 0; overflow: visible; }
        .admin-content { height: auto; min-height: calc(100dvh - 4.5rem); }
        .page-header { align-items: flex-start; flex-direction: column; }
        .page-actions { width: 100%; justify-content: flex-start; }
        .modal-content input, .modal-content select, .modal-content textarea, .modal-content button { min-height: 2.75rem; }
        .theme-toggle { min-height: 2.75rem; }
    }
    @media (max-width: 639px) {
        .dialog-body { padding: .75rem; }
        .dialog-footer { bottom: -.75rem; margin: 0 -.75rem -.75rem; padding: .75rem; }
        .modal-content .grid-cols-2 { grid-template-columns: minmax(0, 1fr) !important; }
        .modal-content .grid-cols-12 > * { grid-column: 1 / -1 !important; }
        .modal-content .flex.items-end { align-items: stretch; flex-direction: column; }
        .modal-content .flex.items-end > button { width: 100%; }
        .primary-button, .outline-button { min-height: 2.75rem; }
    }
    @media (prefers-reduced-motion: reduce) {
        *, *::before, *::after { scroll-behavior: auto !important; animation-duration: .01ms !important; animation-iteration-count: 1 !important; transition-duration: .01ms !important; }
    }
    </style>"#
}
