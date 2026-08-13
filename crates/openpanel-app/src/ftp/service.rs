//! FTP account lifecycle, authentication, and live-session accounting.

use std::{
    collections::HashMap,
    net::IpAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, Site, SiteRepository, User,
    ftp::{FtpAccount, FtpError, FtpLimits, FtpRepository, FtpSession},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Input accepted when creating a site FTP credential.
#[derive(Debug, Clone, Deserialize)]
#[allow(missing_docs)]
#[serde(deny_unknown_fields)]
pub struct CreateFtpAccount {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub read_only: bool,
    pub bandwidth_kb_per_session: Option<u64>,
    pub max_concurrent_connections: Option<u16>,
}

/// Optional account fields accepted by PATCH.
#[derive(Debug, Clone, Deserialize)]
#[allow(missing_docs)]
#[serde(deny_unknown_fields)]
pub struct UpdateFtpAccount {
    pub enabled: Option<bool>,
    pub password: Option<String>,
    pub read_only: Option<bool>,
    pub bandwidth_kb_per_session: Option<u64>,
    pub max_concurrent_connections: Option<u16>,
}

/// Secret-free account representation returned by all read surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[allow(missing_docs)]
pub struct FtpAccountView {
    pub id: Uuid,
    pub site_id: Uuid,
    pub username: String,
    pub home: String,
    pub read_only: bool,
    pub bandwidth_kb_per_session: u64,
    pub max_concurrent_connections: u16,
    pub enabled: bool,
    pub last_login_at: Option<chrono::DateTime<Utc>>,
    pub last_login_ip: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
}

impl From<&FtpAccount> for FtpAccountView {
    fn from(value: &FtpAccount) -> Self {
        Self {
            id: value.id(),
            site_id: value.site_id(),
            username: value.username().into(),
            home: value.home().to_string_lossy().into_owned(),
            read_only: value.read_only(),
            bandwidth_kb_per_session: value.limits().bandwidth_kb_per_session(),
            max_concurrent_connections: value.limits().max_concurrent_connections(),
            enabled: value.enabled(),
            last_login_at: value.last_login_at(),
            last_login_ip: value.last_login_ip().map(str::to_owned),
            created_at: value.created_at(),
        }
    }
}

/// Create response marking the one-time password disclosure contract.
#[derive(Debug, Clone, Serialize)]
#[allow(missing_docs)]
pub struct CreatedFtpAccount {
    #[serde(flatten)]
    pub account: FtpAccountView,
    pub plaintext_password_shown_once: bool,
}

/// In-process live-session registry used for limits and management output.
#[derive(Default)]
pub struct FtpSessionRegistry {
    sessions: Mutex<HashMap<Uuid, Vec<FtpSession>>>,
}

impl FtpSessionRegistry {
    /// Count every authenticated FTP session across all accounts.
    pub fn total(&self) -> usize {
        self.sessions
            .lock()
            .map(|rows| rows.values().map(Vec::len).sum())
            .unwrap_or(usize::MAX)
    }

    /// Snapshot sessions for one account.
    pub fn list(&self, account_id: Uuid) -> Vec<FtpSession> {
        self.sessions
            .lock()
            .map(|rows| rows.get(&account_id).cloned().unwrap_or_default())
            .unwrap_or_default()
    }

    /// Register a session if the account limit permits it.
    pub fn open(&self, account: &FtpAccount, remote_ip: IpAddr) -> Result<FtpSession, FtpError> {
        let mut rows = self
            .sessions
            .lock()
            .map_err(|_| FtpError::Internal("session registry unavailable".into()))?;
        let active = rows.entry(account.id()).or_default();
        let count = u16::try_from(active.len()).unwrap_or(u16::MAX);
        if !account.can_open_session(count) {
            return Err(FtpError::ConnectionLimit);
        }
        let session = FtpSession {
            account_id: account.id(),
            remote_ip: remote_ip.to_string(),
            connected_at: Utc::now(),
            bytes_transferred: 0,
        };
        active.push(session.clone());
        Ok(session)
    }

    /// Remove the session identified by its connection timestamp and peer.
    pub fn close(&self, session: &FtpSession) {
        if let Ok(mut rows) = self.sessions.lock()
            && let Some(active) = rows.get_mut(&session.account_id)
        {
            active.retain(|item| item != session);
        }
    }

    /// Close one live session for an account after a transport logout event.
    pub fn close_one(&self, account_id: Uuid) {
        if let Ok(mut rows) = self.sessions.lock()
            && let Some(active) = rows.get_mut(&account_id)
        {
            active.pop();
        }
    }

    /// Add completed transfer bytes to the most recent session for an account.
    pub fn add_bytes(&self, account_id: Uuid, bytes: u64) {
        if let Ok(mut rows) = self.sessions.lock()
            && let Some(session) = rows.get_mut(&account_id).and_then(|items| items.last_mut())
        {
            session.bytes_transferred = session.bytes_transferred.saturating_add(bytes);
        }
    }
}

/// Per-site FTP account use cases.
pub struct FtpService {
    repo: Arc<dyn FtpRepository>,
    sites: Arc<dyn SiteRepository>,
    audit: Arc<dyn AuditService>,
    sessions: Arc<FtpSessionRegistry>,
}

impl FtpService {
    /// Construct with persistence, site authorization, audit, and sessions.
    pub fn new(
        repo: Arc<dyn FtpRepository>,
        sites: Arc<dyn SiteRepository>,
        audit: Arc<dyn AuditService>,
        sessions: Arc<FtpSessionRegistry>,
    ) -> Self {
        Self {
            repo,
            sites,
            audit,
            sessions,
        }
    }

    /// Create an account under a caller-accessible site.
    pub async fn create(
        &self,
        caller: &User,
        site_id: Uuid,
        input: CreateFtpAccount,
    ) -> Result<CreatedFtpAccount, FtpError> {
        let site = self.site(caller, site_id).await?;
        if self
            .repo
            .find_by_username(site_id, &input.username)
            .await
            .map_err(internal)?
            .is_some()
        {
            return Err(FtpError::Duplicate);
        }
        let limits = FtpLimits::new(
            input.bandwidth_kb_per_session.unwrap_or(1_048_576),
            input.max_concurrent_connections.unwrap_or(4),
        )?;
        let account = FtpAccount::new(
            Uuid::new_v4(),
            site_id,
            input.username,
            PathBuf::from(site.document_root()),
            &input.password,
            input.read_only,
            limits,
            Utc::now(),
        )?;
        self.repo.create(&account).await.map_err(|error| {
            if error.to_string().contains("UNIQUE") {
                FtpError::Duplicate
            } else {
                internal(error)
            }
        })?;
        self.changed(caller, &account, "create").await;
        Ok(CreatedFtpAccount {
            account: (&account).into(),
            plaintext_password_shown_once: true,
        })
    }

    /// List secret-free accounts.
    pub async fn list(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<Vec<FtpAccountView>, FtpError> {
        self.site(caller, site_id).await?;
        Ok(self
            .repo
            .list(site_id)
            .await
            .map_err(internal)?
            .iter()
            .map(Into::into)
            .collect())
    }

    /// Enable an account.
    pub async fn enable(
        &self,
        caller: &User,
        site_id: Uuid,
        id: Uuid,
    ) -> Result<FtpAccountView, FtpError> {
        self.set_enabled(caller, site_id, id, true).await
    }

    /// Disable an account.
    pub async fn disable(
        &self,
        caller: &User,
        site_id: Uuid,
        id: Uuid,
    ) -> Result<FtpAccountView, FtpError> {
        self.set_enabled(caller, site_id, id, false).await
    }

    /// Patch account state, password, and limits without ever returning a secret.
    pub async fn update(
        &self,
        caller: &User,
        site_id: Uuid,
        id: Uuid,
        input: UpdateFtpAccount,
    ) -> Result<FtpAccountView, FtpError> {
        self.site(caller, site_id).await?;
        let mut account = self
            .repo
            .find(site_id, id)
            .await
            .map_err(internal)?
            .ok_or(FtpError::NotFound)?;
        if let Some(password) = input.password {
            account.change_password(&password)?;
        }
        if let Some(enabled) = input.enabled {
            if enabled {
                account.enable();
            } else {
                account.disable(Utc::now());
            }
        }
        let old = account.limits();
        let limits = FtpLimits::new(
            input
                .bandwidth_kb_per_session
                .unwrap_or(old.bandwidth_kb_per_session()),
            input
                .max_concurrent_connections
                .unwrap_or(old.max_concurrent_connections()),
        )?;
        account.change_policy(input.read_only.unwrap_or(account.read_only()), limits);
        self.repo.update(&account).await.map_err(internal)?;
        self.changed(caller, &account, "update").await;
        Ok((&account).into())
    }

    /// Delete an account.
    pub async fn delete(&self, caller: &User, site_id: Uuid, id: Uuid) -> Result<(), FtpError> {
        self.site(caller, site_id).await?;
        let account = self
            .repo
            .find(site_id, id)
            .await
            .map_err(internal)?
            .ok_or(FtpError::NotFound)?;
        if !self.repo.delete(site_id, id).await.map_err(internal)? {
            return Err(FtpError::NotFound);
        }
        self.changed(caller, &account, "delete").await;
        Ok(())
    }

    /// List current sessions for an account.
    pub async fn sessions(
        &self,
        caller: &User,
        site_id: Uuid,
        id: Uuid,
    ) -> Result<Vec<FtpSession>, FtpError> {
        self.site(caller, site_id).await?;
        self.repo
            .find(site_id, id)
            .await
            .map_err(internal)?
            .ok_or(FtpError::NotFound)?;
        Ok(self.sessions.list(id))
    }

    async fn set_enabled(
        &self,
        caller: &User,
        site_id: Uuid,
        id: Uuid,
        enabled: bool,
    ) -> Result<FtpAccountView, FtpError> {
        self.site(caller, site_id).await?;
        let mut account = self
            .repo
            .find(site_id, id)
            .await
            .map_err(internal)?
            .ok_or(FtpError::NotFound)?;
        if enabled {
            account.enable();
        } else {
            account.disable(Utc::now());
        }
        self.repo.update(&account).await.map_err(internal)?;
        self.changed(caller, &account, if enabled { "enable" } else { "disable" })
            .await;
        Ok((&account).into())
    }

    async fn site(&self, caller: &User, site_id: Uuid) -> Result<Site, FtpError> {
        let site = self
            .sites
            .find_by_id(site_id)
            .await
            .map_err(internal)?
            .ok_or(FtpError::SiteNotFound)?;
        if caller.role() != Role::Owner && site.owner_id() != caller.id() {
            return Err(FtpError::Forbidden);
        }
        Ok(site)
    }

    async fn changed(&self, caller: &User, account: &FtpAccount, operation: &str) {
        let _=self.audit.record(AuditEvent::new(caller.username().as_str(),AuditAction::FtpChanged,AuditOutcome::Success).target(account.id().to_string()).metadata(serde_json::json!({"site_id":account.site_id(),"operation":operation,"username":account.username()}))).await;
    }
}

/// Authentication adapter sharing the account repository, site membership, and password primitive.
pub struct FtpAuthenticator {
    repo: Arc<dyn FtpRepository>,
    sites: Arc<dyn SiteRepository>,
    audit: Arc<dyn AuditService>,
    sessions: Arc<FtpSessionRegistry>,
}

impl FtpAuthenticator {
    /// Construct the authentication adapter.
    pub fn new(
        repo: Arc<dyn FtpRepository>,
        sites: Arc<dyn SiteRepository>,
        audit: Arc<dyn AuditService>,
        sessions: Arc<FtpSessionRegistry>,
    ) -> Self {
        Self {
            repo,
            sites,
            audit,
            sessions,
        }
    }

    /// Authenticate a site-qualified credential and reserve a session slot.
    pub async fn authenticate(
        &self,
        site_id: Uuid,
        username: &str,
        plaintext: &str,
        remote_ip: IpAddr,
    ) -> Result<(FtpAccount, FtpSession), FtpError> {
        if self
            .sites
            .find_by_id(site_id)
            .await
            .map_err(internal)?
            .is_none()
        {
            return self.denied(site_id, username).await;
        }
        let mut account = match self
            .repo
            .find_by_username(site_id, username)
            .await
            .map_err(internal)?
        {
            Some(value) => value,
            None => return self.denied(site_id, username).await,
        };
        if !account.enabled() || !account.verify_password(plaintext)? {
            return self.denied(site_id, username).await;
        }
        let session = match self.sessions.open(&account, remote_ip) {
            Ok(value) => value,
            Err(error) => {
                let _ = self
                    .audit
                    .record(
                        AuditEvent::new(
                            username,
                            AuditAction::FtpConcurrentLimit,
                            AuditOutcome::Denied,
                        )
                        .target(site_id.to_string()),
                    )
                    .await;
                return Err(error);
            }
        };
        account.record_login(Utc::now(), remote_ip.to_string());
        self.repo.update(&account).await.map_err(internal)?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(username, AuditAction::FtpLogin, AuditOutcome::Success)
                    .target(site_id.to_string())
                    .metadata(serde_json::json!({"remote_ip":remote_ip})),
            )
            .await;
        Ok((account, session))
    }

    async fn denied<T>(&self, site_id: Uuid, username: &str) -> Result<T, FtpError> {
        let _ = self
            .audit
            .record(
                AuditEvent::new(username, AuditAction::FtpLoginDenied, AuditOutcome::Denied)
                    .target(site_id.to_string()),
            )
            .await;
        Err(FtpError::LoginDenied)
    }
}

fn internal(error: impl std::fmt::Display) -> FtpError {
    FtpError::Internal(error.to_string())
}
