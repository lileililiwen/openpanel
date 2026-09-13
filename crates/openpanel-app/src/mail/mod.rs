//! Hosted mail application ports, aggregates, and use cases.

mod module;
mod repo;
mod secure;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
pub use module::{FakeMailConfigurator, MailModule};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Password, Role,
    mail::{AliasGraph, MailAddress, MailDomainName, MailQuota},
};
use rand::Rng;
pub use repo::{MemoryMailRepository, SqliteMailRepository};
pub use secure::{
    DkimKeyCustody, FilesystemMailConfigurator, MailConfigControl, SystemMailConfigControl,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Mail use-case error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MailServiceError {
    /// Invalid address, password, quota, alias, or transition.
    #[error("invalid mail request")]
    Invalid,
    /// Caller lacks ownership or role permission.
    #[error("mail operation forbidden")]
    Forbidden,
    /// Object is absent.
    #[error("mail object not found")]
    NotFound,
    /// Mandatory readiness check failed.
    #[error("mail not ready: {0}")]
    NotReady(String),
    /// Candidate configuration failed validation or apply.
    #[error("mail configuration failed")]
    Configuration,
    /// Persistence failed.
    #[error("mail persistence failed")]
    Repository,
}

/// Mandatory and advisory readiness result.
#[derive(Debug, Clone, Serialize)]
pub struct Readiness {
    /// All mandatory checks passed.
    pub ready: bool,
    /// Expected MX when missing.
    pub expected_mx: Option<String>,
    /// Redacted check labels.
    pub checks: Vec<String>,
    /// Whether only externally managed DNS remains and may be acknowledged.
    pub can_acknowledge: bool,
}
impl Readiness {
    /// Fully ready deterministic result.
    pub fn ready() -> Self {
        Self {
            ready: true,
            expected_mx: None,
            checks: vec![
                "postfix".into(),
                "dovecot".into(),
                "tls".into(),
                "dns".into(),
            ],
            can_acknowledge: false,
        }
    }

    /// Missing MX result.
    pub fn missing_mx(expected: impl Into<String>) -> Self {
        Self {
            ready: false,
            expected_mx: Some(expected.into()),
            checks: vec!["mx_missing".into()],
            can_acknowledge: true,
        }
    }
}

/// Managed mail domain metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailDomain {
    /// Stable ID.
    pub id: Uuid,
    /// Owner ID.
    pub owner_id: Uuid,
    /// Canonical domain.
    pub name: MailDomainName,
    /// Enablement state.
    pub enabled: bool,
    /// Aggregate quota.
    pub quota_bytes: u64,
}
impl MailDomain {
    /// Deterministic test aggregate.
    pub fn test(id: Uuid, name: &str, enabled: bool) -> Self {
        Self {
            id,
            owner_id: Uuid::nil(),
            name: MailDomainName::new(name).unwrap_or_else(|_| std::process::abort()),
            enabled,
            quota_bytes: 10 * 1024 * 1024 * 1024,
        }
    }
}

/// Secret-free mailbox metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mailbox {
    /// Stable ID.
    pub id: Uuid,
    /// Parent domain.
    pub domain_id: Uuid,
    /// Canonical address.
    pub address: MailAddress,
    /// Storage quota.
    pub quota: MailQuota,
    /// Enablement state.
    pub enabled: bool,
}
/// Persistence mailbox including a non-serializing strong hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMailbox {
    /// Secret-free metadata.
    pub mailbox: Mailbox,
    /// Argon2id PHC hash, never serialized.
    #[serde(skip_serializing)]
    pub password_hash: String,
}
/// Creation/rotation response; plaintext is deliberately separated and returned once.
#[derive(Debug, Serialize)]
pub struct MailboxCredential {
    /// Secret-free mailbox.
    pub mailbox: Mailbox,
    /// One-time plaintext.
    pub password: String,
}
/// Alias metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailAlias {
    /// Stable ID.
    pub id: Uuid,
    /// Parent domain.
    pub domain_id: Uuid,
    /// Alias source.
    pub source: MailAddress,
    /// Forwarding destination.
    pub destination: MailAddress,
}
/// Aggregate service status without message metadata.
#[derive(Debug, Serialize)]
pub struct MailStatus {
    /// Managed domains.
    pub domains: usize,
    /// Managed mailboxes.
    pub mailboxes: usize,
    /// Aggregate queue depth.
    pub queue_depth: u64,
    /// Service state label.
    pub health: String,
}

