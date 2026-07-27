use std::sync::Arc;

use topcoat::{
    context::Cx,
    router::{Path, Router, RouterBuilderDiscoverExt, Methods},
};
use topcoat_router::tower::TowerRoute;

use crate::server::AppState;

/// Builds the Topcoat router that serves the admin UI and bridges API calls
/// to the existing axum-based handlers.
pub fn build_topcoat_app(state: Arc<AppState>) -> Router {
    // Build the API router (handles /models, /chat/completions, /admin/*, etc.)
    let api = crate::server::router::build_api_router(state.clone());
    // Nest it at /v1 so it handles /v1/models, /v1/admin/*, etc.
    let api_v1 = axum::Router::new().nest("/v1", api);
    
    // Build the meta router (handles /health, /stats)
    let meta = crate::server::router::build_meta_router(state.clone());
    
    let router = Router::builder()
        .app_context(state.clone())
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
        .route(TowerRoute::new(
            Methods::Any,
            Path::new("/stats"),
            meta,
        ))
        // Auto-discover all #[page] annotated functions
        .discover()
        .build();
    
    router
}

/// Shared app context accessor.
pub fn app_state(cx: &Cx) -> &AppState {
    topcoat::context::app_context::<Arc<AppState>>(cx)
}
