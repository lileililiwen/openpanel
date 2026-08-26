//! Router builder. Composes routes from each module + the auth middleware.

use std::sync::Arc;

use axum::{Json, Router, middleware::from_fn_with_state, routing::get};
use openpanel_app::{
    ApiTokenService, BackupService, CollaboratorService, ContainerRegistryService,
    ContainerRuntimeService, CronService, DatabasesService, DnsService, DockerService,
    FilesService, FtpService, GrantResolver, HierarchyService, HostingPlansService,
    IdentityService, LogService, MailFilterService, MailService, MailingListService,
    MalwareScannerService, MarketplaceService, MonitoringService, NotificationService, PitrService,
    PluginService, SecurityService, ServerSnapshotService, SiteCacheService, SiteCloneService,
    SiteHttpService, SitesService, SoftwareCenterService, SslService, SsoService, StagingService,
    ThemeableUiService, WafService, WebApplicationInstallerService, WebTerminalService,
    identity::TwoFactorService, security::LoginThrottleService,
    site_clone_template::SqliteSiteCloneTemplateRepository, system_services::ServiceManager,
};

use crate::{
    middleware::session::{ApiAuthState, api_auth_middleware},
    routes::{
        account_hierarchy::router as account_hierarchy_router,
        api_tokens::router as api_tokens_router,
        app_runtimes::router as app_runtimes_router,
        backups::router as backups_router,
        collaborators::router as collaborators_router,
        container_registry::router as container_registry_router,
        container_runtime::router as container_runtime_router,
        cron::router as cron_router,
        databases::{remote_access_router as db_remote_access_router, router as databases_router},
        db_pitr::router as db_pitr_router,
        deliverability::router as deliverability_router,
        dns::router as dns_router,
        docker::router as docker_router,
        files::router as files_router,
        ftp::router as ftp_router,
        hosting_plans::router as hosting_plans_router,
        identity::router as identity_router,
        logs::{policies_router as logs_policies_router, router as logs_router},
        mail::router as mail_router,
        malware_scanner::router as malware_scanner_router,
        monitoring::router as monitoring_router,
        notifications::router as notifications_router,
        plugin_extension::router as plugin_extension_router,
        plugin_marketplace::router as plugin_marketplace_router,
        security::{router as security_router, ssh_keys_router},
        server_snapshots::router as server_snapshots_router,
        site_cache_cdn::router as site_cache_cdn_router,
        site_clone_template::router as site_clone_template_router,
        site_http_controls::router as site_http_controls_router,
        site_staging::router as site_staging_router,
        sites::{router as sites_router, transport_router as site_transport_router},
        software_center::router as software_center_router,
        ssl::router as ssl_router,
        sso::{public_router as sso_public_router, router as sso_router},
        system_services::router as system_services_router,
        themeable_ui::router as themeable_ui_router,
        waf::router as waf_router,
        web_application_installer::router as web_application_installer_router,
        web_terminal::router as web_terminal_router,
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
    mail_filters: Arc<MailFilterService>,
    mailing_lists: Arc<MailingListService>,
    software_center: Arc<SoftwareCenterService>,
    two_factor: Arc<TwoFactorService>,
    waf: Arc<WafService>,
    site_http_controls: Arc<SiteHttpService>,
    site_transport: Arc<openpanel_app::SiteTransportService>,
    log_rotation: Arc<openpanel_app::LogRotationService>,
    host_ssh_keys: Arc<openpanel_app::HostSshKeysService>,
    db_remote_access: Arc<openpanel_app::DbRemoteAccessContext>,
    runtime_env: Arc<openpanel_app::RuntimeEnvService>,
    web_terminal: Arc<WebTerminalService>,
    sso: Arc<SsoService>,
    docker: Arc<DockerService>,
    ftp: Arc<FtpService>,
    api_tokens: Arc<ApiTokenService>,
    notifications: Arc<NotificationService>,
    pitr: Arc<PitrService>,
    server_snapshots: Arc<ServerSnapshotService>,
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
    deliverability: Arc<openpanel_app::DeliverabilityService>,
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
        .nest("/sites", site_http_controls_router(site_http_controls))
        .nest("/sites", site_transport_router(site_transport))
        .merge(web_terminal_router(web_terminal))
        .merge(sso_router(sso.clone()))
        .nest("/sites", ftp_router(ftp))
        .nest("/sites", site_staging_router(staging))
        .nest("/databases", databases_router(databases))
        .nest("/databases", db_remote_access_router(db_remote_access))
        .nest("/runtimes", app_runtimes_router(runtime_env))
        .nest("/mail/domains", deliverability_router(deliverability))
        .nest("/files", files_router(files.clone()))
        .nest("/ssl", ssl_router(ssl))
        .nest("/monitoring", monitoring_router(monitoring))
        .nest("/cron", cron_router(cron))
        .nest("/backups", backups_router(backups))
        .nest("/backups", db_pitr_router(pitr))
        .nest("/server", server_snapshots_router(server_snapshots))
        .nest("/logs", logs_router(logs))
        .nest("/logs", logs_policies_router(log_rotation))
        .nest("/security", security_router(security))
        .nest("/host", ssh_keys_router(host_ssh_keys))
        .nest("/services", system_services_router(system_services))
        .nest("/dns", dns_router(dns))
        .nest("/mail", mail_router(mail, mail_filters, mailing_lists))
        .nest("/software", software_center_router(software_center))
        .nest("/docker", docker_router(docker))
        .nest("/notifications", notifications_router(notifications))
        .nest(
            "/sites",
            collaborators_router(collaborators, grant_resolver),
        )
        .nest("/marketplace", plugin_marketplace_router(marketplace))
        .merge(plugin_extension_router(plugins))
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
        .merge(sso_public_router(sso))
        .route("/health", get(health))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}