/// Dependency preview and short-lived confirmation for domain deletion.
#[derive(Debug, Serialize)]
pub struct DomainDeletionPreview {
    /// Domain to be removed.
    pub domain_id: Uuid,
    /// Dependent mailboxes that will be removed.
    pub mailboxes: usize,
    /// Dependent aliases that will be removed.
    pub aliases: usize,
    /// Opaque, one-use confirmation scoped to this domain.
    pub confirmation_token: String,
    /// Token expiration as Unix seconds.
    pub expires_at: u64,
}

/// Mail persistence contract.
#[async_trait]
pub trait MailRepository: Send + Sync {
    /// Insert or update a mail domain.
    async fn save_domain(&self, domain: &MailDomain) -> Result<(), MailServiceError>;
    /// Fetch a domain by stable ID.
    async fn domain(&self, id: Uuid) -> Result<MailDomain, MailServiceError>;
    /// Insert or update a mailbox with its protected hash.
    async fn save_mailbox(&self, mailbox: &StoredMailbox) -> Result<(), MailServiceError>;
    /// Fetch protected mailboxes for one domain.
    async fn mailboxes(&self, domain_id: Uuid) -> Result<Vec<StoredMailbox>, MailServiceError>;
    /// List domains, optionally restricted to one owner.
    async fn domains(&self, _owner: Option<Uuid>) -> Result<Vec<MailDomain>, MailServiceError> {
        Ok(Vec::new())
    }
    /// Find a domain by canonical name.
    async fn domain_by_name(&self, _name: &MailDomainName) -> Result<MailDomain, MailServiceError> {
        Err(MailServiceError::NotFound)
    }
    /// Fetch a protected mailbox by canonical address.
    async fn mailbox_by_address(
        &self,
        _address: &MailAddress,
    ) -> Result<StoredMailbox, MailServiceError> {
        Err(MailServiceError::NotFound)
    }
    /// Persist a forwarding alias.
    async fn save_alias(&self, _alias: &MailAlias) -> Result<(), MailServiceError> {
        Ok(())
    }
    /// List aliases belonging to a domain.
    async fn aliases(&self, _domain_id: Uuid) -> Result<Vec<MailAlias>, MailServiceError> {
        Ok(Vec::new())
    }
    /// Delete a domain and its dependent metadata.
    async fn delete_domain(&self, _id: Uuid) -> Result<(), MailServiceError> {
        Err(MailServiceError::NotFound)
    }
    /// Delete one mailbox by stable ID.
    async fn delete_mailbox(&self, _id: Uuid) -> Result<(), MailServiceError> {
        Err(MailServiceError::NotFound)
    }
    /// Delete one alias by stable ID.
    async fn delete_alias(&self, _id: Uuid) -> Result<(), MailServiceError> {
        Err(MailServiceError::NotFound)
    }
    /// Fetch one alias by stable ID.
    async fn alias(&self, _id: Uuid) -> Result<MailAlias, MailServiceError> {
        Err(MailServiceError::NotFound)
    }
    /// Return global domain and mailbox counts.
    async fn counts(&self) -> Result<(usize, usize), MailServiceError> {
        Ok((0, 0))
    }
    /// Count objects removed by a cascading domain deletion.
    async fn dependent_counts(&self, domain_id: Uuid) -> Result<(usize, usize), MailServiceError> {
        Ok((
            self.mailboxes(domain_id).await?.len(),
            self.aliases(domain_id).await?.len(),
        ))
    }
}
/// Isolated Postfix/Dovecot configuration transaction.
#[async_trait]
pub trait MailConfigurator: Send + Sync {
    /// Validate a candidate, atomically apply it, reload, or roll back.
    async fn validate_apply_reload(&self, domain: &MailDomain) -> Result<(), MailServiceError>;
}
/// Dependency and DNS/TLS readiness port.
#[async_trait]
pub trait ReadinessPort: Send + Sync {
    /// Check required services, TLS, ports, storage, hostname, and DNS.
    async fn check(&self, domain: &MailDomainName) -> Result<Readiness, MailServiceError>;
}
/// Backup registration hook.
#[async_trait]
pub trait BackupHook: Send + Sync {
    /// Register mail metadata and virtual storage with backup selection.
    async fn register_domain(&self, domain_id: Uuid) -> Result<(), MailServiceError>;
}

