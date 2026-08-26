//! CLI command handlers. Each command bootstraps the application context
//! enough to call into the IdentityService or start the server.

use std::sync::Arc;

use anyhow::Context;
use base64::Engine;
use openpanel_api::build_router;
use openpanel_app::{
    AcmeEndpoint, ApiTokenModule, BackupsModule, CronModule, DatabasesModule, DbPitrModule,
    DnsModule, DockerModule, FilesModule, FtpModule, IdentityModule, InMemoryBinlogSink,
    InMemoryStagingFilesystem, LogService, LogsModule, MailModule, MonitoringModule,
    NotificationModule, SecurityModule, SecurityService, SiteHttpControlsModule, SiteStagingModule,
    SitesModule, SoftwareCenterModule, SslModule, SslPaths, StagingService, SystemServicesModule,
    WafModule,
    databases::{crypto as db_crypto, repo::SqliteDatabaseRepository},
    sites::repo::SqliteSiteRepository,
};
use openpanel_core::{
    AppContext, Config, JobSupervisor, MigrationRunner, Module, SqliteAuditService, SqliteDriver,
};
use openpanel_domain::{
    Role,
    security::{FirewallRule, LoginKey, NetworkCidr, PortRange, Protocol, RuleAction},
};
use tokio::net::TcpListener;

/// Bootstraps persistence, runs all module migrations, and serves the OpenPanel
/// API + agent over HTTP on the configured bind address until the process exits.
pub async fn serve(config: Arc<Config>) -> anyhow::Result<()> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;

    let ctx = AppContext::new(config.clone(), db, audit.clone());

    // Architecture-owned audit schema
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .ok();

    let master_key = load_master_key(&config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let api_token_module = ApiTokenModule::new(&ctx, master_key).await;
    let notification_module = NotificationModule::new(&ctx, master_key)
        .await
        .map_err(anyhow::Error::msg)?;
    let sites_module = SitesModule::new(&ctx).await;
    let waf_module = WafModule::new(&ctx, sites_module.generator().clone()).await;
    let site_http_controls_auth_dir = std::env::var("OPENPANEL__SITE_HTTP__AUTH_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/etc/openpanel/http-auth"));
    let site_http_controls_module = SiteHttpControlsModule::new(
        &ctx,
        sites_module.generator().clone(),
        site_http_controls_auth_dir,
    )
    .await;
    let web_terminal_module = openpanel_app::WebTerminalModule::new(&ctx).await;
    let mail_filtering_module = openpanel_app::MailFilteringModule::new(&ctx).await;
    let sso_module = openpanel_app::SsoModule::new(&ctx, master_key).await;
    let docker_module = DockerModule::new(&ctx, master_key)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let ftp_module = FtpModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let databases_module = DatabasesModule::new(&ctx, master_key).await;
    let files_module = FilesModule::new(&ctx).await;
    let ssl_module = SslModule::new(&ctx, master_key, ssl_contact_email(&config)).await;
    let monitoring_module = MonitoringModule::new(&ctx).await;
    monitoring_module
        .service()
        .attach_notifications(notification_module.service());
    let cron_module = CronModule::new(&ctx).await;
    let backup_root = std::env::var("OPENPANEL__BACKUPS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/var/lib/openpanel/backups"));
    let backups_module = BackupsModule::with_root(
        &ctx,
        backup_root,
        Some(master_key),
        Some((cron_module.service(), std::path::PathBuf::from("/var/www"))),
    )
    .await;
    let snapshots_root = std::env::var("OPENPANEL__SNAPSHOTS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or(std::path::PathBuf::from("/var/lib/openpanel/snapshots"));
    let server_snapshots_svc = std::sync::Arc::new(openpanel_app::ServerSnapshotService::new(
        backups_module.service(),
        pool.clone(),
        snapshots_root,
        audit.clone(),
        env!("CARGO_PKG_VERSION").to_owned(),
    ));
    let log_root = std::env::var("OPENPANEL__LOGS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/var/log/openpanel"));
    let logs_module = LogsModule::with_root(&ctx, log_root).await;
    let security_module = SecurityModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let system_services_module = SystemServicesModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let dns_module = DnsModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mail_module = MailModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let software_center_module =
        SoftwareCenterModule::new(&ctx, sites_module.service(), databases_module.service())
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let runner = MigrationRunner::for_sqlite(pool.clone());
    runner
        .apply_module(identity_module.name(), &identity_module.migrations())
        .await
        .context("apply identity migrations")?;
    runner
        .apply_module(api_token_module.name(), &api_token_module.migrations())
        .await
        .context("apply API-token migrations")?;
    runner
        .apply_module(
            notification_module.name(),
            &notification_module.migrations(),
        )
        .await
        .context("apply notification migrations")?;
    runner
        .apply_module(sites_module.name(), &sites_module.migrations())
        .await
        .context("apply sites migrations")?;
    runner
        .apply_module(waf_module.name(), &waf_module.migrations())
        .await
        .context("apply WAF migrations")?;
    runner
        .apply_module(
            site_http_controls_module.name(),
            &site_http_controls_module.migrations(),
        )
        .await
        .context("apply site-http-controls migrations")?;
    runner
        .apply_module(
            web_terminal_module.name(),
            &web_terminal_module.migrations(),
        )
        .await
        .context("apply web-terminal migrations")?;
    runner
        .apply_module(
            mail_filtering_module.name(),
            &mail_filtering_module.migrations(),
        )
        .await
        .context("apply mail-filtering migrations")?;
    runner
        .apply_module(sso_module.name(), &sso_module.migrations())
        .await
        .context("apply sso migrations")?;
    runner
        .apply_module(docker_module.name(), &docker_module.migrations())
        .await
        .context("apply Docker migrations")?;
    runner
        .apply_module(ftp_module.name(), &ftp_module.migrations())
        .await
        .context("apply FTP migrations")?;
    runner
        .apply_module(databases_module.name(), &databases_module.migrations())
        .await
        .context("apply databases migrations")?;
    runner
        .apply_module(ssl_module.name(), &ssl_module.migrations())
        .await
        .context("apply ssl migrations")?;
    runner
        .apply_module(monitoring_module.name(), &monitoring_module.migrations())
        .await
        .context("apply monitoring migrations")?;
    runner
        .apply_module(cron_module.name(), &cron_module.migrations())
        .await
        .context("apply cron migrations")?;
    runner
        .apply_module(backups_module.name(), &backups_module.migrations())
        .await
        .context("apply backup migrations")?;
    runner
        .apply_module(logs_module.name(), &logs_module.migrations())
        .await
        .context("apply logs migrations")?;
    runner
        .apply_module(security_module.name(), &security_module.migrations())
        .await
        .context("apply security migrations")?;
    runner
        .apply_module(
            system_services_module.name(),
            &system_services_module.migrations(),
        )
        .await
        .context("apply system services migrations")?;
    runner
        .apply_module(dns_module.name(), &dns_module.migrations())
        .await
        .context("apply DNS migrations")?;
    runner
        .apply_module(mail_module.name(), &mail_module.migrations())
        .await
        .context("apply mail migrations")?;
    runner
        .apply_module(
            software_center_module.name(),
            &software_center_module.migrations(),
        )
        .await
        .context("apply software center migrations")?;

    // Feedback widget module: persists NPS-style submissions from the
    // admin interaction surface.
    let feedback_module = openpanel_app::FeedbackModule::new(&ctx).await;
    runner
        .apply_module(feedback_module.name(), &feedback_module.migrations())
        .await
        .context("apply feedback migrations")?;
    let feedback_svc = feedback_module.service();

    let mut background_tasks = docker_module.background_tasks(&ctx);
    background_tasks.extend(ftp_module.background_tasks(&ctx));
    background_tasks.extend(notification_module.background_tasks(&ctx));
    let docker_supervisor = JobSupervisor::new().spawn(background_tasks);

    let identity_svc = identity_module.service();
    let api_token_svc = api_token_module.service();
    let notification_svc = notification_module.service();
    let sites_svc = sites_module.service();
    let databases_svc = databases_module.service();
    let files_svc = files_module.service();
    let ssl_svc = ssl_module.service();
    let monitoring_svc = monitoring_module.service();
    let cron_svc = cron_module.service();
    let backups_svc = backups_module.service();
    let logs_svc = logs_module.service();
    let security_svc = security_module.service();
    let login_throttle = security_module.login_service();
    let system_services_svc = system_services_module.service();
    let dns_svc = dns_module.service();
    let mail_svc = mail_module.service();
    let software_center_svc = software_center_module.service();
    let two_factor_svc = identity_module.two_factor();
    let waf_svc = waf_module.service();
    let site_http_controls_svc = site_http_controls_module.service();
    let web_terminal_svc = web_terminal_module.service();
    let mail_filters_svc = mail_filtering_module.service();
    let mailing_lists_svc = mail_filtering_module.mailing_lists();
    let sso_svc = sso_module.service();
    let docker_svc = docker_module.service();
    let ftp_svc = ftp_module.service();

    // PITR module: in-memory sink for now (no offsite targets yet).
    // Future `add-offsite-backup-targets` change will swap the sink
    // for an S3 / rsync / B2 / Wasabi adapter.
    let pitr_db_lookup: Arc<dyn openpanel_domain::DatabaseLookup> =
        Arc::new(SqliteDatabaseRepository::new(pool.clone()));
    let pitr_sink: Arc<dyn openpanel_app::BinlogSink> = Arc::new(InMemoryBinlogSink::new());
    let pitr_module =
        DbPitrModule::new(&ctx, pitr_sink, Vec::new(), pitr_db_lookup, audit.clone()).await;
    runner
        .apply_module(pitr_module.name(), &pitr_module.migrations())
        .await
        .context("apply PITR migrations")?;
    let pitr_svc = pitr_module.service();

    // Plugin extension framework module.
    let plugin_module = openpanel_app::PluginModule::new(&ctx, audit.clone()).await;
    runner
        .apply_module(plugin_module.name(), &plugin_module.migrations())
        .await
        .context("apply plugin migrations")?;
    let plugin_svc = plugin_module.service();

    // Plugin marketplace module: empty CA + in-process mock client.
    let mp_client: Arc<dyn openpanel_app::MarketplaceClient> =
        Arc::new(openpanel_app::MockMarketplaceClient::new());
    let mp_module =
        openpanel_app::PluginMarketplaceModule::new(&ctx, mp_client, plugin_svc.clone()).await;
    runner
        .apply_module(mp_module.name(), &mp_module.migrations())
        .await
        .context("apply plugin marketplace migrations")?;
    let marketplace_svc = mp_module.service();

    // Per-site collaborators module.
    let collaborators_module = openpanel_app::CollaboratorsModule::new(&ctx, audit.clone()).await;
    runner
        .apply_module(
            collaborators_module.name(),
            &collaborators_module.migrations(),
        )
        .await
        .context("apply collaborators migrations")?;
    let collaborators_svc = collaborators_module.service();
    let grant_resolver = collaborators_module.resolver();

    // Container registry module.
    let registry_module = openpanel_app::ContainerRegistryModule::new(
        &ctx,
        audit.clone(),
        openpanel_domain::RegistryConfig::default(),
        None,
        None,
    )
    .await;
    runner
        .apply_module(registry_module.name(), &registry_module.migrations())
        .await
        .context("apply registry migrations")?;
    let registry_svc = registry_module.service();

    // Container runtime module: per-user quota, registry
    // credentials (encrypted at rest under the master key),
    // per-container metrics, and monthly network egress
    // accounting. Built on top of the docker bounded context.
    let container_runtime_module = openpanel_app::ContainerRuntimeModule::new(
        &ctx,
        audit.clone(),
        master_key,
        openpanel_domain::PlanQuotaCaps::default(),
        None,
    )
    .await;
    runner
        .apply_module(
            container_runtime_module.name(),
            &container_runtime_module.migrations(),
        )
        .await
        .context("apply container-runtime migrations")?;
    let container_runtime_svc = container_runtime_module.service();
    docker_svc.attach_quota_gate(container_runtime_svc.clone());

    // Hosting plans module: plan definitions, lifecycle, and assignment.
    let hosting_plans_module = openpanel_app::HostingPlansModule::new(&ctx).await;
    runner
        .apply_module(
            hosting_plans_module.name(),
            &hosting_plans_module.migrations(),
        )
        .await
        .context("apply hosting-plans migrations")?;
    let hosting_plans_svc = hosting_plans_module.service();

    let account_hierarchy_module = openpanel_app::AccountHierarchyModule::new(&ctx).await;
    runner
        .apply_module(
            account_hierarchy_module.name(),
            &account_hierarchy_module.migrations(),
        )
        .await
        .context("apply account_hierarchy migrations")?;
    let account_hierarchy_svc = account_hierarchy_module.service();

    // Migration importers module: cPanel / Baota backup import
    // pipeline with preview, atomic run, and rollback.
    let migration_importers_module = openpanel_app::MigrationImportersModule::new(&ctx).await;
    runner
        .apply_module(
            migration_importers_module.name(),
            &migration_importers_module.migrations(),
        )
        .await
        .context("apply migration-importers migrations")?;
    let migration_importers_svc = migration_importers_module.service();
    let _ = migration_importers_svc;

    // Offsite backup targets module: encrypted credential
    // lifecycle, KEK management, and remote target attachment.
    let offsite_backup_module = openpanel_app::OffsiteBackupTargetsModule::new(&ctx).await;
    runner
        .apply_module(
            offsite_backup_module.name(),
            &offsite_backup_module.migrations(),
        )
        .await
        .context("apply offsite-backup-targets migrations")?;
    let offsite_backup_svc = offsite_backup_module.service();
    let _ = offsite_backup_svc;

    // Site cache and CDN module: per-site cache policy, CDN
    // integrations, and purge orchestration.
    let site_cache_cdn_module = openpanel_app::SiteCacheCdnModule::new(&ctx).await;
    runner
        .apply_module(
            site_cache_cdn_module.name(),
            &site_cache_cdn_module.migrations(),
        )
        .await
        .context("apply site-cache-cdn migrations")?;
    let site_cache_cdn_svc = site_cache_cdn_module.service();
    let _ = site_cache_cdn_svc;

    // Themeable UI: per-account override + palette validation.
    let themeable_ui_repo =
        openpanel_app::themeable_ui::SqliteThemeableUiRepository::new(pool.clone());
    let themeable_ui_svc = Arc::new(openpanel_app::ThemeableUiService::new(
        Arc::new(themeable_ui_repo),
        audit.clone(),
        None,
    ));

    // Webmail client: real service over the same pool + audit.
    let webmail_repo = openpanel_app::webmail_client::SqliteWebmailRepository::new(pool.clone());
    let webmail_svc = Arc::new(openpanel_app::WebmailService::new(
        Arc::new(webmail_repo),
        Arc::new(openpanel_app::InMemoryMailBridge::default()),
        audit.clone(),
        master_key,
    ));

    // Site clone + template export module: persistence only.
    let site_clone_template_module = openpanel_app::SiteCloneTemplateModule::new(&ctx).await;
    runner
        .apply_module(
            site_clone_template_module.name(),
            &site_clone_template_module.migrations(),
        )
        .await
        .context("apply site-clone-template migrations")?;

    // Site-staging module: in-memory filesystem layer for the CLI
    // (no live nginx / rsync in offline mode).
    let staging_fs: Arc<dyn openpanel_app::StagingFilesystemLayer> =
        Arc::new(InMemoryStagingFilesystem::new());
    let staging_sites_repo: Arc<dyn openpanel_domain::SiteRepository> =
        Arc::new(SqliteSiteRepository::new(pool.clone()));
    let staging_module =
        SiteStagingModule::new(&ctx, staging_sites_repo, staging_fs, audit.clone()).await;
    runner
        .apply_module(staging_module.name(), &staging_module.migrations())
        .await
        .context("apply staging migrations")?;
    let staging_svc: Arc<StagingService> = staging_module.service();
    let _ = staging_svc;
    let app = build_router(
        identity_svc.clone(),
        sites_svc.clone(),
        databases_svc.clone(),
        files_svc.clone(),
        ssl_svc.clone(),
        monitoring_svc.clone(),
        cron_svc.clone(),
        backups_svc.clone(),
        logs_svc.clone(),
        security_svc.clone(),
        login_throttle.clone(),
        system_services_svc.clone(),
        dns_svc.clone(),
        mail_svc.clone(),
        mail_filters_svc.clone(),
        mailing_lists_svc.clone(),
        software_center_svc.clone(),
        two_factor_svc.clone(),
        waf_svc.clone(),
        site_http_controls_svc.clone(),
        sites_module.transport(),
        logs_module.rotation(),
        security_module.ssh_keys(),
        std::sync::Arc::new(openpanel_app::DbRemoteAccessContext {
            controller: std::sync::Arc::new(
                openpanel_app::RemoteAccessController::new(
                    std::sync::Arc::new(
                        openpanel_app::SqliteDbPrivilegeRepository::new(pool.clone()),
                    ),
                    audit.clone(),
                ),
            ),
            port: std::sync::Arc::new(openpanel_app::MySqlShellGrantPort::new(
                "/usr/bin/mysql",
            )),
        }),
        web_terminal_svc.clone(),
        sso_svc.clone(),
        docker_svc.clone(),
        ftp_svc.clone(),
        api_token_svc.clone(),
        notification_svc.clone(),
        pitr_svc.clone(),
        server_snapshots_svc.clone(),
        staging_svc.clone(),
        plugin_svc.clone(),
        marketplace_svc.clone(),
        collaborators_svc.clone(),
        grant_resolver.clone(),
        registry_svc.clone(),
        container_runtime_svc.clone(),
        hosting_plans_svc.clone(),
        account_hierarchy_svc.clone(),
        site_cache_cdn_svc.clone(),
        // Site clone + template export: real service constructed
        // from the same sites module, sites repository, and audit
        // service that the rest of the CLI uses.
        Arc::new(openpanel_app::SiteCloneService::new(
            site_clone_template_module.repo(),
            sites_svc.clone(),
            audit.clone(),
            master_key,
            None,
            None,
        )),
        site_clone_template_module.repo(),
        themeable_ui_svc.clone(),
        // Web application installer: real service over the same
        // pool + audit, with the production filesystem and
        // reqwest downloader.
        Arc::new(openpanel_app::WebApplicationInstallerService::new(
            Arc::new(
                openpanel_app::web_application_installer::SqliteWebApplicationInstallerRepository::new(
                    pool.clone(),
                ),
            ),
            audit.clone(),
            Arc::new(openpanel_app::RealInstallerFs),
            Arc::new(openpanel_app::ReqwestArtifactDownloader::new()),
            master_key,
            None,
        )),
        // Malware scanner: real service over the same pool + audit.
        Arc::new(openpanel_app::MalwareScannerService::new(
            Arc::new(
                openpanel_app::malware_scanner::SqliteMalwareScannerRepository::new(pool.clone()),
            ),
            audit.clone(),
            Arc::new(openpanel_app::RealScannerFs),
            None,
        )),
    )
    .merge(openpanel_web::router(
        identity_svc,
        sites_svc,
        databases_svc,
        files_svc,
        ssl_svc,
        monitoring_svc,
        cron_svc,
        backups_svc,
        logs_svc,
        security_svc,
        login_throttle,
        system_services_svc,
        dns_svc,
        mail_svc,
        software_center_svc,
        two_factor_svc,
        waf_svc,
        site_http_controls_svc.clone(),
        docker_svc,
        ftp_svc,
        api_token_svc,
        notification_svc,
        pitr_svc,
        staging_svc,
        collaborators_svc,
        registry_svc,
        container_runtime_svc,
        themeable_ui_svc.clone(),
        webmail_svc.clone(),
        feedback_svc,
        web_runtime(&config, audit).with_capabilities(
            openpanel_web::layout::CapabilitySet::shipped()
                .with("cron")
                .with("backups")
                .with("logs")
                .with("host-security")
                .with("system-services")
                .with("dns")
                .with("mail")
                .with("software-center")
                .with("docker")
                .with("ftp")
                .with("webmail")
                .with("container-registry")
                .with("plugins")
                .with("marketplace")
                .with("audit")
                .with("themeable-ui"),
        ),
    ));

    let addr = format!("{}:{}", config.server().bind, config.server().port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    tracing::info!(%addr, "openpanel listening on http://{addr}");
    let server_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await;
    docker_supervisor.join().await;
    server_result?;
    Ok(())
}

