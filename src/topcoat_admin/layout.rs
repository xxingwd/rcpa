use topcoat::{
    Result,
    context::Cx,
    view::{component, view, View},
};

/// The root layout for all admin pages.
#[component]
pub async fn admin_layout(cx: &Cx, child: View) -> Result {
    // Check if this is an htmx request for partial rendering
    let is_htmx = topcoat::htmx::hx_request(cx);
    
    if is_htmx {
        // For htmx requests, return only the content (no full page wrapper)
        return Ok(child);
    }
    
    // Full page render
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"RCPA Admin"</title>
                <script src="https://cdn.tailwindcss.com"></script>
                <script src="https://unpkg.com/htmx.org@2.0.4"></script>
                <style>
                    "
                    [x-cloak] { display: none !important; }
                    .htmx-indicator { opacity: 0; transition: opacity 200ms; }
                    .htmx-request .htmx-indicator { opacity: 1; }
                    .htmx-request.htmx-indicator { opacity: 1; }
                    "
                </style>
            </head>
            <body class="bg-zinc-50 text-zinc-900" hx-boost="true">
                (child)
            </body>
        </html>
    }
}
