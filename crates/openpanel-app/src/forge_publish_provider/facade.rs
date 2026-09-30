//! Forge publish provider facade. Owns the
//! `forge-publish-provider/0.1.0` request translation,
//! terminal-evidence projection, idempotency replay, and audit
//! fan-out. The facade does not own a lifecycle of its own: every
//! mutation delegates to `ApplicationDeliveryService` and every
//! rollout plan flows through `DeploymentAdapterService`.
//!
//! The facade is the only Rust module that knows both wire shapes
//! (in `openpanel_domain::forge_publish_provider`) and internal
//! services (in `openpanel_app::application_delivery` and
//! `openpanel_app::deployment_adapters`). The stdio JSON-RPC
//! adapter in `protocol.rs` and the process harness in
//! `lifecycle.rs` are the only places that own transport.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use openpanel_core::audit::{AuditAction, AuditEvent, AuditOutcome, AuditService, redact_metadata};
use openpanel_domain::deployment_adapters::{
    DeploymentAction, DeploymentEvidence, DeploymentPlan, DeploymentState, OperationKey, SecretRef,
};
use openpanel_domain::forge_publish_provider::{
    FORGE_PUBLISH_PROVIDER_IDENTITY, FORGE_PUBLISH_PROVIDER_IMPLEMENTATION_VERSION,
    FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION, ForgeProviderCapabilities, ForgeProviderError,
    ForgeProviderHealthSummary, ForgeProviderOperation, ForgeProviderProviderIdentity,
    ForgeProviderRequest, ForgeProviderResponse, ForgeProviderRevision,
    ForgeProviderRollbackStatus, ForgeProviderStatus, ForgeProviderTerminalEvidence,
    redact_diagnostic,
};
use openpanel_domain::{Application, ApplicationDeliveryError, Release, ReleaseState, Role, User};

use crate::application_delivery::{ApplicationDeliveryService, ApplicationDeliveryServiceError};
use crate::deployment_adapters::{DeploymentAdapterService, DeploymentServiceError};

/// Maximum length accepted for a synthesised plan target or digest
/// (matches `deployment_adapters::MAX_IDENTIFIER_LEN`).
const MAX_PLAN_IDENTIFIER_LEN: usize = 254;

/// Configuration for the provider facade. Constructed at
/// composition time; immutable afterwards.
#[derive(Debug, Clone)]
pub struct ForgePublishProviderConfig {
    /// Adapter id to use for publish + rollback plans. The id MUST
    /// be registered with the `DeploymentAdapterService` the facade
    /// is composed with.
    pub adapter_id: String,
    /// Logical actor recorded on every audit event and plan.
    pub audit_actor: String,
}

impl Default for ForgePublishProviderConfig {
    fn default() -> Self {
        Self {
            adapter_id: "openpanel-default".to_string(),
            audit_actor: "forge-publish-provider".to_string(),
        }
    }
}

/// Service-level errors raised by `ForgePublishProvider`. Every
/// variant maps deterministically onto a wire-level
/// `ForgeProviderStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ForgePublishProviderError {
    /// The wire-level request violated the contract.
    #[error("{0}")]
    Wire(ForgeProviderError),
    /// The application or environment does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// The target release is not in a state that can be mutated.
    #[error("release not ready: {0}")]
    ReleaseNotReady(String),
    /// The target is currently unhealthy.
    #[error("unhealthy: {0}")]
    Unhealthy(String),
    /// The requested action is not supported on the target.
    #[error("unsupported action: {0}")]
    Unsupported(String),
    /// Idempotency replay rejected the retry.
    #[error("idempotency conflict: {0}")]
    IdempotencyConflict(String),
    /// The deployment adapter rejected the plan.
    #[error("adapter failure: {0}")]
    Adapter(String),
    /// The application-delivery service rejected the operation.
    #[error("delivery failure: {0}")]
    Delivery(String),
    /// An internal persistence / state failure.
    #[error("persistence: {0}")]
    Persistence(String),
}

impl From<ForgeProviderError> for ForgePublishProviderError {
    fn from(value: ForgeProviderError) -> Self {
        Self::Wire(value)
    }
}

