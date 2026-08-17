//! Agent bounded context: per-host agent runtime, registration,
//! and typed recipe manifest.
//!
//! The agent is a dedicated per-host daemon that exposes only
//! idempotent operations to the control plane. Its trust
//! relationship is anchored by a `FleetToken` (mTLS client cert
//! fingerprint + scoped bearer fallback) and signed
//! `RecipeManifest`s that the agent validates before executing.

use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Stable identifier for an agent record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(pub Uuid);

impl AgentId {
    /// Brand a uuid as an AgentId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Lifecycle status of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// The agent is registered but has not yet checked in.
    Pending,
    /// The agent is currently heartbeating.
    Online,
    /// The agent missed the last heartbeat.
    Offline,
    /// The agent has been revoked by the control plane.
    Revoked,
}

/// The reconciliation policy runs on the agent when the recipe
/// has settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeAction {
    /// Stable identifier for the action (e.g. `install_nginx`).
    pub name: String,
    /// Argument blob. The agent validates the schema per
    /// `name` before invoking the executor.
    pub args: serde_json::Value,
    /// Whether the action is idempotent.
    pub idempotent: bool,
}

/// A signed recipe manifest the agent validates before executing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeManifest {
    id: Uuid,
    name: String,
    actions: Vec<RecipeAction>,
    rollback_actions: Vec<RecipeAction>,
    allowed_runners: BTreeSet<String>,
    signature: String,
    signed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl RecipeManifest {
    /// Build a new manifest. The `allowed_runners` set must be
    /// non-empty; the manifest name must be 3..=64 chars.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        name: impl Into<String>,
        actions: Vec<RecipeAction>,
        rollback_actions: Vec<RecipeAction>,
        allowed_runners: BTreeSet<String>,
        signature: impl Into<String>,
        signed_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, AgentError> {
        let name = name.into();
        if name.len() < 3 || name.len() > 64 {
            return Err(AgentError::InvalidManifest(format!(
                "name must be 3..=64 chars, got {}",
                name.len()
            )));
        }
        if allowed_runners.is_empty() {
            return Err(AgentError::InvalidManifest(
                "allowed_runners must be non-empty".to_string(),
            ));
        }
        if actions.is_empty() {
            return Err(AgentError::InvalidManifest(
                "actions must be non-empty".to_string(),
            ));
        }
        Ok(Self {
            id,
            name,
            actions,
            rollback_actions,
            allowed_runners,
            signature: signature.into(),
            signed_at,
            expires_at,
        })
    }

    /// Restore from persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        name: String,
        actions: Vec<RecipeAction>,
        rollback_actions: Vec<RecipeAction>,
        allowed_runners: BTreeSet<String>,
        signature: String,
        signed_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name,
            actions,
            rollback_actions,
            allowed_runners,
            signature,
            signed_at,
            expires_at,
        }
    }

    /// Recipe id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Recipe name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Forward actions.
    pub fn actions(&self) -> &[RecipeAction] {
        &self.actions
    }

    /// Rollback actions.
    pub fn rollback_actions(&self) -> &[RecipeAction] {
        &self.rollback_actions
    }

    /// Allowed runners.
    pub fn allowed_runners(&self) -> &BTreeSet<String> {
        &self.allowed_runners
    }

    /// Signature.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// When the manifest was signed.
    pub fn signed_at(&self) -> DateTime<Utc> {
        self.signed_at
    }

    /// When the manifest expires.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Whether the manifest has expired at `now`.
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    /// Whether `runner` is in the allowed set.
    pub fn allows_runner(&self, runner: &str) -> bool {
        self.allowed_runners.contains(runner)
    }
}

/// A registered agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRegistration {
    id: AgentId,
    host_fingerprint: String,
    hostname: String,
    status: AgentStatus,
    cert_fingerprint: String,
    last_heartbeat_at: Option<DateTime<Utc>>,
    registered_at: DateTime<Utc>,
    owner_id: Uuid,
}

