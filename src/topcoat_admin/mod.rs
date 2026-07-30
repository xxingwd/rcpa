//! Topcoat-based admin UI for RCPA.
//!
//! This module provides a server-rendered admin interface using the Topcoat
//! framework. It bridges to the existing axum-based API routes via TowerRoute.

pub mod api;
pub mod app;
pub mod components;
pub mod pages;

pub use app::build_topcoat_app;

pub use components::modal::{render_dialog, DialogLayout};
pub use components::page::{
    render_auth_panel, render_list, render_page, AuthLayout, ListLayout, PageLayout,
};

// Re-export shared components for convenience
pub use components::sidebar::{
    render_modal_backdrop, render_shared_scripts, render_shared_styles, render_sidebar,
    render_sidebar_bootstrap, render_toast_container,
};
pub use components::theme::{render_theme_bootstrap, render_theme_scripts, render_theme_toggle};

pub(crate) fn trusted_html(value: String) -> topcoat::view::View {
    use topcoat::view::{HtmlContext, PartsWriter, View, ViewParts};

    let mut parts = ViewParts::new();
    PartsWriter::new(&mut parts, HtmlContext::Text).push_str_unescaped(value);
    View::new(parts)
}

pub(crate) fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

pub(crate) fn escape_inline_js_string(value: &str) -> String {
    escape_html(&serde_json::to_string(value).expect("serializing a string cannot fail"))
}

pub(crate) fn format_duration_ms(milliseconds: f64) -> String {
    let milliseconds = if milliseconds.is_finite() {
        milliseconds.max(0.0)
    } else {
        0.0
    };
    if milliseconds < 1_000.0 {
        return format!("{:.0}ms", milliseconds.round().min(999.0));
    }

    let mut seconds = format!("{:.2}", milliseconds / 1_000.0);
    while seconds.ends_with('0') {
        seconds.pop();
    }
    if seconds.ends_with('.') {
        seconds.pop();
    }
    format!("{seconds}s")
}

const SHANGHAI_UTC_OFFSET_SECONDS: i32 = 8 * 60 * 60;

fn shanghai_time(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    let offset = chrono::FixedOffset::east_opt(SHANGHAI_UTC_OFFSET_SECONDS)?;
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&offset))
}

pub(crate) fn format_shanghai_time_short(value: &str) -> String {
    shanghai_time(value)
        .map(|time| time.format("%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| value.to_string())
}

pub(crate) fn format_shanghai_time_full(value: &str) -> String {
    shanghai_time(value)
        .map(|time| time.format("%Y-%m-%d %H:%M:%S UTC+8").to_string())
        .unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        escape_html, escape_inline_js_string, format_duration_ms, format_shanghai_time_full,
        format_shanghai_time_short, render_shared_styles,
    };

    #[test]
    fn escapes_dynamic_html_and_inline_javascript_strings() {
        assert_eq!(
            escape_html("<script title=\"x\">'&"),
            "&lt;script title=&quot;x&quot;&gt;&#39;&amp;"
        );
        assert_eq!(
            escape_inline_js_string("a'\"</script>"),
            "&quot;a&#39;\\&quot;&lt;/script&gt;&quot;"
        );
    }

    #[test]
    fn admin_workspace_uses_shared_full_width_layout_tokens() {
        let styles = render_shared_styles();

        assert!(styles.contains("--workspace-gutter: .5rem"));
        assert!(styles
            .contains(".admin-content { width: 100%; height: 100%; min-width: 0; margin: 0; }"));
        assert!(styles.contains(".page-header {"));
        assert!(styles.contains(".data-list {"));
        assert!(styles.contains(".dialog-header {"));
        assert!(styles.contains(".dialog-footer {"));
        assert!(!styles.contains(".admin-card"));
        assert!(!styles.contains("max-width: 80rem"));
    }

    #[test]
    fn formats_admin_timestamps_in_shanghai_time() {
        assert_eq!(
            format_shanghai_time_short("2026-07-28T16:30:45Z"),
            "07-29 00:30:45"
        );
        assert_eq!(
            format_shanghai_time_full("2026-07-28T16:30:45+00:00"),
            "2026-07-29 00:30:45 UTC+8"
        );
        assert_eq!(format_shanghai_time_short("invalid"), "invalid");
    }

    #[test]
    fn formats_admin_durations_with_compact_units() {
        assert_eq!(format_duration_ms(0.0), "0ms");
        assert_eq!(format_duration_ms(999.9), "999ms");
        assert_eq!(format_duration_ms(1_000.0), "1s");
        assert_eq!(format_duration_ms(1_250.0), "1.25s");
        assert_eq!(format_duration_ms(12_500.0), "12.5s");
        assert_eq!(format_duration_ms(f64::NAN), "0ms");
    }
}
