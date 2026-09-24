//! Application-delivery domain types: the data shapes the
//! bounded context owns (application, source, environment, release,
//! state, promotion, rollback, secret reference) and the pure
//! redaction helper. The validate / decide / lifecycle helpers live
//! in `logic.rs`; the unit + property tests live in `tests.rs`.
//! This file is I/O-free.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Domain validation and lifecycle failures for application
/// delivery.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApplicationDeliveryError {
    /// A required field is missing or malformed.
    #[error("invalid application-delivery value: {0}")]
    Invalid(String),
    /// A state-machine transition is illegal.
    #[error("illegal transition: {0}")]
    IllegalTransition(String),
    /// The application is missing a required environment.
    #[error("environment is required: {0}")]
    MissingEnvironment(&'static str),
    /// A preview environment tried to access a production secret.
    #[error("preview environment cannot resolve production secret")]
    PreviewCannotUseProductionSecret,
    /// The release is already at or past the requested state.
    #[error("release is not in a state that can be {0}")]
    ReleaseNotReady(&'static str),
    /// The release digest is mutable (e.g. unpinned tag).
    #[error("mutable digest rejected: {0}")]
    MutableDigest(String),
}

/// Source kind for an application. Resolved by a `BuildAdapter`
/// into an immutable digest before any deployment is permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationSourceKind {
    /// Git repository pinned by SHA.
    Git,
    /// OCI image pinned by digest.
    Oci,
    /// Compose file (treated as a Git source with a compose path).
    Compose,
}

impl ApplicationSourceKind {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Oci => "oci",
            Self::Compose => "compose",
        }
    }
}

/// Source pointer: a URL + a ref (git SHA, OCI digest, compose SHA).
/// The ref is what the build adapter resolves to an immutable
/// release digest; the URL alone is never sufficient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationSource {
    kind: ApplicationSourceKind,
    /// Source URL (`https://`, `git@`, `oci://`).
    url: String,
    /// Source ref (git SHA, OCI digest, or compose SHA).
    source_ref: String,
}

impl ApplicationSource {
    /// Build a source. The URL is non-empty; the ref is non-empty.
    pub fn new(
        kind: ApplicationSourceKind,
        url: impl Into<String>,
        source_ref: impl Into<String>,
    ) -> Result<Self, ApplicationDeliveryError> {
        let url = url.into();
        let source_ref = source_ref.into();
        if url.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "source url is required".into(),
            ));
        }
        if source_ref.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "source ref is required".into(),
            ));
        }
        Ok(Self {
            kind,
            url,
            source_ref,
        })
    }

    /// Source kind.
    pub fn kind(&self) -> ApplicationSourceKind {
        self.kind
    }

    /// Source URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Source ref (sha, digest, or compose SHA).
    pub fn source_ref(&self) -> &str {
        &self.source_ref
    }
}

/// Validate a source ref for a given kind. Git refs MUST look like
/// a 7..=64 hex SHA; OCI refs MUST look like
/// `sha256:…`; compose refs are opaque SHAs.
pub fn validate_source_ref(
    kind: ApplicationSourceKind,
    source_ref: &str,
) -> Result<(), ApplicationDeliveryError> {
    let len = source_ref.len();
    match kind {
        ApplicationSourceKind::Git => {
            if !(7..=64).contains(&len) || !source_ref.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ApplicationDeliveryError::Invalid(format!(
                    "git ref must be 7..=64 hex chars, got `{}`",
                    source_ref
                )));
            }
        }
        ApplicationSourceKind::Oci => {
            if !source_ref.starts_with("sha256:") || !(8..=128).contains(&len) {
                return Err(ApplicationDeliveryError::Invalid(format!(
                    "oci ref must be `sha256:...`, got `{}`",
                    source_ref
                )));
            }
            let after = &source_ref[7..];
            if !after.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ApplicationDeliveryError::Invalid(format!(
                    "oci ref must be hex, got `{}`",
                    source_ref
                )));
            }
        }
        ApplicationSourceKind::Compose => {
            if !(7..=64).contains(&len) || !source_ref.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ApplicationDeliveryError::Invalid(format!(
                    "compose ref must be 7..=64 hex chars, got `{}`",
                    source_ref
                )));
            }
        }
    }
    Ok(())
}

/// A reference to a named secret set. Adapters resolve the
/// reference against their own secret store; OpenPanel never
/// receives the value. Production and preview sets are separate
/// keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretSetRef {
    /// Logical name (e.g. `prod-secrets`, `preview-secrets`).
    name: String,
    /// Optional version label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