impl AgentRegistration {
    /// Build a new registration. The host fingerprint is
    /// determined by the agent at install time and is treated as
    /// trusted enough to register.
    pub fn new(
        id: AgentId,
        host_fingerprint: impl Into<String>,
        hostname: impl Into<String>,
        cert_fingerprint: impl Into<String>,
        owner_id: Uuid,
        registered_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            host_fingerprint: host_fingerprint.into(),
            hostname: hostname.into(),
            status: AgentStatus::Pending,
            cert_fingerprint: cert_fingerprint.into(),
            last_heartbeat_at: None,
            registered_at,
            owner_id,
        }
    }

    /// Restore from persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: AgentId,
        host_fingerprint: String,
        hostname: String,
        status: AgentStatus,
        cert_fingerprint: String,
        last_heartbeat_at: Option<DateTime<Utc>>,
        registered_at: DateTime<Utc>,
        owner_id: Uuid,
    ) -> Self {
        Self {
            id,
            host_fingerprint,
            hostname,
            status,
            cert_fingerprint,
            last_heartbeat_at,
            registered_at,
            owner_id,
        }
    }

    /// Agent id.
    pub fn id(&self) -> AgentId {
        self.id
    }

    /// SHA-256 fingerprint of the host's primary MAC address (or
    /// equivalent stable identifier).
    pub fn host_fingerprint(&self) -> &str {
        &self.host_fingerprint
    }

    /// Hostname reported by the agent at install.
    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    /// Current status.
    pub fn status(&self) -> AgentStatus {
        self.status
    }

    /// SHA-256 fingerprint of the agent's mTLS client cert.
    pub fn cert_fingerprint(&self) -> &str {
        &self.cert_fingerprint
    }

    /// Last heartbeat observed.
    pub fn last_heartbeat_at(&self) -> Option<DateTime<Utc>> {
        self.last_heartbeat_at
    }

    /// When the agent was registered.
    pub fn registered_at(&self) -> DateTime<Utc> {
        self.registered_at
    }

    /// Owner account id.
    pub fn owner_id(&self) -> Uuid {
        self.owner_id
    }

    /// Mark the agent online with a fresh heartbeat.
    pub fn mark_online(&mut self, now: DateTime<Utc>) {
        self.status = AgentStatus::Online;
        self.last_heartbeat_at = Some(now);
    }

    /// Mark the agent offline (e.g. missed heartbeat).
    pub fn mark_offline(&mut self) {
        self.status = AgentStatus::Offline;
    }

    /// Revoke the agent.
    pub fn revoke(&mut self) {
        self.status = AgentStatus::Revoked;
    }

    /// Whether the agent is currently usable.
    pub fn is_usable(&self) -> bool {
        matches!(self.status, AgentStatus::Online | AgentStatus::Pending)
    }
}

/// Scope of a bearer token issued to an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetTokenScope {
    /// Read-only access (state, monitoring, audit).
    Read,
    /// Read + signed-recipe execution.
    Execute,
    /// Full control plane access.
    Admin,
}

/// A bearer token issued to an agent as a fallback for environments
/// where mTLS is impractical (e.g. behind a non-TCP load balancer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetToken {
    id: Uuid,
    agent_id: AgentId,
    scope: FleetTokenScope,
    token_hash: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    revoked: bool,
}

impl FleetToken {
    /// Build a new token. The token hash is the SHA-256 of the
    /// plaintext token; the plaintext is never persisted.
    pub fn new(
        id: Uuid,
        agent_id: AgentId,
        scope: FleetTokenScope,
        token_hash: impl Into<String>,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            agent_id,
            scope,
            token_hash: token_hash.into(),
            issued_at,
            expires_at,
            revoked: false,
        }
    }

    /// Restore from persistence.
    pub fn restore(
        id: Uuid,
        agent_id: AgentId,
        scope: FleetTokenScope,
        token_hash: String,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        revoked: bool,
    ) -> Self {
        Self {
            id,
            agent_id,
            scope,
            token_hash,
            issued_at,
            expires_at,
            revoked,
        }
    }

    /// Token id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Agent id.
    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Scope.
    pub fn scope(&self) -> FleetTokenScope {
        self.scope
    }

    /// Hash of the plaintext token.
    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }

    /// Issued at.
    pub fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }

    /// Expires at.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Whether the token has been revoked.
    pub fn is_revoked(&self) -> bool {
        self.revoked
    }

    /// Whether the token is currently usable.
    pub fn is_usable_at(&self, now: DateTime<Utc>) -> bool {
        !self.revoked && now < self.expires_at
    }

    /// Revoke the token.
    pub fn revoke(&mut self) {
        self.revoked = true;
    }
}

