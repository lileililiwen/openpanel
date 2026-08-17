//! Offsite backup upload service: credential lifecycle, remote
//! target attachment, reachability probes, and the object
//! operations driven through a `BackupTargetAdapter`.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    BackupCredential, BackupTargetAdapter, CredentialKind, OffsiteBackupError,
    OffsiteBackupRepository, RemoteTargetConfig,
};
use uuid::Uuid;

use super::kek::{decrypt_payload, encrypt_payload};

/// A decrypted credential payload before it is handed to an
/// adapter. Never persisted; zeroized semantics are the caller's
/// responsibility (drop at end of scope).
#[derive(Debug, Clone)]
pub struct DecryptedCredential {
    kind: CredentialKind,
    payload: String,
}

impl DecryptedCredential {
    /// The storage family.
    pub fn kind(&self) -> CredentialKind {
        self.kind
    }

    /// The plaintext payload (access key / secret / bucket etc.).
    pub fn payload(&self) -> &str {
        &self.payload
    }
}

/// Result of a reachability probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteProbe {
    reachable: bool,
    latency_ms: u64,
}

impl RemoteProbe {
    /// Whether the target accepted the probe.
    pub fn reachable(&self) -> bool {
        self.reachable
    }

    /// Measured round-trip latency in milliseconds.
    pub fn latency_ms(&self) -> u64 {
        self.latency_ms
    }
}

/// Offsite backup application service.
#[derive(Clone)]
pub struct BackupUploadService {
    repo: Arc<dyn OffsiteBackupRepository>,
    audit: Arc<dyn AuditService>,
    /// The backup KEK (32 bytes). Holds the derived key in memory
    /// for the lifetime of the service.
    kek: Arc<[u8; 32]>,
}

impl BackupUploadService {
    /// Build a service with a known KEK (derived once at panel
    /// init and wrapped under the master key).
    pub fn with_kek(
        repo: Arc<dyn OffsiteBackupRepository>,
        audit: Arc<dyn AuditService>,
        kek: [u8; 32],
    ) -> Self {
        Self {
            repo,
            audit,
            kek: Arc::new(kek),
        }
    }