impl SecretSetRef {
    /// Build a secret-set reference.
    pub fn new(
        name: impl Into<String>,
        version: Option<String>,
    ) -> Result<Self, ApplicationDeliveryError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "secret-set name is required".into(),
            ));
        }
        if let Some(v) = version.as_ref()
            && v.trim().is_empty()
        {
            return Err(ApplicationDeliveryError::Invalid(
                "secret-set version is invalid".into(),
            ));
        }
        Ok(Self { name, version })
    }

    /// Secret-set logical name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Secret-set version, if any.
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

/// Environment kind. Production and preview are separate kinds so
/// the type system can refuse a preview deployment that resolves a
/// production secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentKind {
    /// Long-lived production environment.
    Production,
    /// Short-lived preview environment (per PR, per branch).
    Preview,
}

impl EnvironmentKind {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Preview => "preview",
        }
    }

    /// Whether this environment kind is a preview.
    pub fn is_preview(self) -> bool {
        matches!(self, Self::Preview)
    }
}

/// An environment attached to an application. Production environments
/// MUST carry a `production` secret-set; preview environments MUST
/// carry a `preview` secret-set and have an `expires_at` in the
/// future.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    id: Uuid,
    /// Owning application.
    application_id: Uuid,
    /// Environment kind.
    kind: EnvironmentKind,
    /// Logical name (e.g. `production`, `preview-42`).
    name: String,
    /// Quota ceiling (in application-defined units).
    quota: u32,
    /// Secret-set reference.
    secret_set: SecretSetRef,
    /// When the environment was created.
    created_at: DateTime<Utc>,
    /// Expiry timestamp (required for `Preview`, optional for
    /// `Production`).
    expires_at: Option<DateTime<Utc>>,
}

impl Environment {
    /// Build a new environment. Enforces kind-specific invariants
    /// (preview MUST expire; secret-set name MUST match the kind
    /// prefix to make accidental cross-environment access fail at
    /// construction time).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        application_id: Uuid,
        kind: EnvironmentKind,
        name: impl Into<String>,
        quota: u32,
        secret_set: SecretSetRef,
        created_at: DateTime<Utc>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self, ApplicationDeliveryError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "environment name is required".into(),
            ));
        }
        if quota == 0 {
            return Err(ApplicationDeliveryError::Invalid(
                "environment quota must be > 0".into(),
            ));
        }
        let expected_prefix = match kind {
            EnvironmentKind::Production => "prod-",
            EnvironmentKind::Preview => "preview-",
        };
        if !secret_set.name().starts_with(expected_prefix) {
            return Err(ApplicationDeliveryError::Invalid(format!(
                "environment of kind `{}` requires a secret set whose name starts with `{}`",
                kind.as_str(),
                expected_prefix
            )));
        }
        if kind.is_preview() && expires_at.is_none() {
            return Err(ApplicationDeliveryError::Invalid(
                "preview environment must have expires_at".into(),
            ));
        }
        if let Some(expires) = expires_at
            && expires <= created_at
        {
            return Err(ApplicationDeliveryError::Invalid(
                "expires_at must be in the future".into(),
            ));
        }
        Ok(Self {
            id,
            application_id,
            kind,
            name,
            quota,
            secret_set,
            created_at,
            expires_at,
        })
    }

    /// Stable id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Application id.
    pub fn application_id(&self) -> Uuid {
        self.application_id
    }

    /// Environment kind.
    pub fn kind(&self) -> EnvironmentKind {
        self.kind
    }

    /// Logical name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Quota ceiling.
    pub fn quota(&self) -> u32 {
        self.quota
    }

    /// Secret-set reference.
    pub fn secret_set(&self) -> &SecretSetRef {
        &self.secret_set
    }

    /// When the environment was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// When the environment expires.
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }

    /// Whether the environment is expired at `now`.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|e| now >= e)
    }
}

/// Lifecycle state of a release. The state machine is:
///
/// `Pending -> Building -> Deploying -> Healthy`
/// (terminal `Failed`, `RolledBack`, `RolledBackFailed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseState {
    /// Release created; not yet built.
    Pending,
    /// Build in progress.
    Building,
    /// Deployment in progress.
    Deploying,
    /// Health check passed; release is live.
    Healthy,
    /// Build or deployment failed; release is recoverable via
    /// rollback.
    Failed,
    /// Adapter rolled the target back to the prior healthy release.
    RolledBack,
    /// Rollback attempted but failed.
    RolledBackFailed,
}

