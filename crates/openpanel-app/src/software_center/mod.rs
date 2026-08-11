//! Trusted embedded catalog and package transaction orchestration.

mod apt;
mod artifact;
mod catalog;
pub mod host_package_manager;
mod integrated;
mod module;
pub mod seed;
mod source;
mod store;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

pub use apt::{
    AptPackageManager, CommandResult, PackageCommand, PrivilegedCommand, TokioPackageCommand,
    needs_privilege,
};
pub use artifact::{
    ArtifactDigest, ArtifactDownloader, ArtifactFetcher, PLACEHOLDER_SHA256, PinnedArtifact,
    PlacedArtifact, ReqwestArtifactFetcher, SafeArtifactInstaller, pinned_artifact, place_artifact,
    place_artifact_with_gate,
};
use async_trait::async_trait;
pub use catalog::{CatalogVerifier, SignedCatalogEnvelope, VerifiedCatalog};
pub use host_package_manager::{Family, HostPackageManager, current_platform};
pub use integrated::{
    ApplicationDeploymentResources, CreatedApplicationDatabase, CreatedApplicationSite,
    IntegratedApplicationDeployer, OpenPanelApplicationResources,
};
pub use module::{FakeApplicationDeployer, FakePackageManager, SoftwareCenterModule};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
pub use openpanel_domain::software_center::{CatalogQuery, CatalogSearchPage};
use openpanel_domain::{
    Role,
    software_center::{
        JobState, PackageId, PlanAction, SoftwareJob, SoftwarePlan, SupportedPlatform,
    },
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
pub use source::{
    CatalogSource, EmbeddedCatalogSource, FetchedManifest, HttpCatalogSource, HttpSourceConfig,
    StaticCatalogSource,
};
use sqlx::SqlitePool;
pub use store::{
    CatalogActivation, CatalogDiagnostics, CompatibilityHost, CompatibilityReport, RefreshOutcome,
    SoftwareCatalogStore, StorefrontEntry, StorefrontVersion, WizardState, default_catalog_url,
};
use thiserror::Error;
use tokio::sync::{Mutex as AsyncMutex, OnceCell};
use uuid::Uuid;

/// Software Center application error with bounded diagnostics.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SoftwareCenterError {
    /// Caller is not an Owner.
    #[error("software operation forbidden")]
    Forbidden,
    /// Catalog entry, plan, or input is invalid.
    #[error("invalid software request: {0}")]
    Invalid(String),
    /// Host or catalog state differs from the preview.
    #[error("software plan conflict")]
    Conflict,
    /// Managed resources still require the component.
    #[error("software component has {0} managed dependents")]
    Dependencies(u64),
    /// Component validation failed.
    #[error("software validation failed")]
    Validation,
    /// Package adapter failed with a redacted, bounded diagnostic.
    #[error("software package operation failed: {0}")]
    Package(String),
    /// A catalog item is visible but its required deployment adapter is unavailable.
    #[error("software deployment adapter unavailable")]
    Unsupported,
    /// Internal state persistence failed.
    #[error("software state unavailable")]
    Repository,
}

/// Discovered host state used to bind a preview to execution.
#[derive(Debug, Clone)]
pub struct HostSnapshot {
    /// Exact supported platform.
    pub platform: SupportedPlatform,
    /// Stable digest of relevant package and repository state.
    pub state_digest: String,
    /// Package identifiers reported as installed by the native manager.
    pub installed_packages: BTreeSet<String>,
}
impl HostSnapshot {
    /// Deterministic Ubuntu snapshot for adapter tests.
    pub fn test(state_digest: &str) -> Self {
        let platform = match SupportedPlatform::new("ubuntu", "24.04", "x86_64") {
            Ok(value) => value,
            Err(_) => std::process::abort(),
        };
        Self {
            platform,
            state_digest: state_digest.to_owned(),
            installed_packages: BTreeSet::new(),
        }
    }
}

/// Typed package-manager boundary. Implementations own every executable and argument.
#[async_trait]
pub trait PackageManager: Send + Sync {
    /// Discover the platform and relevant package state.
    async fn discover(&self) -> Result<HostSnapshot, SoftwareCenterError>;
    /// Apply the fixed package actions from a confirmed plan.
    async fn apply(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError>;
    /// Validate component configuration and readiness.
    async fn validate(&self, component: &str) -> Result<(), SoftwareCenterError>;
    /// Restore recipe-supported package/configuration state.
    async fn rollback(&self, actions: &[PlanAction]) -> Result<(), SoftwareCenterError>;
}

/// Validated one-click application choices supplied by an Owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationDeploymentInput {
    /// `wordpress` or `drupal`.
    pub application: String,
    /// Unused primary DNS name reserved for the new site.
    pub domain: String,
    /// Supported PHP minor version (`8.3` or `8.4`).
    pub php_version: String,
    /// Installation locale passed as data, never command syntax.
    pub locale: String,
    /// Request DNS integration after the application is healthy.
    pub enable_dns: bool,
    /// Request TLS integration after DNS/site provisioning.
    pub enable_tls: bool,
    /// Register the resulting site in backups.
    pub enable_backups: bool,
}
impl ApplicationDeploymentInput {
    /// Deterministic input for adapter tests.
    pub fn test(application: &str, domain: &str, php_version: &str) -> Self {
        Self {
            application: application.to_owned(),
            domain: domain.to_owned(),
            php_version: php_version.to_owned(),
            locale: "en_US".to_owned(),
            enable_dns: false,
            enable_tls: false,
            enable_backups: false,
        }
    }
}

/// Resources created by an application adapter plus one-time credentials.
#[derive(Debug, Clone)]
pub struct ProvisionedApplication {
    /// Stable deployment identifier.
    pub id: Uuid,
    /// Adapter-owned rollback handles in creation order.
    pub resources: Vec<String>,
    resource_receipts: Vec<ApplicationResource>,
    admin_username: String,
    admin_password: String,
}
impl ProvisionedApplication {
    /// Deterministic receipt for service tests.
    pub fn test(resources: Vec<&str>, username: &str, password: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            resources: resources.into_iter().map(str::to_owned).collect(),
            resource_receipts: Vec::new(),
            admin_username: username.to_owned(),
            admin_password: password.to_owned(),
        }
    }
}

/// Typed rollback receipt for a resource created by an application transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationResource {
    /// OpenPanel site aggregate and its exact document root.
    Site {
        /// Site aggregate identifier.
        id: Uuid,
        /// Exact document root allocated by the site service.
        document_root: String,
    },
    /// OpenPanel database aggregate.
    Database {
        /// Database aggregate identifier.
        id: Uuid,
    },
    /// Verified application files staged below the site's document root.
    Artifact {
        /// Catalog application identifier.
        application: String,
        /// Primary site domain.
        domain: String,
        /// Selected PHP runtime version.
        php_version: String,
        /// Exact managed document root.
        document_root: String,
    },
    /// Optional DNS integration receipt.
    Dns {
        /// Provider record or lease identifier.
        id: String,
    },
    /// Optional TLS integration receipt.
    Tls {
        /// Certificate domain.
        domain: String,
    },
    /// Optional backup-plan integration receipt.
    Backup {
        /// Backup plan identifier.
        id: Uuid,
    },
}

/// Typed transactional boundary for site/database/artifact integrations.
#[async_trait]
pub trait ApplicationDeployer: Send + Sync {
    /// Optional integration capabilities available before any package mutation.
    fn capabilities(&self) -> ApplicationCapabilities {
        ApplicationCapabilities::default()
    }
    /// Create only the resources declared by a confirmed deployment plan.
    async fn provision(
        &self,
        actor: Uuid,
        input: &ApplicationDeploymentInput,
    ) -> Result<ProvisionedApplication, SoftwareCenterError>;
    /// Validate the installed application's health.
    async fn validate(
        &self,
        deployment: &ProvisionedApplication,
    ) -> Result<(), SoftwareCenterError>;
    /// Delete only receipt resources, in reverse creation order.
    async fn rollback(
        &self,
        deployment: &ProvisionedApplication,
    ) -> Result<(), SoftwareCenterError>;
}

/// Optional integrations implemented by an application deployment adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplicationCapabilities {
    /// Provider-backed DNS record creation and rollback.
    pub dns: bool,
    /// Certificate issuance and rollback.
    pub tls: bool,
    /// Backup plan creation and rollback.
    pub backups: bool,
}

pub(crate) struct UnavailableApplicationDeployer;
#[async_trait]
impl ApplicationDeployer for UnavailableApplicationDeployer {
    async fn provision(
        &self,
        _actor: Uuid,
        _input: &ApplicationDeploymentInput,
    ) -> Result<ProvisionedApplication, SoftwareCenterError> {
        Err(SoftwareCenterError::Package(
            "application deployment is unavailable on this host".into(),
        ))
    }

    async fn validate(
        &self,
        _deployment: &ProvisionedApplication,
    ) -> Result<(), SoftwareCenterError> {
        Err(SoftwareCenterError::Validation)
    }

    async fn rollback(
        &self,
        _deployment: &ProvisionedApplication,
    ) -> Result<(), SoftwareCenterError> {
        Ok(())
    }
}

