//! Per-test HTTP server that boots the real axum router.
//!
//! `TestServer::new()` creates a `TestDb`, boots the real axum router
//! from `openpanel_api::build_router` on a random local port, and
//! returns the bound address plus a preconfigured `reqwest::Client`.
//! `Drop` aborts the server task.
//!
//! The Software Center module always uses a `MemoryArtifactFetcher`
//! bound to a per-test sandbox directory; tests can stage pre-canned
//! bytes for a URL via [`TestServer::stage_artifact`].

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use openpanel_api::build_router;
use openpanel_app::{
    AccountHierarchyModule, ApiTokenModule, ApiTokenService, ApplyReport, BackupService,
    BackupsModule, CollaboratorService, ContainerRuntimeModule, ContainerRuntimeService,
    CronModule, CronService, DatabasesModule, DatabasesService, DbPitrModule, DnsModule,
    DnsService, DockerAdapter, DockerModule, DockerService, ExecResult, FilesModule, FilesService,
    FtpModule, FtpService, GrantResolver, HostingPlansModule, HostingPlansService, IdentityModule,
    IdentityService, InMemoryMailBridge, InMemoryStagingFilesystem, LogService, LogsModule,
    MailModule, MailService, MalwareScannerService, MarketplaceService, MigrationImportersModule,
    MonitoringModule, MonitoringService, NotificationModule, NotificationService,
    OffsiteBackupTargetsModule, PitrService, PluginService, RealInstallerFs, RealScannerFs,
    ReqwestArtifactDownloader, SecurityModule, SecurityService, SiteCacheCdnModule,
    SiteCloneService, SiteCloneTemplateModule, SiteStagingModule, SitesModule, SitesService,
    SoftwareCenterModule, SoftwareCenterService, SslModule, SslPaths, SslService, StagingService,
    SystemServicesModule, ThemeableUiService, WafModule, WafService,
    WebApplicationInstallerService, WebmailService,
    identity::two_factor::TwoFactorCrypto,
    security::MemoryFirewall,
    sites::{nginx::NginxPaths, repo::SqliteSiteRepository},
    software_center::ArtifactFetcher,
};

#[derive(Default)]
struct MemoryDocker {
    states: Mutex<HashMap<String, openpanel_app::RuntimeContainerState>>,
}

#[async_trait::async_trait]
impl DockerAdapter for MemoryDocker {
    async fn ping(&self) -> Result<(), openpanel_domain::docker::DockerError> {
        Ok(())
    }

    async fn pull(&self, image: &str) -> Result<String, openpanel_domain::docker::DockerError> {
        Ok(format!("{image}@sha256:test"))
    }

    async fn create(
        &self,
        _spec: &openpanel_domain::docker::ContainerSpec,
    ) -> Result<String, openpanel_domain::docker::DockerError> {
        let id = Uuid::new_v4().to_string();
        self.states.lock().expect("docker states").insert(
            id.clone(),
            openpanel_app::RuntimeContainerState {
                status: "created".into(),
                oom_killed: false,
            },
        );
        Ok(id)
    }

    async fn start(&self, id: &str) -> Result<(), openpanel_domain::docker::DockerError> {
        if let Some(state) = self.states.lock().expect("docker states").get_mut(id) {
            state.status = "running".into();
        }
        Ok(())
    }

    async fn stop(&self, id: &str) -> Result<(), openpanel_domain::docker::DockerError> {
        if let Some(state) = self.states.lock().expect("docker states").get_mut(id) {
            state.status = "exited".into();
        }
        Ok(())
    }

    async fn restart(&self, id: &str) -> Result<(), openpanel_domain::docker::DockerError> {
        self.start(id).await
    }

    async fn remove(
        &self,
        id: &str,
        _force: bool,
    ) -> Result<(), openpanel_domain::docker::DockerError> {
        self.states.lock().expect("docker states").remove(id);
        Ok(())
    }

    async fn inspect(
        &self,
        id: &str,
    ) -> Result<openpanel_app::RuntimeContainerState, openpanel_domain::docker::DockerError> {
        self.states
            .lock()
            .expect("docker states")
            .get(id)
            .cloned()
            .ok_or_else(|| openpanel_domain::docker::DockerError::NotFound(id.into()))
    }

    async fn logs(
        &self,
        _: &str,
        _: u64,
    ) -> Result<Vec<String>, openpanel_domain::docker::DockerError> {
        Ok(vec!["container ready".into()])
    }

    async fn exec(
        &self,
        _: &str,
        command: &[String],
        _user: &str,
    ) -> Result<ExecResult, openpanel_domain::docker::DockerError> {
        Ok(ExecResult {
            exit_code: 0,
            output: command.join(" "),
            truncated: false,
        })
    }

    async fn apply_stack(
        &self,
        _stack: &openpanel_domain::docker::ComposeStack,
    ) -> Result<ApplyReport, openpanel_domain::docker::DockerError> {
        Ok(ApplyReport {
            created: vec!["memory-stack".into()],
            removed: Vec::new(),
        })
    }

