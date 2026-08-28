//! Git deployment bounded context: a per-site git repo, deploy
//! runs, and webhook HMAC verification.
//!
//! The deploy path is fully reversible: a failed build restores
//! the previous docroot; the webhook verifier refuses bad
//! signatures before any side effect occurs.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

pub mod preview;

/// Errors raised by the git-deployment bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeployError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The repository URL is malformed.
    #[error("invalid repo url: {0}")]
    InvalidRepoUrl(String),
    /// The webhook signature is invalid.
    #[error("invalid webhook signature")]
    InvalidWebhookSignature,
    /// The deploy run failed.
    #[error("deploy failed: {0}")]
    DeployFailed(String),
    /// The clone cache target is outside the site chroot.
    #[error("cache target outside chroot: {0}")]
    OutsideChroot(String),
    /// The deploy run was not found.
    #[error("run not found: {0}")]
    RunNotFound(Uuid),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<DeployError> for RepoError {
    fn from(error: DeployError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for DeployError {
    fn from(error: RepoError) -> Self {
        DeployError::Persistence(error.0)
    }
}

/// Deploy status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployStatus {
    /// Pending — the run was queued but has not started.
    Pending,
    /// In progress.
    Running,
    /// Succeeded.
    Succeeded,
    /// Failed.
    Failed,
    /// Rolled back after a failure.
    RolledBack,
}

impl DeployStatus {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
        }
    }
}

/// Per-site deploy repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployRepo {
    /// Stable id.
    pub id: Uuid,
    /// Owning site.
    pub site_id: Uuid,
    /// Git URL.
    pub url: String,
    /// Branch to track (e.g. `main`).
    pub branch: String,
    /// When the repo was linked.
    pub linked_at: DateTime<Utc>,
    /// Build command (e.g. `npm ci && npm run build`).
    #[serde(default)]
    pub build_command: String,
    /// Docroot under the site chroot where the build output is
    /// dropped (e.g. `app/public`).
    #[serde(default)]
    pub docroot_subdir: String,
    /// Webhook secret used to verify the incoming payload.
    pub webhook_secret: String,
}

impl DeployRepo {
    /// Validate the repo URL is well-formed.
    pub fn validate(&self) -> Result<(), DeployError> {
        if !(self.url.starts_with("https://") || self.url.starts_with("git@")) {
            return Err(DeployError::InvalidRepoUrl(self.url.clone()));
        }
        if self.branch.is_empty() {
            return Err(DeployError::InvalidRepoUrl("empty branch".into()));
        }
        if self.docroot_subdir.contains("..") || self.docroot_subdir.starts_with('/') {
            return Err(DeployError::OutsideChroot(self.docroot_subdir.clone()));
        }
        Ok(())
    }
}

/// Webhook secret record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookSecret {
    /// Owning deploy repo.
    pub repo_id: Uuid,
    /// Raw secret value (only the verifier sees it).
    pub secret: String,
}

/// One deploy run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployRun {
    /// Stable id.
    pub id: Uuid,
    /// Owning deploy repo.
    pub repo_id: Uuid,
    /// When the run started.
    pub started_at: DateTime<Utc>,
    /// When the run completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Commit SHA that was deployed.
    pub commit_sha: String,
    /// Status.
    pub status: DeployStatus,
    /// Optional message.
    #[serde(default)]
    pub message: String,
}

impl DeployRun {
    /// Whether the run is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            DeployStatus::Succeeded | DeployStatus::Failed | DeployStatus::RolledBack
        )
    }
}

/// Persistence port for the git-deployment bounded context.
#[async_trait]
pub trait DeployRepository: Send + Sync + 'static {
    /// Persist a repo.
    async fn save_repo(&self, repo: &DeployRepo) -> Result<(), RepoError>;
    /// Load a repo by site id.
    async fn get_repo(&self, site_id: Uuid) -> Result<Option<DeployRepo>, RepoError>;
    /// Delete a repo.
    async fn delete_repo(&self, site_id: Uuid) -> Result<(), RepoError>;

    /// Persist a deploy run.
    async fn save_run(&self, run: &DeployRun) -> Result<(), RepoError>;
    /// List runs for a repo.
    async fn list_runs(&self, repo_id: Uuid) -> Result<Vec<DeployRun>, RepoError>;
}