    /// Create a credential: the plaintext payload is encrypted
    /// under the KEK before it touches persistence.
    pub async fn create_credential(
        &self,
        actor: &str,
        kind: CredentialKind,
        label: &str,
        payload: &str,
    ) -> Result<BackupCredential, OffsiteBackupError> {
        let secret_enc = encrypt_payload(&self.kek, payload)?;
        let credential =
            BackupCredential::new(Uuid::new_v4(), kind, label, secret_enc, Utc::now())?;
        self.repo.insert_credential(&credential).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::BackupCredentialCreated,
                    AuditOutcome::Success,
                )
                .target(credential.id().to_string())
                .metadata(serde_json::json!({ "kind": kind.as_str() })),
            )
            .await
            .ok();
        Ok(credential)
    }

    /// List stored credentials. Secrets are never echoed.
    pub async fn list_credentials(&self) -> Result<Vec<BackupCredential>, OffsiteBackupError> {
        self.repo.list_credentials().await
    }

    /// Decrypt a stored credential for use by an adapter.
    pub async fn decrypt_credential(
        &self,
        id: Uuid,
    ) -> Result<DecryptedCredential, OffsiteBackupError> {
        let credential = self
            .repo
            .find_credential(id)
            .await?
            .ok_or(OffsiteBackupError::CredentialNotFound)?;
        let payload = decrypt_payload(&self.kek, credential.secret_enc())
            .map_err(|_| OffsiteBackupError::CredentialDecrypt)?;
        Ok(DecryptedCredential {
            kind: credential.kind(),
            payload,
        })
    }

    /// Delete a credential. Refuses when it is attached to a plan.
    pub async fn delete_credential(&self, actor: &str, id: Uuid) -> Result<(), OffsiteBackupError> {
        match self.repo.delete_credential(id).await {
            Ok(()) => {
                self.audit
                    .record(
                        AuditEvent::new(
                            actor,
                            AuditAction::BackupCredentialDeleted,
                            AuditOutcome::Success,
                        )
                        .target(id.to_string()),
                    )
                    .await
                    .ok();
                Ok(())
            }
            Err(OffsiteBackupError::CredentialInUse(_)) => {
                self.audit
                    .record(
                        AuditEvent::new(
                            actor,
                            AuditAction::BackupCredentialInUseRejected,
                            AuditOutcome::Denied,
                        )
                        .target(id.to_string()),
                    )
                    .await
                    .ok();
                Err(OffsiteBackupError::CredentialInUse(id))
            }
            Err(e) => Err(e),
        }
    }

    /// Attach (or replace) the remote target config for a plan.
    pub async fn attach_remote_target(
        &self,
        actor: &str,
        config: &RemoteTargetConfig,
    ) -> Result<(), OffsiteBackupError> {
        if self
            .repo
            .find_credential(config.credential_id())
            .await?
            .is_none()
        {
            return Err(OffsiteBackupError::CredentialNotFound);
        }
        self.repo.insert_remote_target(config).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::BackupRemoteTargetAttached,
                    AuditOutcome::Success,
                )
                .target(config.plan_id().to_string())
                .metadata(serde_json::json!({
                    "credential_id": config.credential_id().to_string(),
                    "prefix": config.prefix(),
                })),
            )
            .await
            .ok();
        Ok(())
    }

    /// Probe reachability of a credential with an adapter,
    /// measuring round-trip latency.
    pub async fn test_remote<A>(
        &self,
        actor: &str,
        credential_id: Uuid,
        adapter: &A,
    ) -> Result<RemoteProbe, OffsiteBackupError>
    where
        A: BackupTargetAdapter<Error = OffsiteBackupError>,
    {
        let started = std::time::Instant::now();
        let reachable = adapter.test().await.is_ok();
        let latency_ms = started.elapsed().as_millis() as u64;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::BackupRemoteTested,
                    if reachable {
                        AuditOutcome::Success
                    } else {
                        AuditOutcome::Failure
                    },
                )
                .target(credential_id.to_string())
                .metadata(serde_json::json!({
                    "reachable": reachable,
                    "latency_ms": latency_ms,
                })),
            )
            .await
            .ok();
        Ok(RemoteProbe {
            reachable,
            latency_ms,
        })
    }

    /// Upload `bytes` to `key` through an adapter.
    pub async fn upload<A>(
        &self,
        adapter: &A,
        key: &str,
        bytes: &[u8],
    ) -> Result<(), OffsiteBackupError>
    where
        A: BackupTargetAdapter<Error = OffsiteBackupError>,
    {
        adapter
            .put(key, bytes)
            .await
            .map_err(|e| OffsiteBackupError::Adapter(e.to_string()))
    }

    /// Download `key` through an adapter.
    pub async fn download<A>(&self, adapter: &A, key: &str) -> Result<Vec<u8>, OffsiteBackupError>
    where
        A: BackupTargetAdapter<Error = OffsiteBackupError>,
    {
        adapter
            .get(key)
            .await
            .map_err(|e| OffsiteBackupError::Adapter(e.to_string()))
    }

    /// List objects under a prefix through an adapter.
    pub async fn list_objects<A>(
        &self,
        adapter: &A,
        prefix: &str,
    ) -> Result<Vec<String>, OffsiteBackupError>
    where
        A: BackupTargetAdapter<Error = OffsiteBackupError>,
    {
        adapter
            .list(prefix)
            .await
            .map_err(|e| OffsiteBackupError::Adapter(e.to_string()))
    }
}

