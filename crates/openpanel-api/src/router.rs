//! Router builder. Composes routes from each module + the auth middleware.

use std::sync::Arc;

use axum::{Json, Router, middleware::from_fn_with_state, routing::get};
use openpanel_app::{
    ApiTokenService, BackupService, CollaboratorService, ContainerRegistryService,
    ContainerRuntimeService, CronService, DatabasesService, DnsService, DockerService,
    FilesService, FtpService, GrantResolver, HierarchyService, HostingPlansService,
    IdentityService, LogService, MailService, MalwareScannerService, MarketplaceService,
    MonitoringService, NotificationService, PitrService, PluginService, SecurityService,
    SiteCacheService, SiteCloneService, SitesService, SoftwareCenterService, SslService,
    StagingService, ThemeableUiService, WafService, WebApplicationInstallerService,
    identity::TwoFactorService, security::LoginThrottleService,
    site_clone_template::SqliteSiteCloneTemplateRepository, system_services::ServiceManager,
};

use crate::{
    middleware::session::{ApiAuthState, api_auth_middleware},
    routes::{
        account_hierarchy::router as account_hierarchy_router,
        api_tokens::router as api_tokens_router, backups::router as backups_router,
        collaborators::router as collaborators_router,
        container_registry::router as container_registry_router,
        container_runtime::router as container_runtime_router, cron::router as cron_router,
        databases::router as databases_router, db_pitr::router as db_pitr_router,
        dns::router as dns_router, docker::router as docker_router, files::router as files_router,
        ftp::router as ftp_router, hosting_plans::router as hosting_plans_router,
        identity::router as identity_router, logs::router as logs_router,
        mail::router as mail_router, malware_scanner::router as malware_scanner_router,
        monitoring::router as monitoring_router, notifications::router as notifications_router,
        plugin_marketplace::router as plugin_marketplace_router,
        security::router as security_router, site_cache_cdn::router as site_cache_cdn_router,
        site_clone_template::router as site_clone_template_router,
        site_staging::router as site_staging_router, sites::router as sites_router,
        software_center::router as software_center_router, ssl::router as ssl_router,
        system_services::router as system_services_router,
        themeable_ui::router as themeable_ui_router, waf::router as waf_router,
        web_application_installer::router as web_application_installer_router,
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
    plugins: Arc<PluginService>,
    marketplace: Arc<MarketplaceService>,
    collaborators: Arc<CollaboratorService>,
    grant_resolver: Arc<GrantResolver>,
    registry: Arc<ContainerRegistryService>,
    container_runtime: Arc<ContainerRuntimeService>,
    hosting_plans: Arc<HostingPlansService>,
    account_hierarchy: Arc<HierarchyService>,
    site_cache_cdn: Arc<SiteCacheService>,
    site_clone_template_svc: Arc<SiteCloneService>,
    site_clone_template_repo: Arc<SqliteSiteCloneTemplateRepository>,
    themeable_ui: Arc<ThemeableUiService>,
    web_application_installer: Arc<WebApplicationInstallerService>,
    malware_scanner: Arc<MalwareScannerService>,
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
        .nest(
            "/sites",
            collaborators_router(collaborators, grant_resolver),
        )
        .nest("/marketplace", plugin_marketplace_router(marketplace))
        .nest("/registry", container_registry_router(registry))
        .nest("/container", container_runtime_router(container_runtime))
        .merge(hosting_plans_router(hosting_plans))
        .merge(account_hierarchy_router(account_hierarchy))
        .merge(site_cache_cdn_router(site_cache_cdn))
        .merge(site_clone_template_router(
            site_clone_template_svc,
            site_clone_template_repo,
        ))
        .merge(themeable_ui_router(themeable_ui))
        .merge(web_application_installer_router(web_application_installer))
        .merge(malware_scanner_router(malware_scanner))
        .layer(from_fn_with_state(auth_state, api_auth_middleware));

    Router::new()
        .nest("/api/v1", api)
        .route("/health", get(health))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}