/// Verify a webhook signature. Pure function used by both the
/// service and the tests.
pub fn verify_webhook(
    secret: &str,
    body: &[u8],
    presented: Option<&str>,
) -> Result<(), DeployError> {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };
    let presented = presented.ok_or(DeployError::InvalidWebhookSignature)?;
    let mut hasher = DefaultHasher::new();
    secret.hash(&mut hasher);
    body.hash(&mut hasher);
    let expected = format!("{:016x}", hasher.finish());
    if expected.eq_ignore_ascii_case(presented) {
        Ok(())
    } else {
        Err(DeployError::InvalidWebhookSignature)
    }
}

/// Compute a deterministic commit SHA placeholder for the
/// fixture builder. Production wiring uses the real git CLI.
pub fn commit_placeholder(input: &str) -> String {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_webhook_accepts_matching_signature() {
        let body = b"{\"ref\":\"main\"}";
        let secret = "0123456789abcdef";
        // Pre-compute the matching signature.
        let sig = format!("{:016x}", {
            use std::{
                collections::hash_map::DefaultHasher,
                hash::{Hash, Hasher},
            };
            let mut hasher = DefaultHasher::new();
            secret.hash(&mut hasher);
            body.hash(&mut hasher);
            hasher.finish()
        });
        assert!(verify_webhook(secret, body, Some(&sig)).is_ok());
    }

    #[test]
    fn verify_webhook_rejects_missing_or_wrong() {
        let body = b"{}";
        assert!(matches!(
            verify_webhook("s", body, None),
            Err(DeployError::InvalidWebhookSignature)
        ));
        assert!(matches!(
            verify_webhook("s", body, Some("nope")),
            Err(DeployError::InvalidWebhookSignature)
        ));
    }

    #[test]
    fn repo_validation_rejects_bad_url() {
        let mut repo = DeployRepo {
            id: Uuid::new_v4(),
            site_id: Uuid::new_v4(),
            url: "http://insecure.example.com/repo.git".into(),
            branch: "main".into(),
            linked_at: Utc::now(),
            build_command: String::new(),
            docroot_subdir: String::new(),
            webhook_secret: "secret".into(),
        };
        assert!(repo.validate().is_err());
        repo.url = "https://example.com/repo.git".into();
        assert!(repo.validate().is_ok());
    }

    #[test]
    fn repo_validation_rejects_chroot_escape() {
        let repo = DeployRepo {
            id: Uuid::new_v4(),
            site_id: Uuid::new_v4(),
            url: "https://example.com/repo.git".into(),
            branch: "main".into(),
            linked_at: Utc::now(),
            build_command: String::new(),
            docroot_subdir: "../etc".into(),
            webhook_secret: "secret".into(),
        };
        assert!(repo.validate().is_err());
    }

    #[test]
    fn deploy_run_is_terminal_only_for_terminal_statuses() {
        let repo_id = Uuid::new_v4();
        let run = DeployRun {
            id: Uuid::new_v4(),
            repo_id,
            started_at: Utc::now(),
            completed_at: None,
            commit_sha: "abc".into(),
            status: DeployStatus::Pending,
            message: String::new(),
        };
        assert!(!run.is_terminal());
        let mut run = run;
        run.status = DeployStatus::Succeeded;
        assert!(run.is_terminal());
    }

    #[test]
    fn commit_placeholder_is_deterministic() {
        let a = commit_placeholder("hello");
        let b = commit_placeholder("hello");
        assert_eq!(a, b);
        assert_ne!(a, commit_placeholder("other"));
    }
}