/// Hosted mail orchestration service.
pub struct MailService {
    repo: Arc<dyn MailRepository>,
    config: Arc<dyn MailConfigurator>,
    readiness: Arc<dyn ReadinessPort>,
    backup: Arc<dyn BackupHook>,
    audit: Arc<dyn AuditService>,
    queue: Arc<dyn openpanel_domain::MtaQueuePort>,
    deletion_tokens: Mutex<HashMap<String, (Uuid, u64)>>,
}
impl MailService {
    /// Construct from public ports.
    pub fn new(
        repo: Arc<dyn MailRepository>,
        config: Arc<dyn MailConfigurator>,
        readiness: Arc<dyn ReadinessPort>,
        backup: Arc<dyn BackupHook>,
        audit: Arc<dyn AuditService>,
        queue: Arc<dyn openpanel_domain::MtaQueuePort>,
    ) -> Self {
        Self {
            repo,
            config,
            readiness,
            backup,
            audit,
            queue,
            deletion_tokens: Mutex::new(HashMap::new()),
        }
    }

    fn owner(role: Role) -> Result<(), MailServiceError> {
        if role == Role::Owner {
            Ok(())
        } else {
            Err(MailServiceError::Forbidden)
        }
    }

    fn authorize_domain(
        actor: Uuid,
        role: Role,
        domain: &MailDomain,
    ) -> Result<(), MailServiceError> {
        if matches!(role, Role::Owner | Role::Admin) || domain.owner_id == actor {
            Ok(())
        } else {
            Err(MailServiceError::Forbidden)
        }
    }

    /// Run global dependency diagnostics.
    pub async fn readiness(&self) -> Result<Readiness, MailServiceError> {
        self.readiness
            .check(&MailDomainName::new("example.invalid").map_err(|_| MailServiceError::Invalid)?)
            .await
    }

    /// Create disabled mail domain and register it for backup.
    pub async fn create_domain(
        &self,
        actor: Uuid,
        role: Role,
        name: &str,
    ) -> Result<MailDomain, MailServiceError> {
        Self::owner(role)?;
        let domain = MailDomain {
            id: Uuid::new_v4(),
            owner_id: actor,
            name: MailDomainName::new(name).map_err(|_| MailServiceError::Invalid)?,
            enabled: false,
            quota_bytes: 10 * 1024 * 1024 * 1024,
        };
        self.repo.save_domain(&domain).await?;
        self.backup.register_domain(domain.id).await?;
        self.audit(actor, "domain_created", domain.id).await?;
        Ok(domain)
    }

    /// List domains, scoped for Users.
    pub async fn domains(
        &self,
        actor: Uuid,
        role: Role,
    ) -> Result<Vec<MailDomain>, MailServiceError> {
        self.repo
            .domains((role == Role::User).then_some(actor))
            .await
    }