/// Persistence port.
#[async_trait]
pub trait AgentRepository: Send + Sync + 'static {
    /// Insert a new agent registration.
    async fn insert_agent(&self, agent: &AgentRegistration) -> Result<(), AgentError>;
    /// Find an agent by id.
    async fn find_agent(&self, id: AgentId) -> Result<Option<AgentRegistration>, AgentError>;
    /// Find an agent by cert fingerprint.
    async fn find_agent_by_cert(
        &self,
        cert_fingerprint: &str,
    ) -> Result<Option<AgentRegistration>, AgentError>;
    /// Update an agent.
    async fn update_agent(&self, agent: &AgentRegistration) -> Result<(), AgentError>;
    /// List all agents.
    async fn list_agents(&self) -> Result<Vec<AgentRegistration>, AgentError>;
    /// Insert a new token.
    async fn insert_token(&self, token: &FleetToken) -> Result<(), AgentError>;
    /// Find a token by id.
    async fn find_token(&self, id: Uuid) -> Result<Option<FleetToken>, AgentError>;
    /// Update a token (e.g. revoke).
    async fn update_token(&self, token: &FleetToken) -> Result<(), AgentError>;
    /// Find a token by hash.
    async fn find_token_by_hash(&self, token_hash: &str) -> Result<Option<FleetToken>, AgentError>;
    /// Insert a recipe manifest.
    async fn insert_manifest(&self, manifest: &RecipeManifest) -> Result<(), AgentError>;
    /// Find a manifest by id.
    async fn find_manifest(&self, id: Uuid) -> Result<Option<RecipeManifest>, AgentError>;
    /// Default no-op impl to satisfy the placeholder pattern.
    async fn exists(&self, _id: AgentId) -> Result<bool, RepoError> {
        Ok(true)
    }
}

/// Errors that can occur in the agent bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AgentError {
    /// The manifest is malformed.
    #[error("invalid recipe manifest: {0}")]
    InvalidManifest(String),
    /// The agent is not found.
    #[error("agent not found")]
    AgentNotFound,
    /// The token is not found.
    #[error("fleet token not found")]
    TokenNotFound,
    /// The agent has been revoked.
    #[error("agent revoked")]
    Revoked,
    /// The token has expired or been revoked.
    #[error("fleet token expired")]
    TokenExpired,
    /// The runner is not in the manifest's allowlist.
    #[error("runner not allowed: {0}")]
    RunnerNotAllowed(String),
    /// The signature on the manifest did not validate.
    #[error("manifest signature invalid")]
    SignatureInvalid,
    /// The caller may not invoke the requested action.
    #[error("forbidden")]
    Forbidden,
    /// The manifest has expired.
    #[error("manifest expired")]
    ManifestExpired,
    /// Persistence failure.
    #[error("agent persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for AgentError {
    fn from(error: RepoError) -> Self {
        AgentError::Persistence(error.0)
    }
}

