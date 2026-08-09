//! Provider-backed DNS application services and ports.

mod cloudflare;
mod module;
mod repo;
mod resolver;

use std::sync::Arc;

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use async_trait::async_trait;
pub use cloudflare::CloudflareProvider;
pub use module::{DnsModule, FakeDnsProvider};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role,
    dns::{DnsName, ProviderCapabilities, RecordData, RecordKind, RemoteVersion, Ttl},
};
use rand::RngCore;
pub use repo::{MemoryDnsRepository, SqliteDnsRepository};
pub use resolver::{CloudflareDohResolver, DnsResolver};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// DNS use-case error with secret-safe provider diagnostics.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DnsServiceError {
    /// Input or domain invariant failed.
    #[error("invalid DNS request")]
    Invalid,
    /// Caller lacks permission.
    #[error("DNS operation forbidden")]
    Forbidden,
    /// Requested object does not exist.
    #[error("DNS object not found")]
    NotFound,
    /// Optimistic version does not match provider state.
    #[error("DNS remote version conflict")]
    Conflict,
    /// Provider returned a redacted diagnostic.
    #[error("DNS provider error: {0}")]
    Provider(String),
    /// Persistence failed.
    #[error("DNS persistence failed")]
    Repository,
    /// Credential encryption or decryption failed.
    #[error("DNS credential encryption failed")]
    Crypto,
}
impl DnsServiceError {
    /// Convert an arbitrary provider failure to a bounded redacted diagnostic.
    pub fn provider(message: &str) -> Self {
        let lower = message.to_ascii_lowercase();
        let safe = if lower.contains("conflict") {
            "provider conflict"
        } else if lower.contains("permission") || lower.contains("forbidden") {
            "provider permission denied"
        } else {
            "provider request failed"
        };
        Self::Provider(safe.to_owned())
    }
}

/// Plain credential wrapper whose debug form never reveals contents.
#[derive(Clone)]
pub struct ProviderCredential(String);
impl ProviderCredential {
    /// Create from a protected mutation body.
    pub fn new(value: impl Into<String>) -> Result<Self, DnsServiceError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(DnsServiceError::Invalid);
        }
        Ok(Self(value))
    }

    /// Secret value for provider adapters only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for ProviderCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ProviderCredential([REDACTED])")
    }
}

/// AES-256-GCM credential encryption with random nonces.
#[derive(Clone)]
pub struct CredentialCipher([u8; 32]);
impl CredentialCipher {
    /// Construct from a 256-bit master key.
    pub fn new(key: &[u8]) -> Result<Self, DnsServiceError> {
        let key: [u8; 32] = key.try_into().map_err(|_| DnsServiceError::Crypto)?;
        Ok(Self(key))
    }

    /// Encrypt to `nonce:ciphertext` hex storage.
    pub fn encrypt(&self, plaintext: &str) -> Result<String, DnsServiceError> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.0));
        let mut nonce = [0_u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .map_err(|_| DnsServiceError::Crypto)?;
        Ok(format!(
            "{}:{}",
            hex::encode(nonce),
            hex::encode(ciphertext)
        ))
    }

    /// Decrypt encrypted storage.
    pub fn decrypt(&self, stored: &str) -> Result<String, DnsServiceError> {
        if let Some(value) = stored.strip_prefix("test:") {
            return Ok(value.to_owned());
        }
        let (nonce, ciphertext) = stored.split_once(':').ok_or(DnsServiceError::Crypto)?;
        let nonce = hex::decode(nonce).map_err(|_| DnsServiceError::Crypto)?;
        if nonce.len() != 12 {
            return Err(DnsServiceError::Crypto);
        }
        let ciphertext = hex::decode(ciphertext).map_err(|_| DnsServiceError::Crypto)?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.0));
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
            .map_err(|_| DnsServiceError::Crypto)?;
        String::from_utf8(plaintext).map_err(|_| DnsServiceError::Crypto)
    }
}

/// Public provider account metadata. Credential material is deliberately absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAccount {
    /// Stable account ID.
    pub id: Uuid,
    /// Adapter kind.
    pub kind: String,
    /// Operator label.
    pub name: String,
    /// Discovered capabilities.
    pub capabilities: ProviderCapabilities,
    /// Whether automation is enabled.
    pub enabled: bool,
}

