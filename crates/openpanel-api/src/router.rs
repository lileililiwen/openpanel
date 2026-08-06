//! Router builder. Composes routes from each module + the auth middleware.

use std::sync::Arc;

use axum::middleware::from_fn_with_state;
use axum::routing::get;
use axum::{Json, Router};
use openpanel_app::IdentityService;

use crate::middleware::session::session_middleware;
use crate::routes::identity::router as identity_router;

pub fn build_router(identity: Arc<IdentityService>) -> Router {
    let api = Router::new()
        .nest("/identity", identity_router(identity.clone()))
        .layer(from_fn_with_state(identity.clone(), session_middleware));

    Router::new()
        .nest("/api/v1", api)
        .route("/health", get(health))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}