/// Catalog entry kind.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    /// Host package or runtime.
    SystemComponent,
    /// Deployable site application.
    WebApplication,
    /// Operator tool — system install surfaced under the Tools category.
    Tool,
}

/// Supported component lifecycle mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentAction {
    /// Install an available component.
    Install,
    /// Validate and take management ownership of an external component.
    Adopt,
    /// Upgrade a panel-managed component within its recipe.
    Update,
    /// Remove a panel-managed component when no dependency blocks it.
    Remove,
}

/// Public, secret-free recovery catalog entry.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    /// Stable identifier.
    pub id: String,
    /// Display label.
    pub name: String,
    /// Human-readable purpose without executable content.
    pub description: String,
    /// Stable display category.
    pub category: String,
    /// Typed recipe kind.
    pub kind: CatalogKind,
    /// SPDX-style license label.
    pub license: String,
    /// Selectable supported upstream versions.
    pub versions: Vec<String>,
    /// Exact supported host tuples.
    pub platforms: Vec<SupportedPlatform>,
    /// Logical component identifiers required by this entry.
    pub dependencies: Vec<String>,
    /// Catalog source shown to the operator.
    pub provenance: String,
    /// Fixed distro packages owned by the recipe.
    pub packages: Vec<PackageId>,
    /// Catalog-level availability before host discovery.
    pub lifecycle_state: String,
}

/// Discovered catalog lifecycle without implicit ownership changes.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentInventory {
    /// Catalog identifier.
    pub id: String,
    /// `available` or `externally_managed` in the recovery catalog.
    pub state: String,
}

/// Reviewable installation preview with a one-time confirmation.
#[derive(Debug, Serialize)]
pub struct InstallPreview {
    /// Immutable, digest-bound package plan.
    pub plan: SoftwarePlan,
    /// Opaque one-use confirmation token.
    pub confirmation_token: String,
    /// Aggregate affected service labels.
    pub affected_services: Vec<String>,
    /// Fixed package identifiers expected to be downloaded or changed.
    pub packages: Vec<String>,
    /// Native manager estimate when available.
    pub estimated_download_bytes: Option<u64>,
    /// Native manager disk delta when available.
    pub estimated_disk_delta_bytes: Option<i64>,
    /// OpenPanel-owned configuration roots affected by the recipe.
    pub configuration_paths: Vec<String>,
    /// Whether the recipe can automatically reverse this action.
    pub rollback_supported: bool,
    /// Exact aggregate dependent counts discovered during planning.
    pub dependency_counts: BTreeMap<String, u64>,
    /// Typed conflict explanations; never command output.
    pub conflicts: Vec<String>,
}

#[derive(Clone)]
struct StoredPreview {
    kind: PreviewKind,
    actions: Vec<PlanAction>,
    host_state_digest: String,
    confirmation_token: String,
    expires_at: u64,
}

struct DurableTransactionLock {
    pool: Option<SqlitePool>,
    job_id: Uuid,
}
impl DurableTransactionLock {
    async fn release(mut self) -> Result<(), SoftwareCenterError> {
        if let Some(pool) = self.pool.take() {
            sqlx::query("DELETE FROM software_transaction_lock WHERE singleton=1 AND job_id=?")
                .bind(self.job_id.to_string())
                .execute(&pool)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
        }
        Ok(())
    }
}
impl Drop for DurableTransactionLock {
    fn drop(&mut self) {
        let Some(pool) = self.pool.take() else {
            return;
        };
        let job_id = self.job_id.to_string();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = sqlx::query(
                    "DELETE FROM software_transaction_lock WHERE singleton=1 AND job_id=?",
                )
                .bind(job_id)
                .execute(&pool)
                .await;
            });
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
enum PreviewKind {
    Component { id: String, action: ComponentAction },
    Application(ApplicationDeploymentInput),
}

#[derive(Serialize, Deserialize)]
struct PersistedPreview {
    kind: PreviewKind,
    actions: Vec<PlanAction>,
}

/// Public bounded job projection.
#[derive(Debug, Clone, Serialize)]
pub struct SoftwareJobView {
    /// Stable job ID.
    pub id: Uuid,
    /// Plan digest without command details.
    pub plan_digest: String,
    /// Redacted lifecycle state.
    pub state: String,
}

/// Secret-free aggregate health and ownership counts.
#[derive(Debug, Clone, Serialize)]
pub struct SoftwareDiagnostics {
    /// Trusted entries currently available.
    pub catalog_entries: usize,
    /// Components explicitly managed by OpenPanel.
    pub managed_components: usize,
    /// Compatible components detected outside OpenPanel ownership.
    pub external_components: usize,
    /// Non-terminal package/application jobs.
    pub active_jobs: usize,
    /// Jobs reconciled after interruption.
    pub interrupted_jobs: usize,
}

/// Fresh confirmation generated for a recipe-supported interrupted-job retry.
#[derive(Debug, Clone, Serialize)]
pub struct RetryPreview {
    /// Original immutable plan digest.
    pub plan_digest: String,
    /// New one-use confirmation token.
    pub confirmation_token: String,
    /// Fixed packages retained from the original plan.
    pub packages: Vec<String>,
}

/// Outcome of a one-step artifact install (no plan, no confirmation).
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactInstallResult {
    /// Stable entry id from the catalog.
    pub entry_id: String,
    /// Display name from the catalog.
    pub entry_name: String,
    /// Installed version string.
    pub version: String,
    /// Archive type from the recipe (`tar.gz`, `file`, ...).
    pub archive_type: String,
    /// Absolute path the artifact was placed at.
    pub destination: std::path::PathBuf,
    /// Filename for single-file artifacts; `None` for extracted archives.
    pub filename: Option<String>,
    /// Bytes downloaded and written.
    pub bytes: u64,
    /// Whether the digest was checked. False when the recipe ships a
    /// documented placeholder (the recovery seed for adminer, etc.).
    pub digest_verified: bool,
}

/// Successful application deployment with credentials returned exactly once.
#[derive(Debug, Serialize)]
pub struct ApplicationDeploymentResult {
    /// Completed transaction job.
    pub job: SoftwareJobView,
    /// Application deployment identifier.
    pub deployment_id: Uuid,
    /// Generated administrator username.
    pub admin_username: String,
    /// Generated administrator password; never persisted in job history.
    pub admin_password: String,
}

/// Owner-only Software Center service.
pub struct SoftwareCenterService {
    packages: Arc<dyn PackageManager>,
    applications: Arc<dyn ApplicationDeployer>,
    audit: Arc<dyn AuditService>,
    previews: Mutex<HashMap<String, StoredPreview>>,
    jobs: Mutex<Vec<SoftwareJobView>>,
    owned_components: Mutex<BTreeSet<String>>,
    cancellation_requests: Mutex<BTreeSet<Uuid>>,
    transaction: AsyncMutex<()>,
    pool: Option<SqlitePool>,
    reconciled: OnceCell<()>,
    applications_enabled: bool,
    /// Placeholder-SHA-256 fail-closed gate. When `true` (the
    /// production default), `install_artifact` refuses any entry whose
    /// recipe ships the documented placeholder digest. The
    /// `with_artifact_pipeline` constructor reads the gate from the
    /// test suite overrides it via the explicit field.
    require_verified_digests: bool,
    store: SoftwareCatalogStore,
    catalog_source: Arc<dyn CatalogSource>,
    artifact_fetcher: Arc<dyn ArtifactFetcher>,
    webapps_root: PathBuf,
}
impl SoftwareCenterService {
    /// Construct from the fixed package adapter and append-only audit port.
    pub fn new(packages: Arc<dyn PackageManager>, audit: Arc<dyn AuditService>) -> Self {
        Self::compose(
            packages,
            Arc::new(UnavailableApplicationDeployer),
            audit,
            None,
            false,
            Arc::new(ReqwestArtifactFetcher::default()),
            default_webapps_root(),
        )
    }

    /// Construct with an explicit application deployment adapter.
    pub fn with_deployer(
        packages: Arc<dyn PackageManager>,
        applications: Arc<dyn ApplicationDeployer>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self::compose(
            packages,
            applications,
            audit,
            None,
            true,
            Arc::new(ReqwestArtifactFetcher::default()),
            default_webapps_root(),
        )
    }

    /// Compose the service with an explicit artifact fetcher, a custom
    /// webapps root, and an explicit placeholder-digest gate. Used by
    /// the integration test that swaps in a `MemoryArtifactFetcher`
    /// and a per-test temp directory.
    #[allow(clippy::too_many_arguments)]
    pub fn with_artifact_pipeline(
        packages: Arc<dyn PackageManager>,
        applications: Arc<dyn ApplicationDeployer>,
        audit: Arc<dyn AuditService>,
        pool: Option<SqlitePool>,
        applications_enabled: bool,
        fetcher: Arc<dyn ArtifactFetcher>,
        webapps_root: PathBuf,
        require_verified_digests: bool,
    ) -> Self {
        Self::compose_with_gate(
            packages,
            applications,
            audit,
            pool,
            applications_enabled,
            fetcher,
            webapps_root,
            require_verified_digests,
        )
    }