    /// Enable after readiness and explicit external-DNS acknowledgement.
    pub async fn enable_domain(
        &self,
        actor: Uuid,
        role: Role,
        id: Uuid,
        acknowledged: bool,
    ) -> Result<MailDomain, MailServiceError> {
        Self::owner(role)?;
        let mut domain = self.repo.domain(id).await?;
        let readiness = self.readiness.check(&domain.name).await?;
        if !readiness.ready && !(acknowledged && readiness.can_acknowledge) {
            return Err(MailServiceError::NotReady(
                readiness
                    .expected_mx
                    .unwrap_or_else(|| "mandatory checks failed".into()),
            ));
        }
        self.config.validate_apply_reload(&domain).await?;
        domain.enabled = true;
        self.repo.save_domain(&domain).await?;
        self.audit(actor, "domain_enabled", id).await?;
        Ok(domain)
    }

    /// Disable delivery for a managed domain after a validated config transaction.
    pub async fn disable_domain(
        &self,
        actor: Uuid,
        role: Role,
        id: Uuid,
    ) -> Result<MailDomain, MailServiceError> {
        Self::owner(role)?;
        let mut domain = self.repo.domain(id).await?;
        domain.enabled = false;
        self.config.validate_apply_reload(&domain).await?;
        self.repo.save_domain(&domain).await?;
        self.audit(actor, "domain_disabled", id).await?;
        Ok(domain)
    }

    /// Create mailbox and return plaintext only in this response.
    pub async fn create_mailbox(
        &self,
        actor: Uuid,
        role: Role,
        domain_id: Uuid,
        local: &str,
        quota: MailQuota,
        password: Option<&str>,
    ) -> Result<MailboxCredential, MailServiceError> {
        let domain = self.repo.domain(domain_id).await?;
        Self::authorize_domain(actor, role, &domain)?;
        let existing = self.repo.mailboxes(domain_id).await?;
        let used: u64 = existing.iter().map(|item| item.mailbox.quota.bytes()).sum();
        if used.saturating_add(quota.bytes()) > domain.quota_bytes {
            return Err(MailServiceError::Invalid);
        }
        let generated;
        let plaintext = match password {
            Some(value) => value,
            None => {
                const ALPHABET: &[u8] =
                    b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*";
                generated = (0..24)
                    .map(|_| {
                        let index = rand::thread_rng().gen_range(0..ALPHABET.len());
                        char::from(ALPHABET[index])
                    })
                    .collect::<String>();
                &generated
            }
        };
        let hash = Password::hash(plaintext).map_err(|_| MailServiceError::Invalid)?;
        let mailbox = Mailbox {
            id: Uuid::new_v4(),
            domain_id,
            address: MailAddress::new(local, domain.name).map_err(|_| MailServiceError::Invalid)?,
            quota,
            enabled: true,
        };
        self.repo
            .save_mailbox(&StoredMailbox {
                mailbox: mailbox.clone(),
                password_hash: hash.hash_str().to_owned(),
            })
            .await?;
        self.audit(actor, "mailbox_created", mailbox.id).await?;
        Ok(MailboxCredential {
            mailbox,
            password: plaintext.to_owned(),
        })
    }

    /// List secret-free mailboxes.
    pub async fn mailboxes(
        &self,
        actor: Uuid,
        role: Role,
        domain_id: Uuid,
    ) -> Result<Vec<Mailbox>, MailServiceError> {
        let domain = self.repo.domain(domain_id).await?;
        Self::authorize_domain(actor, role, &domain)?;
        Ok(self
            .repo
            .mailboxes(domain_id)
            .await?
            .into_iter()
            .map(|stored| stored.mailbox)
            .collect())
    }

