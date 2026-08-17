//! Web application installer bounded context: the typed
//! `InstallPlan`, `InstallRun`, `IdempotencyKey`, `SecretCiphertext`
//! records, and the persistence port. Concrete install / upgrade /
//! uninstall orchestration lives in the application layer.
//!
//! I/O-free.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Maximum plan validity in seconds (5 minutes, per the spec).
pub const MAX_PLAN_VALIDITY_SECONDS: i64 = 300;

/// Maximum idempotency window in seconds (24 hours, per the spec).
pub const IDEMPOTENCY_TTL_SECONDS: i64 = 24 * 60 * 60;

/// Confirmation window in seconds (per the spec).
pub const CONFIRM_WINDOW_SECONDS: i64 = 60;

/// A single artifact in an install plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallArtifact {
    /// Stable name within the plan (e.g. `app.tar.gz`).
    pub name: String,
    /// Download URL.
    pub url: String,
    /// Expected SHA-256 hex.
    pub sha256: String,
    /// Declared size in bytes.
    pub size_bytes: u64,
    /// Optional detached signature URL.
    pub signature_url: Option<String>,
}

impl InstallArtifact {
    /// Build an artifact.
    pub fn new(
        name: impl Into<String>,
        url: impl Into<String>,
        sha256: impl Into<String>,
        size_bytes: u64,
    ) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            sha256: sha256.into(),
            size_bytes,
            signature_url: None,
        }
    }

    /// Set the signature URL.
    pub fn with_signature_url(mut self, url: impl Into<String>) -> Self {
        self.signature_url = Some(url.into());
        self
    }
}

/// The database plan inside an install plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum InstallDb {
    /// Reuse the site's existing database.
    Existing,
    /// Create a fresh `openpanel_<owner>_<name>` database.
    Fresh {
        /// The name suffix.
        name: String,
    },
    /// No database required (static site).
    None,
}

/// Overlays that are added on top of the install root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallOverlay {
    /// Path under the install root.
    pub path: String,
    /// Template body (rendered with the site context).
    pub body: String,
}

/// A typed install plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallPlan {
    id: Uuid,
    app_id: String,
    site_id: Uuid,
    artifacts: Vec<InstallArtifact>,
    install_path: String,
    db: InstallDb,
    overlays: Vec<InstallOverlay>,
    warnings: Vec<String>,
    content_hash: String,
    secret_ciphertext: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl InstallPlan {
    /// Build a plan.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        app_id: impl Into<String>,
        site_id: Uuid,
        artifacts: Vec<InstallArtifact>,
        install_path: impl Into<String>,
        db: InstallDb,
        overlays: Vec<InstallOverlay>,
        warnings: Vec<String>,
        content_hash: impl Into<String>,
        secret_ciphertext: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, WebApplicationInstallerError> {
        let app_id = app_id.into();
        if app_id.is_empty() {
            return Err(WebApplicationInstallerError::InvalidAppId);
        }
        let expires_at = created_at + chrono::Duration::seconds(MAX_PLAN_VALIDITY_SECONDS);
        Ok(Self {
            id,
            app_id,
            site_id,
            artifacts,
            install_path: install_path.into(),
            db,
            overlays,
            warnings,
            content_hash: content_hash.into(),
            secret_ciphertext: secret_ciphertext.into(),
            created_at,
            expires_at,
        })
    }

    /// Plan id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// App id.
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Site id.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Artifacts.
    pub fn artifacts(&self) -> &[InstallArtifact] {
        &self.artifacts
    }

    /// Install path under the site root.
    pub fn install_path(&self) -> &str {
        &self.install_path
    }

    /// Database plan.
    pub fn db(&self) -> &InstallDb {
        &self.db
    }

    /// Overlays.
    pub fn overlays(&self) -> &[InstallOverlay] {
        &self.overlays
    }

    /// Warnings.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Content hash.
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    /// Encrypted secret.
    pub fn secret_ciphertext(&self) -> &str {
        &self.secret_ciphertext
    }

    /// Created at.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Expires at.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Whether the plan is valid at `at`.
    pub fn is_valid_at(&self, at: DateTime<Utc>) -> bool {
        at <= self.expires_at
    }
}

/// A confirmed, executed install run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRun {
    id: Uuid,
    plan_id: Uuid,
    site_id: Uuid,
    app_id: String,
    install_id: String,
    install_path: String,
    post_install_url: Option<String>,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    failure_reason: Option<String>,
}