impl From<ApplicationDeliveryServiceError> for ForgePublishProviderError {
    fn from(value: ApplicationDeliveryServiceError) -> Self {
        use ApplicationDeliveryServiceError as E;
        match value {
            E::Domain(err) => match err {
                ApplicationDeliveryError::Invalid(message) => Self::Delivery(message),
                ApplicationDeliveryError::IllegalTransition(message) => {
                    Self::ReleaseNotReady(message)
                }
                ApplicationDeliveryError::MissingEnvironment(name) => {
                    Self::NotFound(name.to_string())
                }
                ApplicationDeliveryError::PreviewCannotUseProductionSecret => {
                    Self::Unsupported("preview environment cannot resolve production secret".into())
                }
                ApplicationDeliveryError::ReleaseNotReady(op) => {
                    Self::ReleaseNotReady(op.to_string())
                }
                ApplicationDeliveryError::MutableDigest(reason) => {
                    Self::Unsupported(format!("mutable digest rejected: {reason}"))
                }
            },
            E::Forbidden => Self::Delivery("forbidden".into()),
            E::Persistence(message) => Self::Persistence(message),
            E::ApplicationNotFound(id) => Self::NotFound(format!("application {id}")),
            E::EnvironmentNotFound(id) => Self::NotFound(format!("environment {id}")),
            E::ReleaseNotFound(id) => Self::NotFound(format!("release {id}")),
            E::ReleaseNotReady(op) => Self::ReleaseNotReady(op.to_string()),
            E::EnvironmentExpired => Self::Unhealthy("environment is expired".into()),
        }
    }
}

impl From<DeploymentServiceError> for ForgePublishProviderError {
    fn from(value: DeploymentServiceError) -> Self {
        use DeploymentServiceError as E;
        match value {
            E::Forbidden => Self::Adapter("forbidden".into()),
            E::AdapterNotFound(id) => Self::NotFound(format!("adapter {id}")),
            E::Validation(message) => Self::Adapter(message),
            E::UnsupportedAction(action) => Self::Unsupported(action.to_string()),
            E::RollbackUnsupported => {
                Self::Unsupported("rollback is not supported by this adapter".into())
            }
            E::Internal => Self::Persistence("deployment adapter service unavailable".into()),
        }
    }
}

/// Failure tuple carried through the handler closures: the typed
/// error plus the environment / diagnostic that should reach the
/// wire response.
type FacadeFailure = (ForgePublishProviderError, Option<String>, Option<String>);

/// Cached terminal evidence for an operation id. The cache is the
/// only state the facade owns; replays are pure cache lookups.
#[derive(Debug, Clone)]
struct CachedReplay {
    environment: String,
    response: ForgeProviderResponse,
}

/// In-process facade statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgePublishProviderStats {
    /// Number of accepted capabilities / preflight / verify
    /// requests (read-only operations).
    pub metadata_requests: u64,
    /// Number of successful mutating operations.
    pub mutations_succeeded: u64,
    /// Number of denied mutating operations.
    pub mutations_denied: u64,
    /// Number of failed mutating operations.
    pub mutations_failed: u64,
    /// Number of idempotency replays served from the cache.
    pub replays: u64,
}

/// Forge publish provider facade. Constructed once at composition
/// time and shared by the JSON-RPC adapter and the test suite.
pub struct ForgePublishProvider {
    config: ForgePublishProviderConfig,
    delivery: Arc<ApplicationDeliveryService>,
    adapters: Arc<DeploymentAdapterService>,
    audit: Arc<dyn AuditService>,
    state: Mutex<FacadeState>,
}

#[derive(Debug, Default)]
struct FacadeState {
    replays: HashMap<Uuid, CachedReplay>,
    stats: ForgePublishProviderStats,
}