    /// Resolve one authorized, enabled mailbox by canonical address.
    ///
    /// `mailbox-surfaces`: every webmail session and mailbox operation
    /// resolves through the authenticated user and the authorized
    /// domain/mailbox relationship. Absence maps to `Forbidden` (never
    /// `NotFound`) so callers cannot probe cross-account existence;
    /// disabled mailboxes are also `Forbidden`. Denied attempts emit a
    /// redacted `mailbox_access_denied` audit event without credentials
    /// or message content.
    pub async fn resolve_authorized_mailbox(
        &self,
        actor: Uuid,
        role: Role,
        address: &str,
    ) -> Result<Mailbox, MailServiceError> {
        let parsed = MailAddress::parse(address).map_err(|_| MailServiceError::Forbidden)?;
        let stored = match self.repo.mailbox_by_address(&parsed).await {
            Ok(stored) => stored,
            Err(_) => {
                self.audit_denied(actor, "mailbox_access_denied").await;
                return Err(MailServiceError::Forbidden);
            }
        };
        let domain = match self.repo.domain(stored.mailbox.domain_id).await {
            Ok(domain) => domain,
            Err(_) => {
                self.audit_denied(actor, "mailbox_access_denied").await;
                return Err(MailServiceError::Forbidden);
            }
        };
        if Self::authorize_domain(actor, role, &domain).is_err() || !stored.mailbox.enabled {
            self.audit_denied(actor, "mailbox_access_denied").await;
            return Err(MailServiceError::Forbidden);
        }
        Ok(stored.mailbox)
    }

    /// Default mailbox for webmail entry: the caller's own address when
    /// it resolves, else the first mailbox in the caller's visible
    /// domains. `mailbox-surfaces`: never synthesizes an address and
    /// never falls back to a fixed demo mailbox.
    pub async fn default_mailbox_for_user(
        &self,
        actor: Uuid,
        role: Role,
        email: &str,
    ) -> Result<Mailbox, MailServiceError> {
        if let Ok(mailbox) = self.resolve_authorized_mailbox(actor, role, email).await {
            return Ok(mailbox);
        }
        let domains = self
            .repo
            .domains((role == Role::User).then_some(actor))
            .await?;
        for domain in domains {
            if Self::authorize_domain(actor, role, &domain).is_err() {
                continue;
            }
            if let Ok(boxes) = self.repo.mailboxes(domain.id).await {
                for stored in boxes {
                    if stored.mailbox.enabled {
                        return Ok(stored.mailbox);
                    }
                }
            }
        }
        self.audit_denied(actor, "mailbox_access_denied").await;
        Err(MailServiceError::Forbidden)
    }

    /// List secret-free aliases after domain ownership authorization.
    pub async fn aliases(
        &self,
        actor: Uuid,
        role: Role,
        domain_id: Uuid,
    ) -> Result<Vec<MailAlias>, MailServiceError> {
        let domain = self.repo.domain(domain_id).await?;
        Self::authorize_domain(actor, role, &domain)?;
        self.repo.aliases(domain_id).await
    }

    /// Add a non-cyclic alias.
    pub async fn add_alias(
        &self,
        actor: Uuid,
        role: Role,
        domain_id: Uuid,
        source: &str,
        destination: &str,
    ) -> Result<MailAlias, MailServiceError> {
        let domain = self.repo.domain(domain_id).await?;
        Self::authorize_domain(actor, role, &domain)?;
        let source =
            MailAddress::new(source, domain.name).map_err(|_| MailServiceError::Invalid)?;
        let destination = MailAddress::parse(destination).map_err(|_| MailServiceError::Invalid)?;
        let mut graph = AliasGraph::default();
        for alias in self.repo.aliases(domain_id).await? {
            graph
                .add(alias.source, alias.destination)
                .map_err(|_| MailServiceError::Invalid)?;
        }
        graph
            .add(source.clone(), destination.clone())
            .map_err(|_| MailServiceError::Invalid)?;
        let alias = MailAlias {
            id: Uuid::new_v4(),
            domain_id,
            source,
            destination,
        };
        self.repo.save_alias(&alias).await?;
        self.audit(actor, "alias_created", alias.id).await?;
        Ok(alias)
    }

