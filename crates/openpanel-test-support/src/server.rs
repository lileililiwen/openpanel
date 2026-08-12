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
    BackupService, BackupsModule, CronModule, CronService, DatabasesModule, DatabasesService,
    DnsModule, DnsService, FilesModule, FilesService, IdentityModule, IdentityService, LogService,
    LogsModule, MailModule, MailService, MonitoringModule, MonitoringService, SecurityModule,
    SecurityService, SitesModule, SitesService, SoftwareCenterModule, SoftwareCenterService,
    SslModule, SslPaths, SslService, SystemServicesModule, security::MemoryFirewall,
    sites::nginx::NginxPaths, software_center::ArtifactFetcher,
};

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
        let db = TestDb::new().await;
        let pool = db.pool();

        let driver = SqliteDriver::new(db.url());
        // Connect the driver so pool() works
        let _ = driver.connect().await;
        let driver: Arc<dyn openpanel_core::DatabaseDriver> = Arc::new(driver);

        let audit: Arc<dyn AuditService> = Arc::new(SqliteAuditService::new(pool.clone()));
        let config = Arc::new(Config::default());

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
        let identity_module = IdentityModule::new(&ctx, master_key).await;
        let sites_module = SitesModule::with_paths(&ctx, paths).await;

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
            .apply_module(sites_module.name(), &sites_module.migrations())
            .await
            .expect("sites migrations");
        runner
            .apply_module(databases_module.name(), &databases_module.migrations())
            .await
            .expect("databases migrations");
        runner
            .apply_module(ssl_module.name(), &ssl_module.migrations())
            .await
            .expect("ssl migrations");

        let monitoring_module = MonitoringModule::new(&ctx).await;
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

        let settings_path = sandbox.path().join("web-preferences.json");
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
                    .with("software-center"),
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

    /// The sites service handle.
    pub fn sites(&self) -> Arc<SitesService> {
        self.sites.clone()
    }

    /// The databases service handle.
    pub fn databases(&self) -> Arc<DatabasesService> {
        self.databases.clone()
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
}