impl ReleaseState {
    /// Whether the state is terminal (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Healthy | Self::Failed | Self::RolledBack | Self::RolledBackFailed
        )
    }

    /// Whether the state is the success terminal (`Healthy`).
    pub fn is_healthy(self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Building => "building",
            Self::Deploying => "deploying",
            Self::Healthy => "healthy",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
            Self::RolledBackFailed => "rolled_back_failed",
        }
    }
}

/// Immutable release identity. The release digest is what the
/// `deployment-adapters` contract consumes; the source pointer is
/// preserved for audit, but the release identity is the digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseDigest {
    /// Stable digest string (`sha256:...`).
    value: String,
    /// When the digest was resolved.
    resolved_at: DateTime<Utc>,
}

impl ReleaseDigest {
    /// Build a release digest. MUST be immutable: `sha256:...` with
    /// only hex characters after the colon.
    pub fn new(
        value: impl Into<String>,
        resolved_at: DateTime<Utc>,
    ) -> Result<Self, ApplicationDeliveryError> {
        let value = value.into();
        if !value.starts_with("sha256:") {
            return Err(ApplicationDeliveryError::MutableDigest(value));
        }
        if value.len() < 8 || value.len() > 128 {
            return Err(ApplicationDeliveryError::Invalid(format!(
                "release digest must be 8..=128 chars, got `{}`",
                value.len()
            )));
        }
        let after = &value[7..];
        if !after.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ApplicationDeliveryError::MutableDigest(value));
        }
        Ok(Self { value, resolved_at })
    }

    /// Digest value (e.g. `sha256:abcdef…`).
    pub fn value(&self) -> &str {
        &self.value
    }

    /// When the digest was resolved.
    pub fn resolved_at(&self) -> DateTime<Utc> {
        self.resolved_at
    }
}

/// A release record. Immutable once created: only the `state` and
/// the `last_transition_at` field can change. `previous_release_id`
/// is set by the rollback path and is what `can_rollback` reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    id: Uuid,
    application_id: Uuid,
    environment_id: Uuid,
    /// Immutable release digest.
    digest: ReleaseDigest,
    /// Source pointer this release was built from.
    source: ApplicationSource,
    /// Current state.
    state: ReleaseState,
    /// When the release was created.
    created_at: DateTime<Utc>,
    /// When the release last transitioned state.
    last_transition_at: DateTime<Utc>,
    /// Optional pointer to the prior healthy release (set when a
    /// rollback creates a new release row that points here).
    previous_release_id: Option<Uuid>,
    /// Bounded, redacted diagnostic for the most recent transition.
    diagnostic: String,
}

impl Release {
    /// Build a new release in the `Pending` state. The
    /// `previous_release_id` is set by the rollback path; on a
    /// normal deploy it is `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        application_id: Uuid,
        environment_id: Uuid,
        digest: ReleaseDigest,
        source: ApplicationSource,
        created_at: DateTime<Utc>,
        previous_release_id: Option<Uuid>,
    ) -> Self {
        Self {
            id,
            application_id,
            environment_id,
            digest,
            source,
            state: ReleaseState::Pending,
            created_at,
            last_transition_at: created_at,
            previous_release_id,
            diagnostic: String::new(),
        }
    }

    /// Stable id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Owning application.
    pub fn application_id(&self) -> Uuid {
        self.application_id
    }

    /// Environment the release targets.
    pub fn environment_id(&self) -> Uuid {
        self.environment_id
    }

    /// Immutable release digest.
    pub fn digest(&self) -> &ReleaseDigest {
        &self.digest
    }

    /// Source pointer.
    pub fn source(&self) -> &ApplicationSource {
        &self.source
    }

    /// Current state.
    pub fn state(&self) -> ReleaseState {
        self.state
    }

    /// When the release was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// When the release last transitioned state.
    pub fn last_transition_at(&self) -> DateTime<Utc> {
        self.last_transition_at
    }

    /// Prior healthy release id (set on rollback).
    pub fn previous_release_id(&self) -> Option<Uuid> {
        self.previous_release_id
    }

    /// Redacted diagnostic for the most recent transition.
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }

    /// Apply a state transition. Refuses illegal moves; updates
    /// `last_transition_at`; stores the redacted diagnostic.
    pub fn transition(
        &mut self,
        target: ReleaseState,
        now: DateTime<Utc>,
        diagnostic: &str,
    ) -> Result<(), ApplicationDeliveryError> {
        if self.state == target {
            return Err(ApplicationDeliveryError::IllegalTransition(format!(
                "release already in state `{}`",
                target.as_str()
            )));
        }
        let legal = matches!(
            (self.state, target),
            (ReleaseState::Pending, ReleaseState::Building)
                | (ReleaseState::Pending, ReleaseState::Failed)
                | (ReleaseState::Building, ReleaseState::Deploying)
                | (ReleaseState::Building, ReleaseState::Failed)
                | (ReleaseState::Deploying, ReleaseState::Healthy)
                | (ReleaseState::Deploying, ReleaseState::Failed)
                | (ReleaseState::Failed, ReleaseState::RolledBack)
                | (ReleaseState::Failed, ReleaseState::RolledBackFailed)
        );
        if !legal {
            return Err(ApplicationDeliveryError::IllegalTransition(format!(
                "cannot move from `{}` to `{}`",
                self.state.as_str(),
                target.as_str()
            )));
        }
        self.state = target;
        self.last_transition_at = now;
        self.diagnostic = redact_diagnostic(diagnostic);
        Ok(())
    }
}