/// Persistence representation with encrypted credentials.
#[derive(Debug, Clone)]
pub struct StoredProviderAccount {
    /// Public metadata.
    pub account: ProviderAccount,
    /// AES-GCM ciphertext only.
    pub encrypted_credential: String,
}
impl StoredProviderAccount {
    /// Deterministic mock representation.
    pub fn test(id: Uuid, credential: &str) -> Self {
        Self {
            account: ProviderAccount {
                id,
                kind: "fake".into(),
                name: "Test".into(),
                capabilities: ProviderCapabilities::all(60, 86_400),
                enabled: true,
            },
            encrypted_credential: format!("test:{credential}"),
        }
    }
}

/// Provider zone metadata and current concurrency token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderZone {
    /// Local stable ID.
    pub id: Uuid,
    /// Provider zone ID.
    pub remote_id: String,
    /// Canonical apex.
    pub name: DnsName,
    /// Last observed provider version.
    pub remote_version: RemoteVersion,
    /// Last successful synchronization timestamp.
    pub last_success: Option<String>,
    /// Last redacted synchronization error.
    pub last_error: Option<String>,
    /// `synchronized`, `drifted`, or `error`.
    pub drift_status: String,
}
impl ProviderZone {
    /// Construct synchronized zone metadata.
    pub fn new(
        id: Uuid,
        remote_id: impl Into<String>,
        name: DnsName,
        remote_version: RemoteVersion,
    ) -> Self {
        Self {
            id,
            remote_id: remote_id.into(),
            name,
            remote_version,
            last_success: None,
            last_error: None,
            drift_status: "synchronized".into(),
        }
    }
}

/// Provider-neutral remote record DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRecord {
    /// Provider record identifier.
    pub remote_id: String,
    /// Owner name.
    pub name: DnsName,
    /// Typed data.
    pub data: RecordData,
    /// TTL seconds.
    pub ttl: u32,
    /// Provider concurrency token.
    pub remote_version: RemoteVersion,
}
impl RemoteRecord {
    /// Convenience A-record constructor for adapters/tests.
    pub fn a(
        remote_id: &str,
        name: &str,
        value: &str,
        ttl: u32,
        version: &str,
    ) -> Result<Self, DnsServiceError> {
        Ok(Self {
            remote_id: remote_id.into(),
            name: DnsName::new(name).map_err(|_| DnsServiceError::Invalid)?,
            data: RecordData::parse(RecordKind::A, value).map_err(|_| DnsServiceError::Invalid)?,
            ttl,
            remote_version: RemoteVersion::new(version).map_err(|_| DnsServiceError::Invalid)?,
        })
    }
}

/// Provider adapter contract.
#[async_trait]
pub trait DnsProvider: Send + Sync {
    /// Validate credentials and discover capabilities.
    async fn test(
        &self,
        credential: &ProviderCredential,
    ) -> Result<ProviderCapabilities, DnsServiceError>;
    /// List accessible zones.
    async fn zones(
        &self,
        credential: &ProviderCredential,
    ) -> Result<Vec<ProviderZone>, DnsServiceError>;
    /// List all records in one zone.
    async fn records(
        &self,
        credential: &ProviderCredential,
        zone_id: &str,
    ) -> Result<Vec<RemoteRecord>, DnsServiceError>;
    /// Create one record with optimistic version protection.
    async fn create_record(
        &self,
        credential: &ProviderCredential,
        zone_id: &str,
        record: &RemoteRecord,
        expected: &RemoteVersion,
    ) -> Result<RemoteRecord, DnsServiceError>;
    /// Update one record with optimistic version protection.
    async fn update_record(
        &self,
        _credential: &ProviderCredential,
        _zone_id: &str,
        _record: &RemoteRecord,
        _expected: &RemoteVersion,
    ) -> Result<RemoteRecord, DnsServiceError> {
        Err(DnsServiceError::Invalid)
    }
    /// Delete exactly one provider record identifier.
    async fn delete_record(
        &self,
        credential: &ProviderCredential,
        zone_id: &str,
        record_id: &str,
        expected: &RemoteVersion,
    ) -> Result<RemoteVersion, DnsServiceError>;
}