    fn compose(
        packages: Arc<dyn PackageManager>,
        applications: Arc<dyn ApplicationDeployer>,
        audit: Arc<dyn AuditService>,
        pool: Option<SqlitePool>,
        applications_enabled: bool,
        fetcher: Arc<dyn ArtifactFetcher>,
        webapps_root: PathBuf,
    ) -> Self {
        let require_verified = !matches!(
            std::env::var("OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS").as_deref(),
            Ok("false") | Ok("0") | Ok("no"),
        );
        Self::compose_with_gate(
            packages,
            applications,
            audit,
            pool,
            applications_enabled,
            fetcher,
            webapps_root,
            require_verified,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn compose_with_gate(
        packages: Arc<dyn PackageManager>,
        applications: Arc<dyn ApplicationDeployer>,
        audit: Arc<dyn AuditService>,
        pool: Option<SqlitePool>,
        applications_enabled: bool,
        fetcher: Arc<dyn ArtifactFetcher>,
        webapps_root: PathBuf,
        require_verified_digests: bool,
    ) -> Self {
        let store = SoftwareCatalogStore::new(pool.clone());
        let catalog_source: Arc<dyn CatalogSource> =
            Arc::new(EmbeddedCatalogSource::new(default_catalog_url()));
        Self {
            packages,
            applications,
            audit,
            previews: Mutex::new(HashMap::new()),
            jobs: Mutex::new(Vec::new()),
            owned_components: Mutex::new(BTreeSet::new()),
            cancellation_requests: Mutex::new(BTreeSet::new()),
            transaction: AsyncMutex::new(()),
            pool,
            reconciled: OnceCell::new(),
            applications_enabled,
            require_verified_digests,
            store,
            catalog_source,
            artifact_fetcher: fetcher,
            webapps_root,
        }
    }

    /// Replace the catalog source used by `refresh_catalog`.
    pub fn set_catalog_source(&mut self, source: Arc<dyn CatalogSource>) {
        self.catalog_source = source;
    }

    /// Underlying catalog store, for advanced flows.
    pub fn store(&self) -> &SoftwareCatalogStore {
        &self.store
    }

    /// List the embedded recovery catalog for an Owner. Reads from the
    /// aggregator store (which is materialized from the new production
    /// seed) and projects the entries to the legacy `CatalogEntry`
    /// shape that the existing API and CLI depend on.
    pub async fn catalog(&self, role: Role) -> Result<Vec<CatalogEntry>, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        self.ensure_seed_materialized().await;
        let query = CatalogQuery {
            page_size: 60,
            ..CatalogQuery::default()
        };
        let page = self.store.search(&query).await?;
        let mut entries: Vec<CatalogEntry> = Vec::with_capacity(page.hits.len());
        for hit in page.hits {
            let detail = self.store.get_entry(&hit.id).await?;
            if let Some(entry) = detail {
                entries.push(project_entry(&entry, self.applications_enabled));
            }
        }
        if !self.applications_enabled {
            for entry in &mut entries {
                if matches!(entry.kind, CatalogKind::WebApplication) {
                    entry.lifecycle_state = "adapter_unavailable".to_owned();
                }
            }
        }
        Ok(entries)
    }

    /// Verify and atomically activate a remote data-only catalog snapshot.
    pub async fn activate_catalog(
        &self,
        actor: Uuid,
        role: Role,
        verifier: &CatalogVerifier,
        envelope: &SignedCatalogEnvelope,
        now: u64,
    ) -> Result<VerifiedCatalog, SoftwareCenterError> {
        owner(role)?;
        let verified = match verifier.verify(envelope, now) {
            Ok(value) => value,
            Err(error) => {
                self.audit
                    .record(
                        AuditEvent::new(
                            actor.to_string(),
                            AuditAction::SoftwareChanged,
                            AuditOutcome::Failure,
                        )
                        .target("catalog-refresh")
                        .metadata(serde_json::json!({"operation":"catalog_rejected"})),
                    )
                    .await
                    .map_err(|_| SoftwareCenterError::Repository)?;
                return Err(error);
            }
        };
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let mut transaction = pool
            .begin()
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        sqlx::query("UPDATE software_catalog_snapshots SET active=0 WHERE active=1")
            .execute(&mut *transaction)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        sqlx::query("INSERT INTO software_catalog_snapshots(id,digest,payload,signature,expires_at,active,created_at) VALUES(?,?,?,?,?,1,?) ON CONFLICT(digest) DO UPDATE SET active=1")
            .bind(Uuid::new_v4().to_string())
            .bind(&verified.digest)
            .bind(&envelope.payload)
            .bind(&envelope.signature)
            .bind(envelope.expires_at.to_string())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&mut *transaction)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        transaction
            .commit()
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        self.record(actor, "catalog_activated", &verified.digest)
            .await?;
        Ok(verified)
    }

