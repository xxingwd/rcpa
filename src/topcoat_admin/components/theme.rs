//! Theme controls and the small bootstrap needed before the page is painted.

use topcoat::{
    context::Cx,
    view::{component, view},
    Result,
};

/// A single theme button that cycles through system, light and dark modes.
#[component]
pub async fn theme_toggle(class_name: &str) -> Result {
    view! {
        <button
            class=(format!("sidebar-action theme-toggle {class_name}"))
            type="button"
            data-theme-control="cycle"
            data-theme-mode="system"
            onclick="cycleThemeMode()"
            aria-label="当前：跟随系统；点击切换为亮色主题"
            title="当前：跟随系统；点击切换为亮色主题"
        >
            <svg class="icon theme-mode-icon theme-mode-icon-system" aria-hidden="true" viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/></svg>
            <svg class="icon theme-mode-icon theme-mode-icon-light" aria-hidden="true" viewBox="0 0 24 24"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.42 1.42M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.42-1.42M17.66 6.34l1.41-1.41"/></svg>
            <svg class="icon theme-mode-icon theme-mode-icon-dark" aria-hidden="true" viewBox="0 0 24 24"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>
            <span class="theme-label sidebar-label">"跟随系统"</span>
        </button>
    }
}

pub async fn render_theme_toggle(cx: &Cx, class_name: &str) -> Result<String> {
    let __cx = cx;
    let rendered: Result = view! { theme_toggle(class_name: class_name) };
    Ok(rendered?.render(cx))
}

pub fn render_theme_bootstrap() -> &'static str {
    r#"<script>
    (() => {
        const stored = localStorage.getItem('rcpa-theme');
        const mode = stored === 'light' || stored === 'dark' || stored === 'system' ? stored : 'system';
        const effective = mode === 'system'
            ? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
            : mode;
        document.documentElement.dataset.themeMode = mode;
        document.documentElement.dataset.theme = effective;
    })();
    </script>"#
}

pub fn render_theme_scripts() -> &'static str {
    r#"<script>
    const ThemeManager = {
        key: 'rcpa-theme',
        mode: 'system',
        order: ['system', 'light', 'dark'],
        metadata: {
            system: { label: '跟随系统', nextLabel: '亮色主题' },
            light: { label: '亮色', nextLabel: '暗色主题' },
            dark: { label: '暗色', nextLabel: '跟随系统' }
        },
        media: window.matchMedia('(prefers-color-scheme: dark)'),
        normalize(mode) {
            return this.order.includes(mode) ? mode : 'system';
        },
        init() {
            this.mode = this.normalize(localStorage.getItem(this.key));
            const onSystemChange = () => {
                if (this.mode === 'system') this.apply('system', false);
            };
            if (this.media.addEventListener) this.media.addEventListener('change', onSystemChange);
            else this.media.addListener(onSystemChange);
            this.apply(this.mode, false);
        },
        cycle() {
            const currentIndex = this.order.indexOf(this.mode);
            this.set(this.order[(currentIndex + 1) % this.order.length]);
        },
        set(mode) {
            this.apply(this.normalize(mode), true);
        },
        apply(mode, persist) {
            const effective = mode === 'system' ? (this.media.matches ? 'dark' : 'light') : mode;
            const metadata = this.metadata[mode];
            this.mode = mode;
            document.documentElement.dataset.themeMode = mode;
            document.documentElement.dataset.theme = effective;
            if (persist) localStorage.setItem(this.key, mode);
            document.querySelectorAll('[data-theme-control="cycle"]').forEach((button) => {
                button.dataset.themeMode = mode;
                button.setAttribute('aria-label', `当前：${metadata.label}；点击切换为${metadata.nextLabel}`);
                button.title = `当前：${metadata.label}；点击切换为${metadata.nextLabel}`;
                const label = button.querySelector('.theme-label');
                if (label) label.textContent = metadata.label;
            });
            window.dispatchEvent(new CustomEvent('rcpa:theme-change', {
                detail: { mode, theme: effective }
            }));
        }
    };
    function cycleThemeMode() { ThemeManager.cycle(); }
    document.addEventListener('DOMContentLoaded', () => ThemeManager.init());
    </script>"#
}
