//! Router builder. Composes routes from each module + the auth middleware.

use std::sync::Arc;

use axum::{Json, Router, middleware::from_fn_with_state, routing::get};
use openpanel_app::{
    ApiTokenService, BackupService, CronService, DatabasesService, DnsService, DockerService,
    FilesService, FtpService, IdentityService, LogService, MailService, MonitoringService,
    NotificationService, PitrService, SecurityService, SitesService, SoftwareCenterService,
    SslService, StagingService, WafService, identity::TwoFactorService,
    security::LoginThrottleService, system_services::ServiceManager,
};

use crate::{
    middleware::session::{ApiAuthState, api_auth_middleware},
    routes::{
        api_tokens::router as api_tokens_router, backups::router as backups_router,
        cron::router as cron_router, databases::router as databases_router,
        db_pitr::router as db_pitr_router, dns::router as dns_router,
        docker::router as docker_router, files::router as files_router, ftp::router as ftp_router,
        identity::router as identity_router, logs::router as logs_router,
        mail::router as mail_router, monitoring::router as monitoring_router,
        notifications::router as notifications_router, security::router as security_router,
        site_staging::router as site_staging_router, sites::router as sites_router,
        software_center::router as software_center_router, ssl::router as ssl_router,
        system_services::router as system_services_router, waf::router as waf_router,
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
    system_services: Arc<ServiceManager>,
    dns: Arc<DnsService>,
    mail: Arc<MailService>,
    software_center: Arc<SoftwareCenterService>,
    two_factor: Arc<TwoFactorService>,
    waf: Arc<WafService>,
    docker: Arc<DockerService>,
    ftp: Arc<FtpService>,
    api_tokens: Arc<ApiTokenService>,
    notifications: Arc<NotificationService>,
    pitr: Arc<PitrService>,
    staging: Arc<StagingService>,
) -> Router {
    let auth_state = ApiAuthState {
        identity: identity.clone(),
        tokens: api_tokens.clone(),
    };

    let api = Router::new()
        .nest(
            "/identity",
            identity_router(identity, two_factor, login_throttle)
                .merge(api_tokens_router(api_tokens)),
        )
        .nest("/sites", sites_router(sites))
        .nest("/sites", waf_router(waf))
        .nest("/sites", ftp_router(ftp))
        .nest("/sites", site_staging_router(staging))
        .nest("/databases", databases_router(databases))
        .nest("/files", files_router(files.clone()))
        .nest("/ssl", ssl_router(ssl))
        .nest("/monitoring", monitoring_router(monitoring))
        .nest("/cron", cron_router(cron))
        .nest("/backups", backups_router(backups))
        .nest("/backups", db_pitr_router(pitr))
        .nest("/logs", logs_router(logs))
        .nest("/security", security_router(security))
        .nest("/services", system_services_router(system_services))
        .nest("/dns", dns_router(dns))
        .nest("/mail", mail_router(mail))
        .nest("/software", software_center_router(software_center))
        .nest("/docker", docker_router(docker))
        .nest("/notifications", notifications_router(notifications))
        .layer(from_fn_with_state(auth_state, api_auth_middleware));

    Router::new()
        .nest("/api/v1", api)
        .route("/health", get(health))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}