/// A typed application record. The application owns its source
/// pointer (single source of truth) and the set of environments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    id: Uuid,
    /// Owning user.
    owner_id: Uuid,
    /// Display name (1..=64 chars, `[a-z0-9-]`).
    name: String,
    /// Source pointer.
    source: ApplicationSource,
    /// Build hint (e.g. `npm ci && npm run build`). May be empty
    /// for pure OCI / compose sources.
    #[serde(default)]
    build_command: String,
    /// Environments attached to the application.
    environments: Vec<Environment>,
    /// When the application was created.
    created_at: DateTime<Utc>,
}

impl Application {
    /// Build a new application record.
    pub fn new(
        id: Uuid,
        owner_id: Uuid,
        name: impl Into<String>,
        source: ApplicationSource,
        build_command: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ApplicationDeliveryError> {
        let name = name.into();
        let build_command = build_command.into();
        if name.is_empty()
            || name.len() > 64
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(ApplicationDeliveryError::Invalid(format!(
                "application name `{}` must be 1..=64 chars of [a-z0-9-]",
                name
            )));
        }
        validate_source_ref(source.kind(), source.source_ref())?;
        Ok(Self {
            id,
            owner_id,
            name,
            source,
            build_command,
            environments: Vec::new(),
            created_at,
        })
    }

    /// Stable id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Owning user.
    pub fn owner_id(&self) -> Uuid {
        self.owner_id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Source pointer.
    pub fn source(&self) -> &ApplicationSource {
        &self.source
    }

    /// Build hint.
    pub fn build_command(&self) -> &str {
        &self.build_command
    }

    /// Environments attached to the application.
    pub fn environments(&self) -> &[Environment] {
        &self.environments
    }

    /// When the application was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Look up an environment by name.
    pub fn environment(&self, name: &str) -> Option<&Environment> {
        self.environments.iter().find(|e| e.name() == name)
    }

    /// Add an environment to the application. The list is kept
    /// sorted by environment name; duplicates are refused.
    pub fn add_environment(
        &mut self,
        environment: Environment,
    ) -> Result<(), ApplicationDeliveryError> {
        if environment.application_id() != self.id {
            return Err(ApplicationDeliveryError::Invalid(
                "environment.application_id does not match this application".into(),
            ));
        }
        if self
            .environments
            .iter()
            .any(|existing| existing.name() == environment.name())
        {
            return Err(ApplicationDeliveryError::Invalid(format!(
                "environment `{}` already exists",
                environment.name()
            )));
        }
        self.environments.push(environment);
        self.environments
            .sort_by(|left, right| left.name().cmp(right.name()));
        Ok(())
    }

    /// Whether the application carries a production environment.
    pub fn has_production(&self) -> bool {
        self.environments
            .iter()
            .any(|e| matches!(e.kind(), EnvironmentKind::Production))
    }

    /// Sorted set of environment names.
    pub fn environment_names(&self) -> BTreeSet<&str> {
        self.environments.iter().map(|e| e.name()).collect()
    }
}

/// Promotion request: deploy release `R` to environment `E`.
/// The operator MUST confirm and MUST provide a `previous_release_id`
/// when the environment already has a healthy release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionRequest {
    application_id: Uuid,
    environment_id: Uuid,
    release_id: Uuid,
    /// Authorised principal (username, never a token).
    actor: String,
    /// Operator-provided confirmation token.
    confirmation: String,
    /// Prior healthy release id (required when the environment
    /// already has a healthy release).
    previous_release_id: Option<Uuid>,
}