    async fn remove_stack(
        &self,
        _stack: &openpanel_domain::docker::ComposeStack,
    ) -> Result<Vec<String>, openpanel_domain::docker::DockerError> {
        Ok(Vec::new())
    }
}

#[async_trait::async_trait]
impl openpanel_app::NetworkAdapter for MemoryDocker {
    async fn ensure(
        &self,
        _site_id: Option<Uuid>,
    ) -> Result<(), openpanel_domain::docker::DockerError> {
        Ok(())
    }

    async fn remove_if_unused(
        &self,
        _site_id: Option<Uuid>,
    ) -> Result<(), openpanel_domain::docker::DockerError> {
        Ok(())
    }
}

/// In-process artifact fetcher used by the test server. Bytes are
/// staged per URL via [`TestServer::stage_artifact`]. Anything not
/// staged is rejected with `SoftwareCenterError::Package`, so a test
/// that forgets to stage the bytes fails loudly instead of hanging on a
/// real network call.
struct StagedArtifactFetcher {
    bytes: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    served: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl ArtifactFetcher for StagedArtifactFetcher {
    async fn fetch(
        &self,
        url: &str,
    ) -> Result<Vec<u8>, openpanel_app::software_center::SoftwareCenterError> {
        self.served
            .lock()
            .expect("served lock")
            .push(url.to_owned());
        self.bytes
            .lock()
            .expect("bytes lock")
            .get(url)
            .cloned()
            .ok_or(
                openpanel_app::software_center::SoftwareCenterError::Package(
                    "operation failed".into(),
                ),
            )
    }
}
use openpanel_core::{
    AppContext, AuditEvent, AuditService, Config, MigrationRunner, Module, SqliteAuditService,
    SqliteDriver,
};
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle};
use uuid::Uuid;

use super::db::TestDb;

/// Per-test HTTP server that boots the real axum router on a random local
/// port. Created by `TestServer::new()` and torn down on drop.
pub struct TestServer {
    addr: String,
    client: reqwest::Client,
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
    system_services: Arc<openpanel_app::ServiceManager>,
    dns: Arc<DnsService>,
    mail: Arc<MailService>,
    software_center: Arc<SoftwareCenterService>,
    waf: Arc<WafService>,
    docker: Arc<DockerService>,
    container_runtime: Arc<ContainerRuntimeService>,
    ftp: Arc<FtpService>,
    api_tokens: Arc<ApiTokenService>,
    notifications: Arc<NotificationService>,
    audit: Arc<dyn AuditService>,
    settings_path: PathBuf,
    _handle: JoinHandle<()>,
    _db: TestDb,
    /// Whether the Software Center refuses placeholder SHA-256
    /// digests by default. Tests that exercise the lenient path set
    /// this to `false`; tests that exercise the gate set it to `true`.
    #[allow(dead_code)]
    require_verified_digests: bool,
    /// Temp directory for sandboxed nginx configs and document roots.
    sandbox: Arc<TempDir>,
    /// Sandbox directory downloaded artifacts are placed at.
    webapps_root: PathBuf,
    /// Sandbox directory the curated config manifest resolves against.
    config_root: PathBuf,
    /// Pre-canned bytes keyed by URL for the in-process artifact fetcher.
    staged_artifacts: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    /// URLs the in-process fetcher served during the test.
    fetched_urls: Arc<Mutex<Vec<String>>>,
    /// Database point-in-time recovery service.
    pitr: Arc<PitrService>,
    /// Per-site staging service.
    staging: Arc<StagingService>,
    /// Plugin extension framework service.
    plugins: Arc<PluginService>,
    /// Plugin marketplace service.
    #[allow(dead_code)] // reserved for future test-server accessors
    marketplace: Arc<MarketplaceService>,
    /// Collaborator service.
    #[allow(dead_code)] // reserved for future test-server accessors
    collaborators: Arc<CollaboratorService>,
    /// Grant resolver.
    #[allow(dead_code)] // reserved for future test-server accessors
    grant_resolver: Arc<GrantResolver>,
    /// Container registry service.
    #[allow(dead_code)] // reserved for future test-server accessors
    registry: Arc<openpanel_app::ContainerRegistryService>,
    /// Hosting plans service.
    #[allow(dead_code)] // reserved for future test-server accessors
    hosting_plans: Arc<HostingPlansService>,
    #[allow(dead_code)] // reserved for future test-server accessors
    account_hierarchy: Arc<openpanel_app::HierarchyService>,
    migration_importers: Arc<openpanel_app::MigrationService>,
    offsite_backup_targets: Arc<openpanel_app::BackupUploadService>,
    site_cache_cdn: Arc<openpanel_app::SiteCacheService>,
    site_clone_template: Arc<openpanel_app::SiteCloneService>,
    themeable_ui: Arc<openpanel_app::ThemeableUiService>,
    web_application_installer: Arc<openpanel_app::WebApplicationInstallerService>,
    malware_scanner: Arc<openpanel_app::MalwareScannerService>,
    webmail: Arc<openpanel_app::WebmailService>,
}

