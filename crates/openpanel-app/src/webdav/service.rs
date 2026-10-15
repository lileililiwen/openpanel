//! WebDAV account lifecycle and Basic-authentication use cases.

use std::{path::PathBuf, sync::Arc};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, Site, SiteRepository, User,
    webdav::{WebDavAccount, WebDavError, WebDavRepository},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Input accepted when creating a site WebDAV credential.
#[derive(Debug, Clone, Deserialize)]
#[allow(missing_docs)]
#[serde(deny_unknown_fields)]
pub struct CreateWebDavAccount {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub read_only: bool,
}

/// Optional account fields accepted by PATCH.
#[derive(Debug, Clone, Deserialize)]
#[allow(missing_docs)]
#[serde(deny_unknown_fields)]
pub struct UpdateWebDavAccount {
    pub enabled: Option<bool>,
    pub password: Option<String>,
    pub read_only: Option<bool>,
}

/// Secret-free account representation returned by all read surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[allow(missing_docs)]
pub struct WebDavAccountView {
    pub id: Uuid,
    pub site_id: Uuid,
    pub username: String,
    pub home: String,
    pub read_only: bool,
    pub enabled: bool,
    pub last_login_at: Option<chrono::DateTime<Utc>>,
    pub last_login_ip: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
}

impl From<&WebDavAccount> for WebDavAccountView {
    fn from(value: &WebDavAccount) -> Self {
        Self {
            id: value.id(),
            site_id: value.site_id(),
            username: value.username().into(),
            home: value.home().to_string_lossy().into_owned(),
            read_only: value.read_only(),
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
pub struct CreatedWebDavAccount {
    #[serde(flatten)]
    pub account: WebDavAccountView,
    pub plaintext_password_shown_once: bool,
}

/// Per-site WebDAV account use cases plus the DAV-facing authenticator.
pub struct WebDavService {
    repo: Arc<dyn WebDavRepository>,
    sites: Arc<dyn SiteRepository>,
    audit: Arc<dyn AuditService>,
}

impl WebDavService {
    /// Construct with persistence, site authorization, and audit.
    pub fn new(
        repo: Arc<dyn WebDavRepository>,
        sites: Arc<dyn SiteRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self { repo, sites, audit }
    }

    /// Create an account under a caller-accessible site.
    pub async fn create(
        &self,
        caller: &User,
        site_id: Uuid,
        input: CreateWebDavAccount,
    ) -> Result<CreatedWebDavAccount, WebDavError> {
        let site = self.site(caller, site_id).await?;
        if self
            .repo
            .find_by_username(site_id, &input.username)
            .await
            .map_err(internal)?
            .is_some()
        {
            return Err(WebDavError::Duplicate);
        }
        let account = WebDavAccount::new(
            Uuid::new_v4(),
            site_id,
            input.username,
            PathBuf::from(site.document_root()),
            &input.password,
            input.read_only,
            Utc::now(),
        )?;
        self.repo.create(&account).await.map_err(|error| {
            if error.to_string().contains("UNIQUE") {
                WebDavError::Duplicate
            } else {
                internal(error)
            }
        })?;
        self.changed(caller, &account, "create").await;
        Ok(CreatedWebDavAccount {
            account: (&account).into(),
            plaintext_password_shown_once: true,
        })
    }

    /// List secret-free accounts.
    pub async fn list(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<Vec<WebDavAccountView>, WebDavError> {
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

    /// Patch account state and password without ever returning a secret.
    pub async fn update(
        &self,
        caller: &User,
        site_id: Uuid,
        id: Uuid,
        input: UpdateWebDavAccount,
    ) -> Result<WebDavAccountView, WebDavError> {
        self.site(caller, site_id).await?;
        let mut account = self
            .repo
            .find(site_id, id)
            .await
            .map_err(internal)?
            .ok_or(WebDavError::NotFound)?;
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
        account.change_read_only(input.read_only.unwrap_or(account.read_only()));
        self.repo.update(&account).await.map_err(internal)?;
        self.changed(caller, &account, "update").await;
        Ok((&account).into())
    }

    /// Delete an account.
    pub async fn delete(&self, caller: &User, site_id: Uuid, id: Uuid) -> Result<(), WebDavError> {
        self.site(caller, site_id).await?;
        let account = self
            .repo
            .find(site_id, id)
            .await
            .map_err(internal)?
            .ok_or(WebDavError::NotFound)?;
        if !self.repo.delete(site_id, id).await.map_err(internal)? {
            return Err(WebDavError::NotFound);
        }
        self.changed(caller, &account, "delete").await;
        Ok(())
    }

    /// Authenticate a DAV request by account id and Basic credentials,
    /// recording login metadata and audit without ever exposing the
    /// secret cause of a denial.
    pub async fn authenticate(
        &self,
        account_id: Uuid,
        username: &str,
        plaintext: &str,
        remote_ip: &str,
    ) -> Result<WebDavAccount, WebDavError> {
        let account = self
            .repo
            .find_by_id(account_id)
            .await
            .map_err(internal)?
            .filter(|value| value.username() == username);
        let Some(mut account) = account else {
            self.denied(account_id, username).await;
            return Err(WebDavError::LoginDenied);
        };
        if !account.enabled() || !account.verify_password(plaintext)? {
            self.denied(account_id, username).await;
            return Err(WebDavError::LoginDenied);
        }
        account.record_login(Utc::now(), remote_ip.to_string());
        self.repo.update(&account).await.map_err(internal)?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(username, AuditAction::WebDavLogin, AuditOutcome::Success)
                    .target(account_id.to_string())
                    .metadata(serde_json::json!({"remote_ip":remote_ip})),
            )
            .await;
        Ok(account)
    }

    async fn denied(&self, account_id: Uuid, username: &str) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    username,
                    AuditAction::WebDavLoginDenied,
                    AuditOutcome::Denied,
                )
                .target(account_id.to_string()),
            )
            .await;
    }

    async fn site(&self, caller: &User, site_id: Uuid) -> Result<Site, WebDavError> {
        let site = self
            .sites
            .find_by_id(site_id)
            .await
            .map_err(internal)?
            .ok_or(WebDavError::SiteNotFound)?;
        if caller.role() != Role::Owner && site.owner_id() != caller.id() {
            return Err(WebDavError::Forbidden);
        }
        Ok(site)
    }

    async fn changed(&self, caller: &User, account: &WebDavAccount, operation: &str) {
        let _=self.audit.record(AuditEvent::new(caller.username().as_str(),AuditAction::WebDavChanged,AuditOutcome::Success).target(account.id().to_string()).metadata(serde_json::json!({"site_id":account.site_id(),"operation":operation,"username":account.username()}))).await;
    }
}

fn internal(error: impl std::fmt::Display) -> WebDavError {
    WebDavError::Internal(error.to_string())
}
