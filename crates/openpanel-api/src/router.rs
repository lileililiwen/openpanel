//! Router builder. Composes routes from each module + the auth middleware.

use std::sync::Arc;

use axum::{Json, Router, middleware::from_fn_with_state, routing::get};
use openpanel_app::{
    BackupService, CronService, DatabasesService, FilesService, IdentityService, LogService,
    MonitoringService, SecurityService, SitesService, SslService, security::LoginThrottleService,
};

use crate::{
    middleware::session::session_middleware,
    routes::{
        backups::router as backups_router, cron::router as cron_router,
        databases::router as databases_router, files::router as files_router,
        identity::router as identity_router, logs::router as logs_router,
        monitoring::router as monitoring_router, security::router as security_router,
        sites::router as sites_router, ssl::router as ssl_router,
    },
};

/// Builds the top-level Axum [`Router`] combining every API module under `/api/v1`
/// and a `/health` endpoint, with session resolution wired in via middleware.
// The composition root lists bounded-context services explicitly so module
// dependencies remain visible and type checked.
#[allow(clippy::too_many_arguments)]
pub fn build_router(
    identity: Arc<IdentityService>,
    sites: Arc<SitesService>,
    databases: Arc<DatabasesService>,
    files: Arc<FilesService>,
    ssl: Arc<SslService>,
    monitoring: Arc<MonitoringService>,
    cron: Arc<CronService>,
    backups: Arc<BackupService>,
    logs: Arc<LogService>,
    security: Arc<SecurityService>,
    login_throttle: Arc<LoginThrottleService>,
) -> Router {
    let identity_for_layer = identity.clone();

    let api = Router::new()
        .nest("/identity", identity_router(identity, login_throttle))
        .nest("/sites", sites_router(sites))
        .nest("/databases", databases_router(databases))
        .nest("/files", files_router(files.clone()))
        .nest("/ssl", ssl_router(ssl))
        .nest("/monitoring", monitoring_router(monitoring))
        .nest("/cron", cron_router(cron))
        .nest("/backups", backups_router(backups))
        .nest("/logs", logs_router(logs))
        .nest("/security", security_router(security))
        .layer(from_fn_with_state(identity_for_layer, session_middleware));

    Router::new()
        .nest("/api/v1", api)
        .route("/health", get(health))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}