fn web_runtime(
    config: &Arc<Config>,
    audit: Arc<dyn openpanel_core::AuditService>,
) -> openpanel_web::WebRuntime {
    let database_path = config
        .database()
        .url
        .strip_prefix("sqlite://")
        .unwrap_or(&config.database().url);
    let data_path = std::path::Path::new(database_path)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("/var/lib/openpanel"))
        .to_path_buf();
    let config_path = std::env::var("OPENPANEL_CONFIG").unwrap_or_else(|_| {
        if std::path::Path::new("/etc/openpanel/openpanel.toml").exists() {
            "/etc/openpanel/openpanel.toml".to_string()
        } else {
            "./openpanel.toml".to_string()
        }
    });
    openpanel_web::WebRuntime::new(
        config.clone(),
        audit,
        data_path.join("web-preferences.json"),
        data_path.to_string_lossy().into_owned(),
        config_path,
    )
}

fn load_master_key(config: &Arc<Config>) -> anyhow::Result<[u8; 32]> {
    let raw = std::env::var("OPENPANEL__DATABASE__MASTER_KEY")
        .ok()
        .or_else(|| {
            config
                .module_config("database")
                .and_then(|v| v.get("master_key"))
                .and_then(|v| v.as_str().map(String::from))
        })
        .ok_or_else(|| {
            anyhow::anyhow!("OPENPANEL__DATABASE__MASTER_KEY must be set (base64-encoded 32 bytes)")
        })?;
    db_crypto::decode_master_key(&raw).map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Resolve the Let's Encrypt contact email from
/// `OPENPANEL__SSL__CONTACT_EMAIL` or fall back to the operator
/// account. Required by ACME.
fn ssl_contact_email(_config: &Arc<Config>) -> String {
    std::env::var("OPENPANEL__SSL__CONTACT_EMAIL")
        .unwrap_or_else(|_| "admin@openpanel.local".to_string())
}

async fn build_cron(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::CronService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let root = std::env::current_dir().context("resolve current directory")?;
    let module = CronModule::with_roots(&ctx, vec![root]).await;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

fn cron_owner() -> uuid::Uuid {
    uuid::Uuid::nil()
}

async fn build_backups(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::BackupService>> {
    let master_key = load_master_key(&config).ok();
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let root = std::env::var("OPENPANEL__BACKUPS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?.join("backups"));
    let cron_module = CronModule::with_roots(&ctx, vec![std::env::current_dir()?]).await;
    let runner = MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(cron_module.name(), &cron_module.migrations())
        .await?;
    let module = BackupsModule::with_root(
        &ctx,
        root,
        master_key,
        Some((cron_module.service(), std::env::current_dir()?)),
    )
    .await;
    runner
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}
fn backup_owner() -> uuid::Uuid {
    uuid::Uuid::nil()
}

async fn build_logs(config: Arc<Config>) -> anyhow::Result<Arc<LogService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let root = std::env::var("OPENPANEL__LOGS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/var/log/openpanel"));
    let module = LogsModule::with_root(&ctx, root).await;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

fn logs_actor() -> openpanel_app::logs::LogActor {
    openpanel_app::logs::LogActor::new(uuid::Uuid::nil(), openpanel_domain::Role::Owner)
}

async fn build_security(config: Arc<Config>) -> anyhow::Result<Arc<SecurityService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let module = SecurityModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

fn security_actor() -> uuid::Uuid {
    uuid::Uuid::nil()
}

async fn build_system_services(
    config: Arc<Config>,
) -> anyhow::Result<Arc<openpanel_app::ServiceManager>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let module = SystemServicesModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}
fn parse_service_action(
    value: &str,
) -> anyhow::Result<openpanel_domain::system_services::ServiceAction> {
    use openpanel_domain::system_services::ServiceAction;
    match value {
        "start" => Ok(ServiceAction::Start),
        "stop" => Ok(ServiceAction::Stop),
        "restart" => Ok(ServiceAction::Restart),
        "reload" => Ok(ServiceAction::Reload),
        "enable" => Ok(ServiceAction::Enable),
        "disable" => Ok(ServiceAction::Disable),
        _ => Err(anyhow::anyhow!("unknown service action")),
    }
}
/// List registered system services.
pub async fn services_list(config: Arc<Config>) -> anyhow::Result<()> {
    for value in build_system_services(config).await?.inventory().await? {
        println!(
            "{} {}",
            value.descriptor.id().as_str(),
            value.status.active_state
        );
    }
    Ok(())
}
/// Show one registered system service.
pub async fn services_status(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let value = build_system_services(config).await?.status(&id).await?;
    println!(
        "{} {}",
        value.descriptor.id().as_str(),
        value.status.active_state
    );
    Ok(())
}
/// Preview dependent capability impact.
pub async fn services_preview(
    config: Arc<Config>,
    id: String,
    action: String,
) -> anyhow::Result<()> {
    let value = build_system_services(config)
        .await?
        .preview(&id, parse_service_action(&action)?)?;
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}
/// Perform one typed local-Owner service action.
pub async fn services_action(
    config: Arc<Config>,
    id: String,
    action: String,
    confirmed: bool,
) -> anyhow::Result<()> {
    let value = build_system_services(config)
        .await?
        .perform(
            uuid::Uuid::nil(),
            Role::Owner,
            &id,
            parse_service_action(&action)?,
            confirmed,
        )
        .await?;
    println!("{}", value.status.active_state);
    Ok(())
}
/// Print bounded health history.
pub async fn services_history(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string(
            &build_system_services(config)
                .await?
                .history(&id, 100)
                .await?
        )?
    );
    Ok(())
}
/// Print bounded redacted journal entries.
pub async fn services_logs(config: Arc<Config>, id: String, limit: usize) -> anyhow::Result<()> {
    for line in build_system_services(config)
        .await?
        .logs(&id, limit)
        .await?
    {
        println!("{line}");
    }
    Ok(())
}

async fn build_dns(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::DnsService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let module = DnsModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}
fn parse_record_kind(value: &str) -> anyhow::Result<openpanel_domain::dns::RecordKind> {
    use openpanel_domain::dns::RecordKind;
    match value.to_ascii_uppercase().as_str() {
        "A" => Ok(RecordKind::A),
        "AAAA" => Ok(RecordKind::Aaaa),
        "CNAME" => Ok(RecordKind::Cname),
        "TXT" => Ok(RecordKind::Txt),
        "MX" => Ok(RecordKind::Mx),
        "CAA" => Ok(RecordKind::Caa),
        "NS" => Ok(RecordKind::Ns),
        "SRV" => Ok(RecordKind::Srv),
        _ => Err(anyhow::anyhow!("unsupported DNS record type")),
    }
}
/// Add a protected DNS provider account.
pub async fn dns_provider_add(
    config: Arc<Config>,
    kind: String,
    name: String,
    credential: String,
) -> anyhow::Result<()> {
    let account = build_dns(config)
        .await?
        .create_account(uuid::Uuid::nil(), Role::Owner, &kind, &name, &credential)
        .await?;
    println!("{} {}", account.id, account.name);
    Ok(())
}
/// List secret-free provider account metadata.
pub async fn dns_providers(config: Arc<Config>) -> anyhow::Result<()> {
    for account in build_dns(config).await?.accounts().await? {
        println!("{} {} {}", account.id, account.kind, account.name);
    }
    Ok(())
}
async fn first_dns_account(
    service: &openpanel_app::DnsService,
) -> anyhow::Result<openpanel_app::dns::ProviderAccount> {
    service
        .accounts()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no DNS provider account"))
}
/// Retest the first configured provider account.
pub async fn dns_provider_test(config: Arc<Config>) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    let tested = service
        .test_account(uuid::Uuid::nil(), Role::Owner, account.id)
        .await?;
    println!("{} tested", tested.id);
    Ok(())
}
/// Rotate the first provider account credential.
pub async fn dns_provider_rotate(config: Arc<Config>, credential: String) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    let rotated = service
        .rotate_account(uuid::Uuid::nil(), Role::Owner, account.id, &credential)
        .await?;
    println!("{} rotated", rotated.id);
    Ok(())
}
/// Enable or disable the first provider account.
pub async fn dns_provider_enabled(config: Arc<Config>, enabled: bool) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    let updated = service
        .set_account_enabled(uuid::Uuid::nil(), Role::Owner, account.id, enabled)
        .await?;
    println!("{} enabled={}", updated.id, updated.enabled);
    Ok(())
}
/// Delete first provider metadata after confirmation.
pub async fn dns_provider_delete(config: Arc<Config>, confirm: bool) -> anyhow::Result<()> {
    if !confirm {
        return Err(anyhow::anyhow!("DNS provider deletion requires --confirm"));
    }
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    service
        .delete_account(uuid::Uuid::nil(), Role::Owner, account.id)
        .await?;
    println!("deleted {}", account.id);
    Ok(())
}
/// Synchronize the first configured provider account.
pub async fn dns_sync(config: Arc<Config>) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let account = first_dns_account(&service).await?;
    let result = service
        .sync(uuid::Uuid::nil(), Role::Owner, account.id)
        .await?;
    println!("zones={} records={}", result.zones, result.imported_records);
    Ok(())
}
/// List synchronized DNS zones.
pub async fn dns_zones(config: Arc<Config>) -> anyhow::Result<()> {
    for zone in build_dns(config).await?.zones().await? {
        println!("{} {}", zone.id, zone.name);
    }
    Ok(())
}
async fn dns_zone(
    service: &openpanel_app::DnsService,
    name: &str,
) -> anyhow::Result<openpanel_app::dns::ProviderZone> {
    service
        .zones()
        .await?
        .into_iter()
        .find(|zone| zone.name.as_str() == name.trim_end_matches('.').to_ascii_lowercase())
        .ok_or_else(|| anyhow::anyhow!("DNS zone not found"))
}
/// Add one typed DNS record.
pub async fn dns_record_add(
    config: Arc<Config>,
    zone: String,
    name: String,
    kind: String,
    value: String,
    ttl: u32,
) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    let record = service
        .create_record(
            uuid::Uuid::nil(),
            Role::Owner,
            zone.id,
            &name,
            parse_record_kind(&kind)?,
            &value,
            ttl,
            zone.remote_version.as_str(),
        )
        .await?;
    println!("{} {} {}", record.remote_id, record.name, record.data);
    Ok(())
}
/// Update one existing DNS record, preserving its type.
pub async fn dns_record_update(
    config: Arc<Config>,
    zone: String,
    record: String,
    value: String,
    ttl: u32,
) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    let found = service
        .records(zone.id)
        .await?
        .into_iter()
        .find(|item| item.name.as_str() == record.trim_end_matches('.').to_ascii_lowercase())
        .ok_or_else(|| anyhow::anyhow!("DNS record not found"))?;
    let updated = service
        .update_record(
            uuid::Uuid::nil(),
            Role::Owner,
            zone.id,
            &found.remote_id,
            found.name.as_str(),
            found.data.kind(),
            &value,
            ttl,
            found.remote_version.as_str(),
        )
        .await?;
    println!("{} {}", updated.name, updated.data);
    Ok(())
}
/// List synchronized records for a zone.
pub async fn dns_records(config: Arc<Config>, zone: String) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    for record in service.records(zone.id).await? {
        println!("{} {} {}", record.remote_id, record.name, record.data);
    }
    Ok(())
}
/// Check propagation status for a zone.
pub async fn dns_check(config: Arc<Config>, zone: String) -> anyhow::Result<()> {
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    let result = service.check(zone.id).await?;
    println!("{}", result.status);
    Ok(())
}
/// Delete exactly one matching record after confirmation.
pub async fn dns_record_delete(
    config: Arc<Config>,
    zone: String,
    record: String,
    confirm: bool,
) -> anyhow::Result<()> {
    if !confirm {
        return Err(anyhow::anyhow!("DNS record deletion requires --confirm"));
    }
    let service = build_dns(config).await?;
    let zone = dns_zone(&service, &zone).await?;
    let found = service
        .records(zone.id)
        .await?
        .into_iter()
        .find(|item| item.name.as_str() == record.trim_end_matches('.').to_ascii_lowercase())
        .ok_or_else(|| anyhow::anyhow!("DNS record not found"))?;
    service
        .delete_record(
            uuid::Uuid::nil(),
            Role::Owner,
            zone.id,
            &found.remote_id,
            found.remote_version.as_str(),
        )
        .await?;
    println!("deleted {}", found.remote_id);
    Ok(())
}

async fn build_mail(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::MailService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let module = MailModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

async fn mail_domain(
    service: &openpanel_app::MailService,
    name: &str,
) -> anyhow::Result<openpanel_app::mail::MailDomain> {
    service
        .domains(uuid::Uuid::nil(), Role::Owner)
        .await?
        .into_iter()
        .find(|domain| domain.name.as_str() == name.trim_end_matches('.').to_ascii_lowercase())
        .ok_or_else(|| anyhow::anyhow!("mail domain not found"))
}

/// Print dependency and DNS/TLS readiness without secrets.
pub async fn mail_readiness(config: Arc<Config>) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string(&build_mail(config).await?.readiness().await?)?
    );
    Ok(())
}

/// Create a disabled hosted-mail domain.
pub async fn mail_domain_add(config: Arc<Config>, name: String) -> anyhow::Result<()> {
    let domain = build_mail(config)
        .await?
        .create_domain(uuid::Uuid::nil(), Role::Owner, &name)
        .await?;
    println!("{} {}", domain.id, domain.name);
    Ok(())
}

/// List hosted-mail domains.
pub async fn mail_domains(config: Arc<Config>) -> anyhow::Result<()> {
    for domain in build_mail(config)
        .await?
        .domains(uuid::Uuid::nil(), Role::Owner)
        .await?
    {
        println!("{} {} enabled={}", domain.id, domain.name, domain.enabled);
    }
    Ok(())
}

/// Create a mailbox and print its one-time credential.
pub async fn mail_mailbox_add(
    config: Arc<Config>,
    domain: String,
    local: String,
    quota: u64,
    password: String,
) -> anyhow::Result<()> {
    let service = build_mail(config).await?;
    let domain = mail_domain(&service, &domain).await?;
    let quota = openpanel_domain::mail::MailQuota::new(quota, 1_024, 1_073_741_824)?;
    let credential = service
        .create_mailbox(
            uuid::Uuid::nil(),
            Role::Owner,
            domain.id,
            &local,
            quota,
            Some(&password),
        )
        .await?;
    println!("{} {}", credential.mailbox.address, credential.password);
    Ok(())
}

/// List secret-free mailboxes for a domain.
pub async fn mail_mailboxes(config: Arc<Config>, domain: String) -> anyhow::Result<()> {
    let service = build_mail(config).await?;
    let domain = mail_domain(&service, &domain).await?;
    for mailbox in service
        .mailboxes(uuid::Uuid::nil(), Role::Owner, domain.id)
        .await?
    {
        println!("{} quota={}", mailbox.address, mailbox.quota.bytes());
    }
    Ok(())
}

/// Create a non-cyclic alias.
pub async fn mail_alias_add(
    config: Arc<Config>,
    domain: String,
    source: String,
    destination: String,
) -> anyhow::Result<()> {
    let service = build_mail(config).await?;
    let domain = mail_domain(&service, &domain).await?;
    let alias = service
        .add_alias(
            uuid::Uuid::nil(),
            Role::Owner,
            domain.id,
            &source,
            &destination,
        )
        .await?;
    println!("{} -> {}", alias.source, alias.destination);
    Ok(())
}

/// Change a mailbox quota.
pub async fn mail_quota(config: Arc<Config>, address: String, bytes: u64) -> anyhow::Result<()> {
    let quota = openpanel_domain::mail::MailQuota::new(bytes, 1_024, 1_073_741_824)?;
    let mailbox = build_mail(config)
        .await?
        .update_quota(uuid::Uuid::nil(), Role::Owner, &address, quota)
        .await?;
    println!("{} quota={}", mailbox.address, mailbox.quota.bytes());
    Ok(())
}

/// Rotate and print a mailbox's one-time credential.
pub async fn mail_password(
    config: Arc<Config>,
    address: String,
    password: String,
) -> anyhow::Result<()> {
    let credential = build_mail(config)
        .await?
        .rotate_password(uuid::Uuid::nil(), Role::Owner, &address, &password)
        .await?;
    println!("{} {}", credential.mailbox.address, credential.password);
    Ok(())
}

/// Print aggregate mail diagnostics only.
pub async fn mail_status(config: Arc<Config>) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string(&build_mail(config).await?.status().await?)?
    );
    Ok(())
}

async fn build_software_center(
    config: Arc<Config>,
) -> anyhow::Result<Arc<openpanel_app::SoftwareCenterService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    sqlx::query(include_str!("audit.sql"))
        .execute(&pool)
        .await
        .context("ensure audit schema")?;
    let ctx = AppContext::new(config, db, audit);
    let sites_module = SitesModule::new(&ctx).await;
    let master_key = if std::env::var("OPENPANEL__SOFTWARE__ADAPTER").as_deref() == Ok("fake") {
        [0_u8; 32]
    } else {
        load_master_key(&ctx.config)?
    };
    let databases_module = DatabasesModule::new(&ctx, master_key).await;
    let module =
        SoftwareCenterModule::new(&ctx, sites_module.service(), databases_module.service())
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let runner = MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(sites_module.name(), &sites_module.migrations())
        .await?;
    runner
        .apply_module(databases_module.name(), &databases_module.migrations())
        .await?;
    runner
        .apply_module(module.name(), &module.migrations())
        .await?;
    Ok(module.service())
}

/// Print the trusted recovery catalog as secret-free JSON.
pub async fn software_catalog(config: Arc<Config>) -> anyhow::Result<()> {
    let catalog = build_software_center(config)
        .await?
        .catalog(Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&catalog)?);
    Ok(())
}

/// Discover catalog component lifecycle state as secret-free JSON.
pub async fn software_inventory(config: Arc<Config>) -> anyhow::Result<()> {
    let inventory = build_software_center(config)
        .await?
        .inventory(Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&inventory)?);
    Ok(())
}

/// Print an immutable installation preview as secret-free JSON.
pub async fn software_preview(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let preview = build_software_center(config)
        .await?
        .preview_install(uuid::Uuid::nil(), Role::Owner, &id)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}

/// Execute a persisted fresh preview by exact digest and one-use confirmation token.
pub async fn software_execute(
    config: Arc<Config>,
    digest: String,
    confirmation_token: String,
) -> anyhow::Result<()> {
    let job = build_software_center(config)
        .await?
        .execute(uuid::Uuid::nil(), Role::Owner, &digest, &confirmation_token)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&job)?);
    Ok(())
}

/// Preview and execute one catalog installation.
pub async fn software_install(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let service = build_software_center(config).await?;
    let preview = service
        .preview_install(uuid::Uuid::nil(), Role::Owner, &id)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let job = service
        .execute(
            uuid::Uuid::nil(),
            Role::Owner,
            preview.plan.digest(),
            &preview.confirmation_token,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&job)?);
    Ok(())
}

/// Preview and execute one adoption, update, or uninstall transaction.
pub async fn software_component_action(
    config: Arc<Config>,
    id: String,
    action: openpanel_app::software_center::ComponentAction,
) -> anyhow::Result<()> {
    let service = build_software_center(config).await?;
    let preview = service
        .preview_component(uuid::Uuid::nil(), Role::Owner, &id, action)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let job = service
        .execute(
            uuid::Uuid::nil(),
            Role::Owner,
            preview.plan.digest(),
            &preview.confirmation_token,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&job)?);
    Ok(())
}

/// Preview and execute one transactional WordPress or Drupal deployment.
pub async fn software_deploy(
    config: Arc<Config>,
    application: String,
    domain: String,
    php_version: String,
    locale: String,
) -> anyhow::Result<()> {
    let service = build_software_center(config).await?;
    let preview = service
        .preview_deployment(
            uuid::Uuid::nil(),
            Role::Owner,
            openpanel_app::software_center::ApplicationDeploymentInput {
                application,
                domain,
                php_version,
                locale,
                enable_dns: false,
                enable_tls: false,
                enable_backups: false,
            },
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let deployment = service
        .execute_deployment(
            uuid::Uuid::nil(),
            Role::Owner,
            preview.plan.digest(),
            &preview.confirmation_token,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&deployment)?);
    Ok(())
}

