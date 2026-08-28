//! Preview deployment domain model.
//!
//! A preview is a per-PR throwaway environment derived from a git
//! deployment. It is modelled as a state machine (`Creating →
//! Building → Ready`, terminal `Failed` / `Destroyed`) with pure
//! transition functions; side effects (DNS, nginx, runtime slots)
//! live in the application service.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Persistence port for preview environments.
#[async_trait::async_trait]
pub trait PreviewRepository: Send + Sync + 'static {
    /// Persist a preview (insert or update).
    async fn save(&self, preview: &PreviewEnvironment) -> Result<(), RepoError>;
    /// Load a preview by id.
    async fn get(&self, id: Uuid) -> Result<Option<PreviewEnvironment>, RepoError>;
    /// List live (non-destroyed) previews for a repo.
    async fn list_live(&self, repo_id: Uuid) -> Result<Vec<PreviewEnvironment>, RepoError>;
    /// List all previews for a site.
    async fn list_for_site(&self, site_id: Uuid) -> Result<Vec<PreviewEnvironment>, RepoError>;
    /// List expired, non-destroyed previews (for the reaper).
    async fn list_expired(&self, now: DateTime<Utc>) -> Result<Vec<PreviewEnvironment>, RepoError>;
    /// Delete a preview row.
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
}

/// Domain validation and transition failures for previews.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreviewError {
    /// The preview is already destroyed; no further transitions.
    #[error("preview is already gone")]
    AlreadyGone,
    /// A value or transition is invalid.
    #[error("invalid preview: {0}")]
    Invalid(String),
    /// The per-repo preview cap has been reached.
    #[error("preview cap reached")]
    CapReached,
    /// The PR number is invalid (≤ 0 or non-numeric).
    #[error("invalid pull request number: {0}")]
    InvalidPr(String),
}

/// Lifecycle state of a preview environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewState {
    /// Slot provisioned, build not yet started.
    Creating,
    /// Build in progress.
    Building,
    /// Build succeeded; preview is live.
    Ready,
    /// Build failed.
    Failed,
    /// Closed, expired, or evicted.
    Destroyed,
}

impl PreviewState {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Creating => "creating",
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Destroyed => "destroyed",
        }
    }
}

/// A preview environment aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewEnvironment {
    id: Uuid,
    site_id: Uuid,
    repo_id: Uuid,
    pr_number: u32,
    state: PreviewState,
    hostname: String,
    created_at: DateTime<Utc>,
    ready_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    destroyed_at: Option<DateTime<Utc>>,
    destroy_reason: Option<String>,
}

impl PreviewEnvironment {
    /// Create a new preview in the `Creating` state.
    ///
    /// `base_domain` is the preview base (e.g. `pr.example.com`); the
    /// derived hostname is `{pr}.{base_domain}`.
    pub fn new(
        id: Uuid,
        site_id: Uuid,
        repo_id: Uuid,
        pr_number: u32,
        base_domain: &str,
        now: DateTime<Utc>,
    ) -> Result<Self, PreviewError> {
        let hostname = derive_hostname(pr_number, base_domain)?;
        Ok(Self {
            id,
            site_id,
            repo_id,
            pr_number,
            state: PreviewState::Creating,
            hostname,
            created_at: now,
            ready_at: None,
            expires_at: None,
            destroyed_at: None,
            destroy_reason: None,
        })
    }

    /// Reconstruct a preview from persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        site_id: Uuid,
        repo_id: Uuid,
        pr_number: u32,
        state: PreviewState,
        hostname: String,
        created_at: DateTime<Utc>,
        ready_at: Option<DateTime<Utc>>,
        expires_at: Option<DateTime<Utc>>,
        destroyed_at: Option<DateTime<Utc>>,
        destroy_reason: Option<String>,
    ) -> Self {
        Self {
            id,
            site_id,
            repo_id,
            pr_number,
            state,
            hostname,
            created_at,
            ready_at,
            expires_at,
            destroyed_at,
            destroy_reason,
        }
    }

    /// Transition `Creating → Building`.
    pub fn start_building(&mut self, now: DateTime<Utc>) -> Result<(), PreviewError> {
        self.transition(PreviewState::Building, now)
    }

    /// Transition `Ready → Building` (rebuild).
    pub fn rebuild(&mut self, now: DateTime<Utc>) -> Result<(), PreviewError> {
        self.transition(PreviewState::Building, now)
    }

    /// Transition `Building → Ready`, setting `ready_at` and
    /// `expires_at = ready_at + ttl_hours`.
    pub fn mark_ready(&mut self, now: DateTime<Utc>, ttl_hours: u32) -> Result<(), PreviewError> {
        if self.state == PreviewState::Destroyed {
            return Err(PreviewError::AlreadyGone);
        }
        if self.state != PreviewState::Building {
            return Err(PreviewError::Invalid(format!(
                "cannot mark ready from {:?}",
                self.state
            )));
        }
        self.state = PreviewState::Ready;
        self.ready_at = Some(now);
        self.expires_at = Some(expires_at(now, ttl_hours));
        Ok(())
    }

    /// Transition `Creating`/`Building → Failed`.
    pub fn fail(&mut self, now: DateTime<Utc>) -> Result<(), PreviewError> {
        self.transition(PreviewState::Failed, now)
    }

    /// Destroy the preview (from any non-destroyed state).
    pub fn destroy(
        &mut self,
        now: DateTime<Utc>,
        reason: impl Into<String>,
    ) -> Result<(), PreviewError> {
        if self.state == PreviewState::Destroyed {
            return Err(PreviewError::AlreadyGone);
        }
        self.state = PreviewState::Destroyed;
        self.destroyed_at = Some(now);
        self.destroy_reason = Some(reason.into());
        Ok(())
    }

    /// Whether the preview is live and past its TTL.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.state == PreviewState::Ready && self.expires_at.is_some_and(|expires| now >= expires)
    }

    /// Whether the preview is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, PreviewState::Failed | PreviewState::Destroyed)
    }

    fn transition(&mut self, target: PreviewState, now: DateTime<Utc>) -> Result<(), PreviewError> {
        if self.state == PreviewState::Destroyed {
            return Err(PreviewError::AlreadyGone);
        }
        let legal = matches!(
            (self.state, target),
            (PreviewState::Creating, PreviewState::Building)
                | (PreviewState::Creating, PreviewState::Failed)
                | (PreviewState::Building, PreviewState::Failed)
                | (PreviewState::Ready, PreviewState::Building)
        );
        if !legal {
            return Err(PreviewError::Invalid(format!(
                "illegal transition {:?} -> {:?}",
                self.state, target
            )));
        }
        self.state = target;
        let _ = now;
        Ok(())
    }

    /// Preview id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Site id.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Repo id.
    pub fn repo_id(&self) -> Uuid {
        self.repo_id
    }

    /// PR number.
    pub fn pr_number(&self) -> u32 {
        self.pr_number
    }

    /// Current state.
    pub fn state(&self) -> PreviewState {
        self.state
    }

    /// Derived hostname.
    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    /// When the preview was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// When the preview became ready.
    pub fn ready_at(&self) -> Option<DateTime<Utc>> {
        self.ready_at
    }

    /// When the preview expires.
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }

    /// When the preview was destroyed.
    pub fn destroyed_at(&self) -> Option<DateTime<Utc>> {
        self.destroyed_at
    }

    /// Why the preview was destroyed.
    pub fn destroy_reason(&self) -> Option<&str> {
        self.destroy_reason.as_deref()
    }
}