    /// Discover installed packages and classify them without adopting them.
    pub async fn inventory(
        &self,
        role: Role,
    ) -> Result<Vec<ComponentInventory>, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        self.ensure_seed_materialized().await;
        let snapshot = self.packages.discover().await?;
        let owned = self.managed_components().await?;
        let mut query = CatalogQuery {
            page_size: 60,
            ..CatalogQuery::default()
        };
        // Only system and tool entries are installable; web apps deploy
        // through the application deployer.
        query.kind = Some(openpanel_domain::software_center::EntryKind::System);
        let page = self.store.search(&query).await?;
        let mut out = Vec::with_capacity(page.hits.len());
        for hit in page.hits {
            let detail = self.store.get_entry(&hit.id).await?;
            let Some(entry) = detail else { continue };
            let supported = !entry.versions.is_empty();
            let latest = entry
                .versions
                .iter()
                .find(|v| v.is_latest)
                .or(entry.versions.first());
            let installed = latest
                .map(|version| {
                    !version.packages.is_empty()
                        && version
                            .packages
                            .iter()
                            .all(|package| snapshot.installed_packages.contains(package))
                })
                .unwrap_or(false);
            let managed = owned.contains(&entry.id);
            let state = if !supported {
                "unsupported".to_owned()
            } else if installed && managed {
                "panel_managed".to_owned()
            } else if installed {
                "externally_managed".to_owned()
            } else {
                "available".to_owned()
            };
            out.push(ComponentInventory {
                id: entry.id,
                state,
            });
        }
        Ok(out)
    }

    /// Preview installation against a discovered host snapshot.
    pub async fn preview_install(
        &self,
        actor: Uuid,
        role: Role,
        component: &str,
    ) -> Result<InstallPreview, SoftwareCenterError> {
        self.preview_component(actor, role, component, ComponentAction::Install)
            .await
    }

    /// Preview one typed component lifecycle action.
    pub async fn preview_component(
        &self,
        actor: Uuid,
        role: Role,
        component: &str,
        action: ComponentAction,
    ) -> Result<InstallPreview, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        self.ensure_seed_materialized().await;
        // Read the entry from the aggregator store, not the legacy
        // hardcoded recovery list — the storefront and the new seed
        // both serve the new store, so the install plan must come from
        // the same source.
        let entry = self
            .store
            .get_entry(component)
            .await?
            .ok_or(SoftwareCenterError::Invalid(
            "plan is no longer in the queue; it may have been consumed, expired, or never created"
                .into(),
        ))?;
        if !matches!(
            entry.kind,
            openpanel_domain::software_center::EntryKind::System
        ) || entry.versions.is_empty()
        {
            return Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ));
        }
        let latest = entry
            .versions
            .iter()
            .find(|version| version.is_latest)
            .cloned()
            .or_else(|| entry.versions.first().cloned())
            .ok_or(SoftwareCenterError::Invalid("plan is no longer in the queue; it may have been consumed, expired, or never created".into()))?;
        if !latest.packages.is_empty() {
            // system entries have packages, not artifacts
        }
        let entry_packages: Vec<PackageId> = latest
            .packages
            .iter()
            .map(|name| {
                PackageId::new(name)
                    .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if entry_packages.is_empty() {
            return Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ));
        }
        let snapshot = self.packages.discover().await?;
        let entry_platforms: Vec<SupportedPlatform> = self
            .store
            .platforms_for(component)
            .await?
            .ok_or(SoftwareCenterError::Invalid(
            "plan is no longer in the queue; it may have been consumed, expired, or never created"
                .into(),
        ))?;
        if !entry_platforms.iter().any(|platform| {
            platform.distribution() == snapshot.platform.distribution()
                && platform.release() == snapshot.platform.release()
                && platform.architecture() == snapshot.platform.architecture()
        }) {
            return Err(SoftwareCenterError::Unsupported);
        }
        let installed = entry_packages
            .iter()
            .all(|package| snapshot.installed_packages.contains(package.as_str()));
        let managed = self.managed_components().await?.contains(component);
        match action {
            ComponentAction::Install if installed => return Err(SoftwareCenterError::Conflict),
            ComponentAction::Adopt if !installed || managed => {
                return Err(SoftwareCenterError::Conflict);
            }
            ComponentAction::Update | ComponentAction::Remove if !installed || !managed => {
                return Err(SoftwareCenterError::Conflict);
            }
            _ => {}
        }
        if action == ComponentAction::Remove {
            let dependents = self.dependency_count(component).await?;
            if dependents > 0 {
                return Err(SoftwareCenterError::Dependencies(dependents));
            }
        }
        if (component == "mysql" && snapshot.installed_packages.contains("mariadb-server"))
            || (component == "mariadb" && snapshot.installed_packages.contains("mysql-server"))
        {
            return Err(SoftwareCenterError::Conflict);
        }
        let actions: Vec<_> = match action {
            ComponentAction::Install => entry_packages
                .iter()
                .filter(|package| !snapshot.installed_packages.contains(package.as_str()))
                .cloned()
                .map(PlanAction::Install)
                .collect(),
            ComponentAction::Update => entry_packages
                .iter()
                .cloned()
                .map(PlanAction::Update)
                .collect(),
            ComponentAction::Remove => entry_packages
                .iter()
                .cloned()
                .map(PlanAction::Remove)
                .collect(),
            ComponentAction::Adopt => Vec::new(),
        };
        let plan = SoftwarePlan::new(
            format!("embedded-v1-{component}-{action:?}"),
            snapshot.platform,
            actions.clone(),
        )
        .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
        let confirmation_token = Uuid::new_v4().to_string();
        let expires_at = now()?.checked_add(300).ok_or(SoftwareCenterError::Invalid(
            "plan is no longer in the queue; it may have been consumed, expired, or never created"
                .into(),
        ))?;
        let stored = StoredPreview {
            kind: PreviewKind::Component {
                id: entry.id.clone(),
                action,
            },
            actions: actions.clone(),
            host_state_digest: snapshot.state_digest,
            confirmation_token: confirmation_token.clone(),
            expires_at,
        };
        self.persist_plan(plan.digest(), &stored).await?;
        self.previews
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?
            .insert(plan.digest().to_owned(), stored);
        self.record(actor, "component_previewed", plan.digest())
            .await?;
        let (download_bytes, installed_bytes) = component_estimate(component);
        let (estimated_download_bytes, estimated_disk_delta_bytes) = match action {
            ComponentAction::Install | ComponentAction::Update => {
                (Some(download_bytes), Some(installed_bytes))
            }
            ComponentAction::Remove => (Some(0), Some(-installed_bytes)),
            ComponentAction::Adopt => (Some(0), Some(0)),
        };
        Ok(InstallPreview {
            plan,
            confirmation_token,
            affected_services: vec![entry.id],
            packages: actions
                .iter()
                .map(|action| match action {
                    PlanAction::Install(package)
                    | PlanAction::Update(package)
                    | PlanAction::Remove(package) => package.as_str().to_owned(),
                })
                .collect(),
            estimated_download_bytes,
            estimated_disk_delta_bytes,
            configuration_paths: vec![format!("/etc/openpanel/software/{component}")],
            rollback_supported: matches!(action, ComponentAction::Install | ComponentAction::Adopt),
            dependency_counts: BTreeMap::new(),
            conflicts: Vec::new(),
        })
    }

    /// Download the pinned artifact for one Web entry and place its files
    /// under the managed webapps root. The full operation is one step:
    /// no preview, no confirmation token, no wizard. This is the path the
    /// web Install button drives for any Web entry that carries an
    /// `ArtifactPin` (adminer, phpmyadmin, joomla, ghost, typecho, ...).
    pub async fn install_artifact(
        &self,
        actor: Uuid,
        role: Role,
        entry_id: &str,
    ) -> Result<ArtifactInstallResult, SoftwareCenterError> {
        owner(role)?;
        self.ensure_seed_materialized().await;
        let entry = self
            .store
            .get_entry(entry_id)
            .await?
            .ok_or(SoftwareCenterError::Invalid(
            "plan is no longer in the queue; it may have been consumed, expired, or never created"
                .into(),
        ))?;
        if !matches!(
            entry.kind,
            openpanel_domain::software_center::EntryKind::Web
        ) {
            return Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ));
        }
        let version = entry
            .versions
            .iter()
            .find(|version| version.is_latest)
            .cloned()
            .or_else(|| entry.versions.first().cloned())
            .ok_or(SoftwareCenterError::Invalid("plan is no longer in the queue; it may have been consumed, expired, or never created".into()))?;
        let pin = version
            .artifact
            .clone()
            .ok_or(SoftwareCenterError::Invalid(
            "plan is no longer in the queue; it may have been consumed, expired, or never created"
                .into(),
        ))?;
        let url = pin.url.as_str().to_owned();
        let bytes = self.artifact_fetcher.fetch(&url).await?;
        if let Some(parent) = self.webapps_root.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
        }
        std::fs::create_dir_all(&self.webapps_root)
            .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
        let placed = place_artifact_with_gate(
            &pin,
            &bytes,
            &self.webapps_root,
            self.require_verified_digests,
        )?;
        let platform = current_platform();
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::SoftwareArtifactInstalled,
                    AuditOutcome::Success,
                )
                .target(entry_id)
                .metadata(serde_json::json!({
                    "operation": "artifact_installed",
                    "entry_name": entry.name,
                    "version": version.version,
                    "archive_type": pin.archive_type,
                    "source_url": pin.url.as_str(),
                    "digest": pin.sha256,
                    "digest_verified": placed.digest_verified,
                    "bytes": bytes.len() as u64,
                    "destination": placed.path.display().to_string(),
                    "platform": platform,
                })),
            )
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        Ok(ArtifactInstallResult {
            entry_id: entry.id,
            entry_name: entry.name,
            version: version.version,
            archive_type: pin.archive_type,
            destination: placed.path,
            filename: placed.filename,
            bytes: bytes.len() as u64,
            digest_verified: placed.digest_verified,
        })
    }

    /// Execute one fresh preview under the exclusive package transaction lock.
    pub async fn execute(
        &self,
        actor: Uuid,
        role: Role,
        plan_digest: &str,
        confirmation_token: &str,
    ) -> Result<SoftwareJobView, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        let _transaction = self.transaction.lock().await;
        let preview = self.take_preview(plan_digest, confirmation_token).await?;
        let current = self.packages.discover().await?;
        if current.state_digest != preview.host_state_digest {
            return Err(SoftwareCenterError::Conflict);
        }
        let id = Uuid::new_v4();
        let durable_lock = self.acquire_durable_lock(id).await?;
        let mut aggregate = SoftwareJob::new(id, plan_digest)
            .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
        aggregate
            .start()
            .map_err(|_| SoftwareCenterError::Repository)?;
        self.push_job(id, plan_digest, aggregate.state()).await?;
        if !preview.actions.is_empty()
            && let Err(error) = self.packages.apply(&preview.actions).await
        {
            let _ = aggregate.fail();
            self.push_job(id, plan_digest, aggregate.state()).await?;
            self.record_failure(actor, "package_failed", plan_digest)
                .await?;
            return Err(error);
        }
        if self.take_cancellation(id)? {
            let _ = self.packages.rollback(&preview.actions).await;
            let view = self.push_job(id, plan_digest, JobState::Cancelled).await?;
            durable_lock.release().await?;
            self.record(actor, "cancelled", plan_digest).await?;
            return Ok(view);
        }
        aggregate
            .validate()
            .map_err(|_| SoftwareCenterError::Repository)?;
        self.push_job(id, plan_digest, aggregate.state()).await?;
        let (component, action) = match preview.kind {
            PreviewKind::Component { id, action } => (id, action),
            PreviewKind::Application(_) => {
                return Err(SoftwareCenterError::Invalid(
                    "invalid software request".into(),
                ));
            }
        };
        if action != ComponentAction::Remove
            && let Err(error) = self.packages.validate(&component).await
        {
            let _ = self.packages.rollback(&preview.actions).await;
            let _ = aggregate.fail();
            self.push_job(id, plan_digest, aggregate.state()).await?;
            self.record_failure(actor, "validation_failed", plan_digest)
                .await?;
            return Err(error);
        }
        if self.take_cancellation(id)? {
            let _ = self.packages.rollback(&preview.actions).await;
            let view = self.push_job(id, plan_digest, JobState::Cancelled).await?;
            durable_lock.release().await?;
            self.record(actor, "cancelled", plan_digest).await?;
            return Ok(view);
        }
        self.set_managed(
            &component,
            action != ComponentAction::Remove,
            &current.state_digest,
        )
        .await?;
        aggregate
            .succeed()
            .map_err(|_| SoftwareCenterError::Repository)?;
        let view = self.push_job(id, plan_digest, aggregate.state()).await?;
        durable_lock.release().await?;
        self.record(actor, "installed", plan_digest).await?;
        Ok(view)
    }

    /// Preview a WordPress or Drupal deployment and its complete stack dependencies.
    pub async fn preview_deployment(
        &self,
        actor: Uuid,
        role: Role,
        input: ApplicationDeploymentInput,
    ) -> Result<InstallPreview, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        self.ensure_seed_materialized().await;
        if !self.applications_enabled {
            return Err(SoftwareCenterError::Unsupported);
        }
        validate_deployment_input(&input)?;
        let capabilities = self.applications.capabilities();
        if (input.enable_dns && !capabilities.dns)
            || (input.enable_tls && !capabilities.tls)
            || (input.enable_backups && !capabilities.backups)
        {
            return Err(SoftwareCenterError::Unsupported);
        }
        let snapshot = self.packages.discover().await?;
        let php_id = format!("php-{}", input.php_version);
        let mut actions = Vec::new();
        for id in ["nginx", php_id.as_str(), "mysql"] {
            let entry = self
                .store
                .get_entry(id)
                .await?
                .ok_or(SoftwareCenterError::Invalid("plan is no longer in the queue; it may have been consumed, expired, or never created".into()))?;
            let platforms = self
                .store
                .platforms_for(id)
                .await?
                .ok_or(SoftwareCenterError::Invalid("plan is no longer in the queue; it may have been consumed, expired, or never created".into()))?;
            if !platforms.iter().any(|platform| {
                platform.distribution() == snapshot.platform.distribution()
                    && platform.release() == snapshot.platform.release()
                    && platform.architecture() == snapshot.platform.architecture()
            }) {
                return Err(SoftwareCenterError::Unsupported);
            }
            let latest = entry
                .versions
                .first()
                .cloned()
                .ok_or(SoftwareCenterError::Invalid("plan is no longer in the queue; it may have been consumed, expired, or never created".into()))?;
            actions.extend(
                latest
                    .packages
                    .iter()
                    .filter(|package| !snapshot.installed_packages.contains(package.as_str()))
                    .filter_map(|name| {
                        // The seed catalog only carries validated
                        // package identifiers, so the conversion is
                        // expected to succeed; fall back to a no-op on
                        // an unexpected failure rather than panicking.
                        PackageId::new(name).ok().map(PlanAction::install)
                    })
                    .collect::<Vec<_>>(),
            );
        }
        let input_bytes = serde_json::to_vec(&input)
            .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
        let target_digest = hex::encode(sha2::Sha256::digest(input_bytes));
        let plan_digest_str = format!("embedded-v1-{target_digest}");
        let plan = SoftwarePlan::new(plan_digest_str, snapshot.platform, actions.clone())
            .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
        let confirmation_token = Uuid::new_v4().to_string();
        let expires_at = now()?.checked_add(300).ok_or(SoftwareCenterError::Invalid(
            "plan is no longer in the queue; it may have been consumed, expired, or never created"
                .into(),
        ))?;
        let stored = StoredPreview {
            kind: PreviewKind::Application(input.clone()),
            actions: actions.clone(),
            host_state_digest: snapshot.state_digest,
            confirmation_token: confirmation_token.clone(),
            expires_at,
        };
        self.persist_plan(plan.digest(), &stored).await?;
        self.previews
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?
            .insert(plan.digest().to_owned(), stored);
        self.record(actor, "deployment_previewed", plan.digest())
            .await?;
        let php_component = format!("php-{}", input.php_version);
        let estimates = ["nginx", php_component.as_str(), "mysql"]
            .into_iter()
            .map(component_estimate)
            .fold((0_u64, 0_i64), |total, value| {
                (
                    total.0.saturating_add(value.0),
                    total.1.saturating_add(value.1),
                )
            });
        Ok(InstallPreview {
            plan,
            confirmation_token,
            affected_services: vec!["nginx".into(), "php-fpm".into(), "mysql".into()],
            packages: actions
                .iter()
                .map(|action| match action {
                    PlanAction::Install(package)
                    | PlanAction::Update(package)
                    | PlanAction::Remove(package) => package.as_str().to_owned(),
                })
                .collect(),
            estimated_download_bytes: Some(estimates.0),
            estimated_disk_delta_bytes: Some(estimates.1),
            configuration_paths: vec![
                format!("/etc/openpanel/software/{}", input.application),
                format!("/var/www/{}/public_html", input.domain),
            ],
            rollback_supported: true,
            dependency_counts: BTreeMap::new(),
            conflicts: Vec::new(),
        })
    }

    /// Execute one confirmed application transaction and return credentials once.
    pub async fn execute_deployment(
        &self,
        actor: Uuid,
        role: Role,
        plan_digest: &str,
        confirmation_token: &str,
    ) -> Result<ApplicationDeploymentResult, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        let _transaction = self.transaction.lock().await;
        let preview = self.take_preview(plan_digest, confirmation_token).await?;
        let input = match preview.kind {
            PreviewKind::Application(input) => input,
            PreviewKind::Component { .. } => {
                return Err(SoftwareCenterError::Invalid(
                    "invalid software request".into(),
                ));
            }
        };
        if self.packages.discover().await?.state_digest != preview.host_state_digest {
            return Err(SoftwareCenterError::Conflict);
        }
        let id = Uuid::new_v4();
        let durable_lock = self.acquire_durable_lock(id).await?;
        self.push_job(id, plan_digest, JobState::Running).await?;
        if let Err(error) = self.packages.apply(&preview.actions).await {
            self.push_job(id, plan_digest, JobState::Failed).await?;
            self.record_failure(actor, "package_failed", plan_digest)
                .await?;
            return Err(error);
        }
        if self.take_cancellation(id)? {
            let _ = self.packages.rollback(&preview.actions).await;
            let view = self.push_job(id, plan_digest, JobState::Cancelled).await?;
            durable_lock.release().await?;
            self.record(actor, "cancelled", plan_digest).await?;
            return Ok(ApplicationDeploymentResult {
                job: view,
                deployment_id: Uuid::nil(),
                admin_username: String::new(),
                admin_password: String::new(),
            });
        }
        let deployment = match self.applications.provision(actor, &input).await {
            Ok(value) => value,
            Err(error) => {
                let _ = self.packages.rollback(&preview.actions).await;
                self.push_job(id, plan_digest, JobState::Failed).await?;
                self.record_failure(actor, "deployment_failed", plan_digest)
                    .await?;
                return Err(error);
            }
        };
        if self.take_cancellation(id)? {
            let _ = self.applications.rollback(&deployment).await;
            let _ = self.packages.rollback(&preview.actions).await;
            let view = self.push_job(id, plan_digest, JobState::Cancelled).await?;
            durable_lock.release().await?;
            self.record(actor, "cancelled", plan_digest).await?;
            return Ok(ApplicationDeploymentResult {
                job: view,
                deployment_id: Uuid::nil(),
                admin_username: String::new(),
                admin_password: String::new(),
            });
        }
        self.push_job(id, plan_digest, JobState::Validating).await?;
        if let Err(error) = self.applications.validate(&deployment).await {
            let _ = self.applications.rollback(&deployment).await;
            let _ = self.packages.rollback(&preview.actions).await;
            self.push_job(id, plan_digest, JobState::Failed).await?;
            self.record_failure(actor, "deployment_failed", plan_digest)
                .await?;
            return Err(error);
        }
        if self.take_cancellation(id)? {
            let _ = self.applications.rollback(&deployment).await;
            let _ = self.packages.rollback(&preview.actions).await;
            let view = self.push_job(id, plan_digest, JobState::Cancelled).await?;
            durable_lock.release().await?;
            self.record(actor, "cancelled", plan_digest).await?;
            return Ok(ApplicationDeploymentResult {
                job: view,
                deployment_id: Uuid::nil(),
                admin_username: String::new(),
                admin_password: String::new(),
            });
        }
        let job = self.push_job(id, plan_digest, JobState::Succeeded).await?;
        self.persist_deployment(actor, &input, deployment.id)
            .await?;
        durable_lock.release().await?;
        self.record(actor, "deployed", plan_digest).await?;
        Ok(ApplicationDeploymentResult {
            job,
            deployment_id: deployment.id,
            admin_username: deployment.admin_username,
            admin_password: deployment.admin_password,
        })
    }

    async fn persist_plan(
        &self,
        plan_digest: &str,
        preview: &StoredPreview,
    ) -> Result<(), SoftwareCenterError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let payload = serde_json::to_string(&PersistedPreview {
            kind: preview.kind.clone(),
            actions: preview.actions.clone(),
        })
        .map_err(|_| SoftwareCenterError::Repository)?;
        sqlx::query("INSERT INTO software_plans(digest,payload,host_state_digest,token_hash,expires_at,consumed,created_at) VALUES(?,?,?,?,?,0,?) ON CONFLICT(digest) DO UPDATE SET payload=excluded.payload,host_state_digest=excluded.host_state_digest,token_hash=excluded.token_hash,expires_at=excluded.expires_at,consumed=0,created_at=excluded.created_at")
            .bind(plan_digest)
            .bind(payload)
            .bind(&preview.host_state_digest)
            .bind(token_hash(&preview.confirmation_token))
            .bind(preview.expires_at.to_string())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        Ok(())
    }

    async fn take_preview(
        &self,
        plan_digest: &str,
        confirmation_token: &str,
    ) -> Result<StoredPreview, SoftwareCenterError> {
        let memory = {
            let mut previews = self
                .previews
                .lock()
                .map_err(|_| SoftwareCenterError::Repository)?;
            match previews.get(plan_digest) {
                Some(preview)
                    if preview.confirmation_token == confirmation_token
                        && now()? <= preview.expires_at =>
                {
                    previews.remove(plan_digest)
                }
                Some(_) => return Err(SoftwareCenterError::Invalid("confirmation token is no longer valid for this plan; open the entry again and confirm the freshly generated plan".into())),
                None => None,
            }
        };
        let Some(pool) = &self.pool else {
            return memory.ok_or(SoftwareCenterError::Invalid("plan is no longer in the queue; it may have been consumed, expired, or never created".into()));
        };
        let row = sqlx::query_as::<_, (String, String, String, String, i64)>(
            "SELECT payload,host_state_digest,token_hash,expires_at,consumed FROM software_plans WHERE digest=?",
        )
        .bind(plan_digest)
        .fetch_optional(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?
        .ok_or(SoftwareCenterError::Invalid("plan is no longer in the queue; it may have been consumed, expired, or never created".into()))?;
        let expires_at = row
            .3
            .parse::<u64>()
            .map_err(|_| SoftwareCenterError::Repository)?;
        if row.4 != 0 || row.2 != token_hash(confirmation_token) || now()? > expires_at {
            return Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ));
        }
        let consumed = sqlx::query(
            "UPDATE software_plans SET consumed=1 WHERE digest=? AND consumed=0 AND token_hash=?",
        )
        .bind(plan_digest)
        .bind(token_hash(confirmation_token))
        .execute(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?;
        if consumed.rows_affected() != 1 {
            return Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ));
        }
        if let Some(preview) = memory {
            return Ok(preview);
        }
        let persisted: PersistedPreview =
            serde_json::from_str(&row.0).map_err(|_| SoftwareCenterError::Repository)?;
        Ok(StoredPreview {
            kind: persisted.kind,
            actions: persisted.actions,
            host_state_digest: row.1,
            confirmation_token: String::new(),
            expires_at,
        })
    }

    /// List bounded job projections for an Owner.
    pub async fn jobs(&self, role: Role) -> Result<Vec<SoftwareJobView>, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        if let Some(pool) = &self.pool {
            let rows = sqlx::query_as::<_, (String, String, String)>(
                "SELECT id,plan_digest,state FROM software_jobs ORDER BY created_at DESC LIMIT 200",
            )
            .fetch_all(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
            return rows
                .into_iter()
                .map(|(id, plan_digest, state)| {
                    Ok(SoftwareJobView {
                        id: Uuid::parse_str(&id).map_err(|_| SoftwareCenterError::Repository)?,
                        plan_digest,
                        state,
                    })
                })
                .collect();
        }
        self.jobs
            .lock()
            .map(|jobs| jobs.clone())
            .map_err(|_| SoftwareCenterError::Repository)
    }

    /// Aggregate catalog, ownership, and recovery state without commands or credentials.
    pub async fn diagnostics(
        &self,
        role: Role,
    ) -> Result<SoftwareDiagnostics, SoftwareCenterError> {
        owner(role)?;
        let catalog_entries = self.catalog(role).await?.len();
        let inventory = self.inventory(role).await?;
        let jobs = self.jobs(role).await?;
        Ok(SoftwareDiagnostics {
            catalog_entries,
            managed_components: inventory
                .iter()
                .filter(|entry| entry.state == "panel_managed")
                .count(),
            external_components: inventory
                .iter()
                .filter(|entry| entry.state == "externally_managed")
                .count(),
            active_jobs: jobs
                .iter()
                .filter(|job| matches!(job.state.as_str(), "queued" | "running" | "validating"))
                .count(),
            interrupted_jobs: jobs.iter().filter(|job| job.state == "interrupted").count(),
        })
    }

    /// Request cancellation; execution observes it at the next recipe-safe checkpoint.
    pub async fn cancel(
        &self,
        actor: Uuid,
        role: Role,
        job_id: Uuid,
    ) -> Result<SoftwareJobView, SoftwareCenterError> {
        owner(role)?;
        let job = self
            .jobs(role)
            .await?
            .into_iter()
            .find(|job| job.id == job_id)
            .ok_or(SoftwareCenterError::Invalid("plan is no longer in the queue; it may have been consumed, expired, or never created".into()))?;
        if !matches!(job.state.as_str(), "queued" | "running" | "validating") {
            return Err(SoftwareCenterError::Conflict);
        }
        self.cancellation_requests
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?
            .insert(job_id);
        self.record(actor, "cancellation_requested", &job.plan_digest)
            .await?;
        Ok(SoftwareJobView {
            state: "cancellation_pending".to_owned(),
            ..job
        })
    }

    /// Re-plan an interrupted transaction against current host state.
    pub async fn retry_preview(
        &self,
        actor: Uuid,
        role: Role,
        job_id: Uuid,
    ) -> Result<RetryPreview, SoftwareCenterError> {
        owner(role)?;
        let job = self
            .jobs(role)
            .await?
            .into_iter()
            .find(|job| job.id == job_id && job.state == "interrupted")
            .ok_or(SoftwareCenterError::Conflict)?;
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let (payload,): (String,) =
            sqlx::query_as("SELECT payload FROM software_plans WHERE digest=?")
                .bind(&job.plan_digest)
                .fetch_one(pool)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
        let persisted: PersistedPreview =
            serde_json::from_str(&payload).map_err(|_| SoftwareCenterError::Repository)?;
        let snapshot = self.packages.discover().await?;
        let confirmation_token = Uuid::new_v4().to_string();
        let stored = StoredPreview {
            kind: persisted.kind,
            actions: persisted.actions,
            host_state_digest: snapshot.state_digest,
            confirmation_token: confirmation_token.clone(),
            expires_at: now()?.checked_add(300).ok_or(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ))?,
        };
        self.persist_plan(&job.plan_digest, &stored).await?;
        self.previews
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?
            .insert(job.plan_digest.clone(), stored.clone());
        self.record(actor, "retry_previewed", &job.plan_digest)
            .await?;
        Ok(RetryPreview {
            plan_digest: job.plan_digest,
            confirmation_token,
            packages: stored
                .actions
                .iter()
                .map(|action| match action {
                    PlanAction::Install(package)
                    | PlanAction::Update(package)
                    | PlanAction::Remove(package) => package.as_str().to_owned(),
                })
                .collect(),
        })
    }

    /// Roll back an interrupted component install when its recipe is reversible.
    pub async fn rollback_interrupted(
        &self,
        actor: Uuid,
        role: Role,
        job_id: Uuid,
    ) -> Result<SoftwareJobView, SoftwareCenterError> {
        owner(role)?;
        let job = self
            .jobs(role)
            .await?
            .into_iter()
            .find(|job| job.id == job_id && job.state == "interrupted")
            .ok_or(SoftwareCenterError::Conflict)?;
        let pool = self.pool.as_ref().ok_or(SoftwareCenterError::Repository)?;
        let (payload,): (String,) =
            sqlx::query_as("SELECT payload FROM software_plans WHERE digest=?")
                .bind(&job.plan_digest)
                .fetch_one(pool)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
        let persisted: PersistedPreview =
            serde_json::from_str(&payload).map_err(|_| SoftwareCenterError::Repository)?;
        if !matches!(
            persisted.kind,
            PreviewKind::Component {
                action: ComponentAction::Install,
                ..
            }
        ) || persisted
            .actions
            .iter()
            .any(|action| !matches!(action, PlanAction::Install(_)))
        {
            return Err(SoftwareCenterError::Unsupported);
        }
        let _transaction = self.transaction.lock().await;
        let durable_lock = self.acquire_durable_lock(job_id).await?;
        self.push_job(job_id, &job.plan_digest, JobState::RollingBack)
            .await?;
        if let Err(error) = self.packages.rollback(&persisted.actions).await {
            self.push_job(job_id, &job.plan_digest, JobState::Failed)
                .await?;
            self.record_failure(actor, "rollback_failed", &job.plan_digest)
                .await?;
            return Err(error);
        }
        let completed = self
            .push_job(job_id, &job.plan_digest, JobState::RolledBack)
            .await?;
        durable_lock.release().await?;
        self.record(actor, "rolled_back", &job.plan_digest).await?;
        Ok(completed)
    }

    fn take_cancellation(&self, job_id: Uuid) -> Result<bool, SoftwareCenterError> {
        Ok(self
            .cancellation_requests
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?
            .remove(&job_id))
    }

    async fn push_job(
        &self,
        id: Uuid,
        plan_digest: &str,
        state: JobState,
    ) -> Result<SoftwareJobView, SoftwareCenterError> {
        let view = SoftwareJobView {
            id,
            plan_digest: plan_digest.to_owned(),
            state: state_label(state).to_owned(),
        };
        if let Some(pool) = &self.pool {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO software_jobs(id,plan_digest,component_id,state,events_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET state=excluded.state,updated_at=excluded.updated_at")
                .bind(id.to_string())
                .bind(plan_digest)
                .bind("")
                .bind(&view.state)
                .bind("[]")
                .bind(&now)
                .bind(&now)
                .execute(pool)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
        }
        let mut jobs = self
            .jobs
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?;
        if let Some(existing) = jobs.iter_mut().find(|job| job.id == id) {
            *existing = view.clone();
        } else {
            jobs.push(view.clone());
        }
        Ok(view)
    }

    async fn managed_components(&self) -> Result<BTreeSet<String>, SoftwareCenterError> {
        let mut managed = self
            .owned_components
            .lock()
            .map_err(|_| SoftwareCenterError::Repository)?
            .clone();
        if let Some(pool) = &self.pool {
            let rows = sqlx::query_scalar::<_, String>(
                "SELECT id FROM software_components WHERE managed=1",
            )
            .fetch_all(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
            managed.extend(rows);
        }
        Ok(managed)
    }

    async fn persist_deployment(
        &self,
        owner_id: Uuid,
        input: &ApplicationDeploymentInput,
        deployment_id: Uuid,
    ) -> Result<(), SoftwareCenterError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let version = recovery_catalog()?
            .into_iter()
            .find(|entry| entry.id == input.application)
            .and_then(|entry| entry.versions.into_iter().next())
            .ok_or(SoftwareCenterError::Invalid("plan is no longer in the queue; it may have been consumed, expired, or never created".into()))?;
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO software_deployments(id,owner_id,application_id,version,site_id,database_id,state,created_at,updated_at) VALUES(?,?,?,?,NULL,NULL,'healthy',?,?)")
            .bind(deployment_id.to_string())
            .bind(owner_id.to_string())
            .bind(&input.application)
            .bind(version)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await
            .map_err(|_| SoftwareCenterError::Repository)?;
        Ok(())
    }

    async fn acquire_durable_lock(
        &self,
        job_id: Uuid,
    ) -> Result<DurableTransactionLock, SoftwareCenterError> {
        let Some(pool) = &self.pool else {
            return Ok(DurableTransactionLock { pool: None, job_id });
        };
        let result = sqlx::query(
            "INSERT INTO software_transaction_lock(singleton,job_id,owner_pid,acquired_at) VALUES(1,?,?,?) ON CONFLICT(singleton) DO NOTHING",
        )
        .bind(job_id.to_string())
        .bind(i64::from(std::process::id()))
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .map_err(|_| SoftwareCenterError::Repository)?;
        if result.rows_affected() != 1 {
            return Err(SoftwareCenterError::Conflict);
        }
        Ok(DurableTransactionLock {
            pool: Some(pool.clone()),
            job_id,
        })
    }

    async fn dependency_count(&self, component: &str) -> Result<u64, SoftwareCenterError> {
        let Some(pool) = &self.pool else {
            return Ok(0);
        };
        let count = match component {
            "nginx" => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sites")
                    .fetch_one(pool)
                    .await
            }
            "php-8.3" => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM sites WHERE php_enabled=1 AND php_version='8.3'",
                )
                .fetch_one(pool)
                .await
            }
            "php-8.4" => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM sites WHERE php_enabled=1 AND php_version='8.4'",
                )
                .fetch_one(pool)
                .await
            }
            "mysql" | "mariadb" => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM databases")
                    .fetch_one(pool)
                    .await
            }
            _ => Ok(0),
        }
        .map_err(|_| SoftwareCenterError::Repository)?;
        u64::try_from(count).map_err(|_| SoftwareCenterError::Repository)
    }

    async fn ensure_reconciled(&self) -> Result<(), SoftwareCenterError> {
        self.reconciled
            .get_or_try_init(|| async {
                if let Some(pool) = &self.pool {
                    let lock = sqlx::query_as::<_, (String, i64)>(
                        "SELECT job_id,owner_pid FROM software_transaction_lock WHERE singleton=1",
                    )
                    .fetch_optional(pool)
                    .await
                    .map_err(|_| SoftwareCenterError::Repository)?;
                    if lock.as_ref().is_some_and(|(_, pid)| {
                        std::path::Path::new(&format!("/proc/{pid}")).exists()
                    }) {
                        return Ok(());
                    }
                    sqlx::query("UPDATE software_jobs SET state='interrupted',updated_at=? WHERE state IN ('queued','running','validating','rolling_back')")
                        .bind(chrono::Utc::now().to_rfc3339())
                        .execute(pool)
                        .await
                        .map_err(|_| SoftwareCenterError::Repository)?;
                    sqlx::query("DELETE FROM software_transaction_lock WHERE singleton=1")
                        .execute(pool)
                        .await
                        .map_err(|_| SoftwareCenterError::Repository)?;
                }
                Ok(())
            })
            .await
            .map(|_| ())
    }

    async fn set_managed(
        &self,
        component: &str,
        managed: bool,
        state_digest: &str,
    ) -> Result<(), SoftwareCenterError> {
        {
            let mut owned = self
                .owned_components
                .lock()
                .map_err(|_| SoftwareCenterError::Repository)?;
            if managed {
                owned.insert(component.to_owned());
            } else {
                owned.remove(component);
            }
        }
        if let Some(pool) = &self.pool {
            sqlx::query("INSERT INTO software_components(id,version,status,managed,state_digest,updated_at) VALUES(?,NULL,?,?,?,?) ON CONFLICT(id) DO UPDATE SET status=excluded.status,managed=excluded.managed,state_digest=excluded.state_digest,updated_at=excluded.updated_at")
                .bind(component)
                .bind(if managed { "installed" } else { "available" })
                .bind(if managed { 1_i64 } else { 0_i64 })
                .bind(state_digest)
                .bind(chrono::Utc::now().to_rfc3339())
                .execute(pool)
                .await
                .map_err(|_| SoftwareCenterError::Repository)?;
        }
        Ok(())
    }

    async fn record(
        &self,
        actor: Uuid,
        operation: &str,
        plan_digest: &str,
    ) -> Result<(), SoftwareCenterError> {
        self.record_outcome(actor, operation, plan_digest, AuditOutcome::Success)
            .await
    }

    async fn record_failure(
        &self,
        actor: Uuid,
        operation: &str,
        plan_digest: &str,
    ) -> Result<(), SoftwareCenterError> {
        self.record_outcome(actor, operation, plan_digest, AuditOutcome::Failure)
            .await
    }

    async fn record_outcome(
        &self,
        actor: Uuid,
        operation: &str,
        plan_digest: &str,
        outcome: AuditOutcome,
    ) -> Result<(), SoftwareCenterError> {
        self.audit
            .record(
                AuditEvent::new(actor.to_string(), AuditAction::SoftwareChanged, outcome)
                    .target(plan_digest)
                    .metadata(serde_json::json!({"operation":operation})),
            )
            .await
            .map_err(|_| SoftwareCenterError::Repository)
    }
}