/// Print bounded Software Center jobs as secret-free JSON.
pub async fn software_jobs(config: Arc<Config>) -> anyhow::Result<()> {
    let jobs = build_software_center(config)
        .await?
        .jobs(Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&jobs)?);
    Ok(())
}

/// Request safe-checkpoint cancellation for an active job.
pub async fn software_cancel(config: Arc<Config>, job: String) -> anyhow::Result<()> {
    let job = build_software_center(config)
        .await?
        .cancel(uuid::Uuid::nil(), Role::Owner, uuid::Uuid::parse_str(&job)?)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&job)?);
    Ok(())
}

/// Generate a fresh confirmation for an interrupted transaction retry.
pub async fn software_retry(config: Arc<Config>, job: String) -> anyhow::Result<()> {
    let preview = build_software_center(config)
        .await?
        .retry_preview(uuid::Uuid::nil(), Role::Owner, uuid::Uuid::parse_str(&job)?)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}

/// Run recipe-supported rollback for an interrupted component installation.
pub async fn software_rollback(config: Arc<Config>, job: String) -> anyhow::Result<()> {
    let rolled_back = build_software_center(config)
        .await?
        .rollback_interrupted(uuid::Uuid::nil(), Role::Owner, uuid::Uuid::parse_str(&job)?)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&rolled_back)?);
    Ok(())
}

/// Print aggregate Software Center diagnostics as secret-free JSON.
pub async fn software_diagnostics(config: Arc<Config>) -> anyhow::Result<()> {
    let service = build_software_center(config).await?;
    let software = service
        .catalog_diagnostics(Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let jobs = service
        .jobs(Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let combined = serde_json::json!({
        "software": software,
        "jobs": jobs,
    });
    println!("{}", serde_json::to_string_pretty(&combined)?);
    Ok(())
}

/// Trigger a manual catalog refresh.
pub async fn software_refresh(config: Arc<Config>) -> anyhow::Result<()> {
    let outcome = build_software_center(config)
        .await?
        .refresh_catalog(uuid::Uuid::nil(), Role::Owner)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    Ok(())
}

/// Search the active catalog snapshot.
#[allow(clippy::too_many_arguments)]
pub async fn software_search(
    config: Arc<Config>,
    query: Option<String>,
    category: Option<String>,
    tag: Option<String>,
    installed_only: bool,
    update_available_only: bool,
    page: usize,
    page_size: usize,
) -> anyhow::Result<()> {
    use openpanel_app::software_center::CatalogQuery;
    let mut builder = CatalogQuery {
        page,
        page_size,
        ..CatalogQuery::default()
    };
    builder.text = query;
    if let Some(category) = category
        && let Ok(parsed) = category.parse()
    {
        builder.categories.push(parsed);
    }
    if let Some(tag) = tag
        && let Ok(parsed) = openpanel_domain::software_center::Tag::new(&tag)
    {
        builder.tags.push(parsed);
    }
    builder.installed_only = installed_only;
    builder.update_available_only = update_available_only;
    let page = build_software_center(config)
        .await?
        .search(Role::Owner, builder)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&page)?);
    Ok(())
}

/// Print a single catalog entry as secret-free JSON.
pub async fn software_show(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let entry = build_software_center(config)
        .await?
        .entry(Role::Owner, &id)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&entry)?);
    Ok(())
}

/// Report host-firewall adapter support.
pub async fn security_status(config: Arc<Config>) -> anyhow::Result<()> {
    let supported = build_security(config).await?.supported().await?;
    println!("supported={supported} table=inet openpanel");
    Ok(())
}

/// Add one validated enabled firewall rule.
pub async fn security_rule_add(
    config: Arc<Config>,
    protocol: String,
    port: u16,
    source: String,
    action: String,
    comment: String,
) -> anyhow::Result<()> {
    let protocol = match protocol.as_str() {
        "tcp" => Protocol::Tcp,
        "udp" => Protocol::Udp,
        _ => return Err(anyhow::anyhow!("protocol must be tcp or udp")),
    };
    let action = match action.as_str() {
        "allow" => RuleAction::Allow,
        "deny" => RuleAction::Deny,
        _ => return Err(anyhow::anyhow!("action must be allow or deny")),
    };
    let rule = FirewallRule::new(
        uuid::Uuid::new_v4(),
        protocol,
        PortRange::new(port, port)?,
        NetworkCidr::parse(&source)?,
        action,
        comment,
        true,
    )?;
    let id = rule.id();
    build_security(config).await?.save_rule(rule).await?;
    println!("created security rule {id}");
    Ok(())
}

/// List managed firewall rules.
pub async fn security_rule_list(config: Arc<Config>) -> anyhow::Result<()> {
    for rule in build_security(config).await?.rules().await? {
        println!(
            "{} {:?} {} {} {:?} {}",
            rule.id(),
            rule.protocol(),
            rule.ports(),
            rule.source(),
            rule.action(),
            rule.comment()
        );
    }
    Ok(())
}

/// Update one rule comment while retaining validated fields.
pub async fn security_rule_update(
    config: Arc<Config>,
    id: String,
    comment: String,
) -> anyhow::Result<()> {
    let service = build_security(config).await?;
    let id = uuid::Uuid::parse_str(&id)?;
    let old = service.rule(id).await?;
    let rule = FirewallRule::new(
        id,
        old.protocol(),
        old.ports(),
        old.source(),
        old.action(),
        comment,
        old.enabled(),
    )?;
    service.save_rule(rule).await?;
    println!("updated {id}");
    Ok(())
}

