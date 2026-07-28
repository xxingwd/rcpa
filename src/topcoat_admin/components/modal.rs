//! Shared dialog primitive for server-rendered modal fragments.

use topcoat::{
    context::Cx,
    view::{component, view, View},
    Result,
};

pub use super::sidebar::render_modal_backdrop;

/// Describes content rendered inside the shared modal backdrop.
pub struct DialogLayout<'a> {
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub class_name: &'a str,
    pub body: View,
}

/// A consistent shell for create, edit, copy and detail dialogs.
#[component]
pub async fn dialog(
    title: String,
    description: Option<String>,
    class_name: String,
    child: View,
) -> Result {
    view! {
        <article class=(format!("dialog-shell {class_name}"))>
            <header class="dialog-header">
                <div class="dialog-heading">
                    <h2 id="dialog-title" class="dialog-title">(title)</h2>
                    if let Some(description) = description {
                        <p class="dialog-description">(description)</p>
                    }
                </div>
                <button class="icon-button dialog-close" type="button" onclick="closeModal()" aria-label="关闭" title="关闭">
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M18 6 6 18M6 6l12 12"/></svg>
                </button>
            </header>
            <div class="dialog-body">(child)</div>
        </article>
    }
}

/// Renders a dialog component around a Topcoat view.
pub async fn render_dialog(cx: &Cx, layout: DialogLayout<'_>) -> Result<View> {
    let __cx = cx;
    let rendered: Result = view! {
        dialog(
            title: layout.title.to_owned(),
            description: layout.description.map(str::to_owned),
            class_name: layout.class_name.to_owned(),
            (layout.body)
        )
    };

    rendered
}
