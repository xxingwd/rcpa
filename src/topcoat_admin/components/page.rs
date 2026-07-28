//! Shared page and list primitives for the admin interface.

use topcoat::{
    context::Cx,
    view::{component, view, View},
    Result,
};

/// Describes the stable shell shared by every admin page.
pub struct PageLayout<'a> {
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub class_name: &'a str,
    pub actions: Option<View>,
    pub body: View,
}

/// Describes a flat list surface, optionally backed by an HTMX fragment.
pub struct ListLayout<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub endpoint: Option<&'a str>,
    pub refresh_event: Option<&'a str>,
    pub body: View,
}

/// Describes the focused authentication surface used outside the admin shell.
pub struct AuthLayout<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub body: View,
}

/// A flat page shell with a consistent title, optional description and actions.
#[component]
pub async fn page(
    title: String,
    description: Option<String>,
    class_name: String,
    has_actions: bool,
    actions: View,
    child: View,
) -> Result {
    view! {
        <section class=(format!("page {class_name}"))>
            <header class="page-header">
                <div class="page-heading">
                    <h1 class="page-title">(title)</h1>
                    if let Some(description) = description {
                        <p class="page-description">(description)</p>
                    }
                </div>
                if has_actions {
                    <div class="page-actions">(actions)</div>
                }
            </header>
            <div class="page-body">(child)</div>
        </section>
    }
}

/// A list surface with one explicit refresh contract.
#[component]
pub async fn list(
    id: String,
    label: String,
    endpoint: Option<String>,
    refresh_event: Option<String>,
    child: View,
) -> Result {
    if let (Some(endpoint), Some(refresh_event)) = (endpoint, refresh_event) {
        let trigger = format!("load, {refresh_event} from:body");
        view! {
            <section
                id=(id)
                class="data-list"
                aria-label=(label)
                aria-live="polite"
                data-list-endpoint=(endpoint.clone())
                hx-get=(endpoint)
                hx-trigger=(trigger)
                hx-swap="innerHTML"
            >
                (child)
            </section>
        }
    } else {
        view! {
            <section id=(id) class="data-list" aria-label=(label)>(child)</section>
        }
    }
}

/// A focused authentication panel with the same typography and icon language.
#[component]
pub async fn auth_panel(title: String, description: String, child: View) -> Result {
    view! {
        <section class="auth-panel" aria-labelledby="login-title">
            <header class="login-header">
                <div class="login-mark">
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M20 13c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V5l8-3 8 3v8Z"/><path d="m9 12 2 2 4-4"/></svg>
                </div>
                <h1 id="login-title" class="login-title">(title)</h1>
                <p class="login-subtitle">(description)</p>
            </header>
            <div class="login-content">(child)</div>
        </section>
    }
}

/// Renders the shared page component around Topcoat views.
pub async fn render_page(cx: &Cx, layout: PageLayout<'_>) -> Result<String> {
    let __cx = cx;
    let has_actions = layout.actions.is_some();
    let actions = layout.actions.unwrap_or_else(View::empty);
    let rendered: Result = view! {
        page(
            title: layout.title.to_owned(),
            description: layout.description.map(str::to_owned),
            class_name: layout.class_name.to_owned(),
            has_actions: has_actions,
            actions: actions,
            (layout.body)
        )
    };

    Ok(rendered?.render(cx))
}

/// Renders the shared list component around a Topcoat view.
pub async fn render_list(cx: &Cx, layout: ListLayout<'_>) -> Result<View> {
    let __cx = cx;
    let rendered: Result = view! {
        list(
            id: layout.id.to_owned(),
            label: layout.label.to_owned(),
            endpoint: layout.endpoint.map(str::to_owned),
            refresh_event: layout.refresh_event.map(str::to_owned),
            (layout.body)
        )
    };

    rendered
}

/// Renders the authentication component around a Topcoat view.
pub async fn render_auth_panel(cx: &Cx, layout: AuthLayout<'_>) -> Result<String> {
    let __cx = cx;
    let rendered: Result = view! {
        auth_panel(
            title: layout.title.to_owned(),
            description: layout.description.to_owned(),
            (layout.body)
        )
    };

    Ok(rendered?.render(cx))
}