/// Enable or disable one rule.
pub async fn security_rule_enabled(
    config: Arc<Config>,
    id: String,
    enabled: bool,
) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_security(config)
        .await?
        .set_rule_enabled(id, enabled)
        .await?;
    println!("{} {id}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

/// Delete one rule.
pub async fn security_rule_delete(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_security(config).await?.delete_rule(id).await?;
    println!("deleted {id}");
    Ok(())
}

/// Print the complete nftables candidate.
pub async fn security_preview(config: Arc<Config>) -> anyhow::Result<()> {
    print!(
        "{}",
        build_security(config).await?.preview_saved(None).await?
    );
    Ok(())
}

/// Apply the complete saved candidate transactionally.
pub async fn security_apply(config: Arc<Config>) -> anyhow::Result<()> {
    build_security(config)
        .await?
        .apply_saved(security_actor(), None)
        .await?;
    println!("applied inet openpanel");
    Ok(())
}

/// Restore the last-known-good OpenPanel ruleset.
pub async fn security_rollback(config: Arc<Config>) -> anyhow::Result<()> {
    build_security(config)
        .await?
        .rollback(security_actor())
        .await?;
    println!("rolled back inet openpanel");
    Ok(())
}

/// List durable login blocks without account metadata beyond the normalized key.
pub async fn security_blocks(config: Arc<Config>) -> anyhow::Result<()> {
    for block in build_security(config).await?.blocks().await? {
        println!("{} {}", block.key().storage_key(), block.expires_at());
    }
    Ok(())
}

/// List canonical login address allowlists.
pub async fn security_allowlist_list(config: Arc<Config>) -> anyhow::Result<()> {
    for network in build_security(config).await?.allowlists().await? {
        println!("{network}");
    }
    Ok(())
}

/// Add a canonical login address allowlist.
pub async fn security_allowlist_add(config: Arc<Config>, network: String) -> anyhow::Result<()> {
    let network = NetworkCidr::parse(&network)?;
    build_security(config)
        .await?
        .add_allowlist(security_actor(), network)
        .await?;
    println!("allowlisted {network}");
    Ok(())
}

/// Delete a canonical login address allowlist.
pub async fn security_allowlist_delete(config: Arc<Config>, network: String) -> anyhow::Result<()> {
    let network = NetworkCidr::parse(&network)?;
    build_security(config)
        .await?
        .remove_allowlist(security_actor(), network)
        .await?;
    println!("removed {network}");
    Ok(())
}

/// End a login block; absence is idempotent for recovery scripts.
pub async fn security_unblock(config: Arc<Config>, key: String) -> anyhow::Result<()> {
    let service = build_security(config).await?;
    let key = LoginKey::stored(&key)?;
    if service.unblock(security_actor(), key).await.is_ok() {
        println!("unblocked");
    } else {
        println!("already unblocked");
    }
    Ok(())
}

/// List registered log sources visible to the local Owner CLI.
pub async fn logs_sources(config: Arc<Config>) -> anyhow::Result<()> {
    let service = build_logs(config).await?;
    for source in service.sources(logs_actor()).await? {
        println!("{} {}", source.name(), source.id());
    }
    Ok(())
}

/// Print newest redacted entries from a registered source.
pub async fn logs_tail(config: Arc<Config>, source: String, limit: usize) -> anyhow::Result<()> {
    let service = build_logs(config).await?;
    let source = service.resolve(logs_actor(), &source).await?;
    let page = service
        .entries(
            logs_actor(),
            openpanel_app::logs::LogReadQuery::new(source.id(), limit),
        )
        .await?;
    for entry in page.entries {
        println!("{}", entry.text);
    }
    Ok(())
}

/// Print retained traffic aggregates as JSON.
pub async fn logs_traffic(config: Arc<Config>) -> anyhow::Result<()> {
    let rows = build_logs(config).await?.traffic(logs_actor()).await?;
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

/// Print recent audit events as JSON.
pub async fn logs_audit(config: Arc<Config>) -> anyhow::Result<()> {
    let events = build_logs(config)
        .await?
        .audit_events(logs_actor(), 100)
        .await?;
    println!("{}", serde_json::to_string_pretty(&events)?);
    Ok(())
}

/// Write a bounded redacted log export to an explicit destination.
pub async fn logs_export(
    config: Arc<Config>,
    source: String,
    output: std::path::PathBuf,
) -> anyhow::Result<()> {
    let service = build_logs(config).await?;
    let source = service.resolve(logs_actor(), &source).await?;
    let bytes = service
        .download(
            logs_actor(),
            source.id(),
            openpanel_app::logs::MAX_DOWNLOAD_BYTES,
        )
        .await?;
    std::fs::write(&output, bytes).with_context(|| format!("write {}", output.display()))?;
    println!("exported {}", output.display());
    Ok(())
}
/// Create a backup plan.
pub async fn backup_plan_create(
    config: Arc<Config>,
    name: String,
    schedule: String,
    timezone: String,
    panel_metadata: bool,
    retention: usize,
) -> anyhow::Result<()> {
    if !panel_metadata {
        return Err(anyhow::anyhow!("select at least one resource"));
    }
    let plan = build_backups(config)
        .await?
        .create_plan(
            backup_owner(),
            openpanel_app::backups::BackupPlanInput {
                name,
                resources: vec![openpanel_domain::backups::BackupResource::PanelMetadata],
                schedule,
                timezone,
                retention_copies: retention,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("created backup plan {}", plan.id());
    Ok(())
}
/// List backup plans.
pub async fn backup_plan_list(config: Arc<Config>) -> anyhow::Result<()> {
    for plan in build_backups(config)
        .await?
        .plans(backup_owner(), false)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!("{}  {}  {}", plan.id(), plan.name(), plan.schedule());
    }
    Ok(())
}
/// Show a backup plan.
pub async fn backup_plan_get(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let plan = build_backups(config)
        .await?
        .plan(backup_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}  {}", plan.id(), plan.name());
    Ok(())
}
/// Update backup retention.
pub async fn backup_plan_update(
    config: Arc<Config>,
    id: String,
    retention: usize,
) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .update_plan(
            backup_owner(),
            false,
            id,
            openpanel_app::backups::BackupPlanUpdate {
                name: None,
                retention_copies: Some(retention),
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("updated {id}");
    Ok(())
}
/// Enable or disable a backup plan.
pub async fn backup_plan_enabled(
    config: Arc<Config>,
    id: String,
    enabled: bool,
) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .set_enabled(backup_owner(), false, id, enabled)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("changed {id}");
    Ok(())
}
/// Delete a backup plan.
pub async fn backup_plan_delete(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .delete_plan(backup_owner(), false, id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}
/// Run a backup plan immediately.
pub async fn backup_run(config: Arc<Config>, plan_id: String) -> anyhow::Result<()> {
    let run = build_backups(config)
        .await?
        .run_plan(backup_owner(), false, uuid::Uuid::parse_str(&plan_id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("backup run {}", run.id());
    Ok(())
}
/// List backup runs.
pub async fn backup_runs(config: Arc<Config>) -> anyhow::Result<()> {
    for run in build_backups(config)
        .await?
        .runs(backup_owner(), false)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!("{}  {:?}", run.id(), run.state());
    }
    Ok(())
}
/// Show backup status.
pub async fn backup_status(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let run = build_backups(config)
        .await?
        .run(backup_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}  {:?}", run.id(), run.state());
    Ok(())
}
/// Verify a backup.
pub async fn backup_verify(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .verify(backup_owner(), false, id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("verified {id}");
    Ok(())
}
/// Preview a restore.
pub async fn backup_restore_preview(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let preview = build_backups(config)
        .await?
        .restore_preview(backup_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("ready={} bytes={}", preview.ready, preview.required_bytes);
    Ok(())
}
/// Start a safe restore.
pub async fn backup_restore_start(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let job = build_backups(config)
        .await?
        .restore(
            backup_owner(),
            false,
            uuid::Uuid::parse_str(&id)?,
            openpanel_app::backups::RestoreInput {
                resources: vec![],
                conflict_policy: "fail".into(),
                confirmation_token: None,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("restore {}", job.id);
    Ok(())
}
/// Delete a finalized backup run.
pub async fn backup_delete(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_backups(config)
        .await?
        .delete_run(backup_owner(), false, id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}

/// Create a direct command cron job.
#[allow(clippy::too_many_arguments)]
pub async fn cron_create(
    config: Arc<Config>,
    name: String,
    schedule: String,
    timezone: String,
    executable: String,
    arguments: Vec<String>,
    working_directory: String,
    timeout: u64,
) -> anyhow::Result<()> {
    let svc = build_cron(config).await?;
    let job = svc
        .create(
            cron_owner(),
            openpanel_app::cron::CronInput {
                name,
                schedule,
                timezone,
                kind: "command".into(),
                executable: Some(executable),
                arguments,
                working_directory: Some(working_directory),
                url: None,
                method: None,
                timeout_secs: timeout,
                overlap_policy: "skip".into(),
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("created cron job {}", job.id());
    Ok(())
}

/// List cron jobs.
pub async fn cron_list(config: Arc<Config>) -> anyhow::Result<()> {
    for job in build_cron(config)
        .await?
        .list(cron_owner(), false)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!(
            "{}  {}  {} {}",
            job.id(),
            job.name(),
            job.schedule().expression(),
            job.schedule().timezone()
        );
    }
    Ok(())
}
/// Show a cron job.
pub async fn cron_get(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let job = build_cron(config)
        .await?
        .get(cron_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}  {}  {}",
        job.id(),
        job.name(),
        job.schedule().expression()
    );
    Ok(())
}
/// Change a cron expression.
pub async fn cron_update(config: Arc<Config>, id: String, schedule: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_cron(config)
        .await?
        .update(
            cron_owner(),
            false,
            id,
            openpanel_app::cron::CronUpdate {
                name: None,
                schedule: Some(schedule),
                timezone: None,
                timeout_secs: None,
                overlap_policy: None,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("updated {id}");
    Ok(())
}
/// Enable or disable a cron job.
pub async fn cron_enabled(config: Arc<Config>, id: String, enabled: bool) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_cron(config)
        .await?
        .set_enabled(cron_owner(), false, id, enabled)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{} {id}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}
/// Execute a cron job now.
pub async fn cron_run(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let run = build_cron(config)
        .await?
        .run_now(cron_owner(), false, uuid::Uuid::parse_str(&id)?)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("run {} {:?}", run.id(), run.state());
    Ok(())
}
/// List cron execution history.
pub async fn cron_runs(config: Arc<Config>, job_id: Option<String>) -> anyhow::Result<()> {
    let job_id = job_id.map(|id| uuid::Uuid::parse_str(&id)).transpose()?;
    for run in build_cron(config)
        .await?
        .runs(cron_owner(), false, job_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!("{}  {}  {:?}", run.id(), run.job_id(), run.state());
    }
    Ok(())
}
/// Delete a cron job.
pub async fn cron_delete(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let id = uuid::Uuid::parse_str(&id)?;
    build_cron(config)
        .await?
        .delete(cron_owner(), false, id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}

// Suppress unused-import warning when the env helpers below are
// pruned. Kept here so future flags (e.g. staging/production
// override) have a place to land.
#[allow(dead_code)]
const _: Option<AcmeEndpoint> = None;
#[allow(dead_code)]
const _: Option<SslPaths> = None;

/// Applies pending identity and sites migrations, then exits without starting the server.
pub async fn migrate(config: Arc<Config>) -> anyhow::Result<()> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let master_key = load_master_key(&ctx.config)?;

    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let notification_module = NotificationModule::new(&ctx, master_key)
        .await
        .map_err(anyhow::Error::msg)?;
    let sites_module = SitesModule::new(&ctx).await;

    let runner = MigrationRunner::for_sqlite(pool.clone());
    runner
        .apply_module(identity_module.name(), &identity_module.migrations())
        .await?;
    runner
        .apply_module(
            notification_module.name(),
            &notification_module.migrations(),
        )
        .await?;
    runner
        .apply_module(sites_module.name(), &sites_module.migrations())
        .await?;
    println!("migrations applied");
    Ok(())
}

/// Creates a new user with the given username, email, password, and role.
pub async fn create_user(
    config: Arc<Config>,
    username: String,
    email: String,
    password: String,
    role: Role,
) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let user = svc
        .create_user(&username, &email, &password, role, "cli")
        .await?;
    println!("created user {} ({})", user.username(), user.id());
    Ok(())
}

/// Lists all users (id, username, email, role) to stdout.
pub async fn list_users(config: Arc<Config>) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    for user in svc.list_users().await? {
        println!(
            "{}  {}  <{}>  {}",
            user.id(),
            user.username(),
            user.email(),
            user.role()
        );
    }
    Ok(())
}

/// Disables the user identified by the given UUID, preventing future logins.
pub async fn disable_user(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    svc.disable_user(uuid, "cli").await?;
    println!("disabled {id}");
    Ok(())
}

/// Deletes the user identified by the given UUID.
pub async fn delete_user(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    svc.delete_user(uuid, "cli").await?;
    println!("deleted {id}");
    Ok(())
}

/// Enroll a TOTP factor for the user. Prints the base32 secret,
/// provisioning URI, and recovery codes to stdout exactly once.
pub async fn enroll_user_totp(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let user_id = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    let username = svc
        .users()
        .find_by_id(user_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.0))?
        .ok_or_else(|| anyhow::anyhow!("user not found"))?
        .username()
        .as_str()
        .to_string();
    let now = chrono::Utc::now();
    let enrollment = svc
        .two_factor()
        .enroll_totp(user_id, &username, &username, now)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let step = enrollment.secret.current_step(now);
    let step = u64::try_from(step).context("invalid TOTP step")?;
    let code = enrollment
        .secret
        .totp("OpenPanel", &username)
        .generate(step);
    let verified = svc
        .two_factor()
        .verify_totp_enrollment("cli", user_id, enrollment.enrollment_id, &code, now)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("factor: {}", verified.factor.id());
    println!("secret_base32: {}", enrollment.secret.to_base32());
    println!("provisioning_uri: {}", enrollment.provisioning_uri);
    println!("recovery_codes (single-use, shown once):");
    for code in &verified.recovery_codes {
        println!("  {code}");
    }
    Ok(())
}

/// List a user's enrolled factors and remaining recovery codes.
pub async fn list_user_factors(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let user_id = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    let factors = svc
        .two_factor()
        .list_factors(user_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let remaining = svc
        .two_factor()
        .count_recovery_codes(user_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    for factor in &factors {
        let revoked = if factor.revoked_at().is_some() {
            " (revoked)"
        } else {
            ""
        };
        println!(
            "{} {} kind={} enrolled={}{revoked}",
            factor.id(),
            factor.user_id(),
            factor.kind(),
            factor.created_at().to_rfc3339(),
        );
    }
    println!("recovery_codes_remaining: {remaining}");
    Ok(())
}

/// Revoke a factor belonging to the user.
pub async fn revoke_user_factor(
    config: Arc<Config>,
    id: String,
    factor_id: String,
) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let user_id = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    let fid = uuid::Uuid::parse_str(&factor_id).context("invalid factor id")?;
    svc.two_factor()
        .revoke_factor("cli", user_id, fid, chrono::Utc::now())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("revoked factor {factor_id} for user {id}");
    Ok(())
}

/// Regenerate a user's recovery codes. Prints the new codes to stdout
/// exactly once.
pub async fn regenerate_user_recovery(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config).await?;
    let user_id = uuid::Uuid::parse_str(&id).context("invalid user id")?;
    let codes = svc
        .two_factor()
        .regenerate_recovery_codes("cli", user_id, chrono::Utc::now())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("recovery_codes (single-use, shown once):");
    for code in &codes {
        println!("  {code}");
    }
    Ok(())
}

/// Creates a new site with the given primary domain, owner, aliases, and PHP options.
pub async fn create_site(
    config: Arc<Config>,
    domain: String,
    owner_username: String,
    aliases: Vec<String>,
    php: bool,
    php_version: Option<String>,
    document_root: Option<String>,
) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin" || u.username().as_str() == owner_username)
        .ok_or_else(|| anyhow::anyhow!("caller user not found"))?;
    let owner = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == owner_username)
        .ok_or_else(|| anyhow::anyhow!("owner user `{owner_username}` not found"))?;
    let site = sites_svc
        .create_site(
            &caller,
            owner.id(),
            &domain,
            aliases,
            php,
            php_version,
            document_root,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("created site {} ({})", site.primary_domain(), site.id());
    Ok(())
}

/// Lists all sites (id, primary domain, status, owner id) to stdout.
pub async fn list_sites(config: Arc<Config>) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    for site in sites_svc
        .list_sites(&caller)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!(
            "{}  {}  {}  owner={}",
            site.id(),
            site.primary_domain(),
            site.status(),
            site.owner_id()
        );
    }
    Ok(())
}

/// Deletes the site identified by the given UUID.
pub async fn delete_site(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    sites_svc
        .delete_site(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}

/// Re-enables the site identified by the given UUID.
pub async fn enable_site(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    sites_svc
        .enable_site(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("enabled {id}");
    Ok(())
}

/// Disables the site identified by the given UUID (stops serving traffic without deletion).
pub async fn disable_site(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (sites_svc, identity_svc, _audit, _pool) = build_sites(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    sites_svc
        .disable_site(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("disabled {id}");
    Ok(())
}

async fn build_staging(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::StagingService>,
    Arc<openpanel_app::IdentityService>,
)> {
    use openpanel_app::site_staging;
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let master_key = load_master_key(&config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let sites_svc = SitesModule::new(&ctx).await.service();
    let staging_fs: Arc<dyn openpanel_app::StagingFilesystemLayer> =
        Arc::new(InMemoryStagingFilesystem::new());
    let sites_repo: Arc<dyn openpanel_domain::SiteRepository> = Arc::new(
        openpanel_app::sites::repo::SqliteSiteRepository::new(pool.clone()),
    );
    let module = SiteStagingModule::new(
        &ctx,
        sites_repo,
        staging_fs,
        Arc::new(openpanel_core::NoopAuditService),
    )
    .await;
    let _ = sites_svc; // suppress
    let _ = site_staging::MODULE_NAME; // suppress
    Ok((module.service(), identity_module.service()))
}

/// `openpanel site staging create`.
pub async fn staging_create(
    config: Arc<Config>,
    id: String,
    subdomain: Option<String>,
) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_staging(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let site_id = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    let req = openpanel_app::site_staging::CreateSlotRequest {
        subdomain,
        document_root: None,
        sync_policy: None,
        php_version: None,
        schedule: None,
    };
    let slot = svc
        .create_slot(&caller, site_id, req)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "staging slot id={} subdomain={} document_root={} db_name={}",
        slot.id(),
        slot.subdomain(),
        slot.document_root(),
        slot.db_name()
    );
    Ok(())
}

/// `openpanel site staging sync`.
pub async fn staging_sync(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_staging(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let site_id = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    let slot = svc
        .sync_snapshot(&caller, site_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "snapshot id={:?}",
        slot.current_snapshot().map(|s| s.as_i64())
    );
    Ok(())
}

/// `openpanel site staging promote`.
pub async fn staging_promote(
    config: Arc<Config>,
    id: String,
    snapshot: i64,
    confirmed_at: String,
) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_staging(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let site_id = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    let snapshot =
        openpanel_domain::SnapshotId::new(snapshot).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let ts: chrono::DateTime<chrono::Utc> = confirmed_at
        .parse()
        .with_context(|| format!("invalid RFC 3339 timestamp `{confirmed_at}`"))?;
    let run = svc
        .promote(&caller, site_id, snapshot, ts)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "promotion id={} status={} snapshot={}",
        run.id(),
        match run.status() {
            openpanel_domain::PromotionStatus::Pending => "pending",
            openpanel_domain::PromotionStatus::Promoting => "promoting",
            openpanel_domain::PromotionStatus::Promoted => "promoted",
            openpanel_domain::PromotionStatus::RolledBack => "rolled_back",
            openpanel_domain::PromotionStatus::Failed => "failed",
        },
        run.snapshot().as_i64()
    );
    Ok(())
}

/// `openpanel site staging delete`.
pub async fn staging_delete(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_staging(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let site_id = uuid::Uuid::parse_str(&id).context("invalid site id")?;
    svc.delete_slot(&caller, site_id, false)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("destroyed staging for {id}");
    Ok(())
}

async fn bootstrap_persistence(
    config: &Arc<Config>,
) -> anyhow::Result<(
    sqlx::Pool<sqlx::Sqlite>,
    Arc<SqliteAuditService>,
    Arc<dyn openpanel_core::DatabaseDriver>,
)> {
    let driver = SqliteDriver::new(config.database().url.clone());
    let pool = driver.connect().await.context("connect sqlite")?;
    SqliteAuditService::new(pool.clone())
        .ensure_schema()
        .await
        .context("ensure audit schema")?;
    let audit = Arc::new(SqliteAuditService::new(pool.clone()));
    let db: Arc<dyn openpanel_core::DatabaseDriver> = Arc::new(driver);
    Ok((pool, audit, db))
}

async fn build_identity(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::IdentityService>,
    Arc<SqliteAuditService>,
    sqlx::Pool<sqlx::Sqlite>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit.clone());
    let master_key = load_master_key(&ctx.config)?;
    let module = IdentityModule::new(&ctx, master_key).await;
    MigrationRunner::for_sqlite(pool.clone())
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply identity migrations")?;
    Ok((module.service(), audit, pool))
}

async fn build_sites(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::SitesService>,
    Arc<openpanel_app::IdentityService>,
    Arc<SqliteAuditService>,
    sqlx::Pool<sqlx::Sqlite>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit.clone());
    let master_key = load_master_key(&ctx.config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let sites_module = SitesModule::new(&ctx).await;
    Ok((
        sites_module.service(),
        identity_module.service(),
        audit,
        pool,
    ))
}

async fn build_waf(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::WafService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let master_key = load_master_key(&ctx.config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let sites_module = if let Ok(root) = std::env::var("OPENPANEL_WAF_NGINX_ROOT") {
        let mut paths = openpanel_app::NginxPaths::under(root.into());
        if let Ok(binary) = std::env::var("OPENPANEL_WAF_NGINX_BINARY") {
            paths.nginx_binary = binary.into();
        }
        SitesModule::with_paths(&ctx, paths).await
    } else {
        SitesModule::new(&ctx).await
    };
    let waf_module = WafModule::new(&ctx, sites_module.generator().clone()).await;
    MigrationRunner::for_sqlite(pool)
        .apply_module(waf_module.name(), &waf_module.migrations())
        .await
        .context("apply WAF migrations")?;
    Ok((waf_module.service(), identity_module.service()))
}

async fn waf_owner(
    identity: &openpanel_app::IdentityService,
) -> anyhow::Result<openpanel_domain::User> {
    identity
        .list_users()
        .await?
        .into_iter()
        .find(|user| user.role() == Role::Owner)
        .ok_or_else(|| anyhow::anyhow!("owner caller not found"))
}

async fn build_site_http(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::SiteHttpService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let master_key = load_master_key(&ctx.config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let sites_module = if let Ok(root) = std::env::var("OPENPANEL_WAF_NGINX_ROOT") {
        let mut paths = openpanel_app::NginxPaths::under(root.into());
        if let Ok(binary) = std::env::var("OPENPANEL_WAF_NGINX_BINARY") {
            paths.nginx_binary = binary.into();
        }
        SitesModule::with_paths(&ctx, paths).await
    } else {
        SitesModule::new(&ctx).await
    };
    let auth_dir = std::env::var("OPENPANEL__SITE_HTTP__AUTH_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/etc/openpanel/http-auth"));
    let module = openpanel_app::SiteHttpControlsModule::new(
        &ctx,
        sites_module.generator().clone(),
        auth_dir,
    )
    .await;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply site-http-controls migrations")?;
    Ok((module.service(), identity_module.service()))
}

/// Print a site's HTTP-controls document as JSON.
pub async fn site_http_show(config: Arc<Config>, site: String) -> anyhow::Result<()> {
    let (service, identity) = build_site_http(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.get(&owner, site_id).await?)?
    );
    Ok(())
}

/// Replace a site's HTTP-controls document from strict JSON.
pub async fn site_http_set(
    config: Arc<Config>,
    site: String,
    controls_json: String,
) -> anyhow::Result<()> {
    let (service, identity) = build_site_http(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let parsed: openpanel_domain::site_http_controls::SiteHttpControls =
        serde_json::from_str(&controls_json).context("invalid controls JSON")?;
    let desired = openpanel_domain::site_http_controls::SiteHttpControls::new(
        site_id,
        parsed.version(),
        openpanel_domain::site_http_controls::SiteHttpControlsInput {
            error_pages: parsed.error_pages().to_vec(),
            redirects: parsed.redirects().to_vec(),
            protected_dirs: parsed.protected_dirs().to_vec(),
            hotlink: parsed.hotlink().cloned(),
            ip_rules: parsed.ip_rules().to_vec(),
            mime_overrides: parsed.mime_overrides().to_vec(),
            index_policy: parsed.index_policy().cloned(),
        },
    )?;
    let saved = service.put(&owner, desired).await?;
    println!("{}", serde_json::to_string_pretty(&saved)?);
    Ok(())
}

async fn build_web_terminal(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::WebTerminalService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let master_key = load_master_key(&ctx.config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let module = openpanel_app::WebTerminalModule::new(&ctx).await;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply web-terminal migrations")?;
    Ok((module.service(), identity_module.service()))
}

/// Mint a one-time browser-terminal ticket for a site.
pub async fn terminal_ticket(config: Arc<Config>, site: String) -> anyhow::Result<()> {
    let (service, identity) = build_web_terminal(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let ticket = service.issue_ticket(&owner, site_id).await?;
    println!("{}", ticket.token.0);
    Ok(())
}

async fn build_mail_filters(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::MailFilterService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let identity_module = IdentityModule::new(&ctx, load_master_key(&ctx.config)?).await;
    let module = openpanel_app::MailFilteringModule::new(&ctx).await;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply mail-filtering migrations")?;
    Ok((module.service(), identity_module.service()))
}

/// Print the outbound-queue snapshot as JSON.
pub async fn mail_queue(config: Arc<Config>) -> anyhow::Result<()> {
    let service = build_mail(config).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.queue_snapshot().await?)?
    );
    Ok(())
}

/// Show or set a mailbox's Sieve filter.
pub async fn mail_filter(
    config: Arc<Config>,
    mailbox: String,
    script: Option<String>,
) -> anyhow::Result<()> {
    let (service, identity) = build_mail_filters(config).await?;
    let owner = waf_owner(&identity).await?;
    let mailbox_id = uuid::Uuid::parse_str(&mailbox).context("invalid mailbox id")?;
    match script {
        Some(script) => {
            let saved = service
                .set_sieve(
                    &owner,
                    openpanel_domain::SieveScript::new(mailbox_id, script)?,
                )
                .await?;
            println!("sieve applied ({} bytes)", saved.script.len());
        }
        None => {
            let current = service.get_sieve(&owner, mailbox_id).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&current.map(|s| s.script))?
            );
        }
    }
    Ok(())
}

/// Set, show, or disable a mailbox's autoresponder.
pub async fn mail_autoresponder(
    config: Arc<Config>,
    mailbox: String,
    body: Option<String>,
    off: bool,
) -> anyhow::Result<()> {
    let (service, identity) = build_mail_filters(config).await?;
    let owner = waf_owner(&identity).await?;
    let mailbox_id = uuid::Uuid::parse_str(&mailbox).context("invalid mailbox id")?;
    if off {
        service.disable_autoresponder(&owner, mailbox_id).await?;
        println!("autoresponder disabled");
        return Ok(());
    }
    let Some(body) = body else {
        let current = service.get_autoresponder(&owner, mailbox_id).await?;
        println!("{}", serde_json::to_string_pretty(&current)?);
        return Ok(());
    };
    let now = chrono::Utc::now();
    let responder = openpanel_domain::AutoResponder {
        mailbox_id,
        enabled: true,
        body,
        mode: openpanel_domain::AutoResponderMode::Once,
        window_start: now,
        window_end: now + chrono::Duration::days(365),
    };
    service.set_autoresponder(&owner, responder).await?;
    println!("autoresponder enabled");
    Ok(())
}

async fn build_sso(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::SsoService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let master_key = load_master_key(&ctx.config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let module = openpanel_app::SsoModule::new(&ctx, master_key).await;
    MigrationRunner::for_sqlite(pool)
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply sso migrations")?;
    Ok((module.service(), identity_module.service()))
}

/// List the caller's active sessions as JSON.
pub async fn auth_sessions(config: Arc<Config>) -> anyhow::Result<()> {
    let (service, identity) = build_sso(config).await?;
    let owner = waf_owner(&identity).await?;
    let sessions = service.list_sessions(&owner).await?;
    let items: Vec<serde_json::Value> = sessions
        .iter()
        .map(|session| {
            serde_json::json!({
                "id": session.id.to_string(),
                "created_at": session.created_at.to_rfc3339(),
                "last_seen_at": session.last_seen_at.to_rfc3339(),
                "source_ip": session.source_ip,
                "user_agent": session.user_agent,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&items)?);
    Ok(())
}

/// Revoke one of the caller's sessions by id.
pub async fn auth_revoke(config: Arc<Config>, session: String) -> anyhow::Result<()> {
    let (service, identity) = build_sso(config).await?;
    let owner = waf_owner(&identity).await?;
    let session_id = uuid::Uuid::parse_str(&session).context("invalid session id")?;
    service.revoke_own_session(&owner, session_id).await?;
    println!("session revoked");
    Ok(())
}

async fn build_docker(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::DockerService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let master_key = load_master_key(&ctx.config)?;
    let identity = IdentityModule::new(&ctx, master_key).await;
    let docker = DockerModule::new(&ctx, master_key)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let runner = MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(identity.name(), &identity.migrations())
        .await
        .context("apply identity migrations")?;
    runner
        .apply_module(docker.name(), &docker.migrations())
        .await
        .context("apply Docker migrations")?;
    Ok((docker.service(), identity.service()))
}

async fn build_ftp(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::FtpService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let master_key = load_master_key(&ctx.config)?;
    let identity = IdentityModule::new(&ctx, master_key).await;
    let sites = SitesModule::new(&ctx).await;
    let ftp = FtpModule::new(&ctx)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let runner = MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(identity.name(), &identity.migrations())
        .await
        .context("apply identity migrations")?;
    runner
        .apply_module(sites.name(), &sites.migrations())
        .await
        .context("apply site migrations")?;
    runner
        .apply_module(ftp.name(), &ftp.migrations())
        .await
        .context("apply FTP migrations")?;
    Ok((ftp.service(), identity.service()))
}

async fn build_api_tokens(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::ApiTokenService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let master_key = load_master_key(&ctx.config)?;
    let identity = IdentityModule::new(&ctx, master_key).await;
    let tokens = ApiTokenModule::new(&ctx, master_key).await;
    let runner = MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(identity.name(), &identity.migrations())
        .await
        .context("apply identity migrations")?;
    runner
        .apply_module(tokens.name(), &tokens.migrations())
        .await
        .context("apply API-token migrations")?;
    Ok((tokens.service(), identity.service()))
}

async fn build_notifications(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::NotificationService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit);
    let master_key = load_master_key(&ctx.config)?;
    let identity = IdentityModule::new(&ctx, master_key).await;
    let notifications = NotificationModule::new(&ctx, master_key)
        .await
        .map_err(anyhow::Error::msg)?;
    let runner = MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(identity.name(), &identity.migrations())
        .await
        .context("apply identity migrations")?;
    runner
        .apply_module(notifications.name(), &notifications.migrations())
        .await
        .context("apply notification migrations")?;
    Ok((notifications.service(), identity.service()))
}

/// Add an SMTP or webhook notification channel.
#[allow(clippy::too_many_arguments)]
pub async fn notification_channel_add(
    config: Arc<Config>,
    kind: String,
    name: String,
    endpoint: String,
    port: u16,
    username: String,
    credential: String,
    from_addr: String,
    allowlist: Vec<String>,
) -> anyhow::Result<()> {
    let (service, identity) = build_notifications(config).await?;
    let owner = waf_owner(&identity).await?;
    let value = if kind == "webhook" {
        service
            .create_webhook(
                &owner,
                openpanel_app::notifications::CreateWebhookChannel {
                    name,
                    url: endpoint,
                    signing_secret: credential,
                    allowlist,
                },
            )
            .await?
    } else if kind == "smtp" {
        service
            .create_smtp(
                &owner,
                openpanel_app::notifications::CreateSmtpChannel {
                    name,
                    host: endpoint,
                    port,
                    username,
                    password: credential,
                    from_addr,
                    tls_mode: openpanel_domain::notifications::TlsMode::StartTls,
                    allowlist,
                },
            )
            .await?
    } else {
        anyhow::bail!("kind must be smtp or webhook")
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// List safe channel metadata.
pub async fn notification_channel_list(config: Arc<Config>) -> anyhow::Result<()> {
    let (service, identity) = build_notifications(config).await?;
    let owner = waf_owner(&identity).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.channels(&owner).await?)?
    );
    Ok(())
}

/// Send a channel diagnostic.
pub async fn notification_channel_test(
    config: Arc<Config>,
    id: String,
    destination: String,
) -> anyhow::Result<()> {
    let (service, identity) = build_notifications(config).await?;
    let owner = waf_owner(&identity).await?;
    service
        .test_channel(
            &owner,
            uuid::Uuid::parse_str(&id).context("invalid channel id")?,
            &destination,
        )
        .await?;
    println!("sent");
    Ok(())
}

/// Disable a channel while retaining history.
pub async fn notification_channel_rm(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (service, identity) = build_notifications(config).await?;
    let owner = waf_owner(&identity).await?;
    service
        .disable_channel(
            &owner,
            uuid::Uuid::parse_str(&id).context("invalid channel id")?,
        )
        .await?;
    println!("disabled");
    Ok(())
}

/// Add an owner subscription.
pub async fn notification_subscription_add(
    config: Arc<Config>,
    channel: String,
    destination: String,
    kind: String,
    filter_json: String,
) -> anyhow::Result<()> {
    let (service, identity) = build_notifications(config).await?;
    let owner = waf_owner(&identity).await?;
    let kind = match kind.as_str() {
        "alert" => openpanel_domain::notifications::EventKind::Alert,
        "audit" => openpanel_domain::notifications::EventKind::Audit,
        "job-terminal" => openpanel_domain::notifications::EventKind::JobTerminal,
        _ => anyhow::bail!("invalid event kind"),
    };
    let value = service
        .create_subscription(
            &owner,
            openpanel_app::notifications::CreateSubscription {
                channel_id: uuid::Uuid::parse_str(&channel).context("invalid channel id")?,
                destination,
                kind,
                filter: serde_json::from_str(&filter_json).context("invalid filter JSON")?,
            },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// List owner subscriptions.
pub async fn notification_subscription_list(config: Arc<Config>) -> anyhow::Result<()> {
    let (service, identity) = build_notifications(config).await?;
    let owner = waf_owner(&identity).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.subscriptions(&owner).await?)?
    );
    Ok(())
}

/// Disable an owner subscription.
pub async fn notification_subscription_rm(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (service, identity) = build_notifications(config).await?;
    let owner = waf_owner(&identity).await?;
    service
        .disable_subscription(
            &owner,
            uuid::Uuid::parse_str(&id).context("invalid subscription id")?,
        )
        .await?;
    println!("disabled");
    Ok(())
}

/// Print rolling per-channel health.
pub async fn notification_health(config: Arc<Config>) -> anyhow::Result<()> {
    let (service, identity) = build_notifications(config).await?;
    let owner = waf_owner(&identity).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.health(&owner).await?)?
    );
    Ok(())
}

/// Create a scoped token for the installation owner and show plaintext once.
pub async fn token_create(
    config: Arc<Config>,
    label: String,
    scopes: Vec<String>,
    cidr_allowlist: Vec<String>,
    expires_in_days: i64,
) -> anyhow::Result<()> {
    let (service, identity) = build_api_tokens(config).await?;
    let owner = waf_owner(&identity).await?;
    let created = service
        .create(
            &owner,
            openpanel_app::api_tokens::CreateApiToken {
                user_id: owner.id(),
                label,
                scopes,
                expires_at: chrono::Utc::now() + chrono::Duration::days(expires_in_days),
                cidr_allowlist,
            },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&created)?);
    Ok(())
}

/// List safe token metadata for the installation owner.
pub async fn token_list(config: Arc<Config>) -> anyhow::Result<()> {
    let (service, identity) = build_api_tokens(config).await?;
    let owner = waf_owner(&identity).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.list(&owner, owner.id()).await?)?
    );
    Ok(())
}

/// Revoke an owner token.
pub async fn token_revoke(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (service, identity) = build_api_tokens(config).await?;
    let owner = waf_owner(&identity).await?;
    service
        .revoke(
            &owner,
            owner.id(),
            uuid::Uuid::parse_str(&id).context("invalid token id")?,
        )
        .await?;
    println!("revoked");
    Ok(())
}

/// Atomically rotate an owner token and show the replacement once.
pub async fn token_rotate(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (service, identity) = build_api_tokens(config).await?;
    let owner = waf_owner(&identity).await?;
    let created = service
        .rotate(
            &owner,
            owner.id(),
            uuid::Uuid::parse_str(&id).context("invalid token id")?,
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&created)?);
    Ok(())
}

/// Create a per-site FTP account.
pub async fn ftp_create(
    config: Arc<Config>,
    site: String,
    username: String,
    password: String,
    read_only: bool,
) -> anyhow::Result<()> {
    let (service, identity) = build_ftp(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let created = service
        .create(
            &owner,
            site_id,
            openpanel_app::CreateFtpAccount {
                username,
                password,
                read_only,
                bandwidth_kb_per_session: None,
                max_concurrent_connections: None,
            },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&created)?);
    Ok(())
}

/// List a site's FTP accounts without credential hashes.
pub async fn ftp_list(config: Arc<Config>, site: String) -> anyhow::Result<()> {
    let (service, identity) = build_ftp(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.list(&owner, site_id).await?)?
    );
    Ok(())
}

/// Enable or disable a site FTP account.
pub async fn ftp_enabled(
    config: Arc<Config>,
    site: String,
    id: String,
    enabled: bool,
) -> anyhow::Result<()> {
    let (service, identity) = build_ftp(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let id = uuid::Uuid::parse_str(&id).context("invalid FTP account id")?;
    let value = if enabled {
        service.enable(&owner, site_id, id).await?
    } else {
        service.disable(&owner, site_id, id).await?
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Delete a site FTP account.
pub async fn ftp_delete(config: Arc<Config>, site: String, id: String) -> anyhow::Result<()> {
    let (service, identity) = build_ftp(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let id = uuid::Uuid::parse_str(&id).context("invalid FTP account id")?;
    service.delete(&owner, site_id, id).await?;
    println!("deleted {id}");
    Ok(())
}

/// Add a trusted image pattern.
pub async fn docker_allow(config: Arc<Config>, pattern: String, pin: bool) -> anyhow::Result<()> {
    let (service, identity) = build_docker(config).await?;
    let owner = waf_owner(&identity).await?;
    service
        .put_allowlist(
            &owner,
            openpanel_domain::docker::ImageAllowlistEntry {
                pattern: pattern.clone(),
                allow_pull: true,
                pin_digest_required: pin,
            },
        )
        .await?;
    println!("allowed {pattern}");
    Ok(())
}

/// Pull an allowlisted image.
pub async fn docker_pull(config: Arc<Config>, image: String) -> anyhow::Result<()> {
    let (service, identity) = build_docker(config).await?;
    let owner = waf_owner(&identity).await?;
    println!("{}", service.pull(&owner, &image).await?);
    Ok(())
}
/// List managed containers.
pub async fn docker_list(config: Arc<Config>) -> anyhow::Result<()> {
    let (service, identity) = build_docker(config).await?;
    let owner = waf_owner(&identity).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.list(&owner).await?)?
    );
    Ok(())
}
/// Inspect one managed container and refresh its OOM state.
pub async fn docker_inspect(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (service, identity) = build_docker(config).await?;
    let owner = waf_owner(&identity).await?;
    let id = uuid::Uuid::parse_str(&id).context("invalid container id")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.inspect(&owner, id).await?)?
    );
    Ok(())
}
/// Create from one strict JSON specification.
pub async fn docker_create(config: Arc<Config>, json: String) -> anyhow::Result<()> {
    let (service, identity) = build_docker(config).await?;
    let owner = waf_owner(&identity).await?;
    let spec = serde_json::from_str(&json).context("invalid container spec JSON")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.create(&owner, spec).await?)?
    );
    Ok(())
}
/// Start, stop, or restart a managed container.
pub async fn docker_action(config: Arc<Config>, id: String, action: &str) -> anyhow::Result<()> {
    let (service, identity) = build_docker(config).await?;
    let owner = waf_owner(&identity).await?;
    let id = uuid::Uuid::parse_str(&id).context("invalid container id")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.action(&owner, id, action).await?)?
    );
    Ok(())
}
/// Print bounded redacted tail logs.
pub async fn docker_logs(config: Arc<Config>, id: String, tail: u64) -> anyhow::Result<()> {
    let (service, identity) = build_docker(config).await?;
    let owner = waf_owner(&identity).await?;
    let id = uuid::Uuid::parse_str(&id).context("invalid container id")?;
    for line in service.logs(&owner, id, tail).await? {
        print!("{line}");
    }
    Ok(())
}
/// Execute an argv as the configured non-root user.
pub async fn docker_exec(
    config: Arc<Config>,
    id: String,
    command: Vec<String>,
) -> anyhow::Result<()> {
    let (service, identity) = build_docker(config).await?;
    let owner = waf_owner(&identity).await?;
    let id = uuid::Uuid::parse_str(&id).context("invalid container id")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.exec(&owner, id, command).await?)?
    );
    Ok(())
}
/// Remove a managed runtime and row.
pub async fn docker_remove(config: Arc<Config>, id: String, force: bool) -> anyhow::Result<()> {
    let (service, identity) = build_docker(config).await?;
    let owner = waf_owner(&identity).await?;
    let id = uuid::Uuid::parse_str(&id).context("invalid container id")?;
    service.remove(&owner, id, force).await?;
    println!("removed {id}");
    Ok(())
}