impl InstallRun {
    /// Build a new run.
    pub fn new(
        id: Uuid,
        plan_id: Uuid,
        site_id: Uuid,
        app_id: impl Into<String>,
        install_id: impl Into<String>,
        install_path: impl Into<String>,
        started_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            plan_id,
            site_id,
            app_id: app_id.into(),
            install_id: install_id.into(),
            install_path: install_path.into(),
            post_install_url: None,
            started_at,
            finished_at: None,
            failure_reason: None,
        }
    }

    /// Run id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Plan id.
    pub fn plan_id(&self) -> Uuid {
        self.plan_id
    }

    /// Site id.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// App id.
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Install id (e.g. "wp-<owner>-example-com").
    pub fn install_id(&self) -> &str {
        &self.install_id
    }

    /// Install path.
    pub fn install_path(&self) -> &str {
        &self.install_path
    }

    /// Post-install URL.
    pub fn post_install_url(&self) -> Option<&str> {
        self.post_install_url.as_deref()
    }

    /// Set the post-install URL.
    pub fn set_post_install_url(&mut self, url: impl Into<String>) {
        self.post_install_url = Some(url.into());
    }

    /// Started at.
    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    /// Finished at.
    pub fn finished_at(&self) -> Option<DateTime<Utc>> {
        self.finished_at
    }

    /// Failure reason.
    pub fn failure_reason(&self) -> Option<&str> {
        self.failure_reason.as_deref()
    }

    /// Mark the run finished.
    pub fn mark_finished(&mut self, at: DateTime<Utc>, failure_reason: Option<String>) {
        self.finished_at = Some(at);
        self.failure_reason = failure_reason;
    }
}

/// An idempotency record (replay protection).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyKey {
    key: String,
    run_id: Uuid,
    created_at: DateTime<Utc>,
}

impl IdempotencyKey {
    /// Build a key.
    pub fn new(key: impl Into<String>, run_id: Uuid, created_at: DateTime<Utc>) -> Self {
        Self {
            key: key.into(),
            run_id,
            created_at,
        }
    }

    /// Key string.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Run id this key maps to.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Created at.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

/// A stored, installed web application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledWebApp {
    install_id: String,
    site_id: Uuid,
    app_id: String,
    version: String,
    install_path: String,
    created_at: DateTime<Utc>,
    removed_at: Option<DateTime<Utc>>,
}

impl InstalledWebApp {
    /// Build a record.
    pub fn new(
        install_id: impl Into<String>,
        site_id: Uuid,
        app_id: impl Into<String>,
        version: impl Into<String>,
        install_path: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            install_id: install_id.into(),
            site_id,
            app_id: app_id.into(),
            version: version.into(),
            install_path: install_path.into(),
            created_at,
            removed_at: None,
        }
    }

    /// Install id.
    pub fn install_id(&self) -> &str {
        &self.install_id
    }

    /// Site id.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// App id.
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Install path.
    pub fn install_path(&self) -> &str {
        &self.install_path
    }

    /// Created at.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Removed at.
    pub fn removed_at(&self) -> Option<DateTime<Utc>> {
        self.removed_at
    }

    /// Mark the install removed.
    pub fn mark_removed(&mut self, at: DateTime<Utc>) {
        self.removed_at = Some(at);
    }
}

