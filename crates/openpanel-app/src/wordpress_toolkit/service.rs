//! WordPress toolkit services: scanner, updater with rollback,
//! cache layer, and the top-level service.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, User, WpCacheMode, WpError, WpRepository, WpSecurityFinding, WpSecurityReport, WpSite,
    WpUpdateResult, WpUpdateSet, compare_versions,
};
use uuid::Uuid;

use crate::wordpress_toolkit::SqliteWpRepository;

/// Filesystem port used by the updater / cache layer. The
/// production implementation walks the real disk; tests use
/// `FakeWpFilesystem`.
pub trait WpFilesystem: Send + Sync + 'static {
    /// Snapshot the wp_root into `dest`. The snapshot is
    /// reference-counted; `restore` reverses it.
    fn snapshot(&self, wp_root: &str, dest: &str) -> Result<(), WpError>;
    /// Restore a snapshot to `wp_root`.
    fn restore(&self, dest: &str, wp_root: &str) -> Result<(), WpError>;
    /// Drop a snapshot.
    fn drop_snapshot(&self, dest: &str) -> Result<(), WpError>;
}

/// In-memory filesystem used by tests. The fake keeps a
/// `Vec<String>` of "files" for each path; `snapshot` records
/// the current set under a new key, `restore` swaps the set back.
pub struct FakeWpFilesystem {
    snapshots: std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>,
    files: std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>,
}

impl FakeWpFilesystem {
    /// Construct a fake with a starting set of files under `wp_root`.
    pub fn new(wp_root: impl Into<String>, files: Vec<String>) -> Self {
        let mut files_map = std::collections::HashMap::new();
        files_map.insert(wp_root.into(), files);
        Self {
            snapshots: std::sync::Mutex::new(std::collections::HashMap::new()),
            files: std::sync::Mutex::new(files_map),
        }
    }
}

impl WpFilesystem for FakeWpFilesystem {
    fn snapshot(&self, wp_root: &str, dest: &str) -> Result<(), WpError> {
        let files = self.files.lock().expect("files");
        let snapshot = files.get(wp_root).cloned().unwrap_or_default();
        drop(files);
        self.snapshots
            .lock()
            .expect("snapshots")
            .insert(dest.to_string(), snapshot);
        Ok(())
    }

    fn restore(&self, dest: &str, wp_root: &str) -> Result<(), WpError> {
        let snapshot = self
            .snapshots
            .lock()
            .expect("snapshots")
            .remove(dest)
            .ok_or_else(|| WpError::UpdateFailedRolledBack(format!("missing snapshot {dest}")))?;
        self.files
            .lock()
            .expect("files")
            .insert(wp_root.to_string(), snapshot);
        Ok(())
    }

    fn drop_snapshot(&self, dest: &str) -> Result<(), WpError> {
        self.snapshots
            .lock()
            .expect("snapshots")
            .remove(dest);
        Ok(())
    }
}

/// Scanner: produces a security report for one site.
pub struct WpScanner {
    repo: Arc<SqliteWpRepository>,
    audit: Arc<dyn AuditService>,
}

impl WpScanner {
    /// Construct a scanner.
    pub fn new(repo: Arc<SqliteWpRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Run a scan and return a `WpSecurityReport`. The fixture
    /// implementation looks at the site's `core_version` and
    /// reports it as `info` when current, `warn` when outdated,
    /// `cve` when the version is below a hard-coded CVE
    /// threshold.
    pub async fn scan(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<WpSecurityReport, WpError> {
        require_admin(caller)?;
        let site = self
            .repo
            .get_site(site_id)
            .await?
            .ok_or(WpError::NotWordpress)?;
        let mut findings = Vec::new();
        let severity = if compare_versions(&site.core_version, "6.0.0").is_lt() {
            "cve"
        } else if compare_versions(&site.core_version, "6.4.0").is_lt() {
            "warn"
        } else {
            "info"
        };
        findings.push(WpSecurityFinding {
            id: Uuid::new_v4(),
            site_id,
            component: "core".into(),
            installed_version: site.core_version.clone(),
            severity: severity.into(),
            summary: format!("WordPress core version is {}", site.core_version),
            detected_at: Utc::now(),
        });
        let report = WpSecurityReport {
            site_id,
            generated_at: Utc::now(),
            findings,
        };
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::WpScanned,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "findings": report.findings.len(),
                })),
            )
            .await;
        Ok(report)
    }
}

/// Updater: applies a set of `WpUpdateSet` rows; on failure,
/// restores the snapshot recorded before the run.
pub struct WpUpdater {
    repo: Arc<SqliteWpRepository>,
    fs: Arc<dyn WpFilesystem>,
    audit: Arc<dyn AuditService>,
}