/// Verify a manifest signature. The pure-domain implementation is a
/// placeholder that compares the signature to a known-good prefix;
/// the real implementation (added by the
/// `add-multi-host-agent-crypto` follow-on) uses an Ed25519
/// verification against the panel's signing key.
pub fn is_manifest_signature_valid(manifest: &RecipeManifest, expected_prefix: &str) -> bool {
    if manifest.signature().is_empty() {
        return false;
    }
    manifest.signature().starts_with(expected_prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> RecipeManifest {
        let mut allowed = BTreeSet::new();
        allowed.insert("nginx".to_string());
        RecipeManifest::new(
            Uuid::new_v4(),
            "install-nginx",
            vec![RecipeAction {
                name: "install_nginx".to_string(),
                args: serde_json::json!({"version": "1.24"}),
                idempotent: true,
            }],
            vec![RecipeAction {
                name: "remove_nginx".to_string(),
                args: serde_json::json!({}),
                idempotent: false,
            }],
            allowed,
            "sig-stub",
            Utc::now(),
            Utc::now() + chrono::Duration::hours(1),
        )
        .unwrap()
    }

    #[test]
    fn manifest_rejects_undersized_name() {
        let mut allowed = BTreeSet::new();
        allowed.insert("nginx".to_string());
        let err = RecipeManifest::new(
            Uuid::new_v4(),
            "ab",
            vec![RecipeAction {
                name: "x".to_string(),
                args: serde_json::json!({}),
                idempotent: true,
            }],
            vec![],
            allowed,
            "sig",
            Utc::now(),
            Utc::now() + chrono::Duration::hours(1),
        )
        .expect_err("must reject");
        assert!(matches!(err, AgentError::InvalidManifest(_)));
    }

    #[test]
    fn manifest_rejects_empty_runner_set() {
        let err = RecipeManifest::new(
            Uuid::new_v4(),
            "valid",
            vec![RecipeAction {
                name: "x".to_string(),
                args: serde_json::json!({}),
                idempotent: true,
            }],
            vec![],
            BTreeSet::new(),
            "sig",
            Utc::now(),
            Utc::now() + chrono::Duration::hours(1),
        )
        .expect_err("must reject");
        assert!(matches!(err, AgentError::InvalidManifest(_)));
    }

    #[test]
    fn manifest_rejects_empty_actions() {
        let mut allowed = BTreeSet::new();
        allowed.insert("nginx".to_string());
        let err = RecipeManifest::new(
            Uuid::new_v4(),
            "valid",
            vec![],
            vec![],
            allowed,
            "sig",
            Utc::now(),
            Utc::now() + chrono::Duration::hours(1),
        )
        .expect_err("must reject");
        assert!(matches!(err, AgentError::InvalidManifest(_)));
    }

    #[test]
    fn manifest_allows_runner_when_present() {
        let m = sample_manifest();
        assert!(m.allows_runner("nginx"));
        assert!(!m.allows_runner("mysql"));
    }

    #[test]
    fn manifest_detects_expiry() {
        let mut allowed = BTreeSet::new();
        allowed.insert("nginx".to_string());
        let now = Utc::now();
        let m = RecipeManifest::new(
            Uuid::new_v4(),
            "valid",
            vec![RecipeAction {
                name: "x".to_string(),
                args: serde_json::json!({}),
                idempotent: true,
            }],
            vec![],
            allowed,
            "sig",
            now,
            now + chrono::Duration::seconds(60),
        )
        .unwrap();
        assert!(!m.is_expired_at(now + chrono::Duration::seconds(30)));
        assert!(m.is_expired_at(now + chrono::Duration::seconds(120)));
    }

    #[test]
    fn agent_status_transitions() {
        let mut agent = AgentRegistration::new(
            AgentId::new(),
            "fp",
            "host",
            "cert",
            Uuid::new_v4(),
            Utc::now(),
        );
        assert_eq!(agent.status(), AgentStatus::Pending);
        assert!(agent.is_usable());
        agent.mark_online(Utc::now());
        assert_eq!(agent.status(), AgentStatus::Online);
        agent.mark_offline();
        assert_eq!(agent.status(), AgentStatus::Offline);
        assert!(!agent.is_usable());
        agent.revoke();
        assert_eq!(agent.status(), AgentStatus::Revoked);
    }

    #[test]
    fn token_revocation_makes_it_unusable() {
        let mut token = FleetToken::new(
            Uuid::new_v4(),
            AgentId::new(),
            FleetTokenScope::Read,
            "hash",
            Utc::now(),
            Utc::now() + chrono::Duration::hours(1),
        );
        assert!(token.is_usable_at(Utc::now()));
        token.revoke();
        assert!(!token.is_usable_at(Utc::now()));
    }
}