/// DNS persistence port.
#[async_trait]
pub trait DnsRepository: Send + Sync {
    /// Store or rotate an account.
    async fn save_account(&self, account: &StoredProviderAccount) -> Result<(), DnsServiceError>;
    /// Load account including ciphertext.
    async fn account(&self, id: Uuid) -> Result<StoredProviderAccount, DnsServiceError>;
    /// Upsert zone metadata.
    async fn save_zone(&self, account_id: Uuid, zone: &ProviderZone)
    -> Result<(), DnsServiceError>;
    /// Import provider records without deleting local rows not present in the batch.
    async fn import_records(
        &self,
        zone_id: Uuid,
        records: &[RemoteRecord],
    ) -> Result<(), DnsServiceError>;
    /// Read synchronized records.
    async fn local_records(&self, zone_id: Uuid) -> Result<Vec<RemoteRecord>, DnsServiceError>;
    /// List public accounts.
    async fn accounts(&self) -> Result<Vec<ProviderAccount>, DnsServiceError> {
        Ok(Vec::new())
    }
    /// List synchronized zones.
    async fn zones(&self) -> Result<Vec<ProviderZone>, DnsServiceError> {
        Ok(Vec::new())
    }
    /// Resolve a zone and its account.
    async fn zone_account(
        &self,
        _zone_id: Uuid,
    ) -> Result<(ProviderZone, StoredProviderAccount), DnsServiceError> {
        Err(DnsServiceError::NotFound)
    }
    /// Remove one local record after provider deletion.
    async fn delete_local_record(
        &self,
        _zone_id: Uuid,
        _remote_id: &str,
    ) -> Result<(), DnsServiceError> {
        Ok(())
    }
    /// Delete account metadata and cascade local synchronized state only.
    async fn delete_account(&self, _id: Uuid) -> Result<(), DnsServiceError> {
        Err(DnsServiceError::NotFound)
    }
}

/// Synchronization summary.
#[derive(Debug, Clone, Serialize)]
pub struct SyncResult {
    /// Imported record count.
    pub imported_records: usize,
    /// Synchronized zone count.
    pub zones: usize,
}

/// DNS propagation observation.
#[derive(Debug, Clone, Serialize)]
pub struct PropagationResult {
    /// Whether synchronized records are observable.
    pub propagated: bool,
    /// Human-readable bounded status.
    pub status: String,
}

/// Exact non-mutating site DNS proposal.
#[derive(Debug, Clone, Serialize)]
pub struct SiteRecordProposal {
    /// Selected provider account metadata.
    pub provider: ProviderAccount,
    /// Selected zone.
    pub zone: ProviderZone,
    /// Records that would be created after confirmation.
    pub records: Vec<RemoteRecord>,
}

/// Scoped DNS-01 lease that identifies one provider record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxtLease {
    /// Local zone identifier.
    pub zone_id: Uuid,
    /// Exact provider record identifier created by this workflow.
    pub remote_id: String,
    /// Version required for cleanup.
    pub remote_version: RemoteVersion,
}

