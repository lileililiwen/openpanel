//! Server snapshot application service: bundles a verified backup
//! run into a manifest-described snapshot, runs preflight, and gates
//! restore behind a single-use confirmation token. Resource
//! application delegates to the existing, tested backup-restore
//! pipeline.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    RepoError, Role, User,
    backups::snapshot::{
        ConfirmToken, PreflightReport, SnapshotEntry, SnapshotEntryKind, SnapshotError,
        SnapshotManifest,
    },
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::backups::service::{BackupService, RestoreInput};

/// Errors surfaced by the server-snapshot service.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerSnapshotError {
    /// The caller is not allowed to manage snapshots.
    #[error("forbidden")]
    Forbidden,
    /// The snapshot or run does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// Manifest or input validation failed.
    #[error("{0}")]
    Validation(String),
    /// Restore attempted without preflight confirmation.
    #[error("{0}")]
    Confirmation(SnapshotError),
    /// Restore aborted partway; `applied` lists the resources that
    /// were processed before the failure.
    #[error("restore aborted at `{failed}` after applying {} resource(s)", applied.len())]
    Partial {
        /// Resources applied before the abort, in order.
        applied: Vec<String>,
        /// The entry whose processing failed.
        failed: String,
    },
    /// Filesystem or persistence failure.
    #[error("snapshot io failure: {0}")]
    Failure(String),
}

impl From<SnapshotError> for ServerSnapshotError {
    fn from(error: SnapshotError) -> Self {
        match error {
            SnapshotError::ConfirmationRequired | SnapshotError::TokenMismatch => {
                ServerSnapshotError::Confirmation(error)
            }
            other => ServerSnapshotError::Validation(other.to_string()),
        }
    }
}

/// Owner-only server snapshot orchestration.
pub struct ServerSnapshotService {
    backups: Arc<BackupService>,
    pool: sqlx::Pool<sqlx::Sqlite>,
    root: PathBuf,
    audit: Arc<dyn AuditService>,
    host_id: Uuid,
    panel_version: String,
    tokens: Mutex<HashMap<Uuid, ConfirmToken>>,
}

impl ServerSnapshotService {
    /// Construct over the backup service and a snapshots root
    /// directory. A stable host id is created on first use.
    pub fn new(
        backups: Arc<BackupService>,
        pool: sqlx::Pool<sqlx::Sqlite>,
        root: PathBuf,
        audit: Arc<dyn AuditService>,
        panel_version: String,
    ) -> Self {
        std::fs::create_dir_all(&root).ok();
        let host_id = match std::fs::read_to_string(root.join("host-id")) {
            Ok(id) => Uuid::parse_str(id.trim()).unwrap_or_else(|_| Uuid::new_v4()),
            Err(_) => Uuid::new_v4(),
        };
        std::fs::write(root.join("host-id"), host_id.to_string()).ok();
        Self {
            backups,
            pool,
            root,
            audit,
            host_id,
            panel_version,
            tokens: Mutex::new(HashMap::new()),
        }
    }

    /// Bundle a verified backup run into a server snapshot.
    pub async fn create_from_run(
        &self,
        caller: &User,
        run_id: Uuid,
    ) -> Result<SnapshotManifest, ServerSnapshotError> {
        self.require_owner(caller)?;
        let run = self
            .backups
            .verify(caller.id(), false, run_id)
            .await
            .map_err(|error| match error {
                crate::backups::BackupServiceError::NotFound => {
                    ServerSnapshotError::NotFound(run_id.to_string())
                }
                crate::backups::BackupServiceError::Corrupt => {
                    ServerSnapshotError::Validation("backup run is corrupt".into())
                }
                other => ServerSnapshotError::Failure(other.to_string()),
            })?;

        let run_dir = self.backups_root_runs().join(run_id.to_string());
        let snapshot_id = Uuid::new_v4();
        let snapshot_dir = self.root.join(snapshot_id.to_string());
        tokio::fs::create_dir_all(&snapshot_dir)
            .await
            .map_err(io_error)?;

        let mut entries = Vec::new();
        for artifact in run.artifacts() {
            let source = run_dir.join(artifact.path());
            let bytes = tokio::fs::read(&source)
                .await
                .map_err(|error| ServerSnapshotError::Failure(error.to_string()))?;
            let destination = snapshot_dir.join(artifact.path());
            if let Some(parent) = destination.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(io_error)?;
            }
            tokio::fs::write(&destination, &bytes)
                .await
                .map_err(io_error)?;
            entries.push(SnapshotEntry {
                kind: kind_for(artifact.resource()),
                reference: reference_for(artifact.resource()),
                path: artifact.path().to_owned(),
                sha256: hex_sha256(&bytes),
            });
        }

