use std::sync::Arc;

use axum::{body::Body, response::Response};
use topcoat::{
    context::Cx,
    router::{Methods, Path, Router, RouterBuilderDiscoverExt},
};
use topcoat_router::tower::TowerRoute;

use crate::server::AppState;

const TAILWIND_CSS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tailwind.css"));
const HTMX_JS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/static/htmx/htmx.min.js"
));
const CHART_JS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/static/chartjs/chart.umd.min.js"
));
const CODEMIRROR_JS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/static/codemirror/codemirror.min.js"
));
const CODEMIRROR_CSS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/static/codemirror/codemirror.min.css"
));
const CODEMIRROR_YAML_JS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/static/codemirror/yaml.min.js"
));
const CODEMIRROR_DRACULA_CSS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/static/codemirror/dracula.min.css"
));

fn static_asset_response(contents: &'static [u8], content_type: &'static str) -> Response {
    Response::builder()
        .status(200)
        .header("content-type", content_type)
        .header("cache-control", "public, max-age=31536000, immutable")
        .body(Body::from(contents))
        .expect("static asset response must be valid")
}

/// Builds the Topcoat router that serves the admin UI and bridges API calls
/// to the existing axum-based handlers.
pub fn build_topcoat_app(state: Arc<AppState>) -> Router {
    // Build the API router (handles /models, /chat/completions, /admin/*, etc.)
    let api = crate::server::router::build_api_router(state.clone());
    // Nest it at /v1 so it handles /v1/models, /v1/admin/*, etc.
    let api_v1 = axum::Router::new().nest("/v1", api);

    // Build the meta router (handles /health, /stats)
    let meta = crate::server::router::build_meta_router(state.clone());

    Router::builder()
        .app_context(state.clone())
        // Serve assets embedded in the Rust binary.
        .route(TowerRoute::new(
            Methods::Only(&[topcoat::router::Method::GET]),
            Path::new("/_topcoat/tailwind.css"),
            axum::Router::new().route(
                "/_topcoat/tailwind.css",
                axum::routing::get(|| async {
                    static_asset_response(TAILWIND_CSS, "text/css; charset=utf-8")
                }),
            ),
        ))
        .route(TowerRoute::new(
            Methods::Only(&[topcoat::router::Method::GET]),
            Path::new("/_topcoat/htmx.min.js"),
            axum::Router::new().route(
                "/_topcoat/htmx.min.js",
                axum::routing::get(|| async {
                    static_asset_response(HTMX_JS, "application/javascript; charset=utf-8")
                }),
            ),
        ))
        .route(TowerRoute::new(
            Methods::Only(&[topcoat::router::Method::GET]),
            Path::new("/_topcoat/chart.umd.min.js"),
            axum::Router::new().route(
                "/_topcoat/chart.umd.min.js",
                axum::routing::get(|| async {
                    static_asset_response(CHART_JS, "application/javascript; charset=utf-8")
                }),
            ),
        ))
        .route(TowerRoute::new(
            Methods::Only(&[topcoat::router::Method::GET]),
            Path::new("/_topcoat/codemirror/codemirror.min.js"),
            axum::Router::new().route(
                "/_topcoat/codemirror/codemirror.min.js",
                axum::routing::get(|| async {
                    static_asset_response(CODEMIRROR_JS, "application/javascript; charset=utf-8")
                }),
            ),
        ))
        .route(TowerRoute::new(
            Methods::Only(&[topcoat::router::Method::GET]),
            Path::new("/_topcoat/codemirror/codemirror.min.css"),
            axum::Router::new().route(
                "/_topcoat/codemirror/codemirror.min.css",
                axum::routing::get(|| async {
                    static_asset_response(CODEMIRROR_CSS, "text/css; charset=utf-8")
                }),
            ),
        ))
        .route(TowerRoute::new(
            Methods::Only(&[topcoat::router::Method::GET]),
            Path::new("/_topcoat/codemirror/yaml.min.js"),
            axum::Router::new().route(
                "/_topcoat/codemirror/yaml.min.js",
                axum::routing::get(|| async {
                    static_asset_response(
                        CODEMIRROR_YAML_JS,
                        "application/javascript; charset=utf-8",
                    )
                }),
            ),
        ))
        .route(TowerRoute::new(
            Methods::Only(&[topcoat::router::Method::GET]),
            Path::new("/_topcoat/codemirror/dracula.min.css"),
            axum::Router::new().route(
                "/_topcoat/codemirror/dracula.min.css",
                axum::routing::get(|| async {
                    static_asset_response(CODEMIRROR_DRACULA_CSS, "text/css; charset=utf-8")
                }),
            ),
        ))
        // Bridge specific API paths to the axum router
        .route(TowerRoute::new(
            Methods::Any,
            Path::new("/v1/{*rest}"),
            api_v1,
        ))
        .route(TowerRoute::new(
            Methods::Any,
            Path::new("/health"),
            meta.clone(),
        ))
        .route(TowerRoute::new(Methods::Any, Path::new("/stats"), meta))
        // Auto-discover all #[page] annotated functions
        .discover()
        .build()
}

/// Shared app context accessor.
pub fn app_state(cx: &Cx) -> &AppState {
    topcoat::context::app_context::<Arc<AppState>>(cx)
}

/// Reject unauthenticated page and fragment requests before rendering any
/// configuration-backed content into HTML.
pub fn require_admin(cx: &Cx) -> topcoat::Result<()> {
    crate::admin::check_admin(app_state(cx), topcoat::router::headers(cx))
        .map_err(|_| topcoat::router::error::redirect("/login").into())
}
