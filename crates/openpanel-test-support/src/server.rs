//! Per-test HTTP server that boots the real axum router.
//!
//! `TestServer::new()` creates a `TestDb`, boots the real axum router
//! from `openpanel_api::build_router` on a random local port, and
//! returns the bound address plus a preconfigured `reqwest::Client`.
//! `Drop` aborts the server task.

use std::sync::Arc;

use openpanel_api::build_router;
use openpanel_app::{
    DatabasesModule, DatabasesService, FilesModule, FilesService, IdentityModule, IdentityService,
    MonitoringModule, MonitoringService, SitesModule, SitesService, SslModule, SslPaths,
    SslService, sites::nginx::NginxPaths,
};
use openpanel_core::{AppContext, Config, MigrationRunner, Module, NoopAuditService, SqliteDriver};
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
    _handle: JoinHandle<()>,
    _db: TestDb,
    /// Temp directory for sandboxed nginx configs and document roots.
    sandbox: Arc<TempDir>,
}

impl TestServer {
    /// Boot a real axum server backed by a fresh `TestDb`.
    pub async fn new() -> Self {
        let db = TestDb::new().await;
        let pool = db.pool();

        let driver = SqliteDriver::new(db.url());
        // Connect the driver so pool() works
        let _ = driver.connect().await;
        let driver: Arc<dyn openpanel_core::DatabaseDriver> = Arc::new(driver);

        let audit: Arc<dyn openpanel_core::AuditService> = Arc::new(NoopAuditService);
        let config = Arc::new(Config::default());

        let ctx = AppContext::new(config, driver, audit);

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

        let identity_module = IdentityModule::new(&ctx).await;
        let sites_module = SitesModule::with_paths(&ctx, paths).await;

        // Master key for databases module — generate a random one for tests.
        let master_key = [0u8; 32];
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
        runner
            .apply_module(monitoring_module.name(), &monitoring_module.migrations())
            .await
            .expect("monitoring migrations");

        let identity_svc = identity_module.service();
        let sites_svc = sites_module.service();
        let databases_svc = databases_module.service();
        let files_svc = files_module.service();
        let ssl_svc = ssl_module.service();
        let monitoring_svc = monitoring_module.service();

        let app = build_router(
            identity_svc.clone(),
            sites_svc.clone(),
            databases_svc.clone(),
            files_svc.clone(),
            ssl_svc.clone(),
            monitoring_svc.clone(),
        )
        .merge(openpanel_web::router(identity_svc.clone()));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind random port");
        let addr = listener.local_addr().expect("local addr").to_string();

        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server error");
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
            _handle: handle,
            _db: db,
            sandbox,
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