impl ForgePublishProvider {
    /// Compose a new facade. The underlying services are owned by
    /// the caller; the facade only borrows them.
    pub fn new(
        config: ForgePublishProviderConfig,
        delivery: Arc<ApplicationDeliveryService>,
        adapters: Arc<DeploymentAdapterService>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            config,
            delivery,
            adapters,
            audit,
            state: Mutex::new(FacadeState::default()),
        }
    }

    /// Lock the facade state, recovering from a poisoned mutex.
    /// The state is a best-effort replay cache plus counters, so a
    /// panic while holding it MUST NOT take the provider down.
    fn lock_state(&self) -> std::sync::MutexGuard<'_, FacadeState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Stable provider identity (used by the JSON-RPC adapter and
    /// the test suite).
    pub fn provider_identity(&self) -> ForgeProviderProviderIdentity {
        ForgeProviderProviderIdentity {
            provider_id: FORGE_PUBLISH_PROVIDER_IDENTITY.to_string(),
            implementation_version: FORGE_PUBLISH_PROVIDER_IMPLEMENTATION_VERSION.to_string(),
            operations: vec![
                ForgeProviderOperation::Capabilities,
                ForgeProviderOperation::Preflight,
                ForgeProviderOperation::Publish,
                ForgeProviderOperation::Verify,
                ForgeProviderOperation::Rollback,
            ],
            protocol_versions: vec![FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION.to_string()],
        }
    }

    /// Snapshot the in-process statistics.
    pub fn provider_stats(&self) -> ForgePublishProviderStats {
        self.lock_state().stats.clone()
    }

    /// Handle a parsed request and return the wire-level response.
    /// The facade never panics; every error path produces a typed
    /// response with a redacted diagnostic.
    pub async fn handle_request(
        &self,
        request: ForgeProviderRequest,
        caller: &User,
    ) -> ForgeProviderResponse {
        let operation = request.operation;
        let result = match operation {
            ForgeProviderOperation::Capabilities => self.handle_capabilities(&request),
            ForgeProviderOperation::Preflight => self.handle_preflight(&request, caller).await,
            ForgeProviderOperation::Publish => self.handle_publish(&request, caller).await,
            ForgeProviderOperation::Verify => self.handle_verify(&request, caller).await,
            ForgeProviderOperation::Rollback => self.handle_rollback(&request, caller).await,
        };
        let mut response = match result {
            Ok(response) => {
                self.record_stats_success(operation);
                response
            }
            Err((err, environment, diagnostic)) => {
                self.record_stats_failure(operation, &err);
                self.build_failure_response(&request, operation, &err, environment, diagnostic)
            }
        };
        response.provider = self.provider_identity();
        self.fan_out_audit(&request, &response).await;
        response
    }

    fn record_stats_success(&self, operation: ForgeProviderOperation) {
        let mut state = self.lock_state();
        match operation {
            ForgeProviderOperation::Capabilities
            | ForgeProviderOperation::Preflight
            | ForgeProviderOperation::Verify => {
                state.stats.metadata_requests = state.stats.metadata_requests.saturating_add(1);
            }
            ForgeProviderOperation::Publish | ForgeProviderOperation::Rollback => {
                state.stats.mutations_succeeded = state.stats.mutations_succeeded.saturating_add(1);
            }
        }
    }

    fn record_stats_failure(
        &self,
        operation: ForgeProviderOperation,
        err: &ForgePublishProviderError,
    ) {
        let mut state = self.lock_state();
        if operation.is_mutating() {
            match err {
                ForgePublishProviderError::Wire(
                    ForgeProviderError::InvalidProject(_)
                    | ForgeProviderError::InvalidRevision(_)
                    | ForgeProviderError::InvalidOperationIdentity(_)
                    | ForgeProviderError::UnsupportedProtocol(_)
                    | ForgeProviderError::UnsupportedOperation(_),
                )
                | ForgePublishProviderError::NotFound(_)
                | ForgePublishProviderError::Unsupported(_)
                | ForgePublishProviderError::IdempotencyConflict(_) => {
                    state.stats.mutations_denied = state.stats.mutations_denied.saturating_add(1);
                }
                _ => {
                    state.stats.mutations_failed = state.stats.mutations_failed.saturating_add(1);
                }
            }
        }
    }

    async fn fan_out_audit(
        &self,
        request: &ForgeProviderRequest,
        response: &ForgeProviderResponse,
    ) {
        let action = match response.operation {
            ForgeProviderOperation::Capabilities
            | ForgeProviderOperation::Preflight
            | ForgeProviderOperation::Verify => AuditAction::ForgeProviderRequestAccepted,
            ForgeProviderOperation::Publish | ForgeProviderOperation::Rollback => {
                match response.status {
                    ForgeProviderStatus::Succeeded | ForgeProviderStatus::DryRun => {
                        AuditAction::ForgeProviderMutationSucceeded
                    }
                    ForgeProviderStatus::InvalidRequest
                    | ForgeProviderStatus::Conflict
                    | ForgeProviderStatus::Unsupported => AuditAction::ForgeProviderMutationDenied,
                    _ => AuditAction::ForgeProviderMutationFailed,
                }
            }
        };
        let outcome = match response.status {
            ForgeProviderStatus::Succeeded | ForgeProviderStatus::DryRun => AuditOutcome::Success,
            ForgeProviderStatus::InvalidRequest
            | ForgeProviderStatus::Conflict
            | ForgeProviderStatus::Unsupported
            | ForgeProviderStatus::Unhealthy => AuditOutcome::Denied,
            ForgeProviderStatus::Failed => AuditOutcome::Failure,
        };
        let metadata = redact_metadata(&serde_json::json!({
            "operation": response.operation.as_str(),
            "status": response.status.as_str(),
            "environment": response.environment,
            "project_slug": request.project.slug,
            "operation_id": response.operation_id.to_string(),
        }));
        let event = AuditEvent::new(self.config.audit_actor.clone(), action, outcome)
            .target(response.operation.as_str())
            .metadata(metadata);
        if let Err(err) = self.audit.record(event).await {
            tracing::warn!(
                target: "forge_publish_provider",
                error = %err,
                "audit fan-out failed"
            );
        }
    }

    fn handle_capabilities(
        &self,
        request: &ForgeProviderRequest,
    ) -> Result<ForgeProviderResponse, FacadeFailure> {
        let environment = request
            .revision
            .as_ref()
            .map(|revision| revision.environment.clone());
        let health = ForgeProviderHealthSummary {
            environment: environment.clone().unwrap_or_else(|| "production".into()),
            release_id: None,
            health: "unknown".into(),
            observed_at: Utc::now(),
        };
        Ok(ForgeProviderResponse {
            provider: self.provider_identity(),
            operation_id: self.effective_operation_id(request),
            operation: ForgeProviderOperation::Capabilities,
            environment,
            status: ForgeProviderStatus::Succeeded,
            capabilities: Some(ForgeProviderCapabilities {
                provider: self.provider_identity(),
                health,
            }),
            health: None,
            rollback: None,
            terminal: None,
            diagnostic: None,
        })
    }

    async fn handle_preflight(
        &self,
        request: &ForgeProviderRequest,
        caller: &User,
    ) -> Result<ForgeProviderResponse, FacadeFailure> {
        let revision = require_revision(request, "preflight")?;
        self.require_operator(caller)
            .map_err(|err| (err, Some(revision.environment.clone()), None))?;
        let plan = self.build_plan(request, &revision, DeploymentAction::Preflight, true)?;
        let evidence = self.run_plan(caller, plan, &revision).await?;
        let status = if evidence.state().is_ready() {
            ForgeProviderStatus::DryRun
        } else {
            ForgeProviderStatus::Unhealthy
        };
        Ok(ForgeProviderResponse {
            provider: self.provider_identity(),
            operation_id: self.effective_operation_id(request),
            operation: ForgeProviderOperation::Preflight,
            environment: Some(revision.environment.clone()),
            status,
            capabilities: None,
            health: Some(self.health_summary_from_evidence(&revision, &evidence)),
            rollback: None,
            terminal: None,
            diagnostic: Some(redact_diagnostic(evidence.diagnostic())),
        })
    }

    async fn handle_publish(
        &self,
        request: &ForgeProviderRequest,
        caller: &User,
    ) -> Result<ForgeProviderResponse, FacadeFailure> {
        let revision = require_revision(request, "publish")?;
        let operation_id = self.require_operation_id(request, "publish")?;
        self.require_operator(caller)
            .map_err(|err| (err, Some(revision.environment.clone()), None))?;
        if !revision.has_immutable_digest() {
            return Err((
                ForgePublishProviderError::Wire(ForgeProviderError::InvalidRevision(
                    "publish requires an immutable release_digest".into(),
                )),
                Some(revision.environment.clone()),
                None,
            ));
        }
        if let Some(response) = self.replay_or_conflict(operation_id, &revision)? {
            return Ok(response);
        }

        // Dry-run publishes never touch the application-delivery
        // state machine; they validate the plan and return the
        // planned effect only.
        if request.dry_run {
            let plan = self.build_plan(request, &revision, DeploymentAction::Deploy, true)?;
            let evidence = self.run_plan(caller, plan, &revision).await?;
            return Ok(ForgeProviderResponse {
                provider: self.provider_identity(),
                operation_id,
                operation: ForgeProviderOperation::Publish,
                environment: Some(revision.environment.clone()),
                status: ForgeProviderStatus::DryRun,
                capabilities: None,
                health: Some(self.health_summary_from_evidence(&revision, &evidence)),
                rollback: None,
                terminal: None,
                diagnostic: Some(redact_diagnostic(evidence.diagnostic())),
            });
        }

        let application = self
            .resolve_application_by_slug(caller, &request.project.slug)
            .map_err(|err| (err, Some(revision.environment.clone()), None))?;
        let release = self
            .delivery
            .build_release(caller, application.id(), &revision.environment)
            .map_err(|err| (err.into(), Some(revision.environment.clone()), None))?;
        let plan = self.build_plan(request, &revision, DeploymentAction::Deploy, false)?;
        let evidence = self.run_plan(caller, plan, &revision).await?;
        if !evidence.state().is_ready() {
            return Err((
                ForgePublishProviderError::Unhealthy(format!(
                    "deployment adapter reported {}",
                    evidence.state().as_str()
                )),
                Some(revision.environment.clone()),
                Some(redact_diagnostic(evidence.diagnostic())),
            ));
        }
        let promoted = self
            .delivery
            .promote(
                caller,
                application.id(),
                &revision.environment,
                release.id(),
                "forge-publish-provider",
            )
            .map_err(|err| (err.into(), Some(revision.environment.clone()), None))?;
        let terminal = ForgeProviderTerminalEvidence {
            operation_id,
            source_kind: revision.kind,
            environment: revision.environment.clone(),
            release_digest: promoted.digest().value().to_string(),
            release_id: promoted.id(),
            completed_at: Utc::now(),
            diagnostic: redact_diagnostic(evidence.diagnostic()),
        };
        let response = ForgeProviderResponse {
            provider: self.provider_identity(),
            operation_id,
            operation: ForgeProviderOperation::Publish,
            environment: Some(revision.environment.clone()),
            status: ForgeProviderStatus::Succeeded,
            capabilities: None,
            health: Some(self.health_summary_from_release(&revision.environment, &promoted)),
            rollback: None,
            terminal: Some(terminal),
            diagnostic: None,
        };
        self.cache_replay(operation_id, &revision.environment, response.clone());
        Ok(response)
    }

    async fn handle_verify(
        &self,
        request: &ForgeProviderRequest,
        caller: &User,
    ) -> Result<ForgeProviderResponse, FacadeFailure> {
        let revision = require_revision(request, "verify")?;
        self.require_operator(caller)
            .map_err(|err| (err, Some(revision.environment.clone()), None))?;
        let plan = self.build_plan(request, &revision, DeploymentAction::Verify, false)?;
        let evidence = self.run_plan(caller, plan, &revision).await?;
        let status = if evidence.state().is_ready() {
            ForgeProviderStatus::Succeeded
        } else {
            ForgeProviderStatus::Unhealthy
        };
        Ok(ForgeProviderResponse {
            provider: self.provider_identity(),
            operation_id: self.effective_operation_id(request),
            operation: ForgeProviderOperation::Verify,
            environment: Some(revision.environment.clone()),
            status,
            capabilities: None,
            health: Some(self.health_summary_from_evidence(&revision, &evidence)),
            rollback: None,
            terminal: None,
            diagnostic: Some(redact_diagnostic(evidence.diagnostic())),
        })
    }

    async fn handle_rollback(
        &self,
        request: &ForgeProviderRequest,
        caller: &User,
    ) -> Result<ForgeProviderResponse, FacadeFailure> {
        let revision = require_revision(request, "rollback")?;
        let operation_id = self.require_operation_id(request, "rollback")?;
        self.require_operator(caller)
            .map_err(|err| (err, Some(revision.environment.clone()), None))?;
        if let Some(response) = self.replay_or_conflict(operation_id, &revision)? {
            return Ok(response);
        }
        let application = self
            .resolve_application_by_slug(caller, &request.project.slug)
            .map_err(|err| (err, Some(revision.environment.clone()), None))?;
        let environment = application
            .environment(&revision.environment)
            .ok_or_else(|| {
                (
                    ForgePublishProviderError::NotFound(format!(
                        "environment {}",
                        revision.environment
                    )),
                    Some(revision.environment.clone()),
                    None,
                )
            })?
            .clone();
        let releases = self
            .delivery
            .list_releases(caller, application.id())
            .map_err(|err| (err.into(), Some(revision.environment.clone()), None))?;
        let prior = rollback_target(&releases, environment.id()).ok_or_else(|| {
            (
                ForgePublishProviderError::ReleaseNotReady(
                    "no rollback-eligible release is available".into(),
                ),
                Some(revision.environment.clone()),
                None,
            )
        })?;
        let restored = self
            .delivery
            .rollback(
                caller,
                application.id(),
                &revision.environment,
                prior.id(),
                "forge-publish-provider",
            )
            .map_err(|err| (err.into(), Some(revision.environment.clone()), None))?;
        let plan = self.build_plan(request, &revision, DeploymentAction::Rollback, false)?;
        let evidence = self.run_plan(caller, plan, &revision).await?;
        let outcome = if evidence.state().is_ready() {
            "succeeded"
        } else {
            "failed"
        };
        let status = if evidence.state().is_ready() {
            ForgeProviderStatus::Succeeded
        } else {
            ForgeProviderStatus::Unhealthy
        };
        let response = ForgeProviderResponse {
            provider: self.provider_identity(),
            operation_id,
            operation: ForgeProviderOperation::Rollback,
            environment: Some(revision.environment.clone()),
            status,
            capabilities: None,
            health: None,
            rollback: Some(ForgeProviderRollbackStatus {
                environment: revision.environment.clone(),
                restored_release_id: Some(restored.id()),
                prior_release_id: restored.previous_release_id(),
                outcome: outcome.to_string(),
                completed_at: Utc::now(),
            }),
            terminal: None,
            diagnostic: Some(redact_diagnostic(evidence.diagnostic())),
        };
        self.cache_replay(operation_id, &revision.environment, response.clone());
        Ok(response)
    }

    fn resolve_application_by_slug(
        &self,
        caller: &User,
        slug: &str,
    ) -> Result<Application, ForgePublishProviderError> {
        let applications = self.delivery.list_applications(caller)?;
        let needle = slug.rsplit('/').next().unwrap_or(slug);
        applications
            .into_iter()
            .find(|application| application.name() == needle)
            .ok_or_else(|| {
                ForgePublishProviderError::NotFound(format!("application for project slug {slug}"))
            })
    }

    fn build_plan(
        &self,
        request: &ForgeProviderRequest,
        revision: &ForgeProviderRevision,
        action: DeploymentAction,
        dry_run: bool,
    ) -> Result<DeploymentPlan, FacadeFailure> {
        let target = plan_target(&request.project.slug, &revision.environment);
        let release_digest = plan_digest(revision);
        let operation_key = if action.is_mutating() {
            let operation_id = request.operation_id.as_ref().ok_or_else(|| {
                (
                    ForgePublishProviderError::Wire(ForgeProviderError::InvalidOperationIdentity(
                        "mutating operations require an operation_id".into(),
                    )),
                    Some(revision.environment.clone()),
                    None,
                )
            })?;
            Some(
                OperationKey::new(&target, action, operation_id.operation_id.to_string()).map_err(
                    |err| {
                        (
                            ForgePublishProviderError::Adapter(err.to_string()),
                            Some(revision.environment.clone()),
                            None,
                        )
                    },
                )?,
            )
        } else {
            None
        };
        let secret_refs = match &request.secret_set {
            Some(secret_set) => vec![
                SecretRef::new(&secret_set.secret_set, secret_set.version.clone()).map_err(
                    |err| {
                        (
                            ForgePublishProviderError::Adapter(err.to_string()),
                            Some(revision.environment.clone()),
                            None,
                        )
                    },
                )?,
            ],
            None => Vec::new(),
        };
        let plan = DeploymentPlan::new(
            target,
            action,
            operation_key,
            release_digest,
            secret_refs,
            self.config.adapter_id.clone(),
            self.config.audit_actor.clone(),
            dry_run,
        )
        .map_err(|err| {
            (
                ForgePublishProviderError::Adapter(err.to_string()),
                Some(revision.environment.clone()),
                None,
            )
        })?;
        Ok(plan)
    }

    async fn run_plan(
        &self,
        caller: &User,
        plan: DeploymentPlan,
        revision: &ForgeProviderRevision,
    ) -> Result<DeploymentEvidence, FacadeFailure> {
        self.adapters
            .run(caller, plan)
            .await
            .map_err(|err| (err.into(), Some(revision.environment.clone()), None))
    }

    fn health_summary_from_evidence(
        &self,
        revision: &ForgeProviderRevision,
        evidence: &DeploymentEvidence,
    ) -> ForgeProviderHealthSummary {
        ForgeProviderHealthSummary {
            environment: revision.environment.clone(),
            release_id: None,
            health: health_label(evidence.state()).to_string(),
            observed_at: Utc::now(),
        }
    }

    fn health_summary_from_release(
        &self,
        environment: &str,
        release: &Release,
    ) -> ForgeProviderHealthSummary {
        ForgeProviderHealthSummary {
            environment: environment.to_string(),
            release_id: Some(release.id()),
            health: release_health_label(release.state()).to_string(),
            observed_at: Utc::now(),
        }
    }

    fn require_operation_id(
        &self,
        request: &ForgeProviderRequest,
        operation: &str,
    ) -> Result<Uuid, FacadeFailure> {
        request
            .operation_id
            .as_ref()
            .map(|identity| identity.operation_id)
            .ok_or_else(|| {
                (
                    ForgePublishProviderError::Wire(ForgeProviderError::InvalidOperationIdentity(
                        format!("{operation} requires an operation_id"),
                    )),
                    request
                        .revision
                        .as_ref()
                        .map(|revision| revision.environment.clone()),
                    None,
                )
            })
    }

    fn replay_or_conflict(
        &self,
        operation_id: Uuid,
        revision: &ForgeProviderRevision,
    ) -> Result<Option<ForgeProviderResponse>, FacadeFailure> {
        let Some(replay) = self.lookup_replay(operation_id) else {
            return Ok(None);
        };
        if replay.environment == revision.environment {
            self.record_replay();
            return Ok(Some(replay.response));
        }
        Err((
            ForgePublishProviderError::IdempotencyConflict(format!(
                "operation_id {operation_id} was previously applied to environment {}",
                replay.environment
            )),
            Some(revision.environment.clone()),
            None,
        ))
    }

    fn effective_operation_id(&self, request: &ForgeProviderRequest) -> Uuid {
        request
            .operation_id
            .as_ref()
            .map(|identity| identity.operation_id)
            .unwrap_or_else(Uuid::new_v4)
    }

    fn build_failure_response(
        &self,
        request: &ForgeProviderRequest,
        operation: ForgeProviderOperation,
        err: &ForgePublishProviderError,
        environment: Option<String>,
        diagnostic: Option<String>,
    ) -> ForgeProviderResponse {
        ForgeProviderResponse {
            provider: self.provider_identity(),
            operation_id: self.effective_operation_id(request),
            operation,
            environment: environment
                .or_else(|| request.revision.as_ref().map(|r| r.environment.clone())),
            status: map_error_to_status(err),
            capabilities: None,
            health: None,
            rollback: None,
            terminal: None,
            diagnostic: Some(match diagnostic {
                Some(text) => redact_diagnostic(text),
                None => redact_diagnostic(err.to_string()),
            }),
        }
    }

    fn require_operator(&self, caller: &User) -> Result<(), ForgePublishProviderError> {
        match caller.role() {
            Role::Owner | Role::Admin => Ok(()),
            _ => Err(ForgePublishProviderError::Delivery(
                "forge publish provider requires an operator caller".into(),
            )),
        }
    }

    fn lookup_replay(&self, operation_id: Uuid) -> Option<CachedReplay> {
        self.lock_state().replays.get(&operation_id).cloned()
    }

    fn cache_replay(&self, operation_id: Uuid, environment: &str, response: ForgeProviderResponse) {
        let cached = CachedReplay {
            environment: environment.to_string(),
            response,
        };
        self.lock_state().replays.insert(operation_id, cached);
    }

    fn record_replay(&self) {
        let mut state = self.lock_state();
        state.stats.replays = state.stats.replays.saturating_add(1);
    }
}