/// Print a site's complete WAF policy as JSON.
pub async fn waf_rules(config: Arc<Config>, site: String) -> anyhow::Result<()> {
    let (service, identity) = build_waf(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.get(&owner, site_id).await?)?
    );
    Ok(())
}

/// Print a site's per-rule WAF hit totals as JSON.
pub async fn waf_hits(config: Arc<Config>, site: String) -> anyhow::Result<()> {
    let (service, identity) = build_waf(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&service.hits(&owner, site_id).await?)?
    );
    Ok(())
}

/// Add one strict JSON rule to a site's policy.
pub async fn waf_add(config: Arc<Config>, site: String, rule_json: String) -> anyhow::Result<()> {
    let (service, identity) = build_waf(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let rule: openpanel_domain::waf::Rule =
        serde_json::from_str(&rule_json).context("invalid rule JSON")?;
    let current = service.get(&owner, site_id).await?;
    let mut rules = current.rules().to_vec();
    let rule_id = rule.id();
    rules.push(rule);
    let desired = openpanel_domain::waf::RuleSet::new(
        site_id,
        current.version().saturating_add(1),
        current.default_action(),
        rules,
    )?;
    service.put(&owner, desired).await?;
    println!("added WAF rule {rule_id}");
    Ok(())
}

/// Remove one rule from a site's policy.
pub async fn waf_remove(config: Arc<Config>, site: String, rule_id: String) -> anyhow::Result<()> {
    let (service, identity) = build_waf(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let rule_id = uuid::Uuid::parse_str(&rule_id).context("invalid rule id")?;
    let current = service.get(&owner, site_id).await?;
    let rules = current
        .rules()
        .iter()
        .filter(|rule| rule.id() != rule_id)
        .cloned()
        .collect();
    let desired = openpanel_domain::waf::RuleSet::new(
        site_id,
        current.version().saturating_add(1),
        current.default_action(),
        rules,
    )?;
    service.put(&owner, desired).await?;
    println!("removed WAF rule {rule_id}");
    Ok(())
}

/// Enable or disable one rule in a site's policy.
pub async fn waf_enabled(
    config: Arc<Config>,
    site: String,
    rule_id: String,
    enabled: bool,
) -> anyhow::Result<()> {
    let (service, identity) = build_waf(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let rule_id = uuid::Uuid::parse_str(&rule_id).context("invalid rule id")?;
    let current = service.get(&owner, site_id).await?;
    let mut rules = current.rules().to_vec();
    let rule = rules
        .iter_mut()
        .find(|rule| rule.id() == rule_id)
        .ok_or_else(|| anyhow::anyhow!("WAF rule not found"))?;
    rule.set_enabled(enabled);
    let desired = openpanel_domain::waf::RuleSet::new(
        site_id,
        current.version().saturating_add(1),
        current.default_action(),
        rules,
    )?;
    service.put(&owner, desired).await?;
    println!(
        "{} WAF rule {rule_id}",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

/// Dry-run one strict JSON rule and request fixture.
pub async fn waf_test(
    config: Arc<Config>,
    site: String,
    rule_json: String,
    request_json: String,
) -> anyhow::Result<()> {
    let (service, identity) = build_waf(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let rule = serde_json::from_str(&rule_json).context("invalid rule JSON")?;
    let request = serde_json::from_str(&request_json).context("invalid request JSON")?;
    let result = service
        .dry_run(
            &owner,
            site_id,
            openpanel_domain::waf::DryRunRequest { rule, request },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn build_databases(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::DatabasesService>,
    Arc<openpanel_app::IdentityService>,
    Arc<SqliteAuditService>,
    sqlx::Pool<sqlx::Sqlite>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit.clone());
    let master_key = load_master_key(&ctx.config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let databases_module = DatabasesModule::new(&ctx, master_key).await;
    Ok((
        databases_module.service(),
        identity_module.service(),
        audit,
        pool,
    ))
}

// Helper removed: callers use `ctx.config` directly to resolve the
// master key.

/// Creates a new MySQL database and DB user owned by the given user.
/// Prints the generated password to stdout (it will not be shown again).
pub async fn create_database(
    config: Arc<Config>,
    owner_username: String,
    suffix: String,
    charset: Option<String>,
) -> anyhow::Result<()> {
    let (databases_svc, identity_svc, _audit, _pool) = build_databases(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin" || u.username().as_str() == owner_username)
        .ok_or_else(|| anyhow::anyhow!("caller user not found"))?;
    let owner = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == owner_username)
        .ok_or_else(|| anyhow::anyhow!("owner user `{owner_username}` not found"))?;
    let (db, password) = databases_svc
        .create_database(&caller, owner.id(), &owner_username, &suffix, charset)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "created database {} (id={})\n  db_user: {}\n  password: {}\n  -> copy this password into your site config; it will not be shown again.",
        db.name(),
        db.id(),
        db.db_user(),
        password
    );
    Ok(())
}

/// Lists all databases (id, name, status, owner id) to stdout.
pub async fn list_databases(config: Arc<Config>) -> anyhow::Result<()> {
    let (databases_svc, identity_svc, _audit, _pool) = build_databases(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    for db in databases_svc
        .list_databases(&caller)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        println!(
            "{}  {}  {}  owner={}",
            db.id(),
            db.name(),
            db.status(),
            db.owner_id()
        );
    }
    Ok(())
}

/// Deletes the database identified by the given UUID.
pub async fn delete_database(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (databases_svc, identity_svc, _audit, _pool) = build_databases(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid database id")?;
    databases_svc
        .delete_database(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("deleted {id}");
    Ok(())
}

/// Rotates the DB user's password for the database identified by the given UUID.
/// Prints the new password to stdout (it will not be shown again).
pub async fn change_database_password(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (databases_svc, identity_svc, _audit, _pool) = build_databases(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid database id")?;
    let new_password = databases_svc
        .change_password(&caller, uuid)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "rotated password for {id}\n  new password: {new_password}\n  -> copy into your site config; it will not be shown again."
    );
    Ok(())
}

async fn build_pitr(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::PitrService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let master_key = load_master_key(&config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let pitr_db_lookup: Arc<dyn openpanel_domain::DatabaseLookup> =
        Arc::new(SqliteDatabaseRepository::new(pool.clone()));
    let pitr_sink: Arc<dyn openpanel_app::BinlogSink> = Arc::new(InMemoryBinlogSink::new());
    let pitr_module = DbPitrModule::new(
        &ctx,
        pitr_sink,
        Vec::new(),
        pitr_db_lookup,
        Arc::new(openpanel_core::NoopAuditService),
    )
    .await;
    let _ = audit;
    Ok((pitr_module.service(), identity_module.service()))
}

/// Show the active binlog stream for a database.
pub async fn pitr_status(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (pitr_svc, identity_svc) = build_pitr(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid database id")?;
    match pitr_svc
        .stream_for(&caller, uuid)
        .await
        .map_err(anyhow::Error::msg)?
    {
        Some(s) => println!(
            "stream id={} status={} last_flushed={} target={}",
            s.id(),
            s.status().as_str(),
            s.last_flushed().to_hex(),
            s.target()
        ),
        None => println!("no active stream for database {id}"),
    }
    Ok(())
}

/// Inspect the available transaction-log range for a database.
pub async fn pitr_inspect(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let (pitr_svc, _identity_svc) = build_pitr(config).await?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid database id")?;
    let range = pitr_svc
        .inspect_range(uuid)
        .await
        .map_err(anyhow::Error::msg)?;
    println!(
        "earliest: {}\nlatest: {}\nstart: {}\nend: {}\nempty: {}",
        range.earliest.to_hex(),
        range.latest.to_hex(),
        range.start.to_rfc3339(),
        range.end.to_rfc3339(),
        range.empty
    );
    Ok(())
}

/// Request a point-in-time restore.
pub async fn pitr_restore(
    config: Arc<Config>,
    id: String,
    timestamp: String,
    confirm: bool,
) -> anyhow::Result<()> {
    let (pitr_svc, identity_svc) = build_pitr(config).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let uuid = uuid::Uuid::parse_str(&id).context("invalid database id")?;
    let ts: chrono::DateTime<chrono::Utc> = timestamp
        .parse()
        .with_context(|| format!("invalid RFC 3339 timestamp `{timestamp}`"))?;
    let restore = pitr_svc
        .request_restore(
            &caller,
            uuid,
            openpanel_app::db_pitr::RestoreRequest {
                timestamp: ts,
                base_backup: None,
                confirm,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "restore id={}\nstatus={}\nstaging_db_id={}\nrequested_ts={}",
        restore.id(),
        restore.status().as_str(),
        restore.staging_db_id().unwrap_or(uuid::Uuid::nil()),
        restore.request_ts()
    );
    Ok(())
}

async fn build_files(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::FilesService>,
    Arc<openpanel_app::SitesService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (_pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config, db, audit.clone());
    let master_key = load_master_key(&ctx.config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let sites_module = SitesModule::new(&ctx).await;
    let files_module = FilesModule::new(&ctx).await;
    Ok((
        files_module.service(),
        sites_module.service(),
        identity_module.service(),
    ))
}

async fn resolve_site_id(
    _sites_svc: &openpanel_app::SitesService,
    site: &str,
) -> anyhow::Result<uuid::Uuid> {
    // Try UUID first
    if let Ok(id) = uuid::Uuid::parse_str(site) {
        return Ok(id);
    }
    // Try domain — use the service's own caller resolution
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.ok();
    let _ = pool;
    anyhow::bail!("site id `{site}` not a UUID; pass the UUID instead")
}

/// Lists entries under `path` inside the site's document root as a tabular stdout dump.
pub async fn file_list(config: Arc<Config>, site: String, path: String) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(if path.is_empty() { "" } else { &path })
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let entries = files_svc
        .list_dir(&caller, site_id, &rel)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{:<8} {:>10}  {:<6}  NAME", "TYPE", "SIZE", "MODE");
    for e in entries {
        let t = if e.is_dir { "dir" } else { "file" };
        println!("{:<8} {:>10}  {:<6}  {}", t, e.size, e.mode, e.name);
    }
    Ok(())
}

/// Writes the raw bytes of the file at `path` inside the site's document root to stdout.
pub async fn file_read(config: Arc<Config>, site: String, path: String) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(if path.is_empty() { "" } else { &path })
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let (bytes, _mtime) = files_svc
        .read_file(&caller, site_id, &rel)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    use std::io::Write;
    std::io::stdout().write_all(&bytes)?;
    Ok(())
}

/// Writes `content` to the file at `path` inside the site's document root.
pub async fn file_write(
    config: Arc<Config>,
    site: String,
    path: String,
    content: String,
) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(&path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    files_svc
        .write_file(&caller, site_id, &rel, content.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {} bytes to {path}", content.len());
    Ok(())
}

/// Creates a directory at `path` inside the site's document root.
pub async fn file_mkdir(config: Arc<Config>, site: String, path: String) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(&path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    files_svc
        .mkdir(&caller, site_id, &rel)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("created {path}");
    Ok(())
}

/// Removes the file or directory at `path` inside the site's document root.
/// If `recursive` is true, non-empty directories are removed as well.
pub async fn file_rm(
    config: Arc<Config>,
    site: String,
    path: String,
    recursive: bool,
) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(&path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    files_svc
        .remove(&caller, site_id, &rel, recursive)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("removed {path}");
    Ok(())
}

/// Renames/moves `from` to `to` within the site's document root.
pub async fn file_rename(
    config: Arc<Config>,
    site: String,
    from: String,
    to: String,
) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let from_rel = openpanel_domain::files::path::Path::new(&from)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let to_rel = openpanel_domain::files::path::Path::new(&to)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    files_svc
        .rename(&caller, site_id, &from_rel, &to_rel)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("renamed {from} -> {to}");
    Ok(())
}

/// Changes the permission bits of the file at `path` inside the site's document root.
/// `mode` is an octal string (e.g. `"755"` or `"0644"`).
pub async fn file_chmod(
    config: Arc<Config>,
    site: String,
    path: String,
    mode: String,
) -> anyhow::Result<()> {
    let (files_svc, sites_svc, identity_svc) = build_files(config).await?;
    let site_id = resolve_site_id(&sites_svc, &site).await?;
    let caller = identity_svc
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("caller not found"))?;
    let rel = openpanel_domain::files::path::Path::new(&path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let m = u32::from_str_radix(mode.trim_start_matches('0'), 8)
        .map_err(|e| anyhow::anyhow!("invalid octal `{mode}`: {e}"))?;
    files_svc
        .chmod(&caller, site_id, &rel, m)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("chmod {mode} {path}");
    Ok(())
}

/// Bootstrap the `SslService` for CLI subcommands. Uses the path
/// overrides from `OPENPANEL__SSL__PATHS__CERT_DIR` /
/// `OPENPANEL__SSL__PATHS__KEY_DIR` if set (so tests can redirect
/// the cert/key writes to a sandbox), otherwise falls back to
/// `/etc/openpanel/ssl/certs` and `/etc/openpanel/ssl/keys`.
async fn build_ssl(config: Arc<Config>) -> anyhow::Result<Arc<openpanel_app::SslService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = openpanel_core::AppContext::new(config.clone(), db, audit);
    let master_key = load_master_key(&config)?;
    let paths = resolve_ssl_paths();
    let module = openpanel_app::SslModule::with_paths(
        &ctx,
        paths,
        openpanel_app::AcmeEndpoint::default_safe(),
        master_key,
        ssl_contact_email(&config),
    )
    .await;
    let runner = openpanel_core::MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply ssl migrations")?;
    Ok(module.service())
}

fn resolve_ssl_paths() -> openpanel_app::SslPaths {
    let cert_dir = std::env::var("OPENPANEL__SSL__PATHS__CERT_DIR").ok();
    let key_dir = std::env::var("OPENPANEL__SSL__PATHS__KEY_DIR").ok();
    match (cert_dir, key_dir) {
        (Some(cert_dir), Some(key_dir)) => openpanel_app::SslPaths {
            cert_dir: std::path::PathBuf::from(cert_dir),
            key_dir: std::path::PathBuf::from(key_dir),
        },
        _ => openpanel_app::SslPaths::default_paths(),
    }
}

/// Pretty-print a `Certificate` (metadata only — never the key) for
/// CLI output.
fn print_certificate_row(cert: &openpanel_domain::Certificate) {
    println!(
        "{domain:<40} {source:<12} {status:<10} issuer={issuer:<24} valid_to={valid_to}",
        domain = cert.domain,
        source = cert.source.as_str(),
        status = format!("{:?}", cert.status(cert.valid_to)).to_lowercase(),
        issuer = cert.issuer,
        valid_to = cert.valid_to.to_rfc3339(),
    );
}

/// `openpanel ssl list` — print every certificate's metadata.
pub async fn ssl_list(config: Arc<Config>) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    let certs = svc
        .list()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if certs.is_empty() {
        println!("(no certificates)");
        return Ok(());
    }
    for cert in certs {
        print_certificate_row(&cert);
    }
    Ok(())
}

/// `openpanel ssl issue <domain> [--production]` — ACME HTTP-01.
///
/// By default targets the staging endpoint so fresh installs don't
/// burn Let's Encrypt rate limits; pass `--production` to switch.
pub async fn ssl_issue(
    config: Arc<Config>,
    domain: String,
    production: bool,
) -> anyhow::Result<()> {
    let endpoint = if production {
        openpanel_app::AcmeEndpoint::Production
    } else {
        openpanel_app::AcmeEndpoint::Staging
    };
    eprintln!(
        "[ssl] using ACME endpoint: {} (override at the module level \
         if you need a different CA)",
        endpoint.as_str()
    );
    // The service holds the configured endpoint at construction
    // time. To honor the CLI flag we'd need to rebuild the module
    // here — for v0.1 we rebuild with the requested endpoint.
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = openpanel_core::AppContext::new(config.clone(), db, audit);
    let master_key = load_master_key(&config)?;
    let paths = openpanel_app::SslPaths::default_paths();
    let acme: Arc<dyn openpanel_app::ssl::acme::AcmeClient> = Arc::new(
        openpanel_app::ssl::acme::RustlsAcmeClient::new(endpoint, ssl_contact_email(&config)),
    );
    let svc = Arc::new(openpanel_app::SslService::new(
        Arc::new(openpanel_app::ssl::SqliteCertificateRepository::new(pool)),
        ctx.audit.clone(),
        master_key,
        paths,
        openpanel_app::AcmeHttpServer::new(),
        acme,
        endpoint,
        ssl_contact_email(&config),
    ));
    let cert = svc
        .issue_acme(&domain)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("issued:");
    print_certificate_row(&cert);
    Ok(())
}

/// `openpanel ssl upload <domain> --cert <pem> [--chain <pem>]
/// --key <pem>` — manual PEM upload.
pub async fn ssl_upload(
    config: Arc<Config>,
    domain: String,
    cert: String,
    chain: Option<String>,
    key: String,
) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    let cert_pem = std::fs::read_to_string(&cert).context("read cert pem")?;
    let chain_pem = match chain {
        Some(p) => std::fs::read_to_string(&p).context("read chain pem")?,
        None => String::new(),
    };
    let key_pem = std::fs::read_to_string(&key).context("read key pem")?;
    let cert = svc
        .upload_manual(&domain, &cert_pem, &chain_pem, &key_pem)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("uploaded:");
    print_certificate_row(&cert);
    Ok(())
}

/// `openpanel ssl self-signed <domain>` — 365-day self-signed.
pub async fn ssl_self_signed(config: Arc<Config>, domain: String) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    let cert = svc
        .generate_self_signed(&domain, 365)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("generated:");
    print_certificate_row(&cert);
    Ok(())
}

/// `openpanel ssl revoke <domain>` — revoke + delete.
pub async fn ssl_revoke(config: Arc<Config>, domain: String) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    svc.delete(&domain)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("revoked {domain}");
    Ok(())
}

/// `openpanel ssl renew <domain>` — force-renew (ACME only).
pub async fn ssl_renew(config: Arc<Config>, domain: String) -> anyhow::Result<()> {
    let svc = build_ssl(config).await?;
    let cert = svc
        .renew_now(&domain)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("renewed:");
    print_certificate_row(&cert);
    Ok(())
}

/// Bootstrap the `MonitoringService` for CLI subcommands.
async fn build_monitoring(
    config: Arc<Config>,
) -> anyhow::Result<Arc<openpanel_app::MonitoringService>> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = openpanel_core::AppContext::new(config.clone(), db, audit);
    let module = MonitoringModule::new(&ctx).await;
    let runner = openpanel_core::MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply monitoring migrations")?;
    Ok(module.service())
}

/// `openpanel dev` — zero-config launcher: pick a writable data dir,
/// run migrations, bootstrap a default owner if none exists, then start
/// the web + API server. Idempotent.
pub async fn dev_up(config: Arc<Config>) -> anyhow::Result<()> {
    let (data_dir, db_url, master_key) = ensure_dev_defaults(&config)?;
    println!("openpanel dev");
    println!("  data dir:   {}", data_dir.display());
    println!("  database:   {db_url}");
    println!(
        "  bind:       {}:{}",
        config.server().bind,
        config.server().port
    );

    // Build a config that reflects the dev defaults without going back
    // through `Config::load` (which would re-read the same env vars
    // we just consumed). The defaults are layered on top of whatever
    // the user already passed via env / file / CLI.
    let config = build_dev_config(config, &db_url, &master_key);

    // 1. Migrations (idempotent: apply_module skips applied ones).
    migrate(config.clone())
        .await
        .context("apply dev migrations")?;

    // 2. Bootstrap default owner if the users table is empty.
    bootstrap_default_owner(&config).await?;

    println!();
    println!(
        "→ Login at http://{}:{}/login",
        config.server().bind,
        config.server().port
    );
    println!("  user:     admin");
    println!("  password: openpanel-dev  (CHANGE THIS IMMEDIATELY in production)");

    // 3. Serve.
    serve(config).await
}

/// Resolve the dev data directory, SQLite URL, and master key:
/// * data dir: `$OPENPANEL_DATA_DIR` or `/tmp/openpanel-dev` (created
///   if missing)
/// * db url:   `$OPENPANEL__DATABASE__URL` or `<data_dir>/openpanel.db`
/// * master key: `$OPENPANEL__DATABASE__MASTER_KEY` if set, else
///   generated and persisted to `<data_dir>/master.key`
fn ensure_dev_defaults(
    _config: &Arc<Config>,
) -> anyhow::Result<(std::path::PathBuf, String, String)> {
    let data_dir = std::env::var("OPENPANEL_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/openpanel-dev"));
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("create data dir {}", data_dir.display()))?;

    let db_url = std::env::var("OPENPANEL__DATABASE__URL")
        .unwrap_or_else(|_| format!("sqlite://{}/openpanel.db", data_dir.display()));

    let master_key = match std::env::var("OPENPANEL__DATABASE__MASTER_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            let key_path = data_dir.join("master.key");
            if key_path.exists() {
                std::fs::read_to_string(&key_path)
                    .with_context(|| format!("read {}", key_path.display()))?
            } else {
                let mut bytes = [0u8; 32];
                use rand::RngCore;
                rand::rngs::OsRng.fill_bytes(&mut bytes);
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                std::fs::write(&key_path, &encoded)
                    .with_context(|| format!("write {}", key_path.display()))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(&key_path)?.permissions();
                    perms.set_mode(0o600);
                    std::fs::set_permissions(&key_path, perms)?;
                }
                encoded
            }
        }
    };

    Ok((data_dir, db_url, master_key))
}

/// Construct a `Config` for dev: layer the chosen database URL and
/// master key into the struct so the existing `load_master_key` and
/// `bootstrap_persistence` see them.
fn build_dev_config(base: Arc<Config>, db_url: &str, master_key: &str) -> Arc<Config> {
    let mut modules = base.modules.clone();
    let db_value = modules
        .entry("database".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(obj) = db_value.as_object_mut() {
        obj.insert(
            "master_key".to_string(),
            serde_json::Value::String(master_key.to_string()),
        );
    }
    Arc::new(openpanel_core::Config {
        server: base.server.clone(),
        database: openpanel_core::config::DatabaseConfig {
            driver: base.database.driver.clone(),
            url: db_url.to_string(),
            max_connections: base.database.max_connections,
        },
        log: base.log.clone(),
        monitoring: base.monitoring.clone(),
        modules,
    })
}

/// Create a default `admin` / `admin` owner if the users table is empty.
/// Prints a clear warning that the password must be changed.
async fn bootstrap_default_owner(config: &Arc<Config>) -> anyhow::Result<()> {
    let (svc, _audit, _pool) = build_identity(config.clone()).await?;
    let users = svc.list_users().await.context("list users")?;
    if !users.is_empty() {
        return Ok(());
    }
    let email_str = "admin@openpanel.local";
    let pw = "openpanel-dev";
    svc.create_user(
        "admin",
        email_str,
        pw,
        openpanel_domain::Role::Owner,
        "dev-bootstrap",
    )
    .await
    .context("create default owner")?;
    println!("  bootstrapped owner 'admin' (password: openpanel-dev)");
    Ok(())
}

/// `openpanel monitoring overview` — print the current host snapshot.
pub async fn monitoring_overview(config: Arc<Config>) -> anyhow::Result<()> {
    let svc = build_monitoring(config).await?;
    let snap = svc
        .snapshot_now()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "timestamp={}  cpu={:.1}%  memory={:.1}%  load={:.2}",
        snap.timestamp.to_rfc3339(),
        snap.cpu,
        snap.memory,
        snap.load
    );
    for d in &snap.disk {
        println!("disk  mount={:<20} {}%", d.mount, d.percent);
    }
    for n in &snap.network {
        println!(
            "net   iface={:<16} rx={} B/s  tx={} B/s",
            n.interface, n.rx_bytes_per_sec, n.tx_bytes_per_sec
        );
    }
    Ok(())
}

/// `openpanel monitoring history --metric <kind> [--range <secs>]`.
pub async fn monitoring_history(
    config: Arc<Config>,
    metric: String,
    range: i64,
) -> anyhow::Result<()> {
    let kind = metric
        .parse::<openpanel_domain::monitoring::MetricKind>()
        .map_err(|e: openpanel_domain::monitoring::MonitoringError| {
            anyhow::anyhow!("unknown metric `{metric}`: {e}")
        })?;
    let svc = build_monitoring(config).await?;
    let since = chrono::Utc::now() - chrono::Duration::seconds(range);
    let samples = svc
        .history(kind, since)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if samples.is_empty() {
        println!("no samples for {metric} in the last {range}s");
        return Ok(());
    }
    for s in samples {
        println!("{}  {} {metric}", s.ts.to_rfc3339(), s.value);
    }
    Ok(())
}

// ---- Plugin marketplace handlers ----

/// Discover and cache the plugin marketplace catalog, printing its metadata.
pub async fn marketplace_discover(config: Arc<Config>) -> anyhow::Result<()> {
    let bundle = build_plugin_bundle(config.clone()).await?;
    let service = &bundle.marketplace;
    let outcome = service
        .discover(None)
        .await
        .context("marketplace discover")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "from_cache": outcome.from_cache,
            "digest": outcome.catalog.digest,
            "publisher_id": outcome.catalog.publisher_id,
            "entries": outcome.catalog.entries.len(),
        }))?
    );
    Ok(())
}