fn recovery_catalog() -> Result<Vec<CatalogEntry>, SoftwareCenterError> {
    let platforms = supported_platforms()?;
    let component = |id: &str,
                     name: &str,
                     description: &str,
                     license: &str,
                     versions: &[&str],
                     packages: &[&str]|
     -> Result<CatalogEntry, SoftwareCenterError> {
        Ok(CatalogEntry {
            id: id.to_owned(),
            name: name.to_owned(),
            description: description.to_owned(),
            category: match id {
                "php-8.3" | "php-8.4" => "Runtimes",
                "mysql" | "mariadb" => "Databases",
                "redis" => "Caching",
                _ => "Web stack",
            }
            .to_owned(),
            kind: CatalogKind::SystemComponent,
            license: license.to_owned(),
            versions: versions.iter().map(|value| (*value).to_owned()).collect(),
            platforms: match id {
                "php-8.3" => platforms
                    .iter()
                    .filter(|platform| {
                        platform.distribution() == "ubuntu" && platform.release() == "24.04"
                    })
                    .cloned()
                    .collect(),
                "php-8.4" => Vec::new(),
                _ => platforms.clone(),
            },
            dependencies: Vec::new(),
            provenance: "OpenPanel embedded recovery catalog".to_owned(),
            packages: packages
                .iter()
                .map(PackageId::new)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?,
            lifecycle_state: "discoverable".to_owned(),
        })
    };
    let application = |id: &str,
                       name: &str,
                       description: &str,
                       version: &str,
                       dependencies: &[&str]|
     -> CatalogEntry {
        CatalogEntry {
            id: id.to_owned(),
            name: name.to_owned(),
            description: description.to_owned(),
            category: "Content management".to_owned(),
            kind: CatalogKind::WebApplication,
            license: "GPL-2.0-or-later".to_owned(),
            versions: vec![version.to_owned()],
            platforms: platforms.clone(),
            dependencies: dependencies
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            provenance: "OpenPanel embedded recovery catalog".to_owned(),
            packages: Vec::new(),
            lifecycle_state: "available".to_owned(),
        }
    };
    Ok(vec![
        component(
            "nginx",
            "Nginx",
            "High-performance HTTP and reverse proxy server",
            "BSD-2-Clause",
            &["1.x"],
            &["nginx"],
        )?,
        component(
            "php-8.3",
            "PHP 8.3",
            "PHP-FPM runtime with common CMS extensions",
            "PHP-3.01",
            &["8.3"],
            &[
                "php8.3-fpm",
                "php8.3-cli",
                "php8.3-curl",
                "php8.3-gd",
                "php8.3-intl",
                "php8.3-mbstring",
                "php8.3-mysql",
                "php8.3-xml",
                "php8.3-zip",
            ],
        )?,
        component(
            "php-8.4",
            "PHP 8.4",
            "PHP-FPM runtime with common CMS extensions",
            "PHP-3.01",
            &["8.4"],
            &[
                "php8.4-fpm",
                "php8.4-cli",
                "php8.4-curl",
                "php8.4-gd",
                "php8.4-intl",
                "php8.4-mbstring",
                "php8.4-mysql",
                "php8.4-xml",
                "php8.4-zip",
            ],
        )?,
        component(
            "mysql",
            "MySQL",
            "MySQL relational database server",
            "GPL-2.0-only",
            &["8.0"],
            &["mysql-server"],
        )?,
        component(
            "mariadb",
            "MariaDB",
            "MariaDB relational database server",
            "GPL-2.0-only",
            &["10.x", "11.x"],
            &["mariadb-server"],
        )?,
        component(
            "redis",
            "Redis",
            "In-memory cache and data store",
            "RSALv2/SSPLv1",
            &["7.x"],
            &["redis-server"],
        )?,
        application(
            "wordpress",
            "WordPress",
            "Pinned WordPress publishing application",
            "7.0.3",
            &["nginx", "php", "database"],
        ),
        application(
            "drupal",
            "Drupal",
            "Pinned Drupal content-management application",
            "11.3.12",
            &["nginx", "php", "database"],
        ),
    ])
}