fn require_revision(
    request: &ForgeProviderRequest,
    operation: &str,
) -> Result<ForgeProviderRevision, FacadeFailure> {
    request.revision.clone().ok_or_else(|| {
        (
            ForgePublishProviderError::Wire(ForgeProviderError::InvalidRevision(format!(
                "{operation} requires a revision"
            ))),
            None,
            None,
        )
    })
}

/// Select the release a rollback should target. The
/// application-delivery policy only allows a release in `Failed`
/// or `Deploying` to be rolled back, so the facade picks the most
/// recent rollback-eligible release in the target environment.
fn rollback_target(releases: &[Release], environment_id: Uuid) -> Option<Release> {
    releases
        .iter()
        .filter(|release| release.environment_id() == environment_id)
        .filter(|release| {
            matches!(
                release.state(),
                ReleaseState::Failed | ReleaseState::Deploying
            )
        })
        .max_by_key(|release| release.created_at())
        .cloned()
}

fn plan_target(project_slug: &str, environment: &str) -> String {
    let target = format!("forge:{project_slug}:{environment}");
    bound_identifier(&target)
}

fn plan_digest(revision: &ForgeProviderRevision) -> String {
    match &revision.release_digest {
        Some(digest) => digest.clone(),
        None => bound_identifier(&format!("unresolved:{}", revision.source_ref)),
    }
}

