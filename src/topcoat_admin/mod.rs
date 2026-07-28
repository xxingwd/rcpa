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
    render_toast_container,
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

#[cfg(test)]
mod tests {
    use super::{escape_html, escape_inline_js_string, render_shared_styles};

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
}
