use std::{collections::HashMap, sync::Arc};

use rcpa::{
    config::{
        AppConfig, AuthKey, CostConfig, EndpointConfig, ModelPricing, ModelRule, ProviderConfig,
        ProviderProtocol, RetryConfig, RoutingConfig, StickyConfig, UpstreamConfig,
    },
    server::AppState,
    store::NewRequestLog,
    topcoat_admin::build_topcoat_app,
};

fn empty_config() -> AppConfig {
    AppConfig {
        providers: Vec::new(),
        upstream: UpstreamConfig { timeout_secs: 60 },
        routing: RoutingConfig {
            sticky: StickyConfig::default(),
        },
        retry: RetryConfig {
            max_attempts: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 10_000,
            retryable_statuses: vec![429, 502, 503, 504],
        },
        cost: CostConfig {
            currency: "USD".to_string(),
            default_input_per_1k: 0.0,
            default_output_per_1k: 0.0,
            models: HashMap::new(),
        },
        keys: Vec::new(),
    }
}

fn populated_config() -> AppConfig {
    let mut config = empty_config();
    config.providers.push(ProviderConfig {
        name: "fixture-provider".to_string(),
        api_key: "fixture-secret".to_string(),
        models: vec![ModelRule {
            name: "upstream-model".to_string(),
            status: "enabled".to_string(),
            pricing: Some(ModelPricing {
                input_per_1k: 0.125,
                output_per_1k: 0.25,
            }),
            aliases: vec!["public-model".to_string()],
        }],
        endpoints: vec![EndpointConfig {
            protocol: ProviderProtocol::Responses,
            base_url: "https://fixture.example/v1/responses".to_string(),
        }],
        headers: HashMap::from([("X-Fixture".to_string(), "fixture-header".to_string())]),
        status: "enabled".to_string(),
        priority: 7,
    });
    config.keys.push(AuthKey {
        id: "fixture-key".to_string(),
        name: Some("Fixture Key".to_string()),
        key: "rcpa_fixture_secret".to_string(),
        models: vec![ModelRule {
            name: "public-model".to_string(),
            status: "disabled".to_string(),
            pricing: None,
            aliases: Vec::new(),
        }],
        model_aliases: HashMap::from([("fast-fixture".to_string(), "public-model".to_string())]),
        allowed_providers: vec!["fixture-provider".to_string()],
        status: "enabled".to_string(),
        labels: Some("fixture-label".to_string()),
    });
    config
}