/// Repository port.
#[async_trait]
pub trait WebApplicationInstallerRepository: Send + Sync + 'static {
    /// Persist a plan.
    async fn upsert_plan(&self, plan: &InstallPlan) -> Result<(), WebApplicationInstallerError>;
    /// Load a plan by id.
    async fn find_plan(
        &self,
        id: Uuid,
    ) -> Result<Option<InstallPlan>, WebApplicationInstallerError>;
    /// Persist a run.
    async fn insert_run(&self, run: &InstallRun) -> Result<(), WebApplicationInstallerError>;
    /// Update a run's finish state.
    async fn finish_run(
        &self,
        run_id: Uuid,
        finished_at: DateTime<Utc>,
        failure_reason: Option<String>,
    ) -> Result<(), WebApplicationInstallerError>;
    /// Look up a run by idempotency key (replay).
    async fn find_run_by_idempotency_key(
        &self,
        key: &str,
    ) -> Result<Option<InstallRun>, WebApplicationInstallerError>;
    /// Persist an idempotency key.
    async fn insert_idempotency_key(
        &self,
        key: &IdempotencyKey,
    ) -> Result<(), WebApplicationInstallerError>;
    /// Persist an installed record.
    async fn insert_installed(
        &self,
        installed: &InstalledWebApp,
    ) -> Result<(), WebApplicationInstallerError>;
    /// Mark an installed record removed.
    async fn mark_installed_removed(
        &self,
        install_id: &str,
        at: DateTime<Utc>,
    ) -> Result<(), WebApplicationInstallerError>;
    /// Look up an installed record by `(site_id, app_id)`.
    async fn find_installed(
        &self,
        site_id: Uuid,
        app_id: &str,
    ) -> Result<Option<InstalledWebApp>, WebApplicationInstallerError>;
    /// List installed records for a site (newest first).
    async fn list_installed(
        &self,
        site_id: Uuid,
    ) -> Result<Vec<InstalledWebApp>, WebApplicationInstallerError>;
}

/// Errors that can occur in the web-application-installer
/// bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WebApplicationInstallerError {
    /// The plan has expired.
    #[error("install plan expired")]
    PlanExpired,
    /// The app id is invalid.
    #[error("invalid app id")]
    InvalidAppId,
    /// An artifact failed verification (sha256 mismatch).
    #[error("artifact rejected: {0}")]
    InstallArtifactRejected(String),
    /// The artifact signature is invalid.
    #[error("artifact signature failed")]
    ArtifactSignatureInvalid,
    /// A confirmation is missing.
    #[error("missing confirmation")]
    MissingConfirmation,
    /// The install is already in flight (concurrency).
    #[error("install in flight for (site, app)")]
    InstallInFlight,
    /// The app is already installed at the same path.
    #[error("app already installed at the same path")]
    AlreadyInstalled,
    /// Overlay diff requires destructive confirmation.
    #[error("overlay diff requires confirmation: {0:?}")]
    OverlayDiffRequiresConfirmation(Vec<String>),
    /// The site is missing.
    #[error("site not found")]
    SiteNotFound,
    /// The manifest was missing.
    #[error("manifest not found")]
    ManifestNotFound,
    /// The caller's role is forbidden.
    #[error("forbidden")]
    Forbidden,
    /// The install was not found.
    #[error("install not found")]
    InstallNotFound,
    /// The install was refused.
    #[error("install refused: {0}")]
    Refused(String),
    /// Persistence layer failure.
    #[error("web-app-installer persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for WebApplicationInstallerError {
    fn from(error: RepoError) -> Self {
        WebApplicationInstallerError::Persistence(error.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn plan_validity_is_5_minutes() {
        let plan = InstallPlan::new(
            Uuid::new_v4(),
            "wordpress",
            Uuid::new_v4(),
            vec![InstallArtifact::new(
                "wp.tar.gz",
                "https://example.com/wp.tar.gz",
                "deadbeef",
                1024,
            )],
            "/var/www/example.com/wordpress",
            InstallDb::Fresh {
                name: "wp".to_string(),
            },
            vec![],
            vec!["warn".to_string()],
            "abc",
            "opaque",
            now(),
        )
        .expect("plan");
        assert!(plan.is_valid_at(plan.created_at()));
        assert!(!plan.is_valid_at(plan.expires_at() + chrono::Duration::seconds(1)));
    }

    #[test]
    fn plan_rejects_empty_app_id() {
        let err = InstallPlan::new(
            Uuid::new_v4(),
            "",
            Uuid::new_v4(),
            vec![],
            "/var/www/x/wp",
            InstallDb::None,
            vec![],
            vec![],
            "abc",
            "opaque",
            now(),
        )
        .expect_err("empty app id");
        assert!(matches!(err, WebApplicationInstallerError::InvalidAppId));
    }

    #[test]
    fn install_run_lifecycle() {
        let site_id = Uuid::new_v4();
        let mut run = InstallRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            site_id,
            "wordpress",
            "wp-alice-example-com",
            "/var/www/example.com/wordpress",
            now(),
        );
        run.set_post_install_url("https://example.com/wp-admin");
        assert_eq!(run.site_id(), site_id);
        run.mark_finished(Utc::now(), None);
        assert!(run.finished_at().is_some());
    }
}