impl TestServer {
    /// Stage a pre-baked byte stream that the in-process artifact
    /// fetcher will return when the Software Center requests `url`.
    /// Tests use this instead of mocking the network: one staging call
    /// per artifact the install flow needs.
    pub fn stage_artifact(&self, url: &str, bytes: Vec<u8>) {
        self.staged_artifacts
            .lock()
            .expect("staged artifacts lock")
            .insert(url.to_owned(), bytes);
    }

    /// URLs the artifact fetcher served during the test, in call order.
    pub fn fetched_artifacts(&self) -> Vec<String> {
        self.fetched_urls.lock().expect("fetched lock").clone()
    }

    /// Sandbox directory the Software Center drops downloaded artifacts
    /// into. The default is `<sandbox>/webapps`.
    pub fn webapps_root(&self) -> &std::path::Path {
        &self.webapps_root
    }

    /// Sandbox directory the curated config manifest resolves against.
    /// Config files land under `<sandbox>/config/etc/...` so the config
    /// editor never touches the real host configuration.
    pub fn config_root(&self) -> &std::path::Path {
        &self.config_root
    }
}

impl TestServer {
    /// Boot a real axum server backed by a fresh `TestDb` with the
    /// placeholder-SHA-256 gate off (lenient mode). The existing
    /// adminer / phpmyadmin install tests rely on the lenient mode
    /// because the recovery seed still ships placeholder digests.
    pub async fn new() -> Self {
        Self::new_with_gate(false).await
    }

    /// Boot a real axum server with an explicit placeholder-SHA-256
    /// gate. The strict gate is the production default; the lenient
    /// gate is what the existing install tests use.
    pub async fn new_with_gate(require_verified_digests: bool) -> Self {
        Self::new_with_gate_config_and_crypto(require_verified_digests, Config::default(), None)
            .await
    }

    /// Boot a real server with an explicit validated configuration.
    pub async fn new_with_config(config: Config) -> Self {
        Self::new_with_gate_config_and_crypto(false, config, None).await
    }

    /// Boot with deterministic two-factor cryptography.
    pub async fn new_with_two_factor_crypto(crypto: Arc<dyn TwoFactorCrypto>) -> Self {
        Self::new_with_gate_config_and_crypto(false, Config::default(), Some(crypto)).await
    }

    /// Boot with a deterministic per-token bucket configuration.
    pub async fn new_with_api_token_rate(burst: u32, per_minute: u32) -> Self {
        let mut config = Config::default();
        config.modules.insert(
            "api-tokens".into(),
            serde_json::json!({"burst": burst, "per_minute": per_minute}),
        );
        Self::new_with_gate_config_and_crypto(false, config, None).await
    }