impl PromotionRequest {
    /// Build a promotion request.
    pub fn new(
        application_id: Uuid,
        environment_id: Uuid,
        release_id: Uuid,
        actor: impl Into<String>,
        confirmation: impl Into<String>,
        previous_release_id: Option<Uuid>,
    ) -> Result<Self, ApplicationDeliveryError> {
        let actor = actor.into();
        let confirmation = confirmation.into();
        if actor.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "actor is required".into(),
            ));
        }
        if confirmation.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "confirmation is required".into(),
            ));
        }
        Ok(Self {
            application_id,
            environment_id,
            release_id,
            actor,
            confirmation,
            previous_release_id,
        })
    }

    /// Owning application id.
    pub fn application_id(&self) -> Uuid {
        self.application_id
    }
    /// Target environment id.
    pub fn environment_id(&self) -> Uuid {
        self.environment_id
    }
    /// Release id being promoted.
    pub fn release_id(&self) -> Uuid {
        self.release_id
    }
    /// Authorised principal (username, never a token).
    pub fn actor(&self) -> &str {
        &self.actor
    }
    /// Operator-provided confirmation token.
    pub fn confirmation(&self) -> &str {
        &self.confirmation
    }
    /// Prior healthy release id (required when the
    /// environment already has a healthy release).
    pub fn previous_release_id(&self) -> Option<Uuid> {
        self.previous_release_id
    }
}

/// Rollback request: move environment `E` back to the prior healthy
/// release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackRequest {
    application_id: Uuid,
    environment_id: Uuid,
    /// Target release to roll back to.
    target_release_id: Uuid,
    /// Authorised principal.
    actor: String,
    /// Operator-provided confirmation.
    confirmation: String,
}

impl RollbackRequest {
    /// Build a rollback request.
    pub fn new(
        application_id: Uuid,
        environment_id: Uuid,
        target_release_id: Uuid,
        actor: impl Into<String>,
        confirmation: impl Into<String>,
    ) -> Result<Self, ApplicationDeliveryError> {
        let actor = actor.into();
        let confirmation = confirmation.into();
        if actor.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "actor is required".into(),
            ));
        }
        if confirmation.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "confirmation is required".into(),
            ));
        }
        Ok(Self {
            application_id,
            environment_id,
            target_release_id,
            actor,
            confirmation,
        })
    }

    /// Owning application id.
    pub fn application_id(&self) -> Uuid {
        self.application_id
    }
    /// Target environment id.
    pub fn environment_id(&self) -> Uuid {
        self.environment_id
    }
    /// Target release to roll back to.
    pub fn target_release_id(&self) -> Uuid {
        self.target_release_id
    }
    /// Authorised principal (username, never a token).
    pub fn actor(&self) -> &str {
        &self.actor
    }
    /// Operator-provided confirmation token.
    pub fn confirmation(&self) -> &str {
        &self.confirmation
    }
}

/// Audit tag attached to a release row. The tag is what the audit
/// service filters on; the field is plain so the audit redactor
/// can pass it through without leaking the value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditTag {
    /// Stable tag id (e.g. `release.create`, `release.rollback`).
    id: String,
}

impl AuditTag {
    /// Build an audit tag.
    pub fn new(id: impl Into<String>) -> Result<Self, ApplicationDeliveryError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(ApplicationDeliveryError::Invalid(
                "audit tag is required".into(),
            ));
        }
        Ok(Self { id })
    }

    /// Stable tag id (e.g. `release.create`, `release.rollback`).
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Truncate a diagnostic to a bounded length and remove any
/// substring that looks like a secret value. Reuses the same
/// marker list as `deployment_adapters::redact_diagnostic` so the
/// two contracts stay in lockstep.
pub fn redact_diagnostic(value: &str) -> String {
    const MAX: usize = 1024;
    let markers = [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "private-key",
        "credential",
        "bearer",
    ];
    let mut out = String::new();
    for line in value.lines() {
        let lower = line.to_lowercase();
        let redacted = if markers.iter().any(|m| lower.contains(m)) {
            if let Some(eq) = line.find('=') {
                format!("{} = <redacted>", &line[..eq])
            } else if let Some(colon) = line.find(':') {
                format!("{}: <redacted>", &line[..colon])
            } else {
                "<redacted>".to_string()
            }
        } else if line.contains("-----BEGIN") {
            "<redacted:pem>".to_string()
        } else {
            line.to_string()
        };
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&redacted);
    }
    if out.len() > MAX {
        out.truncate(MAX);
    }
    out
}

impl From<ApplicationDeliveryError> for RepoError {
    fn from(error: ApplicationDeliveryError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for ApplicationDeliveryError {
    fn from(error: RepoError) -> Self {
        ApplicationDeliveryError::Invalid(error.0)
    }
}