/// Print the cached plugin marketplace catalog, if one exists.
pub async fn marketplace_cached(config: Arc<Config>) -> anyhow::Result<()> {
    let bundle = build_plugin_bundle(config.clone()).await?;
    let snapshot = bundle
        .marketplace
        .cached()
        .await
        .context("marketplace cached")?;
    match snapshot {
        Some(snapshot) => {
            println!("{}", serde_json::to_string_pretty(&snapshot.catalog)?);
        }
        None => {
            println!(
                "{{\"catalog\":null,\"hint\":\"run `openpanel plugin marketplace discover` first\"}}"
            );
        }
    }
    Ok(())
}

/// Show one plugin entry from the cached marketplace catalog.
pub async fn marketplace_show(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let bundle = build_plugin_bundle(config.clone()).await?;
    let snapshot = bundle
        .marketplace
        .cached()
        .await
        .context("marketplace cached")?
        .ok_or_else(|| anyhow::anyhow!("no cached marketplace; run `discover` first"))?;
    let entry = snapshot
        .catalog
        .find(&id)
        .ok_or_else(|| anyhow::anyhow!("plugin {id} not in catalog"))?;
    println!("{}", serde_json::to_string_pretty(&entry)?);
    Ok(())
}

// ---- Plugin lifecycle handlers ----

/// List installed plugins.
pub async fn plugin_list(config: Arc<Config>) -> anyhow::Result<()> {
    let bundle = build_plugin_bundle(config.clone()).await?;
    let records = bundle.plugins.list().await.context("plugin list")?;
    println!("{}", serde_json::to_string_pretty(&records)?);
    Ok(())
}

/// Install a plugin from a manifest file (or stdin when the path is `-`).
pub async fn plugin_install(config: Arc<Config>, manifest_path: String) -> anyhow::Result<()> {
    let bundle = build_plugin_bundle(config.clone()).await?;
    let raw = if manifest_path == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("read manifest `{manifest_path}`"))?
    };
    let manifest: openpanel_domain::PluginManifest =
        serde_json::from_str(&raw).with_context(|| "parse manifest JSON")?;
    bundle
        .plugins
        .install_manifest(&manifest, "admin")
        .await
        .map_err(|e| anyhow::anyhow!(format!("{e}")))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "id": manifest.id.as_str(),
            "version": manifest.version.as_str(),
            "status": "installed",
        }))?
    );
    Ok(())
}

/// Enable a plugin by id.
pub async fn plugin_enable(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let bundle = build_plugin_bundle(config.clone()).await?;
    let plugin_id = openpanel_domain::PluginId::new(id)
        .map_err(|e| anyhow::anyhow!("invalid plugin id: {e}"))?;
    bundle
        .plugins
        .enable(&plugin_id, "admin")
        .await
        .context("plugin enable")?;
    println!("{{\"enabled\":\"{}\"}}", plugin_id);
    Ok(())
}

/// Disable a plugin by id.
pub async fn plugin_disable(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let bundle = build_plugin_bundle(config.clone()).await?;
    let plugin_id = openpanel_domain::PluginId::new(id)
        .map_err(|e| anyhow::anyhow!("invalid plugin id: {e}"))?;
    bundle
        .plugins
        .disable(&plugin_id, "admin")
        .await
        .context("plugin disable")?;
    println!("{{\"disabled\":\"{}\"}}", plugin_id);
    Ok(())
}

/// Uninstall a plugin by id.
pub async fn plugin_uninstall(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let bundle = build_plugin_bundle(config.clone()).await?;
    let plugin_id = openpanel_domain::PluginId::new(id)
        .map_err(|e| anyhow::anyhow!("invalid plugin id: {e}"))?;
    bundle
        .plugins
        .uninstall(&plugin_id, "admin")
        .await
        .context("plugin uninstall")?;
    println!("{{\"uninstalled\":\"{}\"}}", plugin_id);
    Ok(())
}

/// Build the small bundle of plugin services the CLI needs without
/// spinning up the full router.
pub async fn build_plugin_bundle(config: Arc<Config>) -> anyhow::Result<PluginBundle> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let _ = pool;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let plugin_module = openpanel_app::PluginModule::new(&ctx, audit.clone()).await;
    let mp_client: Arc<dyn openpanel_app::MarketplaceClient> =
        Arc::new(openpanel_app::MockMarketplaceClient::new());
    let mp_module =
        openpanel_app::PluginMarketplaceModule::new(&ctx, mp_client, plugin_module.service()).await;
    Ok(PluginBundle {
        plugins: plugin_module.service(),
        marketplace: mp_module.service(),
    })
}

/// Bundle of plugin services the CLI handlers reuse.
pub struct PluginBundle {
    /// Plugin lifecycle service.
    pub plugins: Arc<openpanel_app::PluginService>,
    /// Plugin marketplace service.
    pub marketplace: Arc<openpanel_app::MarketplaceService>,
}

// ---- Per-site collaborator handlers ----

/// Invite a collaborator to a site with the given scopes.
pub async fn collab_invite(
    config: Arc<Config>,
    site_id: String,
    email: String,
    scopes: Vec<String>,
) -> anyhow::Result<()> {
    use openpanel_app::collaborators::InviteRequest;
    use openpanel_domain::Permission;
    let bundle = build_collaborator_bundle(config.clone()).await?;
    let site_uuid = uuid::Uuid::parse_str(&site_id).context("invalid site id")?;
    let mut set = openpanel_domain::PermissionSet::EMPTY;
    for s in &scopes {
        let p = Permission::parse(s).ok_or_else(|| anyhow::anyhow!("unknown scope `{s}`"))?;
        set.insert(p);
    }
    let collaborator = bundle
        .service
        .invite(
            uuid::Uuid::nil(),
            InviteRequest {
                email,
                site_id: site_uuid,
                permissions: set,
            },
            "admin",
        )
        .await
        .context("invite collaborator")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "collaborator_id": collaborator.collaborator_id.to_string(),
            "status": collaborator.status.as_str(),
        }))?
    );
    Ok(())
}

/// List collaborators granted on a site.
pub async fn collab_list(config: Arc<Config>, site_id: String) -> anyhow::Result<()> {
    let bundle = build_collaborator_bundle(config.clone()).await?;
    let site_uuid = uuid::Uuid::parse_str(&site_id).context("invalid site id")?;
    let grants = bundle
        .service
        .grants_for_site(site_uuid)
        .await
        .context("list grants")?;
    println!("{}", serde_json::to_string_pretty(&grants)?);
    Ok(())
}

/// Revoke a collaborator from a site.
pub async fn collab_revoke(
    config: Arc<Config>,
    site_id: String,
    collaborator: String,
) -> anyhow::Result<()> {
    use std::str::FromStr;
    let bundle = build_collaborator_bundle(config.clone()).await?;
    let site_uuid = uuid::Uuid::parse_str(&site_id).context("invalid site id")?;
    let id = openpanel_domain::CollaboratorId::from_str(&collaborator)
        .map_err(|e| anyhow::anyhow!("invalid collaborator id: {e}"))?;
    bundle
        .service
        .revoke(&id, site_uuid, "admin")
        .await
        .context("revoke")?;
    println!(
        "{{\"revoked\":\"{}\",\"site\":\"{}\"}}",
        collaborator, site_id
    );
    Ok(())
}