    /// Update quota by address.
    pub async fn update_quota(
        &self,
        actor: Uuid,
        role: Role,
        address: &str,
        quota: MailQuota,
    ) -> Result<Mailbox, MailServiceError> {
        let address = MailAddress::parse(address).map_err(|_| MailServiceError::Invalid)?;
        let mut stored = self.repo.mailbox_by_address(&address).await?;
        let domain = self.repo.domain(stored.mailbox.domain_id).await?;
        Self::authorize_domain(actor, role, &domain)?;
        stored.mailbox.quota = quota;
        self.repo.save_mailbox(&stored).await?;
        self.audit(actor, "quota_changed", stored.mailbox.id)
            .await?;
        Ok(stored.mailbox)
    }

    /// Rotate a mailbox password and return plaintext once.
    pub async fn rotate_password(
        &self,
        actor: Uuid,
        role: Role,
        address: &str,
        password: &str,
    ) -> Result<MailboxCredential, MailServiceError> {
        let address = MailAddress::parse(address).map_err(|_| MailServiceError::Invalid)?;
        let mut stored = self.repo.mailbox_by_address(&address).await?;
        let domain = self.repo.domain(stored.mailbox.domain_id).await?;
        Self::authorize_domain(actor, role, &domain)?;
        stored.password_hash = Password::hash(password)
            .map_err(|_| MailServiceError::Invalid)?
            .hash_str()
            .to_owned();
        self.repo.save_mailbox(&stored).await?;
        self.audit(actor, "password_rotated", stored.mailbox.id)
            .await?;
        Ok(MailboxCredential {
            mailbox: stored.mailbox,
            password: password.to_owned(),
        })
    }

    /// Enable or disable a mailbox after ownership authorization.
    pub async fn set_mailbox_enabled(
        &self,
        actor: Uuid,
        role: Role,
        address: &str,
        enabled: bool,
    ) -> Result<Mailbox, MailServiceError> {
        let address = MailAddress::parse(address).map_err(|_| MailServiceError::Invalid)?;
        let mut stored = self.repo.mailbox_by_address(&address).await?;
        let domain = self.repo.domain(stored.mailbox.domain_id).await?;
        Self::authorize_domain(actor, role, &domain)?;
        stored.mailbox.enabled = enabled;
        self.repo.save_mailbox(&stored).await?;
        self.audit(actor, "mailbox_enabled_changed", stored.mailbox.id)
            .await?;
        Ok(stored.mailbox)
    }

    /// Delete one mailbox after explicit confirmation.
    pub async fn delete_mailbox(
        &self,
        actor: Uuid,
        role: Role,
        address: &str,
        confirmed: bool,
    ) -> Result<(), MailServiceError> {
        if !confirmed {
            return Err(MailServiceError::Invalid);
        }
        let address = MailAddress::parse(address).map_err(|_| MailServiceError::Invalid)?;
        let stored = self.repo.mailbox_by_address(&address).await?;
        let domain = self.repo.domain(stored.mailbox.domain_id).await?;
        Self::authorize_domain(actor, role, &domain)?;
        self.repo.delete_mailbox(stored.mailbox.id).await?;
        self.audit(actor, "mailbox_deleted", stored.mailbox.id)
            .await
    }

    /// Delete one alias after ownership authorization and explicit confirmation.
    pub async fn delete_alias(
        &self,
        actor: Uuid,
        role: Role,
        alias_id: Uuid,
        confirmed: bool,
    ) -> Result<(), MailServiceError> {
        if !confirmed {
            return Err(MailServiceError::Invalid);
        }
        let alias = self.repo.alias(alias_id).await?;
        let domain = self.repo.domain(alias.domain_id).await?;
        Self::authorize_domain(actor, role, &domain)?;
        self.repo.delete_alias(alias_id).await?;
        self.audit(actor, "alias_deleted", alias_id).await
    }

