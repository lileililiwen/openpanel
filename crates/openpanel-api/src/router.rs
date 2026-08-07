//! Router builder. Composes routes from each module + the auth middleware.

use std::sync::Arc;

use axum::middleware::from_fn_with_state;
use axum::routing::get;
use axum::{Json, Router};
use openpanel_app::{DatabasesService, FilesService, IdentityService, SitesService};

use crate::middleware::session::session_middleware;
use crate::routes::databases::router as databases_router;
use crate::routes::files::router as files_router;
use crate::routes::identity::router as identity_router;
use crate::routes::sites::router as sites_router;

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