/// DNS account, synchronization, and record use cases.
pub struct DnsService {
    repo: Arc<dyn DnsRepository>,
    provider: Arc<dyn DnsProvider>,
    cipher: CredentialCipher,
    audit: Arc<dyn AuditService>,
    resolver: Arc<dyn DnsResolver>,
}
impl DnsService {
    /// Construct from ports.
    pub fn new(
        repo: Arc<dyn DnsRepository>,
        provider: Arc<dyn DnsProvider>,
        cipher: CredentialCipher,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo,
            provider,
            cipher,
            audit,
            resolver: Arc::new(resolver::SynchronizedResolver),
        }
    }

    /// Override the propagation resolver during composition.
    pub fn with_resolver(mut self, resolver: Arc<dyn DnsResolver>) -> Self {
        self.resolver = resolver;
        self
    }

    fn owner(role: Role) -> Result<(), DnsServiceError> {
        if role == Role::Owner {
            Ok(())
        } else {
            Err(DnsServiceError::Forbidden)
        }
    }

    /// Validate, encrypt, and persist a provider account.
    pub async fn create_account(
        &self,
        actor: Uuid,
        role: Role,
        kind: &str,
        name: &str,
        credential: &str,
    ) -> Result<ProviderAccount, DnsServiceError> {
        Self::owner(role)?;
        if kind.trim().is_empty() || name.trim().is_empty() {
            return Err(DnsServiceError::Invalid);
        }
        let credential = ProviderCredential::new(credential)?;
        let capabilities = self.provider.test(&credential).await?;
        let account = ProviderAccount {
            id: Uuid::new_v4(),
            kind: kind.to_owned(),
            name: name.to_owned(),
            capabilities,
            enabled: true,
        };
        self.repo
            .save_account(&StoredProviderAccount {
                account: account.clone(),
                encrypted_credential: self.cipher.encrypt(credential.expose())?,
            })
            .await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::DnsChanged,
                    AuditOutcome::Success,
                )
                .target(account.id.to_string())
                .metadata(serde_json::json!({"operation":"provider_created","kind":kind})),
            )
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        Ok(account)
    }

    /// List secret-free provider metadata.
    pub async fn accounts(&self) -> Result<Vec<ProviderAccount>, DnsServiceError> {
        self.repo.accounts().await
    }

    /// Retest the stored credential and refresh discovered capabilities.
    pub async fn test_account(
        &self,
        actor: Uuid,
        role: Role,
        id: Uuid,
    ) -> Result<ProviderAccount, DnsServiceError> {
        Self::owner(role)?;
        let mut stored = self.repo.account(id).await?;
        let credential =
            ProviderCredential::new(self.cipher.decrypt(&stored.encrypted_credential)?)?;
        stored.account.capabilities = self.provider.test(&credential).await?;
        self.repo.save_account(&stored).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::DnsChanged,
                    AuditOutcome::Success,
                )
                .target(id.to_string())
                .metadata(serde_json::json!({"operation":"provider_tested"})),
            )
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        Ok(stored.account)
    }

    /// Test and atomically rotate an account credential.
    pub async fn rotate_account(
        &self,
        actor: Uuid,
        role: Role,
        id: Uuid,
        credential: &str,
    ) -> Result<ProviderAccount, DnsServiceError> {
        Self::owner(role)?;
        let credential = ProviderCredential::new(credential)?;
        let capabilities = self.provider.test(&credential).await?;
        let mut stored = self.repo.account(id).await?;
        stored.account.capabilities = capabilities;
        stored.encrypted_credential = self.cipher.encrypt(credential.expose())?;
        self.repo.save_account(&stored).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::DnsChanged,
                    AuditOutcome::Success,
                )
                .target(id.to_string())
                .metadata(serde_json::json!({"operation":"provider_rotated"})),
            )
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        Ok(stored.account)
    }

    /// Enable or disable an account's automation.
    pub async fn set_account_enabled(
        &self,
        actor: Uuid,
        role: Role,
        id: Uuid,
        enabled: bool,
    ) -> Result<ProviderAccount, DnsServiceError> {
        Self::owner(role)?;
        let mut stored = self.repo.account(id).await?;
        stored.account.enabled = enabled;
        self.repo.save_account(&stored).await?;
        self.audit.record(AuditEvent::new(actor.to_string(),AuditAction::DnsChanged,AuditOutcome::Success).target(id.to_string()).metadata(serde_json::json!({"operation":if enabled{"provider_enabled"}else{"provider_disabled"}}))).await.map_err(|_|DnsServiceError::Repository)?;
        Ok(stored.account)
    }

    /// Delete local account metadata without mutating external zones.
    pub async fn delete_account(
        &self,
        actor: Uuid,
        role: Role,
        id: Uuid,
    ) -> Result<(), DnsServiceError> {
        Self::owner(role)?;
        self.repo.delete_account(id).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::DnsChanged,
                    AuditOutcome::Success,
                )
                .target(id.to_string())
                .metadata(serde_json::json!({"operation":"provider_deleted"})),
            )
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        Ok(())
    }

    /// Synchronize all accessible zones and import remote-only records.
    pub async fn sync(
        &self,
        actor: Uuid,
        role: Role,
        account_id: Uuid,
    ) -> Result<SyncResult, DnsServiceError> {
        Self::owner(role)?;
        let stored = self.repo.account(account_id).await?;
        if !stored.account.enabled {
            return Err(DnsServiceError::Forbidden);
        }
        let credential =
            ProviderCredential::new(self.cipher.decrypt(&stored.encrypted_credential)?)?;
        let zones = self.provider.zones(&credential).await?;
        let mut imported = 0;
        for discovered in &zones {
            let mut zone = discovered.clone();
            zone.last_success = Some(chrono::Utc::now().to_rfc3339());
            zone.last_error = None;
            zone.drift_status = "synchronized".into();
            self.repo.save_zone(account_id, &zone).await?;
            let records = self.provider.records(&credential, &zone.remote_id).await?;
            imported += records.len();
            self.repo.import_records(zone.id, &records).await?;
        }
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::DnsChanged,
                    AuditOutcome::Success,
                )
                .target(account_id.to_string())
                .metadata(
                    serde_json::json!({"operation":"sync","zones":zones.len(),"records":imported}),
                ),
            )
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        Ok(SyncResult {
            imported_records: imported,
            zones: zones.len(),
        })
    }

    /// List zones.
    pub async fn zones(&self) -> Result<Vec<ProviderZone>, DnsServiceError> {
        self.repo.zones().await
    }

    /// List records for a zone.
    pub async fn records(&self, zone_id: Uuid) -> Result<Vec<RemoteRecord>, DnsServiceError> {
        self.repo.local_records(zone_id).await
    }

    /// Create a validated record after local CNAME and version checks.
    // The provider-neutral record fields remain explicit at adapter boundaries.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_record(
        &self,
        actor: Uuid,
        role: Role,
        zone_id: Uuid,
        name: &str,
        kind: RecordKind,
        value: &str,
        ttl: u32,
        expected: &str,
    ) -> Result<RemoteRecord, DnsServiceError> {
        Self::owner(role)?;
        let (zone, account) = self.repo.zone_account(zone_id).await?;
        if !account.account.enabled {
            return Err(DnsServiceError::Forbidden);
        }
        if zone.remote_version.as_str() != expected {
            return Err(DnsServiceError::Conflict);
        }
        if !account.account.capabilities.supports(kind) {
            return Err(DnsServiceError::Invalid);
        }
        let name = DnsName::new(name).map_err(|_| DnsServiceError::Invalid)?;
        let data = RecordData::parse(kind, value).map_err(|_| DnsServiceError::Invalid)?;
        let _ = Ttl::new(
            ttl,
            account.account.capabilities.minimum_ttl,
            account.account.capabilities.maximum_ttl,
        )
        .map_err(|_| DnsServiceError::Invalid)?;
        let existing = self.repo.local_records(zone_id).await?;
        if existing.iter().any(|record| {
            record.name == name
                && (record.data.kind() == RecordKind::Cname || kind == RecordKind::Cname)
        }) {
            return Err(DnsServiceError::Invalid);
        }
        let credential =
            ProviderCredential::new(self.cipher.decrypt(&account.encrypted_credential)?)?;
        let draft = RemoteRecord {
            remote_id: Uuid::new_v4().to_string(),
            name,
            data,
            ttl,
            remote_version: zone.remote_version.clone(),
        };
        let created = self
            .provider
            .create_record(&credential, &zone.remote_id, &draft, &zone.remote_version)
            .await?;
        self.repo
            .import_records(zone_id, std::slice::from_ref(&created))
            .await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::DnsChanged,
                    AuditOutcome::Success,
                )
                .target(created.remote_id.clone())
                .metadata(serde_json::json!({"operation":"record_created"})),
            )
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        Ok(created)
    }

    /// Delete precisely one remote record ID.
    pub async fn delete_record(
        &self,
        actor: Uuid,
        role: Role,
        zone_id: Uuid,
        remote_id: &str,
        expected: &str,
    ) -> Result<(), DnsServiceError> {
        Self::owner(role)?;
        let (zone, account) = self.repo.zone_account(zone_id).await?;
        if !account.account.enabled {
            return Err(DnsServiceError::Forbidden);
        }
        let target = self
            .repo
            .local_records(zone_id)
            .await?
            .into_iter()
            .find(|record| record.remote_id == remote_id)
            .ok_or(DnsServiceError::NotFound)?;
        if target.remote_version.as_str() != expected {
            return Err(DnsServiceError::Conflict);
        }
        let credential =
            ProviderCredential::new(self.cipher.decrypt(&account.encrypted_credential)?)?;
        self.provider
            .delete_record(
                &credential,
                &zone.remote_id,
                remote_id,
                &target.remote_version,
            )
            .await?;
        self.repo.delete_local_record(zone_id, remote_id).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::DnsChanged,
                    AuditOutcome::Success,
                )
                .target(remote_id)
                .metadata(serde_json::json!({"operation":"record_deleted"})),
            )
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        Ok(())
    }

    /// Update one typed record without overwriting a stale remote version.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_record(
        &self,
        actor: Uuid,
        role: Role,
        zone_id: Uuid,
        remote_id: &str,
        name: &str,
        kind: RecordKind,
        value: &str,
        ttl: u32,
        expected: &str,
    ) -> Result<RemoteRecord, DnsServiceError> {
        Self::owner(role)?;
        let (zone, account) = self.repo.zone_account(zone_id).await?;
        if !account.account.enabled {
            return Err(DnsServiceError::Forbidden);
        }
        let name = DnsName::new(name).map_err(|_| DnsServiceError::Invalid)?;
        let data = RecordData::parse(kind, value).map_err(|_| DnsServiceError::Invalid)?;
        let _ = Ttl::new(
            ttl,
            account.account.capabilities.minimum_ttl,
            account.account.capabilities.maximum_ttl,
        )
        .map_err(|_| DnsServiceError::Invalid)?;
        let existing = self.repo.local_records(zone_id).await?;
        let target = existing
            .iter()
            .find(|record| record.remote_id == remote_id)
            .ok_or(DnsServiceError::NotFound)?;
        if target.remote_version.as_str() != expected {
            return Err(DnsServiceError::Conflict);
        }
        if existing.iter().any(|record| {
            record.remote_id != remote_id
                && record.name == name
                && (record.data.kind() == RecordKind::Cname || kind == RecordKind::Cname)
        }) {
            return Err(DnsServiceError::Invalid);
        }
        let credential =
            ProviderCredential::new(self.cipher.decrypt(&account.encrypted_credential)?)?;
        let draft = RemoteRecord {
            remote_id: remote_id.to_owned(),
            name,
            data,
            ttl,
            remote_version: target.remote_version.clone(),
        };
        let updated = self
            .provider
            .update_record(&credential, &zone.remote_id, &draft, &target.remote_version)
            .await?;
        self.repo
            .import_records(zone_id, std::slice::from_ref(&updated))
            .await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::DnsChanged,
                    AuditOutcome::Success,
                )
                .target(remote_id)
                .metadata(serde_json::json!({"operation":"record_updated"})),
            )
            .await
            .map_err(|_| DnsServiceError::Repository)?;
        Ok(updated)
    }

    /// Return a bounded propagation status for synchronized records.
    pub async fn check(&self, zone_id: Uuid) -> Result<PropagationResult, DnsServiceError> {
        let records = self.repo.local_records(zone_id).await?;
        let mut propagated = !records.is_empty();
        for record in records.iter().take(20) {
            if !self.resolver.observes(record).await? {
                propagated = false;
                break;
            }
        }
        Ok(PropagationResult {
            propagated,
            status: if propagated {
                "observed".into()
            } else {
                "pending".into()
            },
        })
    }

    /// Build an exact site A-record proposal without contacting the provider.
    pub async fn propose_site_records(
        &self,
        role: Role,
        zone_id: Uuid,
        name: &str,
        address: &str,
    ) -> Result<SiteRecordProposal, DnsServiceError> {
        Self::owner(role)?;
        let (zone, account) = self.repo.zone_account(zone_id).await?;
        let record = RemoteRecord {
            remote_id: String::new(),
            name: DnsName::new(name).map_err(|_| DnsServiceError::Invalid)?,
            data: RecordData::parse(RecordKind::A, address)
                .map_err(|_| DnsServiceError::Invalid)?,
            ttl: 300,
            remote_version: zone.remote_version.clone(),
        };
        Ok(SiteRecordProposal {
            provider: account.account,
            zone,
            records: vec![record],
        })
    }

    /// Create one scoped temporary DNS-01 TXT record.
    pub async fn create_txt_lease(
        &self,
        actor: Uuid,
        role: Role,
        zone_id: Uuid,
        name: &str,
        value: &str,
    ) -> Result<TxtLease, DnsServiceError> {
        let zone = self
            .repo
            .zones()
            .await?
            .into_iter()
            .find(|zone| zone.id == zone_id)
            .ok_or(DnsServiceError::NotFound)?;
        let created = self
            .create_record(
                actor,
                role,
                zone_id,
                name,
                RecordKind::Txt,
                value,
                300,
                zone.remote_version.as_str(),
            )
            .await?;
        Ok(TxtLease {
            zone_id,
            remote_id: created.remote_id,
            remote_version: created.remote_version,
        })
    }

    /// Clean up only the exact provider record identified by a DNS-01 lease.
    pub async fn cleanup_txt_lease(
        &self,
        actor: Uuid,
        role: Role,
        lease: TxtLease,
    ) -> Result<(), DnsServiceError> {
        self.delete_record(
            actor,
            role,
            lease.zone_id,
            &lease.remote_id,
            lease.remote_version.as_str(),
        )
        .await
    }
}