/// Project a normalized `StorefrontEntry` to the legacy `CatalogEntry`
/// shape used by the existing API and CLI. The aggregator store is the
/// source of truth; this projection exists only for backward compatibility.
fn project_entry(entry: &StorefrontEntry, _applications_enabled: bool) -> CatalogEntry {
    let kind = match entry.kind {
        openpanel_domain::software_center::EntryKind::System => CatalogKind::SystemComponent,
        openpanel_domain::software_center::EntryKind::Web => CatalogKind::WebApplication,
        openpanel_domain::software_center::EntryKind::Tool => CatalogKind::Tool,
    };
    let latest = entry
        .versions
        .iter()
        .find(|version| version.is_latest)
        .or(entry.versions.first())
        .cloned()
        .unwrap_or_else(|| StorefrontVersion {
            version: String::new(),
            size_bytes: 0,
            supports_php: Vec::new(),
            released_at: None,
            changelog_url: None,
            is_latest: true,
            packages: Vec::new(),
            artifact: None,
        });
    let packages: Vec<PackageId> = latest
        .packages
        .iter()
        .map(|name| {
            PackageId::new(name)
                .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    CatalogEntry {
        id: entry.id.clone(),
        name: entry.name.clone(),
        description: entry.description.clone(),
        category: entry.category.label().to_owned(),
        kind,
        license: entry.license.clone(),
        versions: entry.versions.iter().map(|v| v.version.clone()).collect(),
        platforms: Vec::new(),
        dependencies: entry.dependencies.clone(),
        provenance: entry.provenance.source_url.clone(),
        packages,
        lifecycle_state: "discoverable".to_owned(),
    }
}

fn component_estimate(component: &str) -> (u64, i64) {
    const MIB: u64 = 1024 * 1024;
    let (download, installed) = match component {
        "nginx" => (4, 14),
        "php-8.3" | "php-8.4" => (32, 118),
        "mysql" | "mariadb" => (42, 190),
        "redis" => (2, 7),
        _ => (0, 0),
    };
    (download * MIB, (installed * MIB) as i64)
}

fn supported_platforms() -> Result<Vec<SupportedPlatform>, SoftwareCenterError> {
    [
        ("ubuntu", "22.04", "x86_64"),
        ("ubuntu", "22.04", "aarch64"),
        ("ubuntu", "24.04", "x86_64"),
        ("ubuntu", "24.04", "aarch64"),
        ("debian", "12", "x86_64"),
        ("debian", "12", "aarch64"),
    ]
    .into_iter()
    .map(|(distribution, release, architecture)| {
        SupportedPlatform::new(distribution, release, architecture)
            .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))
    })
    .collect()
}