/// Bundle of collaborator services the CLI handlers reuse.
pub struct CollaboratorBundle {
    /// Collaborator service.
    pub service: Arc<openpanel_app::CollaboratorService>,
    /// Grant resolver used to check site permissions.
    pub resolver: Arc<openpanel_app::GrantResolver>,
}

/// Build the small bundle of collaborator services the CLI needs without
/// spinning up the full router.
pub async fn build_collaborator_bundle(config: Arc<Config>) -> anyhow::Result<CollaboratorBundle> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let _ = pool;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let module = openpanel_app::CollaboratorsModule::new(&ctx, audit).await;
    Ok(CollaboratorBundle {
        service: module.service(),
        resolver: module.resolver(),
    })
}

// ---- Container registry handlers ----

/// Print the container registry configuration.
pub async fn registry_config(config: Arc<Config>) -> anyhow::Result<()> {
    let bundle = build_registry_bundle(config.clone()).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "storage_root": bundle.service.config().storage_root.display().to_string(),
            "retention": bundle.service.config().retention,
            "scan_on_push": bundle.service.config().scan_on_push,
        }))?
    );
    Ok(())
}

/// List container registry namespaces.
pub async fn registry_namespaces(config: Arc<Config>) -> anyhow::Result<()> {
    let bundle = build_registry_bundle(config.clone()).await?;
    let list = bundle.service.list_namespaces().await?;
    println!("{}", serde_json::to_string_pretty(&list)?);
    Ok(())
}

/// Create a container registry namespace for an owner with a byte quota.
pub async fn registry_create_namespace(
    config: Arc<Config>,
    namespace: String,
    owner: String,
    quota_bytes: u64,
) -> anyhow::Result<()> {
    let bundle = build_registry_bundle(config.clone()).await?;
    let ns_id = openpanel_domain::NamespaceId::new(&namespace)
        .map_err(|e| anyhow::anyhow!("invalid namespace id: {e}"))?;
    let owner_uuid = uuid::Uuid::parse_str(&owner).context("invalid owner uuid")?;
    let ns = bundle
        .service
        .create_namespace(ns_id, owner_uuid, quota_bytes)
        .await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "namespace_id": ns.namespace_id.to_string(),
            "owner": ns.owner.to_string(),
            "quota_bytes": ns.quota_bytes,
        }))?
    );
    Ok(())
}

/// List images in a container registry namespace.
pub async fn registry_images(config: Arc<Config>, namespace: String) -> anyhow::Result<()> {
    let bundle = build_registry_bundle(config.clone()).await?;
    let ns_id = openpanel_domain::NamespaceId::new(&namespace)
        .map_err(|e| anyhow::anyhow!("invalid namespace id: {e}"))?;
    let list = bundle.service.list_images(&ns_id).await?;
    println!("{}", serde_json::to_string_pretty(&list)?);
    Ok(())
}

/// Bundle of registry services the CLI handlers reuse.
pub struct RegistryBundle {
    /// Container registry service.
    pub service: Arc<openpanel_app::ContainerRegistryService>,
}

/// Build the small bundle of container registry services the CLI needs
/// without spinning up the full router.
pub async fn build_registry_bundle(config: Arc<Config>) -> anyhow::Result<RegistryBundle> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let _ = pool;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let module = openpanel_app::ContainerRegistryModule::new(
        &ctx,
        audit,
        openpanel_domain::RegistryConfig::default(),
        None,
        None,
    )
    .await;
    Ok(RegistryBundle {
        service: module.service(),
    })
}

// ---- IaC contract handlers ----

const IAC_SAMPLE: &str = r#"{
    "openapi": "3.0.0",
    "info": { "version": "1.0.0", "title": "OpenPanel" },
    "paths": {
        "/sites": {
            "get": { "operationId": "sites.list", "tags": ["sites"] },
            "post": { "operationId": "sites.create", "tags": ["sites"] }
        },
        "/sites/{id}": {
            "get": { "operationId": "sites.read", "tags": ["sites"] },
            "delete": { "operationId": "sites.delete", "tags": ["sites"] }
        },
        "/identity/users": {
            "get": { "operationId": "users.list", "tags": ["identity/users"] },
            "post": { "operationId": "users.create", "tags": ["identity/users"] },
            "delete": { "operationId": "users.delete", "tags": ["identity/users"] }
        },
        "/dns/zones": {
            "get": { "operationId": "dns_zones.list", "tags": ["dns/zones"] },
            "post": { "operationId": "dns_zones.create", "tags": ["dns/zones"] }
        },
        "/dns/zones/{id}": {
            "get": { "operationId": "dns_zones.read", "tags": ["dns/zones"] },
            "delete": { "operationId": "dns_zones.delete", "tags": ["dns/zones"] }
        },
        "/backups/plans": {
            "get": { "operationId": "backups.list", "tags": ["backups/plans"] },
            "post": { "operationId": "backups.create", "tags": ["backups/plans"] }
        },
        "/backups/plans/{id}": {
            "get": { "operationId": "backups.read", "tags": ["backups/plans"] },
            "delete": { "operationId": "backups.delete", "tags": ["backups/plans"] }
        }
    }
}"#;

/// Generate IaC contract surfaces and scaffolds from an OpenAPI document.
pub async fn iac_generate(_config: Arc<Config>, openapi: Option<String>) -> anyhow::Result<()> {
    let json = match openapi {
        Some(path) => tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| anyhow::anyhow!("read openapi {path}: {e}"))?,
        None => IAC_SAMPLE.to_string(),
    };
    let parsed = openpanel_app::ParsedOpenApi::parse_json(&json)
        .map_err(|e| anyhow::anyhow!("parse openapi: {e}"))?;
    let contract = parsed.into_contract();
    let codegen = openpanel_app::CodegenContract::new(contract.openapi_version.clone());
    let surface = codegen.build(&contract);
    println!("rust:\n{}", serde_json::to_string_pretty(&surface.rust)?);
    println!("\ngo:\n{}", serde_json::to_string_pretty(&surface.go)?);
    println!("\nts:\n{}", serde_json::to_string_pretty(&surface.ts)?);
    println!(
        "\nprovider:\n{}",
        serde_json::to_string_pretty(&surface.provider)?
    );
    println!(
        "\n----- Rust scaffold -----\n{}",
        openpanel_app::render_rust_stub(&contract)
    );
    println!(
        "\n----- Go scaffold -----\n{}",
        openpanel_app::render_go_stub(&contract)
    );
    println!(
        "\n----- TS scaffold -----\n{}",
        openpanel_app::render_typescript_stub(&contract)
    );
    println!(
        "\n----- Terraform scaffold -----\n{}",
        openpanel_app::render_provider_stub(&contract)
    );
    Ok(())
}

/// Check the generated IaC contract against committed artifacts for drift.
pub async fn iac_drift_check(
    _config: Arc<Config>,
    openapi: Option<String>,
    committed_rust: Option<String>,
    committed_provider: Option<String>,
) -> anyhow::Result<()> {
    let json = match openapi {
        Some(path) => tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| anyhow::anyhow!("read openapi {path}: {e}"))?,
        None => IAC_SAMPLE.to_string(),
    };
    let parsed = openpanel_app::ParsedOpenApi::parse_json(&json)
        .map_err(|e| anyhow::anyhow!("parse openapi: {e}"))?;
    let contract = parsed.into_contract();
    let codegen = openpanel_app::CodegenContract::new(contract.openapi_version.clone());
    // When committed paths are absent, generate fresh and report
    // a clean drift result (the caller can use this as a "what
    // would we generate" check).
    let committed = match (committed_rust, committed_provider) {
        (Some(r), Some(p)) => {
            let rust = tokio::fs::read_to_string(&r)
                .await
                .map_err(|e| anyhow::anyhow!("read rust {r}: {e}"))?;
            let provider = tokio::fs::read_to_string(&p)
                .await
                .map_err(|e| anyhow::anyhow!("read provider {p}: {e}"))?;
            openpanel_app::CommittedArtifacts {
                rust_sdk_json: rust,
                provider_json: provider,
            }
        }
        _ => {
            let surface = codegen.build(&contract);
            openpanel_app::CommittedArtifacts {
                rust_sdk_json: serde_json::to_string_pretty(&surface.rust)?,
                provider_json: serde_json::to_string_pretty(&surface.provider)?,
            }
        }
    };
    let outcome = codegen
        .drift(&contract, &committed)
        .map_err(|e| anyhow::anyhow!("drift: {e}"))?;
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    if !outcome.clean {
        std::process::exit(2);
    }
    Ok(())
}

// ---- Container runtime handlers ----

/// Bundle of container-runtime services the CLI handlers reuse.
pub struct ContainerRuntimeBundle {
    /// Container runtime service.
    pub service: Arc<openpanel_app::ContainerRuntimeService>,
    /// Identity service (used to resolve the owner caller).
    pub identity: Arc<openpanel_app::IdentityService>,
}

async fn build_container_runtime_bundle(
    config: Arc<Config>,
) -> anyhow::Result<ContainerRuntimeBundle> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let master_key = load_master_key(&ctx.config)?;
    let identity = IdentityModule::new(&ctx, master_key).await;
    let module = openpanel_app::ContainerRuntimeModule::new(
        &ctx,
        audit,
        master_key,
        openpanel_domain::PlanQuotaCaps::default(),
        None,
    )
    .await;
    let runner = MigrationRunner::for_sqlite(pool);
    runner
        .apply_module(identity.name(), &identity.migrations())
        .await
        .context("apply identity migrations")?;
    runner
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply container-runtime migrations")?;
    Ok(ContainerRuntimeBundle {
        service: module.service(),
        identity: identity.service(),
    })
}

/// Show the per-user container quota (with plan overrides applied).
pub async fn container_runtime_quota_show(config: Arc<Config>) -> anyhow::Result<()> {
    let bundle = build_container_runtime_bundle(config).await?;
    let caller = waf_owner(&bundle.identity).await?;
    let eff = bundle
        .service
        .get_quota(&caller, caller.id())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "quota": {
                "user_id": eff.quota.user_id.to_string(),
                "max_concurrent": eff.quota.max_concurrent,
                "max_total": eff.quota.max_total,
                "cpu_pct_max": eff.quota.cpu_pct_max,
                "memory_bytes_max": eff.quota.memory_bytes_max,
                "egress_bytes_per_month": eff.quota.egress_bytes_per_month,
                "updated_at": eff.quota.updated_at.to_rfc3339(),
            },
            "plan_overrides": eff.plan_overrides.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}

/// Update one or more axes of the per-user container quota.
pub async fn container_runtime_quota_set(
    config: Arc<Config>,
    max_concurrent: Option<u32>,
    max_total: Option<u32>,
    cpu: Option<u8>,
    memory_bytes: Option<u64>,
    egress_bytes: Option<u64>,
) -> anyhow::Result<()> {
    let bundle = build_container_runtime_bundle(config).await?;
    let caller = waf_owner(&bundle.identity).await?;
    let update = openpanel_app::container_runtime::QuotaUpdate {
        max_concurrent,
        max_total,
        cpu_pct_max: cpu,
        memory_bytes_max: memory_bytes,
        egress_bytes_per_month: egress_bytes,
    };
    let eff = bundle
        .service
        .set_quota(&caller, caller.id(), update)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "quota": {
                "max_concurrent": eff.quota.max_concurrent,
                "max_total": eff.quota.max_total,
                "cpu_pct_max": eff.quota.cpu_pct_max,
                "memory_bytes_max": eff.quota.memory_bytes_max,
                "egress_bytes_per_month": eff.quota.egress_bytes_per_month,
            },
            "plan_overrides": eff.plan_overrides.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}

/// List the latest metrics samples for a container.
pub async fn container_runtime_metrics(
    config: Arc<Config>,
    container_id: String,
    limit: u32,
) -> anyhow::Result<()> {
    let bundle = build_container_runtime_bundle(config).await?;
    let caller = waf_owner(&bundle.identity).await?;
    let container_id = uuid::Uuid::parse_str(&container_id).context("invalid container id")?;
    let list = bundle
        .service
        .list_metrics(&caller, container_id, limit)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&list)?);
    Ok(())
}

/// Pull an image on behalf of a container.
pub async fn container_runtime_pull(
    config: Arc<Config>,
    container_id: String,
    image: String,
    credential_id: Option<String>,
) -> anyhow::Result<()> {
    let bundle = build_container_runtime_bundle(config).await?;
    let caller = waf_owner(&bundle.identity).await?;
    let container_id = uuid::Uuid::parse_str(&container_id).context("invalid container id")?;
    let cred_id = match credential_id {
        Some(s) => Some(uuid::Uuid::parse_str(&s).context("invalid credential id")?),
        None => None,
    };
    let result = bundle
        .service
        .pull_image(&caller, container_id, image, cred_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "image_digest": result.image_digest,
            "ref_count": result.ref_count,
        }))?
    );
    Ok(())
}

/// Raise the per-user monthly egress limit.
pub async fn container_runtime_raise_egress_limit(
    config: Arc<Config>,
    bytes_per_month: u64,
) -> anyhow::Result<()> {
    let bundle = build_container_runtime_bundle(config).await?;
    let caller = waf_owner(&bundle.identity).await?;
    let quota = bundle
        .service
        .raise_egress_limit(&caller, caller.id(), bytes_per_month)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "egress_bytes_per_month": quota.egress_bytes_per_month,
        }))?
    );
    Ok(())
}

/// Add a registry credential. The plaintext password is
/// returned exactly once on stdout.
pub async fn container_runtime_registry_credential_add(
    config: Arc<Config>,
    registry: String,
    user: String,
    password: String,
) -> anyhow::Result<()> {
    let bundle = build_container_runtime_bundle(config).await?;
    let caller = waf_owner(&bundle.identity).await?;
    let result = bundle
        .service
        .create_registry_credential(&caller, registry, user, password)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "id": result.credential.id.to_string(),
            "registry": result.credential.registry,
            "username": result.credential.username,
            // Plaintext returned exactly once.
            "plaintext_once": result.plaintext_once,
        }))?
    );
    Ok(())
}

/// List the caller's registry credentials (redacted).
pub async fn container_runtime_registry_credential_list(config: Arc<Config>) -> anyhow::Result<()> {
    let bundle = build_container_runtime_bundle(config).await?;
    let caller = waf_owner(&bundle.identity).await?;
    let list = bundle
        .service
        .list_registry_credentials(&caller, caller.id())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let view: Vec<_> = list
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id.to_string(),
                "registry": c.registry,
                "username": c.username,
                "last_used_at": c.last_used_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

/// Remove a registry credential by id.
pub async fn container_runtime_registry_credential_remove(
    config: Arc<Config>,
    id: String,
) -> anyhow::Result<()> {
    let bundle = build_container_runtime_bundle(config).await?;
    let caller = waf_owner(&bundle.identity).await?;
    let credential_id = uuid::Uuid::parse_str(&id).context("invalid credential id")?;
    let removed = bundle
        .service
        .delete_registry_credential(&caller, credential_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "removed": removed }))?
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// site-cache-cdn CLI handlers (`openpanel site cache ...`, `openpanel cdn ...`).
// ---------------------------------------------------------------------------

/// Bootstrap the `SiteCacheService` for CLI subcommands.
async fn build_site_cache_cdn(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::SiteCacheService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let master_key = load_master_key(&config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let module = openpanel_app::SiteCacheCdnModule::new(&ctx).await;
    // Apply migrations so the CLI works against an existing panel DB.
    let runner = MigrationRunner::for_sqlite(pool.clone());
    runner
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply site-cache-cdn migrations")?;
    Ok((module.service(), identity_module.service()))
}

async fn resolve_admin_caller(
    identity: &openpanel_app::IdentityService,
) -> anyhow::Result<openpanel_domain::User> {
    identity
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))
}

/// `openpanel site cache show <site>`.
pub async fn site_cache_show(config: Arc<Config>, site: String) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_site_cache_cdn(config).await?;
    let _caller = resolve_admin_caller(&identity_svc).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let policy = svc
        .cache_policy(site_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "site_id": policy.site_id().to_string(),
            "ttl_seconds": policy.ttl_seconds(),
            "static_assets_ttl_seconds": policy.static_assets_ttl_seconds(),
            "bypass_paths": policy.bypass_paths(),
            "keyed_cookies": policy.keyed_cookies(),
            "stale_while_revalidate": policy.stale_while_revalidate(),
            "revalidation_required": policy.revalidation_required(),
        }))?
    );
    Ok(())
}

/// `openpanel site cache set <site> --ttl ... --bypass ... --cookies ... [--swr]`.
#[allow(clippy::too_many_arguments)]
pub async fn site_cache_set(
    config: Arc<Config>,
    site: String,
    ttl: u32,
    static_ttl: Option<u32>,
    bypass: Vec<String>,
    cookies: Vec<String>,
    swr: bool,
) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_site_cache_cdn(config).await?;
    let caller = resolve_admin_caller(&identity_svc).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let mut policy = openpanel_domain::SiteCachePolicy::with_ttl(site_id, ttl)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if let Some(s) = static_ttl {
        policy = policy
            .with_static_assets_ttl(s)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }
    policy = policy
        .with_bypass_paths(bypass)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    policy = policy
        .with_keyed_cookies(cookies)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    policy = policy
        .with_stale_while_revalidate(swr)
        .with_revalidation_required(true);
    svc.set_cache_policy(caller.username().as_str(), &policy)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "site_id": site_id.to_string(),
            "ttl_seconds": policy.ttl_seconds(),
            "bypass_paths": policy.bypass_paths(),
        }))?
    );
    Ok(())
}

/// `openpanel site cache purge <site> --paths ...`.
pub async fn site_cache_purge(
    config: Arc<Config>,
    site: String,
    paths: Vec<String>,
) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_site_cache_cdn(config).await?;
    let caller = resolve_admin_caller(&identity_svc).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let receipt = svc
        .purge_site_cache(caller.username().as_str(), site_id, paths)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "site_id": site_id.to_string(),
            "purged": receipt.purged(),
            "at": receipt.at().to_rfc3339(),
        }))?
    );
    Ok(())
}

/// `openpanel cdn integrations`.
pub async fn cdn_integrations_list(config: Arc<Config>) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_site_cache_cdn(config).await?;
    let _caller = resolve_admin_caller(&identity_svc).await?;
    let rows = svc
        .list_integrations()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let view: Vec<_> = rows
        .iter()
        .map(|i| {
            serde_json::json!({
                "id": i.id().to_string(),
                "name": i.name(),
                "kind": i.kind().as_str(),
                "enabled": i.enabled(),
                "created_at": i.created_at().to_rfc3339(),
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

/// `openpanel cdn purge --integration <id> --paths ...`.
pub async fn cdn_purge(
    config: Arc<Config>,
    integration: String,
    paths: Vec<String>,
) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_site_cache_cdn(config).await?;
    let caller = resolve_admin_caller(&identity_svc).await?;
    let integration_id = uuid::Uuid::parse_str(&integration).context("invalid integration id")?;
    let summary = svc
        .purge_via_integration(caller.username().as_str(), integration_id, paths)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "successful": summary.successful,
            "failed": summary.failed,
            "redacted": summary.redacted,
        }))?
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// site-clone-template CLI handlers.
// ---------------------------------------------------------------------------

/// Build the `SiteCloneService` for CLI subcommands.
async fn build_site_clone_template(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::SiteCloneService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let master_key = load_master_key(&config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let sites_module = SitesModule::new(&ctx).await;
    let sites_svc = sites_module.service();
    let module = openpanel_app::SiteCloneTemplateModule::new(&ctx).await;
    let runner = MigrationRunner::for_sqlite(pool.clone());
    runner
        .apply_module(module.name(), &module.migrations())
        .await
        .context("apply site-clone-template migrations")?;
    let svc = Arc::new(openpanel_app::SiteCloneService::new(
        module.repo(),
        sites_svc,
        audit,
        master_key,
        None,
        None,
    ));
    Ok((svc, identity_module.service()))
}

/// `openpanel site clone run <site> --target-domain ... --target-owner ...`.
pub async fn site_clone_run(
    config: Arc<Config>,
    site: String,
    target_domain: String,
    target_owner: String,
    keep_pii: bool,
) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_site_clone_template(config).await?;
    let caller = identity_svc
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))?;
    let source_id = uuid::Uuid::parse_str(&site).context("invalid source site id")?;
    let target_owner_id =
        uuid::Uuid::parse_str(&target_owner).context("invalid target owner id")?;
    let policy = if keep_pii {
        openpanel_domain::PiiPolicy::Keep
    } else {
        openpanel_domain::PiiPolicy::Standard
    };
    let plan = svc
        .plan_live_clone(
            caller.username().as_str(),
            source_id,
            target_domain,
            target_owner_id,
            policy,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "plan_id": plan.id().to_string(),
            "files": plan.files().len(),
            "content_hash": plan.content_hash(),
            "expires_at": plan.expires_at().to_rfc3339(),
        }))?
    );
    Ok(())
}