    async fn new_with_gate_config_and_crypto(
        require_verified_digests: bool,
        config: Config,
        two_factor_crypto: Option<Arc<dyn TwoFactorCrypto>>,
    ) -> Self {
        let db = TestDb::new().await;
        let pool = db.pool();

        let driver = SqliteDriver::new(db.url());
        // Connect the driver so pool() works
        let _ = driver.connect().await;
        let driver: Arc<dyn openpanel_core::DatabaseDriver> = Arc::new(driver);

        let audit: Arc<dyn AuditService> = Arc::new(SqliteAuditService::new(pool.clone()));
        let config = Arc::new(config);

        let ctx = AppContext::new(config.clone(), driver, audit.clone());

        // Run audit schema
        sqlx::query(include_str!("migrations/000_audit.sql"))
            .execute(&pool)
            .await
            .ok();

        // Sandbox for nginx configs and document roots.
        let sandbox = Arc::new(
            tempfile::Builder::new()
                .prefix("openpanel-test-")
                .tempdir()
                .expect("tempdir"),
        );
        let nginx_root = sandbox.path().to_path_buf();
        let paths = NginxPaths::under(nginx_root.clone());

        // Master key for databases + identity modules — fixed to zeros for tests.
        let master_key = [0u8; 32];
        let identity_module = if let Some(crypto) = two_factor_crypto {
            IdentityModule::new_with_two_factor_crypto(&ctx, master_key, crypto).await
        } else {
            IdentityModule::new(&ctx, master_key).await
        };
        let api_token_module = ApiTokenModule::new(&ctx, master_key).await;
        let notification_module = NotificationModule::new(&ctx, master_key)
            .await
            .expect("notification module");
        let sites_module = SitesModule::with_paths(&ctx, paths).await;
        let waf_module = WafModule::new(&ctx, sites_module.generator().clone()).await;
        let docker_runtime = Arc::new(MemoryDocker::default());
        let docker_module =
            DockerModule::with_adapters(&ctx, docker_runtime.clone(), docker_runtime, master_key)
                .await;
        let container_runtime_module = ContainerRuntimeModule::new(
            &ctx,
            audit.clone(),
            master_key,
            openpanel_domain::PlanQuotaCaps::default(),
            None,
        )
        .await;
        let hosting_plans_module = HostingPlansModule::new(&ctx).await;
        let account_hierarchy_module = AccountHierarchyModule::new(&ctx).await;
        let ftp_module = FtpModule::new(&ctx).await.expect("ftp module");

        let databases_module = DatabasesModule::new(&ctx, master_key).await;
        let files_module = FilesModule::new(&ctx).await;

        // SSL module: shares the same master key as the databases module
        // (the encryption envelope is identical). Sandbox the cert /
        // key writes under the same temp dir as nginx configs.
        let ssl_paths = SslPaths::under(nginx_root.join("ssl"));
        let ssl_module = SslModule::with_paths(
            &ctx,
            ssl_paths,
            openpanel_app::AcmeEndpoint::default_safe(),
            master_key,
            "[email protected]",
        )
        .await;

        let runner = MigrationRunner::for_sqlite(pool.clone());
        runner
            .apply_module(identity_module.name(), &identity_module.migrations())
            .await
            .expect("identity migrations");
        runner
            .apply_module(api_token_module.name(), &api_token_module.migrations())
            .await
            .expect("API-token migrations");
        runner
            .apply_module(
                notification_module.name(),
                &notification_module.migrations(),
            )
            .await
            .expect("notification migrations");
        runner
            .apply_module(sites_module.name(), &sites_module.migrations())
            .await
            .expect("sites migrations");
        runner
            .apply_module(waf_module.name(), &waf_module.migrations())
            .await
            .expect("waf migrations");
        runner
            .apply_module(docker_module.name(), &docker_module.migrations())
            .await
            .expect("docker migrations");
        runner
            .apply_module(
                container_runtime_module.name(),
                &container_runtime_module.migrations(),
            )
            .await
            .expect("container-runtime migrations");
        runner
            .apply_module(
                hosting_plans_module.name(),
                &hosting_plans_module.migrations(),
            )
            .await
            .expect("hosting-plans migrations");
        runner
            .apply_module(
                account_hierarchy_module.name(),
                &account_hierarchy_module.migrations(),
            )
            .await
            .expect("account-hierarchy migrations");
        let migration_importers_module = MigrationImportersModule::new(&ctx).await;
        runner
            .apply_module(
                migration_importers_module.name(),
                &migration_importers_module.migrations(),
            )
            .await
            .expect("migration-importers migrations");
        let offsite_backup_module = OffsiteBackupTargetsModule::new(&ctx).await;
        runner
            .apply_module(
                offsite_backup_module.name(),
                &offsite_backup_module.migrations(),
            )
            .await
            .expect("offsite-backup-targets migrations");
        let site_cache_cdn_module = SiteCacheCdnModule::new(&ctx).await;
        runner
            .apply_module(
                site_cache_cdn_module.name(),
                &site_cache_cdn_module.migrations(),
            )
            .await
            .expect("site-cache-cdn migrations");
        let site_clone_template_module = SiteCloneTemplateModule::new(&ctx).await;
        runner
            .apply_module(
                site_clone_template_module.name(),
                &site_clone_template_module.migrations(),
            )
            .await
            .expect("site-clone-template migrations");
        runner
            .apply_module(ftp_module.name(), &ftp_module.migrations())
            .await
            .expect("ftp migrations");
        runner
            .apply_module(databases_module.name(), &databases_module.migrations())
            .await
            .expect("databases migrations");
        runner
            .apply_module(ssl_module.name(), &ssl_module.migrations())
            .await
            .expect("ssl migrations");

        let monitoring_module = MonitoringModule::new(&ctx).await;
        monitoring_module
            .service()
            .attach_notifications(notification_module.service());
        let cron_module = CronModule::with_roots(&ctx, vec![sandbox.path().to_path_buf()]).await;
        let backups_module = BackupsModule::with_root(
            &ctx,
            sandbox.path().join("backups"),
            Some(master_key),
            Some((cron_module.service(), sandbox.path().to_path_buf())),
        )
        .await;
        let logs_module = LogsModule::with_root(&ctx, sandbox.path().join("logs")).await;
        let security_module =
            SecurityModule::with_firewall(&ctx, Arc::new(MemoryFirewall::default()))
                .await
                .expect("security module");
        let system_services_module = SystemServicesModule::memory(&ctx)
            .await
            .expect("system services module");
        let dns_module = DnsModule::memory(&ctx).await.expect("dns module");
        let mail_module = MailModule::memory(&ctx).await.expect("mail module");
        let webapps_root = sandbox.path().join("webapps");
        let config_root = sandbox.path().join("config");
        let staged_artifacts: Arc<Mutex<HashMap<String, Vec<u8>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let fetched_urls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let fetcher: Arc<dyn ArtifactFetcher> = Arc::new(StagedArtifactFetcher {
            bytes: staged_artifacts.clone(),
            served: fetched_urls.clone(),
        });
        let software_center_module = SoftwareCenterModule::memory_with_artifact_and_gate(
            &ctx,
            fetcher,
            webapps_root.clone(),
            config_root.clone(),
            require_verified_digests,
        )
        .await
        .expect("software center module");
        runner
            .apply_module(monitoring_module.name(), &monitoring_module.migrations())
            .await
            .expect("monitoring migrations");
        runner
            .apply_module(cron_module.name(), &cron_module.migrations())
            .await
            .expect("cron migrations");
        runner
            .apply_module(backups_module.name(), &backups_module.migrations())
            .await
            .expect("backup migrations");
        runner
            .apply_module(logs_module.name(), &logs_module.migrations())
            .await
            .expect("logs migrations");
        runner
            .apply_module(security_module.name(), &security_module.migrations())
            .await
            .expect("security migrations");
        runner
            .apply_module(
                system_services_module.name(),
                &system_services_module.migrations(),
            )
            .await
            .expect("system services migrations");
        runner
            .apply_module(dns_module.name(), &dns_module.migrations())
            .await
            .expect("dns migrations");
        runner
            .apply_module(mail_module.name(), &mail_module.migrations())
            .await
            .expect("mail migrations");
        runner
            .apply_module(
                software_center_module.name(),
                &software_center_module.migrations(),
            )
            .await
            .expect("software center migrations");

        let identity_svc = identity_module.service();
        let api_token_svc = api_token_module.service();
        let notification_svc = notification_module.service();
        let sites_svc = sites_module.service();
        let waf_svc = waf_module.service();
        let docker_svc = docker_module.service();
        let container_runtime_svc = container_runtime_module.service();
        let hosting_plans_svc = hosting_plans_module.service();
        let account_hierarchy_svc = account_hierarchy_module.service();
        let migration_importers_svc = migration_importers_module.service();
        let _ = migration_importers_svc;
        let offsite_backup_svc = offsite_backup_module.service();
        let _ = offsite_backup_svc;
        let site_cache_cdn_svc = site_cache_cdn_module.service();
        let _ = site_cache_cdn_svc;
        let site_clone_template_svc = Arc::new(SiteCloneService::new(
            site_clone_template_module.repo(),
            sites_svc.clone(),
            audit.clone(),
            master_key,
            None,
            None,
        ));
        let themeable_ui_repo =
            openpanel_app::themeable_ui::SqliteThemeableUiRepository::new(pool.clone());
        // Apply the themeable_ui migration so the integration
        // tests can upsert and read theme overrides. The
        // migration SQL is exposed via the `openpanel_app` crate.
        sqlx::query(openpanel_app::migrations::THEMEABLE_UI_V001)
            .execute(&pool)
            .await
            .expect("themeable_ui migrations");
        let themeable_ui_svc = Arc::new(ThemeableUiService::new(
            Arc::new(themeable_ui_repo),
            audit.clone(),
            None,
        ));
        // Apply the web-app-installer migration and build the
        // service over the same pool + audit.
        sqlx::query(openpanel_app::migrations::WEB_APPLICATION_INSTALLER_V001)
            .execute(&pool)
            .await
            .expect("web_application_installer migrations");
        let web_application_installer_svc = Arc::new(WebApplicationInstallerService::new(
            Arc::new(
                openpanel_app::web_application_installer::SqliteWebApplicationInstallerRepository::new(
                    pool.clone(),
                ),
            ),
            audit.clone(),
            Arc::new(RealInstallerFs),
            Arc::new(ReqwestArtifactDownloader::new()),
            master_key,
            Some(sandbox.path().join("webapp-archives")),
        ));
        // Apply the malware-scanner migration and build the service.
        sqlx::query(openpanel_app::migrations::MALWARE_SCANNER_V001)
            .execute(&pool)
            .await
            .expect("malware_scanner migrations");
        let malware_scanner_svc = Arc::new(MalwareScannerService::new(
            Arc::new(
                openpanel_app::malware_scanner::SqliteMalwareScannerRepository::new(pool.clone()),
            ),
            audit.clone(),
            Arc::new(RealScannerFs),
            Some(sandbox.path().join("quarantine")),
        ));
        // Apply the webmail migration and build the service.
        sqlx::query(openpanel_app::migrations::WEBMAIL_CLIENT_V001)
            .execute(&pool)
            .await
            .expect("webmail migrations");
        let webmail_svc = Arc::new(WebmailService::new(
            Arc::new(openpanel_app::webmail_client::SqliteWebmailRepository::new(
                pool.clone(),
            )),
            Arc::new(InMemoryMailBridge::default()),
            audit.clone(),
            master_key,
        ));
        let site_clone_template_repo = site_clone_template_module.repo();
        docker_svc.attach_quota_gate(container_runtime_svc.clone());
        let ftp_svc = ftp_module.service();
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

        // Site-staging module: in-memory filesystem layer for tests.
        let staging_fs: Arc<dyn openpanel_app::StagingFilesystemLayer> =
            Arc::new(InMemoryStagingFilesystem::new());
        let staging_sites_repo: Arc<dyn openpanel_domain::SiteRepository> =
            Arc::new(SqliteSiteRepository::new(pool.clone()));
        let staging_module =
            SiteStagingModule::new(&ctx, staging_sites_repo, staging_fs, audit.clone()).await;
        runner
            .apply_module(staging_module.name(), &staging_module.migrations())
            .await
            .expect("staging migrations");
        let staging_svc = staging_module.service();

        // PITR module: in-memory sink, no engine tailer wired in tests.
        // The PITR service takes a `DatabaseLookup`; we pass the
        // shared SQLite `SqliteDatabaseRepository` (which implements
        // the slim lookup trait) so each test gets a real database
        // row out of the seeded pool.
        let pitr_db_lookup: Arc<dyn openpanel_domain::DatabaseLookup> =
            Arc::new(openpanel_app::databases::repo::SqliteDatabaseRepository::new(pool.clone()));
        let pitr_sink: Arc<dyn openpanel_app::BinlogSink> =
            Arc::new(openpanel_app::InMemoryBinlogSink::new());
        let pitr_module =
            DbPitrModule::new(&ctx, pitr_sink, Vec::new(), pitr_db_lookup, audit.clone()).await;
        runner
            .apply_module(pitr_module.name(), &pitr_module.migrations())
            .await
            .expect("pitr migrations");
        let pitr_svc = pitr_module.service();

        // Plugin extension framework module.
        let plugin_module = openpanel_app::PluginModule::new(&ctx, audit.clone()).await;
        runner
            .apply_module(plugin_module.name(), &plugin_module.migrations())
            .await
            .expect("plugin migrations");
        let plugin_svc = plugin_module.service();

        // Plugin marketplace module: in-process mock client + empty
        // CA so the routes are reachable but the install path must
        // verify signatures before delegating.
        let mp_client: Arc<dyn openpanel_app::MarketplaceClient> =
            Arc::new(openpanel_app::MockMarketplaceClient::new());
        let mp_module =
            openpanel_app::PluginMarketplaceModule::new(&ctx, mp_client, plugin_svc.clone()).await;
        runner
            .apply_module(mp_module.name(), &mp_module.migrations())
            .await
            .expect("plugin marketplace migrations");
        let mp_svc = mp_module.service();

        // Per-site collaborators module.
        let collaborators_module =
            openpanel_app::CollaboratorsModule::new(&ctx, audit.clone()).await;
        runner
            .apply_module(
                collaborators_module.name(),
                &collaborators_module.migrations(),
            )
            .await
            .expect("collaborators migrations");
        let collaborators_svc = collaborators_module.service();
        let grant_resolver = collaborators_module.resolver();

        // Container registry module: in-memory storage + no-op scan hook
        // so tests can exercise push / quota / retention without
        // touching the network.
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
            .expect("registry migrations");
        let registry_svc = registry_module.service();

        let settings_path = sandbox.path().join("web-preferences.json");
        let two_factor_svc = identity_module.two_factor();
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
            software_center_svc.clone(),
            two_factor_svc.clone(),
            waf_svc.clone(),
            docker_svc.clone(),
            ftp_svc.clone(),
            api_token_svc.clone(),
            notification_svc.clone(),
            pitr_svc.clone(),
            staging_svc.clone(),
            plugin_svc.clone(),
            mp_svc.clone(),
            collaborators_svc.clone(),
            grant_resolver.clone(),
            registry_svc.clone(),
            container_runtime_svc.clone(),
            hosting_plans_svc.clone(),
            account_hierarchy_svc.clone(),
            site_cache_cdn_svc.clone(),
            // Site clone + template export: real service constructed
            // from the same sites repository, audit, and master key
            // that the rest of the test server uses.
            site_clone_template_svc.clone(),
            site_clone_template_repo.clone(),
            // Themeable UI: real service over the same SQLite
            // pool and audit sink.
            themeable_ui_svc.clone(),
            // Web application installer: real service over the
            // same pool + audit; the in-memory install fs keeps
            // tests hermetic.
            web_application_installer_svc.clone(),
            // Malware scanner: real service over the same pool +
            // audit; the scan fs writes under the sandbox.
            malware_scanner_svc.clone(),
        )
        .merge(openpanel_web::router(
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
            login_throttle,
            system_services_svc.clone(),
            dns_svc.clone(),
            mail_svc.clone(),
            software_center_svc.clone(),
            two_factor_svc.clone(),
            waf_svc.clone(),
            docker_svc.clone(),
            ftp_svc.clone(),
            api_token_svc.clone(),
            notification_svc.clone(),
            pitr_svc.clone(),
            staging_svc.clone(),
            collaborators_svc.clone(),
            registry_svc.clone(),
            container_runtime_svc.clone(),
            themeable_ui_svc.clone(),
            webmail_svc.clone(),
            openpanel_web::WebRuntime::new(
                config,
                audit.clone(),
                settings_path.clone(),
                sandbox.path().to_string_lossy().into_owned(),
                sandbox
                    .path()
                    .join("openpanel.toml")
                    .to_string_lossy()
                    .into_owned(),
            )
            .with_capabilities(
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
                    .with("ftp"),
            ),
        ));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind random port");
        let addr = listener.local_addr().expect("local addr").to_string();