fn bound_identifier(value: &str) -> String {
    if value.len() <= MAX_PLAN_IDENTIFIER_LEN {
        return value.to_string();
    }
    let mut cut = MAX_PLAN_IDENTIFIER_LEN;
    while cut > 0 && !value.is_char_boundary(cut) {
        cut -= 1;
    }
    value[..cut].to_string()
}

fn health_label(state: DeploymentState) -> &'static str {
    match state {
        DeploymentState::Ready => "healthy",
        DeploymentState::Failed
        | DeploymentState::RollbackFailed
        | DeploymentState::PreflightRejected => "failed",
        _ => "degraded",
    }
}

fn release_health_label(state: ReleaseState) -> &'static str {
    match state {
        ReleaseState::Healthy => "healthy",
        ReleaseState::Failed => "failed",
        ReleaseState::RolledBack
        | ReleaseState::RolledBackFailed
        | ReleaseState::Pending
        | ReleaseState::Building
        | ReleaseState::Deploying => "degraded",
    }
}

fn map_error_to_status(err: &ForgePublishProviderError) -> ForgeProviderStatus {
    use ForgePublishProviderError as E;
    match err {
        E::Wire(
            ForgeProviderError::UnsupportedProtocol(_)
            | ForgeProviderError::UnsupportedOperation(_)
            | ForgeProviderError::InvalidProject(_)
            | ForgeProviderError::InvalidRevision(_)
            | ForgeProviderError::InvalidOperationIdentity(_)
            | ForgeProviderError::NotFound(_),
        ) => ForgeProviderStatus::InvalidRequest,
        E::Wire(ForgeProviderError::IdempotencyConflict(_)) | E::IdempotencyConflict(_) => {
            ForgeProviderStatus::Conflict
        }
        E::Wire(ForgeProviderError::UnsupportedAction(_)) | E::Unsupported(_) => {
            ForgeProviderStatus::Unsupported
        }
        E::NotFound(_) => ForgeProviderStatus::InvalidRequest,
        E::ReleaseNotReady(_) | E::Unhealthy(_) => ForgeProviderStatus::Unhealthy,
        E::Adapter(_) | E::Delivery(_) | E::Persistence(_) | E::Wire(_) => {
            ForgeProviderStatus::Failed
        }
    }
}