    /// Read-only outbound-queue snapshot; degrades to `unknown` when
    /// the MTA cannot be queried.
    pub async fn queue_snapshot(
        &self,
    ) -> Result<openpanel_domain::MailQueueSnapshot, MailServiceError> {
        Ok(self
            .queue
            .snapshot()
            .await
            .unwrap_or_else(|_| openpanel_domain::MailQueueSnapshot::unknown()))
    }

    /// Aggregate diagnostics only.
    pub async fn status(&self) -> Result<MailStatus, MailServiceError> {
        let (domains, mailboxes) = self.repo.counts().await?;
        let snapshot = self
            .queue
            .snapshot()
            .await
            .unwrap_or_else(|_| openpanel_domain::MailQueueSnapshot::unknown());
        let health = match snapshot.health {
            openpanel_domain::QueueHealth::Ok => "ok",
            openpanel_domain::QueueHealth::Unknown => "unknown",
        };
        Ok(MailStatus {
            domains,
            mailboxes,
            queue_depth: snapshot.queue_depth,
            health: if snapshot.queue_depth == 0 && snapshot.oldest_deferred_at.is_none() {
                format!("{health}/ready")
            } else {
                health.to_owned()
            },
        })
    }

    /// Return dependency counts and issue a one-use, five-minute delete token.
    pub async fn preview_delete_domain(
        &self,
        actor: Uuid,
        role: Role,
        domain_id: Uuid,
    ) -> Result<DomainDeletionPreview, MailServiceError> {
        Self::owner(role)?;
        self.repo.domain(domain_id).await?;
        let (mailboxes, aliases) = self.repo.dependent_counts(domain_id).await?;
        let now = unix_time()?;
        let expires_at = now.checked_add(300).ok_or(MailServiceError::Invalid)?;
        let confirmation_token = Uuid::new_v4().to_string();
        self.deletion_tokens
            .lock()
            .map_err(|_| MailServiceError::Repository)?
            .insert(confirmation_token.clone(), (domain_id, expires_at));
        self.audit(actor, "domain_delete_previewed", domain_id)
            .await?;
        Ok(DomainDeletionPreview {
            domain_id,
            mailboxes,
            aliases,
            confirmation_token,
            expires_at,
        })
    }

    /// Delete a domain cascade after consuming its scoped confirmation token.
    pub async fn delete_domain(
        &self,
        actor: Uuid,
        role: Role,
        domain_id: Uuid,
        confirmation_token: &str,
    ) -> Result<(), MailServiceError> {
        Self::owner(role)?;
        {
            let mut tokens = self
                .deletion_tokens
                .lock()
                .map_err(|_| MailServiceError::Repository)?;
            let token = tokens
                .get(confirmation_token)
                .copied()
                .ok_or(MailServiceError::Invalid)?;
            if token.0 != domain_id || unix_time()? > token.1 {
                return Err(MailServiceError::Invalid);
            }
            tokens.remove(confirmation_token);
        }
        self.repo.delete_domain(domain_id).await?;
        self.audit(actor, "domain_deleted", domain_id).await
    }

    async fn audit(
        &self,
        actor: Uuid,
        operation: &str,
        target: Uuid,
    ) -> Result<(), MailServiceError> {
        self.audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::MailChanged,
                    AuditOutcome::Success,
                )
                .target(target.to_string())
                .metadata(serde_json::json!({"operation":operation})),
            )
            .await
            .map_err(|_| MailServiceError::Repository)
    }

    async fn audit_denied(&self, actor: Uuid, operation: &str) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::MailChanged,
                    AuditOutcome::Denied,
                )
                .target(actor.to_string())
                .metadata(serde_json::json!({"operation":operation})),
            )
            .await;
    }
}

fn unix_time() -> Result<u64, MailServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| MailServiceError::Invalid)
}