        let handle = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .expect("server error");
        });

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client");

        Self {
            addr,
            client,
            identity: identity_svc,
            sites: sites_svc,
            databases: databases_svc,
            files: files_svc,
            ssl: ssl_svc,
            monitoring: monitoring_svc,
            cron: cron_svc,
            backups: backups_svc,
            logs: logs_svc,
            security: security_svc,
            system_services: system_services_svc,
            dns: dns_svc,
            mail: mail_svc,
            software_center: software_center_svc,
            waf: waf_svc,
            docker: docker_svc,
            container_runtime: container_runtime_svc,
            hosting_plans: hosting_plans_svc,
            account_hierarchy: account_hierarchy_svc,
            migration_importers: migration_importers_svc,
            offsite_backup_targets: offsite_backup_svc,
            site_cache_cdn: site_cache_cdn_svc,
            site_clone_template: site_clone_template_svc,
            themeable_ui: themeable_ui_svc,
            web_application_installer: web_application_installer_svc,
            malware_scanner: malware_scanner_svc,
            webmail: webmail_svc,
            ftp: ftp_svc,
            api_tokens: api_token_svc,
            notifications: notification_svc,
            audit,
            settings_path,
            _handle: handle,
            _db: db,
            sandbox,
            require_verified_digests,
            webapps_root,
            config_root,
            staged_artifacts,
            fetched_urls,
            pitr: pitr_svc,
            staging: staging_svc,
            plugins: plugin_svc,
            marketplace: mp_svc,
            collaborators: collaborators_svc,
            grant_resolver,
            registry: registry_svc,
        }
    }

    /// The base URL, e.g. `http://127.0.0.1:54321`.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// A preconfigured `reqwest::Client`.
    pub fn client(&self) -> reqwest::Client {
        self.client.clone()
    }

    /// The identity service handle (for bootstrapping users).
    pub fn identity(&self) -> Arc<IdentityService> {
        self.identity.clone()
    }

    /// API-token lifecycle service handle.
    pub fn api_tokens(&self) -> Arc<ApiTokenService> {
        self.api_tokens.clone()
    }

    /// Notification lifecycle and dispatch service handle.
    pub fn notifications(&self) -> Arc<NotificationService> {
        self.notifications.clone()
    }

    /// The sites service handle.
    pub fn sites(&self) -> Arc<SitesService> {
        self.sites.clone()
    }

    /// The databases service handle.
    pub fn databases(&self) -> Arc<DatabasesService> {
        self.databases.clone()
    }

    /// The point-in-time recovery service handle.
    pub fn pitr(&self) -> Arc<openpanel_app::PitrService> {
        self.pitr.clone()
    }

    /// The per-site staging service handle.
    pub fn staging(&self) -> Arc<openpanel_app::StagingService> {
        self.staging.clone()
    }

    /// The shared SQLite pool, exposed so integration tests can
    /// seed rows directly when the public APIs require a real
    /// database (e.g. to satisfy a foreign key).
    pub fn pool(&self) -> sqlx::Pool<sqlx::Sqlite> {
        self._db.pool()
    }

    /// The provider-backed DNS service handle.
    pub fn dns(&self) -> Arc<DnsService> {
        self.dns.clone()
    }

    /// The hosted-mail administration service.
    pub fn mail(&self) -> Arc<MailService> {
        self.mail.clone()
    }

    /// The curated Software Center service.
    pub fn software_center(&self) -> Arc<SoftwareCenterService> {
        self.software_center.clone()
    }

    /// The migration importers service.
    pub fn migration_importers(&self) -> Arc<openpanel_app::MigrationService> {
        self.migration_importers.clone()
    }

    /// The offsite backup targets service.
    pub fn offsite_backup_targets(&self) -> Arc<openpanel_app::BackupUploadService> {
        self.offsite_backup_targets.clone()
    }

    /// The site cache and CDN service.
    pub fn site_cache_cdn(&self) -> Arc<openpanel_app::SiteCacheService> {
        self.site_cache_cdn.clone()
    }

    /// The site clone + template export service.
    pub fn site_clone_template(&self) -> Arc<openpanel_app::SiteCloneService> {
        self.site_clone_template.clone()
    }

    /// The themeable UI / white-label service.
    pub fn themeable_ui(&self) -> Arc<openpanel_app::ThemeableUiService> {
        self.themeable_ui.clone()
    }

    /// The web application installer service.
    pub fn web_application_installer(&self) -> Arc<openpanel_app::WebApplicationInstallerService> {
        self.web_application_installer.clone()
    }

    /// The malware scanner service.
    pub fn malware_scanner(&self) -> Arc<openpanel_app::MalwareScannerService> {
        self.malware_scanner.clone()
    }

    /// The webmail client service.
    pub fn webmail(&self) -> Arc<openpanel_app::WebmailService> {
        self.webmail.clone()
    }

    /// The per-site WAF service.
    pub fn waf(&self) -> Arc<WafService> {
        self.waf.clone()
    }

    /// The plugin extension framework service.
    pub fn plugins(&self) -> Arc<PluginService> {
        self.plugins.clone()
    }

    /// Docker service handle for staging allowlist and runtime observations.
    pub fn docker(&self) -> Arc<DockerService> {
        self.docker.clone()
    }

    /// Container runtime service handle.
    pub fn container_runtime(&self) -> Arc<ContainerRuntimeService> {
        self.container_runtime.clone()
    }

    /// FTP account service handle.
    pub fn ftp(&self) -> Arc<FtpService> {
        self.ftp.clone()
    }

    /// Underlying isolated SQLite pool for persistence assertions.
    pub fn database_pool(&self) -> sqlx::SqlitePool {
        self._db.pool()
    }

    /// The files service handle.
    pub fn files(&self) -> Arc<FilesService> {
        self.files.clone()
    }

    /// The SSL service handle.
    pub fn ssl(&self) -> Arc<SslService> {
        self.ssl.clone()
    }

    /// The monitoring service handle.
    pub fn monitoring(&self) -> Arc<MonitoringService> {
        self.monitoring.clone()
    }

    /// The cron scheduling service handle.
    pub fn cron(&self) -> Arc<CronService> {
        self.cron.clone()
    }

    /// The backup and restore service handle.
    pub fn backups(&self) -> Arc<BackupService> {
        self.backups.clone()
    }

    /// The authorized log browsing service handle.
    pub fn logs(&self) -> Arc<LogService> {
        self.logs.clone()
    }

    /// The host-security service handle.
    pub fn security(&self) -> Arc<SecurityService> {
        self.security.clone()
    }

    /// The allowlisted system-service manager.
    pub fn system_services(&self) -> Arc<openpanel_app::ServiceManager> {
        self.system_services.clone()
    }

    /// Path used by the web preference store.
    pub fn settings_path(&self) -> PathBuf {
        self.settings_path.clone()
    }

    /// Return recent audit events, newest first.
    pub async fn audit_events(&self) -> Vec<AuditEvent> {
        self.audit.recent(100).await.expect("recent audit events")
    }

    /// Resolve a sandboxed absolute path under this server's temp directory.
    /// Tests should use this for `document_root` instead of `/var/www/...`
    /// or `/tmp/openpanel-test/...` directly, so cleanup is automatic.
    pub fn sandbox_path(&self, sub: &str) -> String {
        self.sandbox.path().join(sub).to_string_lossy().into_owned()
    }

    /// Boot a server and create an owner user, returning its login token.
    pub async fn bootstrap_owner(&self, username: &str, password: &str) -> String {
        self.identity()
            .create_user(
                username,
                "owner@example.com",
                password,
                openpanel_domain::Role::Owner,
                "test",
            )
            .await
            .expect("create owner");
        self.login(username, password).await
    }

    /// Log in and return the session token.
    pub async fn login(&self, username: &str, password: &str) -> String {
        let resp = self
            .client()
            .post(format!("{}/api/v1/identity/login", self.base_url()))
            .json(&serde_json::json!({
                "username_or_email": username,
                "password": password,
            }))
            .send()
            .await
            .expect("login request");
        assert_eq!(resp.status(), 200, "login should succeed");
        let body: serde_json::Value = resp.json().await.expect("login body");
        body["token"].as_str().expect("token").to_string()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self._handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn testserver_boots_and_health() {
        let server = TestServer::new().await;
        let resp = server
            .client()
            .get(format!("{}/health", server.base_url()))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "ok");

        // A second server works concurrently
        let server2 = TestServer::new().await;
        let resp2 = server2
            .client()
            .get(format!("{}/health", server2.base_url()))
            .send()
            .await
            .unwrap();
        assert_eq!(resp2.status(), 200);
    }

    #[tokio::test]
    async fn notification_rest_metadata_and_browser_test_send_enforce_csrf() {
        let server = TestServer::new().await;
        let token = server
            .bootstrap_owner("notify-owner", "correct horse battery staple")
            .await;
        let created = server
            .client()
            .post(format!(
                "{}/api/v1/notifications/channels",
                server.base_url()
            ))
            .bearer_auth(&token)
            .json(&serde_json::json!({
                "kind":"smtp",
                "name":"test relay",
                "host":"smtp.example.test",
                "port":587,
                "username":"mailer",
                "password":"must-never-be-returned",
                "from_addr":"sender@example.test",
                "tls_mode":"start_tls",
                "allowlist":["ops@example.test"]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), 201);
        let channel: serde_json::Value = created.json().await.unwrap();
        assert!(!channel.to_string().contains("must-never-be-returned"));
        let id = channel["id"].as_str().unwrap();

        let page = server
            .client()
            .get(format!("{}/settings/notifications", server.base_url()))
            .header("cookie", format!("openpanel_session={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), 200);
        assert!(page.text().await.unwrap().contains("Notifications"));

        let rejected = server
            .client()
            .post(format!(
                "{}/settings/notifications/channels/{id}/test",
                server.base_url()
            ))
            .header("cookie", format!("openpanel_session={token}"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body("destination=ops%40example.test")
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status(), 403);
    }
}