impl WpUpdater {
    /// Construct an updater.
    pub fn new(
        repo: Arc<SqliteWpRepository>,
        fs: Arc<dyn WpFilesystem>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self { repo, fs, audit }
    }

    /// Apply `updates` to `site_id`. The updater takes a
    /// snapshot before the run; on failure the snapshot is
    /// restored and the run is recorded as `rolled_back`.
    pub async fn apply(
        &self,
        caller: &User,
        site_id: Uuid,
        updates: Vec<WpUpdateSet>,
    ) -> Result<WpUpdateResult, WpError> {
        require_admin(caller)?;
        let mut site = self
            .repo
            .get_site(site_id)
            .await?
            .ok_or(WpError::NotWordpress)?;
        let snapshot_db = format!("snapshots/{}", Uuid::new_v4());
        let snapshot_files = format!("snapshots/{}-files", Uuid::new_v4());
        self.fs
            .snapshot(&site.wp_root, &snapshot_files)
            .map_err(|e| WpError::UpdateFailedRolledBack(e.to_string()))?;
        site.last_snapshot_db = Some(snapshot_db.clone());
        site.last_snapshot_files = Some(snapshot_files.clone());
        self.repo.save_site(&site).await?;
        let started_at = Utc::now();
        let mut success = true;
        let mut message = String::new();
        for update in &updates {
            if compare_versions(&update.from_version, &update.to_version).is_ge() {
                success = false;
                message = format!(
                    "refusing downgrade from {} to {}",
                    update.from_version, update.to_version
                );
                break;
            }
        }
        if success {
            for update in &updates {
                site.core_version = update.to_version.clone();
            }
            self.repo.save_site(&site).await?;
        } else {
            self.fs
                .restore(&snapshot_files, &site.wp_root)
                .map_err(|e| WpError::UpdateFailedRolledBack(e.to_string()))?;
        }
        let _ = self.fs.drop_snapshot(&snapshot_files);
        let result = WpUpdateResult {
            id: Uuid::new_v4(),
            site_id,
            started_at,
            completed_at: Some(Utc::now()),
            updates,
            success,
            rolled_back: !success,
            message,
        };
        self.repo.save_update_run(&result).await?;
        let action = if result.success {
            AuditAction::WpUpdated
        } else {
            AuditAction::WpUpdateRolledBack
        };
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    action,
                    if result.success {
                        AuditOutcome::Success
                    } else {
                        AuditOutcome::Failure
                    },
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "updates": result.updates.len(),
                    "rolled_back": result.rolled_back,
                })),
            )
            .await;
        Ok(result)
    }
}

/// Cache layer: toggles the `WpCacheMode` for a site.
pub struct WpCacheLayer {
    repo: Arc<SqliteWpRepository>,
    audit: Arc<dyn AuditService>,
}

impl WpCacheLayer {
    /// Construct a cache layer.
    pub fn new(repo: Arc<SqliteWpRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Set the cache mode.
    pub async fn set(
        &self,
        caller: &User,
        site_id: Uuid,
        mode: WpCacheMode,
    ) -> Result<WpSite, WpError> {
        require_admin(caller)?;
        let mut site = self
            .repo
            .get_site(site_id)
            .await?
            .ok_or(WpError::NotWordpress)?;
        site.cache_mode = mode;
        self.repo.save_site(&site).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::WpCacheChanged,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({ "cache_mode": mode.as_str() })),
            )
            .await;
        Ok(site)
    }
}

/// Top-level façade.
pub struct WpToolkitService {
    scanner: WpScanner,
    updater: WpUpdater,
    cache: WpCacheLayer,
}

impl WpToolkitService {
    /// Construct the façade.
    pub fn new(scanner: WpScanner, updater: WpUpdater, cache: WpCacheLayer) -> Self {
        Self {
            scanner,
            updater,
            cache,
        }
    }

    /// Register a managed WP site.
    pub async fn register(
        &self,
        caller: &User,
        site: WpSite,
    ) -> Result<WpSite, WpError> {
        require_admin(caller)?;
        self.scanner.repo.save_site(&site).await?;
        Ok(site)
    }

    /// Forward to the scanner.
    pub async fn scan(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<WpSecurityReport, WpError> {
        self.scanner.scan(caller, site_id).await
    }

    /// Forward to the updater.
    pub async fn apply(
        &self,
        caller: &User,
        site_id: Uuid,
        updates: Vec<WpUpdateSet>,
    ) -> Result<WpUpdateResult, WpError> {
        self.updater.apply(caller, site_id, updates).await
    }

    /// Forward to the cache layer.
    pub async fn set_cache(
        &self,
        caller: &User,
        site_id: Uuid,
        mode: WpCacheMode,
    ) -> Result<WpSite, WpError> {
        self.cache.set(caller, site_id, mode).await
    }
}

fn require_admin(caller: &User) -> Result<(), WpError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(WpError::Forbidden),
    }
}