/// Default managed base directory for downloaded web application
/// artifacts. Each install drops its files under
/// `{webapps_root}/{entry_id}/{version}/`.
fn default_webapps_root() -> PathBuf {
    PathBuf::from("/var/lib/openpanel/webapps")
}

fn owner(role: Role) -> Result<(), SoftwareCenterError> {
    if role == Role::Owner {
        Ok(())
    } else {
        Err(SoftwareCenterError::Forbidden)
    }
}

fn validate_deployment_input(
    input: &ApplicationDeploymentInput,
) -> Result<(), SoftwareCenterError> {
    if !matches!(input.application.as_str(), "wordpress" | "drupal")
        || !matches!(input.php_version.as_str(), "8.3" | "8.4")
        || input.locale.is_empty()
        || input.locale.len() > 32
        || !input
            .locale
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(SoftwareCenterError::Invalid(
            "invalid software request".into(),
        ));
    }
    let labels: Vec<_> = input.domain.split('.').collect();
    if input.domain.len() > 253
        || labels.len() < 2
        || labels.iter().any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
    {
        return Err(SoftwareCenterError::Invalid(
            "invalid software request".into(),
        ));
    }
    Ok(())
}

fn now() -> Result<u64, SoftwareCenterError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| SoftwareCenterError::Repository)
}