/// TTL for the derived KEK in the panel: this is the in-memory
/// key lifecycle, not the off-host re-derivation window.
#[allow(dead_code)] // reserved for the in-memory KEK TTL expiry task
pub const KEK_MEMORY_TTL: Duration = Duration::hours(24);

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::{super::SqliteOffsiteBackupRepository, *};

    struct MemoryAdapter {
        objects: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl BackupTargetAdapter for MemoryAdapter {
        type Error = OffsiteBackupError;

        async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Self::Error> {
            self.objects
                .lock()
                .unwrap()
                .insert(key.to_string(), bytes.to_vec());
            Ok(())
        }

        async fn get(&self, key: &str) -> Result<Vec<u8>, Self::Error> {
            self.objects
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .ok_or(OffsiteBackupError::CredentialNotFound)
        }

        async fn list(&self, prefix: &str) -> Result<Vec<String>, Self::Error> {
            let mut keys = self
                .objects
                .lock()
                .unwrap()
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect::<Vec<_>>();
            keys.sort();
            Ok(keys)
        }

        async fn delete(&self, key: &str) -> Result<(), Self::Error> {
            self.objects.lock().unwrap().remove(key);
            Ok(())
        }

        async fn test(&self) -> Result<(), Self::Error> {
            if self.fail {
                Err(OffsiteBackupError::Adapter("unreachable".into()))
            } else {
                Ok(())
            }
        }
    }

    fn kek() -> [u8; 32] {
        [0x42u8; 32]
    }

    async fn repo() -> Arc<dyn OffsiteBackupRepository> {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(crate::migrations::OFFSITE_BACKUP_TARGETS_V001)
            .execute(&pool)
            .await
            .unwrap();
        Arc::new(SqliteOffsiteBackupRepository::new(pool))
    }

    fn service(repo: Arc<dyn OffsiteBackupRepository>) -> BackupUploadService {
        BackupUploadService::with_kek(
            repo,
            Arc::new(openpanel_test_support::MockAudit::stub()),
            kek(),
        )
    }

    #[tokio::test]
    async fn create_then_decrypt_roundtrip() {
        let svc = service(repo().await);
        let cred = svc
            .create_credential("admin", CredentialKind::S3, "prod", "AKIA:secret")
            .await
            .unwrap();
        assert_ne!(cred.secret_enc(), "AKIA:secret");
        let decrypted = svc.decrypt_credential(cred.id()).await.unwrap();
        assert_eq!(decrypted.payload(), "AKIA:secret");
        assert_eq!(decrypted.kind(), CredentialKind::S3);
    }

    #[tokio::test]
    async fn list_never_echoes_secrets() {
        let svc = service(repo().await);
        svc.create_credential("admin", CredentialKind::B2, "b2-prod", "key:app")
            .await
            .unwrap();
        let listed = svc.list_credentials().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].secret_enc().contains("key:app"));
    }

    #[tokio::test]
    async fn delete_refused_when_credential_in_use() {
        let svc = service(repo().await);
        let cred = svc
            .create_credential("admin", CredentialKind::S3, "prod", "AKIA:secret")
            .await
            .unwrap();
        let config =
            RemoteTargetConfig::new(Uuid::new_v4(), cred.id(), "host1/backups", None).unwrap();
        svc.attach_remote_target("admin", &config).await.unwrap();
        let err = svc.delete_credential("admin", cred.id()).await.unwrap_err();
        assert_eq!(err, OffsiteBackupError::CredentialInUse(cred.id()));
    }

    #[tokio::test]
    async fn delete_succeeds_when_unused() {
        let svc = service(repo().await);
        let cred = svc
            .create_credential("admin", CredentialKind::Rsync, "nas", "user@host")
            .await
            .unwrap();
        svc.delete_credential("admin", cred.id()).await.unwrap();
        assert!(svc.decrypt_credential(cred.id()).await.is_err());
    }

    #[tokio::test]
    async fn attach_remote_target_requires_existing_credential() {
        let svc = service(repo().await);
        let config =
            RemoteTargetConfig::new(Uuid::new_v4(), Uuid::new_v4(), "host1/backups", None).unwrap();
        let err = svc
            .attach_remote_target("admin", &config)
            .await
            .unwrap_err();
        assert_eq!(err, OffsiteBackupError::CredentialNotFound);
    }

    #[tokio::test]
    async fn remote_probe_reports_latency_and_reachability() {
        let svc = service(repo().await);
        let adapter = MemoryAdapter {
            objects: std::sync::Mutex::new(std::collections::HashMap::new()),
            fail: false,
        };
        let probe = svc
            .test_remote("admin", Uuid::new_v4(), &adapter)
            .await
            .unwrap();
        assert!(probe.reachable());
        let failing = MemoryAdapter {
            objects: std::sync::Mutex::new(std::collections::HashMap::new()),
            fail: true,
        };
        let probe = svc
            .test_remote("admin", Uuid::new_v4(), &failing)
            .await
            .unwrap();
        assert!(!probe.reachable());
    }

    #[tokio::test]
    async fn upload_download_list_delete_via_adapter() {
        let svc = service(repo().await);
        let adapter = MemoryAdapter {
            objects: std::sync::Mutex::new(std::collections::HashMap::new()),
            fail: false,
        };
        svc.upload(&adapter, "host1/site.tar.gz", b"payload")
            .await
            .unwrap();
        let keys = svc.list_objects(&adapter, "host1/").await.unwrap();
        assert_eq!(keys, vec!["host1/site.tar.gz".to_string()]);
        let bytes = svc.download(&adapter, "host1/site.tar.gz").await.unwrap();
        assert_eq!(bytes, b"payload");
        adapter.delete("host1/site.tar.gz").await.unwrap();
        let keys = svc.list_objects(&adapter, "host1/").await.unwrap();
        assert!(keys.is_empty());
    }
}