        // Panel metadata entry describing the source run linkage.
        entries.push(SnapshotEntry {
            kind: SnapshotEntryKind::PanelMetadata,
            reference: Some(run_id.to_string()),
            path: "meta/panel.json".into(),
            sha256: hex_sha256(run_id.to_string().as_bytes()),
        });
        let meta_dir = snapshot_dir.join("meta");
        tokio::fs::create_dir_all(&meta_dir)
            .await
            .map_err(io_error)?;
        tokio::fs::write(meta_dir.join("panel.json"), run_id.to_string())
            .await
            .map_err(io_error)?;

        let manifest = SnapshotManifest::new(
            Utc::now(),
            self.host_id,
            self.panel_version.clone(),
            entries,
        )?;
        let manifest_path = snapshot_dir.join("manifest.json");
        tokio::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest)
                .map_err(|e| ServerSnapshotError::Failure(e.to_string()))?,
        )
        .await
        .map_err(io_error)?;

        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SnapshotCreated,
                    AuditOutcome::Success,
                )
                .target(snapshot_id.to_string())
                .metadata(serde_json::json!({ "entries": manifest.entries.len() })),
            )
            .await;
        Ok(manifest)
    }

    /// List available snapshot manifests.
    pub async fn list(&self) -> Result<Vec<(Uuid, SnapshotManifest)>, ServerSnapshotError> {
        let mut out = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&self.root).await.map_err(io_error)?;
        while let Some(entry) = read_dir.next_entry().await.map_err(io_error)? {
            let path = entry.path();
            let manifest_path = path.join("manifest.json");
            if !path.is_dir() || !manifest_path.exists() {
                continue;
            }
            let raw = tokio::fs::read_to_string(&manifest_path)
                .await
                .map_err(io_error)?;
            let manifest: SnapshotManifest = serde_json::from_str(&raw)
                .map_err(|e| ServerSnapshotError::Validation(e.to_string()))?;
            let validated = manifest.validated()?;
            if let Some(id) = path.file_name().and_then(|name| name.to_str())
                && let Ok(id) = Uuid::parse_str(id)
            {
                out.push((id, validated));
            }
        }
        out.sort_by_key(|entry| std::cmp::Reverse(entry.1.created_at));
        Ok(out)
    }

    /// Load one manifest by snapshot id.
    pub async fn get(&self, snapshot_id: Uuid) -> Result<SnapshotManifest, ServerSnapshotError> {
        let path = self
            .root
            .join(snapshot_id.to_string())
            .join("manifest.json");
        let raw = tokio::fs::read_to_string(path)
            .await
            .map_err(|_| ServerSnapshotError::NotFound(snapshot_id.to_string()))?;
        let manifest: SnapshotManifest = serde_json::from_str(&raw)
            .map_err(|e| ServerSnapshotError::Validation(e.to_string()))?;
        Ok(manifest.validated()?)
    }

    /// Run restore preflight and mint the single-use confirmation
    /// token (returned as `(snapshot_id, token)`).
    pub async fn preflight(
        &self,
        caller: &User,
        snapshot_id: Uuid,
    ) -> Result<(String, PreflightReport), ServerSnapshotError> {
        self.require_owner(caller)?;
        let manifest = self.get(snapshot_id).await?;
        let existing_names = self.existing_site_names().await;
        let report = openpanel_domain::backups::snapshot::preflight(
            &manifest,
            self.host_id,
            &self.panel_version,
            &existing_names,
        );
        if !report.ready {
            return Err(ServerSnapshotError::Validation(report.blockers.join("; ")));
        }
        let token = ConfirmToken::mint();
        let value = token.expose().to_owned();
        #[allow(clippy::expect_used)] // mutex poisoning is an unrecoverable invariant violation
        self.tokens
            .lock()
            .expect("snapshot token lock")
            .insert(snapshot_id, token);
        Ok((value, report))
    }

    /// Verify every entry's hash, then delegate resource application
    /// to the existing backup-restore pipeline for the source run.
    pub async fn restore(
        &self,
        caller: &User,
        snapshot_id: Uuid,
        confirm_token: &str,
        overwrite: bool,
    ) -> Result<usize, ServerSnapshotError> {
        self.require_owner(caller)?;
        let manifest = self.get(snapshot_id).await?;

        // Single-use token consumption happens before any mutation.
        let consume_result = {
            #[allow(clippy::expect_used)] // mutex poisoning is an unrecoverable invariant violation
            let mut guard = self.tokens.lock().expect("snapshot token lock");
            match guard.get_mut(&snapshot_id) {
                Some(token) => token.consume(confirm_token),
                None => Err(SnapshotError::ConfirmationRequired),
            }
        };
        consume_result?;

        // Verify each bundled file against its recorded hash, in
        // manifest order. A mismatch aborts the restore at that
        // resource and reports what was applied so far.
        let snapshot_dir = self.root.join(snapshot_id.to_string());
        let mut applied: Vec<String> = Vec::new();
        for entry in &manifest.entries {
            let bytes = tokio::fs::read(snapshot_dir.join(&entry.path))
                .await
                .map_err(|_| {
                    ServerSnapshotError::Confirmation(SnapshotError::HashMismatch(
                        entry.path.clone(),
                    ))
                })?;
            if hex_sha256(&bytes) != entry.sha256 {
                return Err(ServerSnapshotError::Partial {
                    applied,
                    failed: entry.path.clone(),
                });
            }
            applied.push(entry.path.clone());
        }

        // Delegate to the tested restore pipeline using the source run.
        let source_run = manifest
            .entries
            .iter()
            .find(|entry| {
                matches!(entry.kind, SnapshotEntryKind::PanelMetadata)
                    && entry.path == "meta/panel.json"
            })
            .and_then(|entry| entry.reference.clone())
            .and_then(|reference| Uuid::parse_str(&reference).ok())
            .ok_or_else(|| {
                ServerSnapshotError::Validation("manifest lacks source-run linkage".into())
            })?;
        let applied = manifest.entries.len();
        eprintln!("[snapshot] delegating restore of run {source_run}");
        self.backups
            .restore(
                caller.id(),
                false,
                source_run,
                RestoreInput {
                    resources: Vec::new(),
                    conflict_policy: if overwrite {
                        "overwrite".to_owned()
                    } else {
                        "fail".to_owned()
                    },
                    confirmation_token: overwrite.then(|| confirm_token.to_owned()),
                },
            )
            .await
            .map_err(|error| match error {
                crate::backups::BackupServiceError::NotFound => {
                    ServerSnapshotError::NotFound(source_run.to_string())
                }
                other => ServerSnapshotError::Failure(other.to_string()),
            })?;

        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SnapshotRestored,
                    AuditOutcome::Success,
                )
                .target(snapshot_id.to_string())
                .metadata(serde_json::json!({ "applied": applied })),
            )
            .await;
        Ok(applied)
    }

    async fn existing_site_names(&self) -> Vec<String> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT primary_domain FROM sites")
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
        rows.into_iter().map(|(name,)| name).collect()
    }

    fn backups_root_runs(&self) -> PathBuf {
        // The backup service stores runs under `<root>/runs/<run_id>`;
        // our root is a sibling directory, so resolve via the parent.
        self.root
            .parent()
            .map(|parent| parent.join("backups").join("runs"))
            .unwrap_or_else(|| PathBuf::from("/var/lib/openpanel/backups/runs"))
    }

    fn require_owner(&self, caller: &User) -> Result<(), ServerSnapshotError> {
        if caller.role() != Role::Owner {
            return Err(ServerSnapshotError::Forbidden);
        }
        Ok(())
    }

    /// Access the underlying backup service.
    pub fn backups(&self) -> &Arc<BackupService> {
        &self.backups
    }
}

fn kind_for(resource: &openpanel_domain::backups::BackupResource) -> SnapshotEntryKind {
    use openpanel_domain::backups::BackupResource;
    match resource {
        BackupResource::Site(_) => SnapshotEntryKind::Site,
        BackupResource::Database(_) => SnapshotEntryKind::Database,
        BackupResource::PanelMetadata => SnapshotEntryKind::PanelMetadata,
    }
}

fn reference_for(resource: &openpanel_domain::backups::BackupResource) -> Option<String> {
    use openpanel_domain::backups::BackupResource;
    match resource {
        BackupResource::Site(site) => Some(site.to_string()),
        BackupResource::Database(db) => Some(db.to_string()),
        BackupResource::PanelMetadata => None,
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn io_error(error: std::io::Error) -> ServerSnapshotError {
    ServerSnapshotError::Failure(error.to_string())
}

// Keep RepoError referenced for future repo-backed metadata.
const _: fn(RepoError) -> ServerSnapshotError = |error| ServerSnapshotError::Failure(error.0);