fn token_hash(token: &str) -> String {
    hex::encode(sha2::Sha256::digest(token.as_bytes()))
}

fn state_label(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Validating => "validating",
        JobState::RollingBack => "rolling_back",
        JobState::Succeeded => "succeeded",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
        JobState::RolledBack => "rolled_back",
        JobState::Interrupted => "interrupted",
    }
}

// ============================================================================
// Aggregator API — exposed by the new `store` and `source` modules.
// ============================================================================
impl SoftwareCenterService {
    /// Lazy materialization of the embedded seed. Called by the public
    /// search/entry/diagnostics methods so that callers don't have to wait
    /// for migrations or the embedded bootstrap to complete up front.
    async fn ensure_seed_materialized(&self) {
        let embedded = EmbeddedCatalogSource::new(default_catalog_url());
        let _ = self.store.materialize_seed_if_empty(&embedded).await;
    }

    /// Search the active catalog snapshot.
    pub async fn search(
        &self,
        role: Role,
        query: CatalogQuery,
    ) -> Result<CatalogSearchPage, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        self.ensure_seed_materialized().await;
        self.store.search(&query).await
    }

    /// Fetch a single entry with versions, tags, dependencies, conflicts,
    /// and provenance.
    pub async fn entry(
        &self,
        role: Role,
        id: &str,
    ) -> Result<Option<StorefrontEntry>, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        self.ensure_seed_materialized().await;
        self.store.get_entry(id).await
    }

    /// Refresh the active snapshot from the configured source. On failure
    /// the active snapshot is unchanged.
    pub async fn refresh_catalog(
        &self,
        actor: Uuid,
        role: Role,
    ) -> Result<RefreshOutcome, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        let source_id = self.catalog_source.source_id();
        let source_url = self.catalog_source.source_url().to_owned();
        let now_unix = now()?;
        match self.catalog_source.fetch(now_unix).await {
            Ok(fetched) => {
                let activation = self.store.activate(source_id, &fetched).await?;
                self.store
                    .record_refresh(
                        &source_url,
                        "success",
                        Some(&activation.manifest_digest),
                        None,
                    )
                    .await?;
                self.record_outcome(
                    actor,
                    "catalog_refreshed",
                    &activation.manifest_digest,
                    AuditOutcome::Success,
                )
                .await?;
                Ok(RefreshOutcome {
                    manifest_digest: activation.manifest_digest,
                    entry_count: activation.entry_count,
                    source_url: activation.source_url,
                    activated_at: activation.activated_at,
                })
            }
            Err(error) => {
                let error_text = error.to_string();
                self.store
                    .record_refresh(&source_url, "failure", None, Some(&error_text))
                    .await?;
                self.record_outcome(
                    actor,
                    "catalog_rejected",
                    &error_text,
                    AuditOutcome::Failure,
                )
                .await?;
                Err(error)
            }
        }
    }

    /// Build diagnostics for the diagnostics strip / `software diagnostics` CLI.
    pub async fn catalog_diagnostics(
        &self,
        role: Role,
    ) -> Result<CatalogDiagnostics, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        self.ensure_seed_materialized().await;
        self.store.diagnostics().await
    }

    /// Compute the pre-flight compatibility report for an entry/version
    /// combination against a host context.
    pub async fn compatibility(
        &self,
        role: Role,
        entry_id: &str,
        version: &str,
        host: CompatibilityHost,
        managed_php_versions: Vec<String>,
    ) -> Result<CompatibilityReport, SoftwareCenterError> {
        owner(role)?;
        self.ensure_reconciled().await?;
        let snapshot = self.packages.discover().await?;
        let mut installed_components: Vec<(String, bool)> = snapshot
            .installed_packages
            .iter()
            .map(|package| (package.clone(), false))
            .collect();
        for id in self.managed_components().await? {
            installed_components.push((id, true));
        }
        self.store
            .compatibility_for(
                entry_id,
                version,
                &host,
                &managed_php_versions,
                &installed_components,
            )
            .await
    }
}
