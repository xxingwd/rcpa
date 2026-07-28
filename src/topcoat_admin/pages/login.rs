use topcoat::{context::Cx, router::page, view::view, Result};

use crate::topcoat_admin::{
    app::app_state, render_auth_panel, render_shared_styles, render_theme_bootstrap,
    render_theme_scripts, render_theme_toggle, trusted_html, AuthLayout,
};

#[page("/login")]
pub async fn login_page(cx: &Cx) -> Result {
    if crate::admin::check_admin(app_state(cx), topcoat::router::headers(cx)).is_ok() {
        return Err(topcoat::router::error::redirect("/dashboard").into());
    }

    let auth_body: Result = view! {
        <form id="login-form" class="login-form">
            <div class="field-group">
                <label class="field-label" for="token-input">"Token"</label>
                <input id="token-input" class="login-input" type="password" name="token" placeholder="请输入控制台 Token..." autocomplete="current-password" required="" autofocus="">
            </div>
            <div id="login-error" class="login-error" role="alert" hidden=""></div>
            <button id="login-btn" class="primary-button login-submit" type="submit">"登录"</button>
        </form>
    };
    let auth_html = render_auth_panel(
        cx,
        AuthLayout {
            title: "RCPA 管理登录",
            description: "输入控制台 Token 继续",
            body: auth_body?,
        },
    )
    .await?;
    let theme_bootstrap = render_theme_bootstrap();
    let theme_toggle = render_theme_toggle(cx, "login-theme").await?;
    let theme_scripts = render_theme_scripts();

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RCPA 管理登录</title>
    {theme_bootstrap}
    <link rel="stylesheet" href="/_topcoat/tailwind.css">
    {}
    <style>
        .login-page {{ position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; padding: 1rem; background: var(--background); }}
        .login-theme {{ position: absolute; top: 1.5rem; right: 1.5rem; width: auto; min-width: 7.5rem; }}
        .auth-panel {{ width: 100%; max-width: 25rem; padding: 1.5rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--card); color: var(--card-foreground); animation: login-in 300ms ease-out; }}
        .login-header {{ padding-bottom: .5rem; text-align: center; }}
        .login-mark {{ display: flex; width: 2.75rem; height: 2.75rem; align-items: center; justify-content: center; margin: 0 auto 1rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--muted); color: var(--primary); }}
        .login-mark .icon {{ width: 1.3125rem; height: 1.3125rem; }}
        .login-title {{ margin: 0; font-size: 1.25rem; line-height: 1.5; font-weight: 600; letter-spacing: 0; }}
        .login-subtitle {{ margin: .125rem 0 0; color: var(--muted-foreground); font-size: .875rem; }}
        .login-content {{ padding-top: 1.5rem; }}
        .login-form {{ display: flex; flex-direction: column; gap: 1rem; }}
        .field-group {{ display: flex; flex-direction: column; gap: .5rem; }}
        .field-label {{ color: var(--muted-foreground); font-size: .75rem; font-weight: 500; text-transform: uppercase; letter-spacing: 0; }}
        .login-input {{ display: flex; width: 100%; height: 2.25rem; padding: .25rem .75rem; border: 1px solid var(--border); border-radius: 6px; background: var(--card); color: var(--foreground); font-size: .875rem; }}
        .login-input::placeholder {{ color: var(--muted-foreground); }}
        .login-error {{ padding: .5rem .75rem; border: 1px solid color-mix(in oklch, var(--destructive) 22%, transparent); border-radius: 6px; background: color-mix(in oklch, var(--destructive) 10%, transparent); color: var(--destructive); font-size: .75rem; font-weight: 500; }}
        .login-submit {{ width: 100%; height: 2.5rem; }}
        @keyframes login-in {{ from {{ opacity: 0; }} to {{ opacity: 1; }} }}
        @media (max-width: 480px) {{ .login-theme {{ top: 1rem; right: 1rem; }} }}
    </style>
</head>
<body>
    <main class="login-page">
        {theme_toggle}
        {}
    </main>
    {theme_scripts}
    <script>
        function showLoginError(message) {{
            const error = document.getElementById('login-error');
            error.textContent = message;
            error.hidden = false;
        }}
        document.getElementById('login-form').addEventListener('submit', async (event) => {{
            event.preventDefault();
            const token = document.getElementById('token-input').value.trim();
            if (!token) return showLoginError('请输入控制台 Token');

            const button = document.getElementById('login-btn');
            const error = document.getElementById('login-error');
            error.hidden = true;
            button.disabled = true;
            button.textContent = '验证中...';
            try {{
                const response = await fetch('/v1/admin/login', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    credentials: 'include',
                    body: JSON.stringify({{ token }})
                }});
                if (!response.ok) throw new Error('认证失败，请检查控制台 Token');
                const target = new URLSearchParams(window.location.search).get('next');
                window.location.replace(target && target.startsWith('/') && !target.startsWith('//') ? target : '/dashboard');
            }} catch (cause) {{
                showLoginError(cause.message || '无法连接至网关服务');
                button.disabled = false;
                button.textContent = '登录';
                document.getElementById('token-input').focus();
            }}
        }});
    </script>
</body>
</html>"##,
        render_shared_styles(),
        auth_html
    );

    Ok(trusted_html(html))
}