#[tokio::test]
async fn admin_pages_require_cookie_and_render_the_shared_rust_shell() {
    let state = Arc::new(AppState::from_config(populated_config()).await.unwrap());
    state
        .store
        .insert_request_log_entry(NewRequestLog {
            request_id: "fixture-request",
            api_key_id: "fixture-key",
            session_hash: None,
            provider_name: "fixture-provider",
            protocol: "responses",
            model: "upstream-model",
            operation: "responses",
            status_code: 200,
            success: true,
            input_tokens: 8,
            output_tokens: 4,
            total_tokens: 12,
            cached_tokens: 2,
            cache_write_tokens: 0,
            cost_cents: 1,
            latency_ms: 80,
            first_byte_latency_ms: 40,
            metadata_json: "{}",
            request_body: None,
            response_body: None,
        })
        .await
        .unwrap();
    let app = build_topcoat_app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        topcoat::serve_until(listener, app, async {
            let _ = shutdown_rx.await;
        })
        .await
        .unwrap();
    });

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let base = format!("http://{address}");

    for (path, content_type) in [
        ("/_topcoat/tailwind.css", "text/css; charset=utf-8"),
        (
            "/_topcoat/htmx.min.js",
            "application/javascript; charset=utf-8",
        ),
        (
            "/_topcoat/chart.umd.min.js",
            "application/javascript; charset=utf-8",
        ),
        (
            "/_topcoat/codemirror/codemirror.min.js",
            "application/javascript; charset=utf-8",
        ),
        (
            "/_topcoat/codemirror/codemirror.min.css",
            "text/css; charset=utf-8",
        ),
        (
            "/_topcoat/codemirror/yaml.min.js",
            "application/javascript; charset=utf-8",
        ),
        (
            "/_topcoat/codemirror/dracula.min.css",
            "text/css; charset=utf-8",
        ),
    ] {
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert!(response.status().is_success(), "path: {path}");
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            content_type,
            "path: {path}"
        );
        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "public, max-age=31536000, immutable",
            "path: {path}"
        );
        assert!(!response.bytes().await.unwrap().is_empty(), "path: {path}");
    }

    for path in ["/admin", "/assets/legacy.js"] {
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 404, "path: {path}");
    }

    for path in [
        "/dashboard",
        "/keys",
        "/keys/table",
        "/keys/new",
        "/keys/missing/edit",
        "/providers/table",
        "/providers/new",
        "/providers/missing/edit",
        "/providers/missing/copy",
        "/logs/table",
        "/config",
        "/dashboard/analytics",
    ] {
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 307, "path: {path}");
        assert_eq!(
            response.headers().get("location").unwrap(),
            "/login",
            "path: {path}"
        );
    }

    let root = client.get(&base).send().await.unwrap();
    assert_eq!(root.status().as_u16(), 307);
    assert_eq!(root.headers().get("location").unwrap(), "/dashboard");

    let login_page = client.get(format!("{base}/login")).send().await.unwrap();
    assert!(login_page.status().is_success());
    let login_html = login_page.text().await.unwrap();
    assert!(login_html.contains("RCPA 管理登录"));
    assert!(login_html.contains("class=\"auth-panel\""));
    assert!(login_html.contains("const mode = stored === 'light'"));
    assert!(login_html.contains("? stored : 'system'"));
    assert_eq!(
        login_html
            .matches("data-theme-control=\"cycle\" data-theme-mode=\"system\"")
            .count(),
        1
    );
    assert!(login_html.contains("onclick=\"cycleThemeMode()\""));
    assert!(login_html.contains("order: ['system', 'light', 'dark']"));
    assert!(login_html.contains("this.set(this.order[(currentIndex + 1) % this.order.length])"));
    assert!(!login_html.contains("class=\"theme-switcher"));
    assert!(!login_html.contains("aria-pressed="));
    assert!(login_html.contains("prefers-color-scheme: dark"));
    assert!(!login_html.contains("cdn.tailwindcss.com"));

    let login = client
        .post(format!("{base}/v1/admin/login"))
        .json(&serde_json::json!({ "token": "admin-token" }))
        .send()
        .await
        .unwrap();
    assert!(login.status().is_success());
    let cookie = login
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let authenticated_login = client
        .get(format!("{base}/login"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(authenticated_login.status().as_u16(), 307);
    assert_eq!(
        authenticated_login.headers().get("location").unwrap(),
        "/dashboard"
    );

    let oversized_response = client
        .post(format!("{base}/v1/responses"))
        .header("x-api-key", "rcpa_fixture_secret")
        .json(&serde_json::json!({
            "model": "missing-model",
            "input": "x".repeat(2 * 1024 * 1024),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized_response.status().as_u16(), 404);

    let logs_table = client
        .get(format!("{base}/logs/table"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert!(logs_table.status().is_success());
    let logs_table_html = logs_table.text().await.unwrap();
    assert!(logs_table_html.contains("Fixture Key"));
    assert!(!logs_table_html.contains("fixture-key"));

    let dashboard_analytics = client
        .get(format!("{base}/dashboard/analytics"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert!(dashboard_analytics.status().is_success());
    let dashboard_analytics: serde_json::Value =
        serde_json::from_str(&dashboard_analytics.text().await.unwrap()).unwrap();
    assert_eq!(dashboard_analytics["by_key"][0]["group_key"], "Fixture Key");

    for path in ["/dashboard", "/keys", "/providers", "/logs", "/config"] {
        let response = client
            .get(format!("{base}{path}"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "path: {path}");
        let html = response.text().await.unwrap();
        assert!(html.contains("class=\"sidebar\""), "path: {path}");
        assert!(html.contains("class=\"mobile-header\""), "path: {path}");
        assert!(html.contains("/_topcoat/tailwind.css"), "path: {path}");
        assert!(html.contains("class=\"page "), "path: {path}");
        assert!(html.contains("class=\"page-header\""), "path: {path}");
        assert!(html.contains("class=\"page-title\""), "path: {path}");
        assert_eq!(
            html.matches("data-theme-control=\"cycle\" data-theme-mode=\"system\"")
                .count(),
            1,
            "path: {path}"
        );
        assert!(
            html.contains("onclick=\"cycleThemeMode()\""),
            "path: {path}"
        );
        assert!(html.contains("order: ['system', 'light', 'dark']"));
        assert!(!html.contains("class=\"theme-switcher"), "path: {path}");
        assert!(!html.contains("aria-pressed="), "path: {path}");
        assert!(html.contains("this.mode = this.normalize(localStorage.getItem(this.key))"));
        assert!(html.contains("this.media.addEventListener('change', onSystemChange)"));
        assert!(
            !html.contains("<section class=\"admin-card"),
            "path: {path}"
        );
        assert!(!html.contains("cdn.tailwindcss.com"), "path: {path}");

        if path == "/keys" {
            assert!(html.contains("id=\"keys-list\""));
            assert!(html.contains("hx-trigger=\"load, rcpa-keys-refresh from:body\""));
            assert!(html.contains("refreshData('keys')"));
        }
        if path == "/dashboard" {
            assert!(html.contains("animation: false"));
            assert!(html.contains("tokenChart.update('none')"));
            assert!(html.contains("requestChart.update('none')"));
            assert!(html.contains("if (analyticsLoading) return Promise.resolve()"));
            assert!(html.contains("formatTimelineLabel(b.label || b.group_key)"));
            assert!(html.contains("const SHANGHAI_OFFSET_MS = 8 * 60 * 60 * 1000"));
            assert!(html.contains("chartDataSignature === nextSignature"));
            assert!(!html.contains("date.getFullYear()"));
            assert!(!html.contains("tokenChart.destroy()"));
            assert!(!html.contains("requestChart.destroy()"));
        }
        if path == "/providers" {
            assert!(html.contains("id=\"providers-list\""));
            assert!(html.contains("hx-trigger=\"load, rcpa-providers-refresh from:body\""));
            assert!(html.contains("refreshData('providers')"));
            assert!(html.contains("data.priority = Number.isFinite(priority) ? priority : 0"));
            assert!(html.contains("pricing: hasPricing ?"));
        }
        if path == "/logs" {
            assert!(html.contains("id=\"logs-list\""));
            assert!(html.contains("class=\"dialog-shell log-dialog\""));
            assert!(html.contains(".logs-page { min-width: 0; overflow: hidden; }"));
            assert!(html.contains("timeZone: 'Asia/Shanghai'"));
            assert!(html.contains("let logsRequest = null"));
        }
    }

    for (path, title) in [
        ("/keys/new", "生成 API 密钥"),
        ("/keys/missing/edit", "编辑 API 密钥"),
        ("/providers/new", "注册新供应商"),
        ("/providers/missing/edit", "编辑供应商"),
        ("/providers/missing/copy", "复制供应商"),
    ] {
        let response = client
            .get(format!("{base}{path}"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "path: {path}");
        let html = response.text().await.unwrap();
        assert!(html.contains("class=\"dialog-shell "), "path: {path}");
        assert!(html.contains("class=\"dialog-header\""), "path: {path}");
        assert!(html.contains("class=\"dialog-body\""), "path: {path}");
        assert!(html.contains(title), "path: {path}");
        assert_eq!(
            html.matches("class=\"icon-button dialog-close\"").count(),
            1
        );
        assert!(!html.contains("<!DOCTYPE"), "path: {path}");

        if path == "/providers/new" {
            assert!(html.contains("var endpointIndex = 100"));
            assert!(html.contains("function addEndpointRow()"));
        }
    }

    for path in [
        "/providers/fixture-provider/edit",
        "/providers/fixture-provider/copy",
    ] {
        let response = client
            .get(format!("{base}{path}"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "path: {path}");
        let html = response.text().await.unwrap();
        assert!(html.contains("value=\"fixture-secret\""), "path: {path}");
        assert!(
            html.contains("value=\"https://fixture.example/v1/responses\""),
            "path: {path}"
        );
        assert!(
            html.contains("<option value=\"responses\" selected>"),
            "path: {path}"
        );
        assert!(
            html.contains("name=\"priority\" value=\"7\""),
            "path: {path}"
        );
        assert!(
            html.contains("<option value=\"enabled\" selected>"),
            "path: {path}"
        );
        assert!(html.contains("value=\"X-Fixture\""), "path: {path}");
        assert!(html.contains("value=\"fixture-header\""), "path: {path}");
        assert!(html.contains("value=\"upstream-model\""), "path: {path}");
        assert!(html.contains("value=\"public-model\""), "path: {path}");
        assert!(html.contains("value=\"0.125\""), "path: {path}");
        assert!(html.contains("value=\"0.25\""), "path: {path}");

        if path.ends_with("/edit") {
            assert!(html.contains("value=\"fixture-provider\""));
            assert!(html.contains("readonly"));
        } else {
            assert!(html.contains("name=\"name\" value=\"\""));
        }
    }

    let key_edit = client
        .get(format!("{base}/keys/fixture-key/edit"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert!(key_edit.status().is_success());
    let key_html = key_edit.text().await.unwrap();
    assert!(key_html.contains("value=\"rcpa_fixture_secret\""));
    assert!(key_html.contains("value=\"Fixture Key\""));
    assert!(key_html.contains("value=\"fixture-provider\" class=\"provider-check"));
    assert!(key_html.contains("provider-check mt-0.5 h-4 w-4 rounded border-zinc-300\" checked"));
    assert!(key_html.contains("value=\"public-model\""));
    assert!(key_html.contains("value=\"fast-fixture\""));
    assert!(key_html.contains("value=\"fixture-label\""));

    let _ = shutdown_tx.send(());
    server.await.unwrap();
}