/// Derive the preview hostname for a PR on a base domain.
///
/// PR 42 on `pr.example.com` yields `42.pr.example.com`. PR numbers
/// ≤ 0 are rejected.
pub fn derive_hostname(pr_number: u32, base_domain: &str) -> Result<String, PreviewError> {
    if pr_number == 0 {
        return Err(PreviewError::InvalidPr(pr_number.to_string()));
    }
    if base_domain.trim().is_empty() {
        return Err(PreviewError::Invalid("base domain is empty".into()));
    }
    Ok(format!("{pr_number}.{base_domain}"))
}

/// Compute the expiry time for a preview that became ready at
/// `ready_at` with a `ttl_hours` lifetime.
pub fn expires_at(ready_at: DateTime<Utc>, ttl_hours: u32) -> DateTime<Utc> {
    ready_at + chrono::Duration::hours(i64::from(ttl_hours))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn preview(now: DateTime<Utc>) -> PreviewEnvironment {
        PreviewEnvironment::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            42,
            "pr.example.com",
            now,
        )
        .unwrap()
    }

    #[test]
    fn creating_building_ready_is_legal() {
        let now = now();
        let mut p = preview(now);
        assert_eq!(p.state(), PreviewState::Creating);
        p.start_building(now).unwrap();
        assert_eq!(p.state(), PreviewState::Building);
        p.mark_ready(now, 72).unwrap();
        assert_eq!(p.state(), PreviewState::Ready);
        assert_eq!(p.ready_at(), Some(now));
        assert_eq!(p.expires_at(), Some(expires_at(now, 72)));
    }

    #[test]
    fn ready_to_destroyed_on_close() {
        let now = now();
        let mut p = preview(now);
        p.start_building(now).unwrap();
        p.mark_ready(now, 72).unwrap();
        p.destroy(now, "pr_closed").unwrap();
        assert_eq!(p.state(), PreviewState::Destroyed);
        assert_eq!(p.destroy_reason(), Some("pr_closed"));
    }

    #[test]
    fn transition_out_of_destroyed_is_illegal() {
        let now = now();
        let mut p = preview(now);
        p.destroy(now, "expired").unwrap();
        assert!(matches!(
            p.start_building(now),
            Err(PreviewError::AlreadyGone)
        ));
        assert!(matches!(
            p.mark_ready(now, 72),
            Err(PreviewError::AlreadyGone)
        ));
        assert!(matches!(
            p.destroy(now, "again"),
            Err(PreviewError::AlreadyGone)
        ));
    }

    #[test]
    fn mark_ready_requires_building() {
        let now = now();
        let mut p = preview(now);
        assert!(matches!(
            p.mark_ready(now, 72),
            Err(PreviewError::Invalid(_))
        ));
    }

    #[test]
    fn hostname_derivation() {
        assert_eq!(
            derive_hostname(42, "pr.example.com").unwrap(),
            "42.pr.example.com"
        );
        assert!(matches!(
            derive_hostname(0, "pr.example.com"),
            Err(PreviewError::InvalidPr(_))
        ));
    }

    #[test]
    fn expiry_detection() {
        let now = now();
        let mut p = preview(now);
        p.start_building(now).unwrap();
        p.mark_ready(now, 1).unwrap();
        assert!(!p.is_expired(now));
        assert!(p.is_expired(now + chrono::Duration::hours(2)));
    }
}

#[cfg(test)]
mod prop {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn prop_hostname_is_safe_and_unique(
            pr in 1u32..100_000,
            base in "[a-z0-9.-]{3,20}",
        ) {
            let host = derive_hostname(pr, &base).unwrap();
            prop_assert!(host.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-'));
            // Same (repo, pr) always yields the same host; different pr yields different host.
            let other = derive_hostname(pr + 1, &base).unwrap();
            prop_assert_ne!(host, other);
        }
    }
}