/// `openpanel site template export <site> --name ...`.
pub async fn site_template_export(
    config: Arc<Config>,
    site: String,
    name: String,
) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_site_clone_template(config).await?;
    let caller = identity_svc
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let (template, _artifact) = svc
        .export_template(caller.username().as_str(), site_id, name)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "id": template.id().to_string(),
            "name": template.name(),
            "source_site_id": template.source_site_id().to_string(),
            "signature_valid": template.signature_valid(),
            "pii_policy": template.pii_policy().as_str(),
        }))?
    );
    Ok(())
}

/// `openpanel site template list`.
pub async fn site_template_list(config: Arc<Config>) -> anyhow::Result<()> {
    let (svc, _identity_svc) = build_site_clone_template(config).await?;
    let rows = svc
        .list_templates()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let view: Vec<_> = rows
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id().to_string(),
                "name": t.name(),
                "source_site_id": t.source_site_id().to_string(),
                "signature_valid": t.signature_valid(),
                "pii_policy": t.pii_policy().as_str(),
                "created_at": t.created_at().to_rfc3339(),
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// themeable-ui CLI handlers.
// ---------------------------------------------------------------------------

/// Build the `ThemeableUiService` for CLI subcommands.
async fn build_themeable_ui(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::ThemeableUiService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let master_key = load_master_key(&config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let repo = openpanel_app::themeable_ui::SqliteThemeableUiRepository::new(pool);
    let svc = Arc::new(openpanel_app::ThemeableUiService::new(
        Arc::new(repo),
        audit,
        None,
    ));
    Ok((svc, identity_module.service()))
}

/// `openpanel site branding show`.
pub async fn branding_show(config: Arc<Config>) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_themeable_ui(config).await?;
    let caller = identity_svc
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))?;
    let override_ = svc
        .get_override(caller.username().as_str(), caller.id())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    match override_ {
        Some(o) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "owner_id": o.owner_id().to_string(),
                    "brand_name": o.brand_name(),
                    "color_fg": o.palette().color_fg().as_hex(),
                    "color_bg": o.palette().color_bg().as_hex(),
                    "color_accent": o.palette().color_accent().as_hex(),
                    "contrast_min": o.palette().contrast_min(),
                    "font_family": o.typography().font_family(),
                    "base_size_px": o.typography().base_size_px(),
                    "panel_domain": o.panel_domain().map(|d| d.fqdn().to_string()),
                    "logo_path": o.logo_path(),
                }))?
            );
        }
        None => println!("(no override)"),
    }
    Ok(())
}

/// `openpanel site branding set ...`.
#[allow(clippy::too_many_arguments)]
pub async fn branding_set(
    config: Arc<Config>,
    brand_name: String,
    color_fg: String,
    color_bg: String,
    color_accent: String,
    contrast_min: f64,
    font_family: String,
    base_size_px: u16,
    panel_domain: Option<String>,
) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_themeable_ui(config).await?;
    let caller = identity_svc
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))?;
    let fg = openpanel_domain::HexColor::parse(&color_fg)
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    let bg = openpanel_domain::HexColor::parse(&color_bg)
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    let accent = openpanel_domain::HexColor::parse(&color_accent)
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    let palette = openpanel_domain::Palette::with_min_contrast(fg, bg, accent, contrast_min)
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    let typography = openpanel_domain::Typography::new(font_family, base_size_px)
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    let panel_domain = match panel_domain {
        Some(fqdn) => Some(
            openpanel_domain::PanelDomain::new(fqdn)
                .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?,
        ),
        None => None,
    };
    svc.set_override(
        caller.username().as_str(),
        caller.id(),
        brand_name,
        palette,
        typography,
        panel_domain,
        openpanel_domain::BrandingScope::Reseller,
    )
    .await
    .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    println!("ok");
    Ok(())
}

/// `openpanel site branding clear`.
pub async fn branding_clear(config: Arc<Config>) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_themeable_ui(config).await?;
    let caller = identity_svc
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))?;
    svc.clear_override(
        caller.username().as_str(),
        caller.id(),
        openpanel_domain::BrandingScope::Reseller,
    )
    .await
    .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    println!("ok");
    Ok(())
}

// ---------------------------------------------------------------------------
// web-application-installer CLI handlers.
// ---------------------------------------------------------------------------

/// Build the `WebApplicationInstallerService` for CLI subcommands.
async fn build_web_application_installer(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::WebApplicationInstallerService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let master_key = load_master_key(&config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let repo =
        openpanel_app::web_application_installer::SqliteWebApplicationInstallerRepository::new(
            pool,
        );
    let svc = Arc::new(openpanel_app::WebApplicationInstallerService::new(
        Arc::new(repo),
        audit,
        Arc::new(openpanel_app::RealInstallerFs),
        Arc::new(openpanel_app::ReqwestArtifactDownloader::new()),
        master_key,
        None,
    ));
    Ok((svc, identity_module.service()))
}

/// `openpanel site webapp preview --site ... --app ... --path ...`.
pub async fn webapp_preview(
    config: Arc<Config>,
    site: String,
    app: String,
    path: String,
) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_web_application_installer(config).await?;
    let caller = identity_svc
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let plan = svc
        .plan(
            caller.username().as_str(),
            app,
            site_id,
            path,
            vec![],
            openpanel_domain::InstallDb::None,
            vec![],
            vec!["preview-only; no artifacts are fetched".to_string()],
        )
        .await
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "plan_id": plan.id().to_string(),
            "app_id": plan.app_id(),
            "site_id": plan.site_id().to_string(),
            "install_path": plan.install_path(),
            "content_hash": plan.content_hash(),
            "expires_at": plan.expires_at().to_rfc3339(),
        }))?
    );
    Ok(())
}

/// `openpanel site webapp list --site ...`.
pub async fn webapp_list(config: Arc<Config>, site: String) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_web_application_installer(config).await?;
    let _caller = identity_svc
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let rows = svc
        .list_installed(site_id)
        .await
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    let view: Vec<_> = rows
        .iter()
        .map(|i| {
            serde_json::json!({
                "install_id": i.install_id(),
                "site_id": i.site_id().to_string(),
                "app_id": i.app_id(),
                "version": i.version(),
                "install_path": i.install_path(),
                "created_at": i.created_at().to_rfc3339(),
                "removed_at": i.removed_at().map(|t| t.to_rfc3339()),
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// malware-scanner CLI handlers.
// ---------------------------------------------------------------------------

/// Build the `MalwareScannerService` for CLI subcommands.
async fn build_malware_scanner(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::MalwareScannerService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let master_key = load_master_key(&config)?;
    let identity_module = IdentityModule::new(&ctx, master_key).await;
    let repo = openpanel_app::malware_scanner::SqliteMalwareScannerRepository::new(pool);
    let svc = Arc::new(openpanel_app::MalwareScannerService::new(
        Arc::new(repo),
        audit,
        Arc::new(openpanel_app::RealScannerFs),
        None,
    ));
    Ok((svc, identity_module.service()))
}

/// `openpanel site scan start --site ...`.
pub async fn scan_start(config: Arc<Config>, site: String) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_malware_scanner(config).await?;
    let caller = identity_svc
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let chroot = std::path::PathBuf::from(format!("/var/www/{site_id}"));
    let run = svc
        .scan(caller.username().as_str(), site_id, chroot)
        .await
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "scan_id": run.id().to_string(),
            "site_id": run.site_id().to_string(),
            "status": "completed",
            "findings": run.findings().len(),
        }))?
    );
    Ok(())
}

/// `openpanel site scan list --site ...`.
pub async fn scan_list(config: Arc<Config>, site: String) -> anyhow::Result<()> {
    let (svc, identity_svc) = build_malware_scanner(config).await?;
    let _caller = identity_svc
        .list_users()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .ok_or_else(|| anyhow::anyhow!("admin user not found"))?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    let rows = svc
        .list_runs(site_id)
        .await
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;
    let view: Vec<_> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id().to_string(),
                "profile_id": r.profile_id().to_string(),
                "status": format!("{:?}", r.status()).to_lowercase(),
                "started_at": r.started_at().to_rfc3339(),
                "finished_at": r.finished_at().map(|t| t.to_rfc3339()),
                "findings": r.findings().len(),
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Server snapshot CLI handlers.
// ---------------------------------------------------------------------------

/// Build the admin SSH host-key service plus identity for callers.
async fn build_ssh_keys(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::HostSshKeysService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let identity_module = IdentityModule::new(&ctx, load_master_key(&ctx.config)?).await;
    let security_module = SecurityModule::new(&ctx).await?;
    MigrationRunner::for_sqlite(pool.clone())
        .apply_module(security_module.name(), &security_module.migrations())
        .await?;
    Ok((security_module.ssh_keys(), identity_module.service()))
}

/// `openpanel ssh-keys …`.
pub async fn ssh_keys(config: Arc<Config>, action: crate::SshKeysAction) -> anyhow::Result<()> {
    let (svc, identity) = build_ssh_keys(config).await?;
    let owner = waf_owner(&identity).await?;
    match action {
        crate::SshKeysAction::List => {
            for key in svc
                .list(&owner)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?
            {
                println!(
                    "{}  {}  {}  last_used={}",
                    key.id(),
                    key.fingerprint(),
                    key.label(),
                    key.last_used_at()
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_else(|| "never".into()),
                );
            }
        }
        crate::SshKeysAction::Add { label, key } => {
            let added = svc
                .add(&owner, label, &key)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            println!("{} {}", added.id(), added.fingerprint());
        }
        crate::SshKeysAction::Remove { id } => {
            let uuid = uuid::Uuid::parse_str(&id).context("invalid key id")?;
            svc.remove(&owner, uuid)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            println!("removed");
        }
    }
    Ok(())
}

/// Build the log rotation service plus identity for callers.
async fn build_logs_rotation(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::LogRotationService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let identity_module = IdentityModule::new(&ctx, load_master_key(&ctx.config)?).await;
    let logs_module = LogsModule::with_root(&ctx, std::env::current_dir()?).await;
    MigrationRunner::for_sqlite(pool.clone())
        .apply_module(logs_module.name(), &logs_module.migrations())
        .await?;
    Ok((logs_module.rotation(), identity_module.service()))
}

/// `openpanel logs policy …`.
pub async fn logs_policy(
    config: Arc<Config>,
    action: crate::LogsPolicyAction,
) -> anyhow::Result<()> {
    let (svc, identity) = build_logs_rotation(config).await?;
    let owner = waf_owner(&identity).await?;
    match action {
        crate::LogsPolicyAction::Show { class } => {
            let class = openpanel_domain::logs::SourceClass::parse(&class)
                .ok_or_else(|| anyhow::anyhow!("unknown source class"))?;
            let (policy, drift) = svc
                .get(&owner, class)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "policy": serde_json::to_value(&policy)?,
                    "drift": drift,
                }))?
            );
        }
        crate::LogsPolicyAction::Set {
            class,
            max_age_days,
            max_size_mb,
            keep_generations,
            compress,
        } => {
            let class = openpanel_domain::logs::SourceClass::parse(&class)
                .ok_or_else(|| anyhow::anyhow!("unknown source class"))?;
            let policy = openpanel_domain::logs::RotationPolicy::new(
                class,
                max_age_days,
                max_size_mb,
                keep_generations,
                compress,
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let saved = svc
                .set(&owner, policy)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            println!(
                "rotation policy saved for {} (maxage {}d, {}M, keep {}, compress={})",
                saved.source_class().as_str(),
                saved.max_age_days(),
                saved.max_size_mb(),
                saved.keep_generations(),
                saved.compress()
            );
        }
    }
    Ok(())
}

/// Build the per-site transport service plus identity for callers.
async fn build_site_transport(
    config: Arc<Config>,
) -> anyhow::Result<(
    Arc<openpanel_app::SiteTransportService>,
    Arc<openpanel_app::IdentityService>,
)> {
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let identity_module = IdentityModule::new(&ctx, load_master_key(&ctx.config)?).await;
    let sites_module = SitesModule::new(&ctx).await;
    MigrationRunner::for_sqlite(pool.clone())
        .apply_module(sites_module.name(), &sites_module.migrations())
        .await?;
    Ok((sites_module.transport(), identity_module.service()))
}

/// `openpanel site transport …`.
pub async fn site_transport(
    config: Arc<Config>,
    site: String,
    action: crate::TransportAction,
) -> anyhow::Result<()> {
    use openpanel_domain::{ByteSize, CompressionPolicy, TransportPolicy};
    let (svc, identity) = build_site_transport(config).await?;
    let owner = waf_owner(&identity).await?;
    let site_id = uuid::Uuid::parse_str(&site).context("invalid site id")?;
    match action {
        crate::TransportAction::Show => {
            let policy = svc
                .get(&owner, site_id)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "http3_enabled": policy.http3_enabled(),
                    "tls_min_version": format!("{:?}", policy.tls_min_version()),
                    "hsts": policy.hsts().map(|h| serde_json::json!({
                        "max_age_secs": h.render_value(),
                    })),
                    "compression": match policy.compression() {
                        CompressionPolicy::Off => serde_json::json!("off"),
                        CompressionPolicy::Gzip(l) => serde_json::json!({"gzip": l}),
                        CompressionPolicy::Brotli(l) => serde_json::json!({"brotli": l}),
                    },
                    "body_size_cap_bytes": policy.body_size_cap().as_u64(),
                }))?
            );
        }
        crate::TransportAction::Http3 { on } => {
            let current = svc
                .get(&owner, site_id)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let updated = TransportPolicy::new(
                on,
                current.tls_min_version(),
                current.hsts().cloned(),
                current.compression().clone(),
                ByteSize::new(current.body_size_cap().as_u64())
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?,
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            svc.set(&owner, site_id, updated)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            println!("http3 {}", if on { "enabled" } else { "disabled" });
        }
    }
    Ok(())
}

/// Build the `ServerSnapshotService` for CLI subcommands.
async fn build_server_snapshots(
    config: Arc<Config>,
) -> anyhow::Result<Arc<openpanel_app::ServerSnapshotService>> {
    let master_key = load_master_key(&config).ok();
    let (pool, audit, db) = bootstrap_persistence(&config).await?;
    let ctx = AppContext::new(config.clone(), db, audit.clone());
    let backup_root = std::env::var("OPENPANEL__BACKUPS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/var/lib/openpanel/backups"));
    let cron_module = CronModule::with_roots(&ctx, vec![std::env::current_dir()?]).await;
    let runner = MigrationRunner::for_sqlite(pool.clone());
    runner
        .apply_module(cron_module.name(), &cron_module.migrations())
        .await?;
    let backups_module = BackupsModule::with_root(
        &ctx,
        backup_root,
        master_key,
        Some((cron_module.service(), std::env::current_dir()?)),
    )
    .await;
    runner
        .apply_module(backups_module.name(), &backups_module.migrations())
        .await?;
    let snapshots_root = std::env::var("OPENPANEL__SNAPSHOTS__ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or(std::path::PathBuf::from("/var/lib/openpanel/snapshots"));
    Ok(Arc::new(openpanel_app::ServerSnapshotService::new(
        backups_module.service(),
        pool,
        snapshots_root,
        audit,
        env!("CARGO_PKG_VERSION").to_owned(),
    )))
}

fn snapshot_caller() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Role, Username};
    // The password is never verified: this principal represents the
    // local root-equivalent CLI operator. It only has to satisfy the
    // domain's 12-character minimum to hash.
    #[allow(clippy::expect_used)] // static constant — cannot fail
    openpanel_domain::User::new(
        uuid::Uuid::nil(),
        Username::new("admin").expect("static admin username is valid"),
        Email::new("admin@openpanel.local").expect("static admin email is valid"),
        Password::hash("openpanel-local-admin").expect("static admin password hashes"),
        Role::Owner,
    )
}

/// `openpanel server-snapshot list`.
pub async fn server_snapshot_list(config: Arc<Config>) -> anyhow::Result<()> {
    let svc = build_server_snapshots(config).await?;
    let snapshots = svc
        .list()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    for (id, manifest) in &snapshots {
        println!(
            "{}  {}  {}",
            id,
            manifest.created_at.to_rfc3339(),
            manifest.entries.len()
        );
    }
    if snapshots.is_empty() {
        println!("(no snapshots)");
    }
    Ok(())
}

/// `openpanel server-snapshot get --id ...`.
pub async fn server_snapshot_get(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let svc = build_server_snapshots(config).await?;
    let snapshot_id = uuid::Uuid::parse_str(&id).context("invalid snapshot id")?;
    let snapshot = svc
        .get(snapshot_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "id": snapshot_id.to_string(),
            "host_id": snapshot.source_host_id.to_string(),
            "panel_version": snapshot.panel_version,
            "created_at": snapshot.created_at.to_rfc3339(),
            "entries": snapshot.entries.len(),
        }))?
    );
    Ok(())
}

/// `openpanel server-snapshot create`.
pub async fn server_snapshot_create(config: Arc<Config>, retain: usize) -> anyhow::Result<()> {
    let svc = build_server_snapshots(config).await?;
    let caller = snapshot_caller();
    let backups_svc = svc.backups();
    let runs = backups_svc
        .runs(caller.id(), false)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    use openpanel_domain::backups::BackupRunState;
    let latest_completed = runs
        .iter()
        .filter(|r| r.state() == BackupRunState::Completed)
        .max_by_key(|r| r.id())
        .ok_or_else(|| anyhow::anyhow!("no completed backup runs found"))?;
    let manifest = svc
        .create_from_run(&caller, latest_completed.id())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("created snapshot with {} entries", manifest.entries.len());
    if retain > 0 {
        let pruned = svc
            .prune_retention(&caller, retain)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        println!("pruned {} old snapshot(s)", pruned.len());
    }
    Ok(())
}

/// `openpanel server-snapshot schedule ...`.
pub async fn server_snapshot_schedule(
    config: Arc<Config>,
    name: String,
    schedule: String,
    timezone: String,
    retain: usize,
) -> anyhow::Result<()> {
    let svc = build_server_snapshots(config.clone()).await?;
    let caller = snapshot_caller();
    let working_root = std::env::current_dir()?;
    let cron = build_cron(config).await?;
    let job_id = svc
        .schedule(
            &caller,
            &cron,
            &working_root,
            name,
            schedule,
            timezone,
            retain,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{job_id}");
    Ok(())
}

/// `openpanel server-snapshot preflight --id ...`.
pub async fn server_snapshot_preflight(config: Arc<Config>, id: String) -> anyhow::Result<()> {
    let svc = build_server_snapshots(config).await?;
    let caller = snapshot_caller();
    let snapshot_id = uuid::Uuid::parse_str(&id).context("invalid snapshot id")?;
    let (token, report) = svc
        .preflight(&caller, snapshot_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "snapshot_id": snapshot_id.to_string(),
            "warnings": report.warnings,
            "blockers": report.blockers,
            "collisions": report.collisions,
            "ready": report.ready,
            "confirm_token": token,
        }))?
    );
    Ok(())
}

/// `openpanel server-snapshot restore --id ... --confirm ...`.
pub async fn server_snapshot_restore(
    config: Arc<Config>,
    id: String,
    confirm: String,
) -> anyhow::Result<()> {
    let svc = build_server_snapshots(config).await?;
    let caller = snapshot_caller();
    let snapshot_id = uuid::Uuid::parse_str(&id).context("invalid snapshot id")?;
    let applied = svc
        .restore(&caller, snapshot_id, &confirm, true)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("restored {applied} resources from snapshot {snapshot_id}");
    Ok(())
}
