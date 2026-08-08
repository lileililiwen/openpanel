//! Router builder. Composes routes from each module + the auth middleware.

use std::sync::Arc;

use axum::{Json, Router, middleware::from_fn_with_state, routing::get};
use openpanel_app::{DatabasesService, FilesService, IdentityService, SitesService};

use crate::{
    middleware::session::session_middleware,
    routes::{
        databases::router as databases_router, files::router as files_router,
        identity::router as identity_router, sites::router as sites_router,
    },
};

/// Builds the top-level Axum [`Router`] combining every API module under `/api/v1`
/// and a `/health` endpoint, with session resolution wired in via middleware.
pub fn build_router(
    identity: Arc<IdentityService>,
    sites: Arc<SitesService>,
    databases: Arc<DatabasesService>,
    files: Arc<FilesService>,
) -> Router {
    let identity_for_layer = identity.clone();

    let api = Router::new()
        .nest("/identity", identity_router(identity))
        .nest("/sites", sites_router(sites))
        .nest("/databases", databases_router(databases))
        .nest("/files", files_router(files.clone()))
        .layer(from_fn_with_state(identity_for_layer, session_middleware));

    Router::new()
        .nest("/api/v1", api)
        .route("/health", get(health))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}
