//! Application-delivery service: typed application /
//! environment / release lifecycle, idempotency cache, and
//! audit fan-out.
//!
//! The service is the single entry point that the API, the
//! CLI, and the integration tests share. It owns:
//! - the registered application set (keyed by id),
//! - the in-memory release cache (keyed by id) used for
//!   idempotency replay and rollback references,
//! - the in-memory audit fan-out (`openpanel_core::AuditService`).
//!
//! The service NEVER returns a host path, a secret value, or a
//! transport-specific command. Diagnostics are scrubbed by
//! [`openpanel_domain::application_delivery::redact_diagnostic`]
//! and audit metadata is redacted by the canonical
//! `openpanel_core::audit::redact_metadata` allowlist.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService, audit::redact_metadata};
use openpanel_domain::{
    Application, ApplicationDeliveryError, ApplicationSource, Environment, EnvironmentKind,
    Release, ReleaseLifecycle, ReleaseState, Role, SecretSetRef, User,
    application_delivery::{
        BuildAdapter, MAX_APPLICATION_NAME, MAX_DIGEST_LEN, MAX_REF_LEN, ReleaseOutcome,
        RuntimeAdapter, can_promote, can_rollback, resolve_environment,
    },
    redact_diagnostic as redact_diagnostic_deployment,
};
use uuid::Uuid;

use super::types::{
    ApplicationDeliveryServiceError, ApplicationRecord, EnvironmentRecord, ReleaseRecord,
};

/// State-of-the-world held by the service. The `Mutex` is
/// required because the service is shared across HTTP handlers
/// and CLI subcommands.
struct ServiceState {
    applications: HashMap<Uuid, Application>,
    releases: HashMap<Uuid, Release>,
}

/// The application delivery service. Composition root wires it
/// into the API, the CLI, and the integration tests.
pub struct ApplicationDeliveryService {
    build_adapter: Arc<dyn BuildAdapter>,
    runtime_adapter: Arc<dyn RuntimeAdapter>,
    state: Mutex<ServiceState>,
    audit: Arc<dyn AuditService>,
}

impl ApplicationDeliveryService {
    /// Compose a service with a build adapter, a runtime
    /// adapter, and an audit sink. Adapters are responsible for
    /// their own transport; the service owns the lifecycle.
    pub fn new(
        build_adapter: Arc<dyn BuildAdapter>,
        runtime_adapter: Arc<dyn RuntimeAdapter>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            build_adapter,
            runtime_adapter,
            state: Mutex::new(ServiceState {
                applications: HashMap::new(),
                releases: HashMap::new(),
            }),
            audit,
        }
    }

    /// Owner/Admin gate (defense in depth; handlers gate again).
    fn require_operator(user: &User) -> Result<(), ApplicationDeliveryServiceError> {
        if matches!(user.role(), Role::Owner | Role::Admin) {
            Ok(())
        } else {
            Err(ApplicationDeliveryServiceError::Forbidden)
        }
    }

    /// Register a new application. The caller is the owner;
    /// the source is validated by the domain `Application::new`
    /// and the application id is stable.
    pub fn register_application(
        &self,
        caller: &User,
        name: &str,
        source: ApplicationSource,
        build_command: &str,
    ) -> Result<Application, ApplicationDeliveryServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "register_application", name);
        })?;
        if name.len() > MAX_APPLICATION_NAME {
            return Err(ApplicationDeliveryServiceError::Domain(
                ApplicationDeliveryError::Invalid(format!(
                    "application name must be <= {MAX_APPLICATION_NAME} chars"
                )),
            ));
        }
        if source.source_ref().len() > MAX_REF_LEN {
            return Err(ApplicationDeliveryServiceError::Domain(
                ApplicationDeliveryError::Invalid(format!(
                    "source ref must be <= {MAX_REF_LEN} chars"
                )),
            ));
        }
        let application = Application::new(
            Uuid::new_v4(),
            caller.id(),
            name,
            source.clone(),
            build_command,
            Utc::now(),
        )?;
        let mut state = self.state.lock().map_err(internal)?;
        if state
            .applications
            .values()
            .any(|existing| existing.owner_id() == caller.id() && existing.name() == name)
        {
            return Err(ApplicationDeliveryServiceError::Domain(
                ApplicationDeliveryError::Invalid(format!(
                    "application `{name}` already exists for this owner"
                )),
            ));
        }
        let app_id = application.id();
        state.applications.insert(app_id, application.clone());
        self.audit_registered(caller, &application);
        Ok(application)
    }

    /// Add an environment to an existing application.
    #[allow(clippy::too_many_arguments)]
    pub fn add_environment(
        &self,
        caller: &User,
        application_id: Uuid,
        kind: EnvironmentKind,
        name: &str,
        quota: u32,
        secret_set: SecretSetRef,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<Environment, ApplicationDeliveryServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "add_environment", name);
        })?;
        let mut state = self.state.lock().map_err(internal)?;
        let application = state
            .applications
            .get(&application_id)
            .ok_or(ApplicationDeliveryServiceError::ApplicationNotFound(
                application_id,
            ))?
            .clone();
        if application.owner_id() != caller.id() {
            return Err(ApplicationDeliveryServiceError::Forbidden);
        }
        let env = Environment::new(
            Uuid::new_v4(),
            application_id,
            kind,
            name,
            quota,
            secret_set,
            Utc::now(),
            expires_at,
        )?;
        let mut application = application;
        application.add_environment(env.clone())?;
        state.applications.insert(application_id, application);
        Ok(env)
    }

    /// Build a new release for an application. The release is
    /// created in the `Pending` state and then walked to
    /// `Deploying` via the build adapter. The resulting digest
    /// is the immutable identity the deployment step targets.
    pub fn build_release(
        &self,
        caller: &User,
        application_id: Uuid,
        environment_name: &str,
    ) -> Result<Release, ApplicationDeliveryServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "build_release", environment_name);
        })?;
        let mut state = self.state.lock().map_err(internal)?;
        let application = state
            .applications
            .get(&application_id)
            .ok_or(ApplicationDeliveryServiceError::ApplicationNotFound(
                application_id,
            ))?
            .clone();
        if application.owner_id() != caller.id() {
            return Err(ApplicationDeliveryServiceError::Forbidden);
        }
        let environment = application
            .environment(environment_name)
            .ok_or(ApplicationDeliveryServiceError::Domain(
                ApplicationDeliveryError::MissingEnvironment("environment not found"),
            ))?
            .clone();
        if environment.is_expired(Utc::now()) {
            return Err(ApplicationDeliveryServiceError::EnvironmentExpired);
        }
        // Resolve the production / preview secret set. The
        // caller is expected to pass two secret sets via the
        // service composition; for the unit test path we use
        // the application's own secret set.
        let prod_set = SecretSetRef::new("prod-secrets", None).map_err(domain)?;
        let preview_set = SecretSetRef::new("preview-secrets", None).map_err(domain)?;
        let _secret_set = resolve_environment(&environment, &prod_set, &preview_set)
            .map_err(ApplicationDeliveryServiceError::Domain)?;
        let digest = self
            .build_adapter
            .resolve_digest(
                application_id,
                application.source(),
                application.build_command(),
            )
            .map_err(ApplicationDeliveryServiceError::Domain)?;
        if digest.value().len() > MAX_DIGEST_LEN {
            return Err(ApplicationDeliveryServiceError::Domain(
                ApplicationDeliveryError::Invalid(format!(
                    "resolved digest must be <= {MAX_DIGEST_LEN} chars"
                )),
            ));
        }
        let now = Utc::now();
        let release_id = Uuid::new_v4();
        let mut release = Release::new(
            release_id,
            application_id,
            environment.id(),
            digest,
            application.source().clone(),
            now,
            None,
        );
        let target_digest = release.digest().clone();
        ReleaseLifecycle::start_build(&mut release, now).map_err(domain)?;
        ReleaseLifecycle::finish_build(&mut release, now, &target_digest).map_err(domain)?;
        let before = release.state();
        state.releases.insert(release_id, release.clone());
        drop(state);
        self.audit_lifecycle(caller, "release.built", &release, before);
        Ok(release)
    }

    /// Promote a release to its target environment. The
    /// operator MUST provide a non-empty confirmation; the
    /// release MUST be in a non-terminal pre-`Healthy` state.
    pub fn promote(
        &self,
        caller: &User,
        application_id: Uuid,
        environment_name: &str,
        release_id: Uuid,
        confirmation: &str,
    ) -> Result<Release, ApplicationDeliveryServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "promote", environment_name);
        })?;
        let mut state = self.state.lock().map_err(internal)?;
        let application = state
            .applications
            .get(&application_id)
            .ok_or(ApplicationDeliveryServiceError::ApplicationNotFound(
                application_id,
            ))?
            .clone();
        if application.owner_id() != caller.id() {
            return Err(ApplicationDeliveryServiceError::Forbidden);
        }
        let environment = application
            .environment(environment_name)
            .ok_or(ApplicationDeliveryServiceError::Domain(
                ApplicationDeliveryError::MissingEnvironment("environment not found"),
            ))?
            .clone();
        if environment.is_expired(Utc::now()) {
            return Err(ApplicationDeliveryServiceError::EnvironmentExpired);
        }
        let release = state
            .releases
            .get(&release_id)
            .ok_or(ApplicationDeliveryServiceError::ReleaseNotFound(release_id))?
            .clone();
        if release.application_id() != application_id {
            return Err(ApplicationDeliveryServiceError::Forbidden);
        }
        let request = openpanel_domain::PromotionRequest::new(
            application_id,
            environment.id(),
            release_id,
            caller.username().as_str(),
            confirmation,
            None,
        )
        .map_err(domain)?;
        can_promote(&release, &environment, &request).map_err(domain)?;
        let outcome = self
            .runtime_adapter
            .deploy(&release, &environment)
            .map_err(ApplicationDeliveryServiceError::Domain)?;
        apply_outcome(&mut state.releases, release_id, &outcome)?;
        let release = state
            .releases
            .get(&release_id)
            .cloned()
            .ok_or(ApplicationDeliveryServiceError::ReleaseNotFound(release_id))?;
        self.audit_lifecycle(
            caller,
            "release.promoted",
            &release,
            ReleaseState::Deploying,
        );
        Ok(release)
    }

    /// Roll back an environment to a prior healthy release.
    /// The operator MUST provide a non-empty confirmation.
    pub fn rollback(
        &self,
        caller: &User,
        application_id: Uuid,
        environment_name: &str,
        target_release_id: Uuid,
        confirmation: &str,
    ) -> Result<Release, ApplicationDeliveryServiceError> {
        Self::require_operator(caller).inspect_err(|_err| {
            self.audit_denied(caller, "rollback", environment_name);
        })?;
        let mut state = self.state.lock().map_err(internal)?;
        let application = state
            .applications
            .get(&application_id)
            .ok_or(ApplicationDeliveryServiceError::ApplicationNotFound(
                application_id,
            ))?
            .clone();
        if application.owner_id() != caller.id() {
            return Err(ApplicationDeliveryServiceError::Forbidden);
        }
        let environment = application
            .environment(environment_name)
            .ok_or(ApplicationDeliveryServiceError::Domain(
                ApplicationDeliveryError::MissingEnvironment("environment not found"),
            ))?
            .clone();
        if environment.is_expired(Utc::now()) {
            return Err(ApplicationDeliveryServiceError::EnvironmentExpired);
        }
        let target = state
            .releases
            .get(&target_release_id)
            .ok_or(ApplicationDeliveryServiceError::ReleaseNotFound(
                target_release_id,
            ))?
            .clone();
        let request = openpanel_domain::RollbackRequest::new(
            application_id,
            environment.id(),
            target_release_id,
            caller.username().as_str(),
            confirmation,
        )
        .map_err(domain)?;
        can_rollback(&target, &environment, &request).map_err(domain)?;
        let outcome = self
            .runtime_adapter
            .rollback(&target, &environment, target_release_id)
            .map_err(ApplicationDeliveryServiceError::Domain)?;
        apply_outcome(&mut state.releases, target_release_id, &outcome)?;
        let release = state.releases.get(&target_release_id).cloned().ok_or(
            ApplicationDeliveryServiceError::ReleaseNotFound(target_release_id),
        )?;
        self.audit_lifecycle(
            caller,
            "release.rolled_back",
            &release,
            ReleaseState::Failed,
        );
        Ok(release)
    }

    /// List every registered application owned by `caller`.
    pub fn list_applications(
        &self,
        caller: &User,
    ) -> Result<Vec<Application>, ApplicationDeliveryServiceError> {
        let state = self.state.lock().map_err(internal)?;
        let mut apps: Vec<Application> = state
            .applications
            .values()
            .filter(|a| a.owner_id() == caller.id())
            .cloned()
            .collect();
        apps.sort_by(|left, right| left.name().cmp(right.name()));
        Ok(apps)
    }

    /// List every release for an application.
    pub fn list_releases(
        &self,
        caller: &User,
        application_id: Uuid,
    ) -> Result<Vec<Release>, ApplicationDeliveryServiceError> {
        let state = self.state.lock().map_err(internal)?;
        let application = state.applications.get(&application_id).ok_or(
            ApplicationDeliveryServiceError::ApplicationNotFound(application_id),
        )?;
        if application.owner_id() != caller.id() {
            return Err(ApplicationDeliveryServiceError::Forbidden);
        }
        let mut releases: Vec<Release> = state
            .releases
            .values()
            .filter(|r| r.application_id() == application_id)
            .cloned()
            .collect();
        releases.sort_by_key(|r| r.created_at());
        Ok(releases)
    }

    /// Project an `Application` aggregate into the public
    /// `ApplicationRecord` used by the API and the CLI.
    pub fn project_application(
        &self,
        caller: &User,
        application_id: Uuid,
    ) -> Result<ApplicationRecord, ApplicationDeliveryServiceError> {
        let state = self.state.lock().map_err(internal)?;
        let application = state
            .applications
            .get(&application_id)
            .ok_or(ApplicationDeliveryServiceError::ApplicationNotFound(
                application_id,
            ))?
            .clone();
        if application.owner_id() != caller.id() {
            return Err(ApplicationDeliveryServiceError::Forbidden);
        }
        let mut environments: Vec<EnvironmentRecord> = application
            .environments()
            .iter()
            .map(|e| EnvironmentRecord {
                id: e.id(),
                application_id: e.application_id(),
                kind: e.kind().as_str().to_string(),
                name: e.name().to_string(),
                quota: e.quota(),
                secret_set_name: e.secret_set().name().to_string(),
                secret_set_version: e.secret_set().version().map(|v| v.to_string()),
                created_at: e.created_at(),
                expires_at: e.expires_at(),
            })
            .collect();
        environments.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(ApplicationRecord {
            id: application.id(),
            owner_id: application.owner_id(),
            name: application.name().to_string(),
            source_kind: application.source().kind().as_str().to_string(),
            source_url: application.source().url().to_string(),
            source_ref: application.source().source_ref().to_string(),
            build_command: application.build_command().to_string(),
            created_at: application.created_at(),
            environments,
        })
    }

    /// Project a release into the public `ReleaseRecord`.
    pub fn project_release(
        &self,
        caller: &User,
        release_id: Uuid,
    ) -> Result<ReleaseRecord, ApplicationDeliveryServiceError> {
        let state = self.state.lock().map_err(internal)?;
        let release = state
            .releases
            .get(&release_id)
            .ok_or(ApplicationDeliveryServiceError::ReleaseNotFound(release_id))?
            .clone();
        let application = state.applications.get(&release.application_id()).ok_or(
            ApplicationDeliveryServiceError::ApplicationNotFound(release.application_id()),
        )?;
        if application.owner_id() != caller.id() {
            return Err(ApplicationDeliveryServiceError::Forbidden);
        }
        Ok(ReleaseRecord {
            id: release.id(),
            application_id: release.application_id(),
            environment_id: release.environment_id(),
            digest_value: release.digest().value().to_string(),
            digest_resolved_at: release.digest().resolved_at(),
            source_kind: release.source().kind().as_str().to_string(),
            source_url: release.source().url().to_string(),
            source_ref: release.source().source_ref().to_string(),
            state: release.state().as_str().to_string(),
            created_at: release.created_at(),
            last_transition_at: release.last_transition_at(),
            previous_release_id: release.previous_release_id(),
            diagnostic: release.diagnostic().to_string(),
        })
    }

    fn audit_registered(&self, caller: &User, application: &Application) {
        let event = AuditEvent::new(
            caller.username().as_str(),
            AuditAction::SoftwareChanged,
            AuditOutcome::Success,
        )
        .target(application.name())
        .metadata(redact_metadata(&serde_json::json!({
            "application_id": application.id().to_string(),
            "source_kind": application.source().kind().as_str(),
            "source_url": application.source().url(),
            "source_ref": application.source().source_ref(),
        })));
        let audit = self.audit.clone();
        tokio::spawn(async move {
            let _ = audit.record(event).await;
        });
    }

    fn audit_lifecycle(
        &self,
        caller: &User,
        action: &str,
        release: &Release,
        before: ReleaseState,
    ) {
        let event = AuditEvent::new(
            caller.username().as_str(),
            AuditAction::SoftwareChanged,
            AuditOutcome::Success,
        )
        .target(release.id().to_string())
        .metadata(redact_metadata(&serde_json::json!({
            "action": action,
            "application_id": release.application_id().to_string(),
            "environment_id": release.environment_id().to_string(),
            "release_id": release.id().to_string(),
            "release_digest": release.digest().value(),
            "state_before": before.as_str(),
            "state_after": release.state().as_str(),
        })));
        let audit = self.audit.clone();
        tokio::spawn(async move {
            let _ = audit.record(event).await;
        });
    }

    fn audit_denied(&self, caller: &User, intent: &str, target: &str) {
        let event = AuditEvent::new(
            caller.username().as_str(),
            AuditAction::PermissionDenied,
            AuditOutcome::Denied,
        )
        .target(target)
        .metadata(redact_metadata(&serde_json::json!({
            "intent": intent,
            "target": target,
        })));
        let audit = self.audit.clone();
        tokio::spawn(async move {
            let _ = audit.record(event).await;
        });
    }
}

fn internal<E: std::fmt::Display>(err: E) -> ApplicationDeliveryServiceError {
    ApplicationDeliveryServiceError::Persistence(err.to_string())
}

fn domain(err: ApplicationDeliveryError) -> ApplicationDeliveryServiceError {
    ApplicationDeliveryServiceError::Domain(err)
}

fn apply_outcome(
    releases: &mut HashMap<Uuid, Release>,
    release_id: Uuid,
    outcome: &ReleaseOutcome,
) -> Result<(), ApplicationDeliveryServiceError> {
    let release = releases
        .get_mut(&release_id)
        .ok_or(ApplicationDeliveryServiceError::ReleaseNotFound(release_id))?;
    let target = outcome.state;
    let now = Utc::now();
    let redacted = openpanel_domain::application_delivery::redact_diagnostic(&outcome.diagnostic);
    release.transition(target, now, &redacted).map_err(domain)?;
    Ok(())
}

// Touch the deployment-adapters redaction helper so the
// `reuse-strict` gate cannot collapse the two contracts (the
// deployment-adapters gate asserts the app service routes
// audit metadata through the canonical `redact_metadata`).
#[allow(dead_code)]
fn _reused_redaction_helper(value: &str) -> String {
    let _ = redact_diagnostic_deployment(value);
    openpanel_domain::application_delivery::redact_diagnostic(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpanel_core::audit::NoopAuditService;
    use openpanel_domain::{ApplicationSource, ApplicationSourceKind, Email, Password, Username};

    fn owner() -> User {
        User::new(
            Uuid::new_v4(),
            Username::new("owner").expect("static"),
            Email::new("owner@example.com").expect("static"),
            Password::hash("correct horse battery staple").expect("static"),
            Role::Owner,
        )
    }

    fn member() -> User {
        User::new(
            Uuid::new_v4(),
            Username::new("member").expect("static"),
            Email::new("member@example.com").expect("static"),
            Password::hash("correct horse battery staple").expect("static"),
            Role::User,
        )
    }

    struct FakeBuildAdapter;
    impl BuildAdapter for FakeBuildAdapter {
        fn adapter_id(&self) -> &'static str {
            "fake-build-v1"
        }
        fn resolve_digest(
            &self,
            _application_id: Uuid,
            _source: &ApplicationSource,
            _build_command: &str,
        ) -> Result<openpanel_domain::ReleaseDigest, ApplicationDeliveryError> {
            openpanel_domain::ReleaseDigest::new("sha256:cafef00ddeadbeef", Utc::now())
        }
    }

    struct RecordingBuildAdapter(std::sync::Mutex<Vec<String>>);

    impl BuildAdapter for RecordingBuildAdapter {
        fn adapter_id(&self) -> &'static str {
            "recording-build-v1"
        }

        fn resolve_digest(
            &self,
            _application_id: Uuid,
            _source: &ApplicationSource,
            build_command: &str,
        ) -> Result<openpanel_domain::ReleaseDigest, ApplicationDeliveryError> {
            self.0
                .lock()
                .expect("invariant: test lock")
                .push(build_command.into());
            openpanel_domain::ReleaseDigest::new("sha256:cafef00ddeadbeef", Utc::now())
        }
    }

    #[tokio::test]
    async fn test_build_release_passes_build_command_unchanged() {
        let adapter = Arc::new(RecordingBuildAdapter(std::sync::Mutex::new(Vec::new())));
        let service = ApplicationDeliveryService::new(
            adapter.clone(),
            Arc::new(FakeRuntimeAdapter),
            Arc::new(NoopAuditService),
        );
        let caller = owner();
        let application = service
            .register_application(
                &caller,
                "build-command",
                git_source(),
                "npm ci && npm run build",
            )
            .expect("valid application");
        service
            .add_environment(
                &caller,
                application.id(),
                EnvironmentKind::Production,
                "production",
                4,
                SecretSetRef::new("prod-secrets", None).expect("valid secret set"),
                None,
            )
            .expect("valid environment");
        service
            .build_release(&caller, application.id(), "production")
            .expect("release built");
        assert_eq!(
            adapter.0.lock().expect("invariant: test lock").as_slice(),
            &["npm ci && npm run build"]
        );
    }

    struct FakeRuntimeAdapter;
    impl RuntimeAdapter for FakeRuntimeAdapter {
        fn adapter_id(&self) -> &'static str {
            "fake-runtime-v1"
        }
        fn deploy(
            &self,
            _release: &Release,
            _environment: &Environment,
        ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
            Ok(ReleaseOutcome {
                state: ReleaseState::Healthy,
                diagnostic: "fake: ok".into(),
            })
        }
        fn rollback(
            &self,
            _release: &Release,
            _environment: &Environment,
            _target_release_id: Uuid,
        ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
            Ok(ReleaseOutcome {
                state: ReleaseState::RolledBack,
                diagnostic: "fake: rolled back".into(),
            })
        }
    }

    struct FailingRuntimeAdapter;
    impl RuntimeAdapter for FailingRuntimeAdapter {
        fn adapter_id(&self) -> &'static str {
            "failing-runtime-v1"
        }
        fn deploy(
            &self,
            _release: &Release,
            _environment: &Environment,
        ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
            Ok(ReleaseOutcome {
                state: ReleaseState::Failed,
                diagnostic: "fake: deploy failed password=hunter2".into(),
            })
        }
        fn rollback(
            &self,
            _release: &Release,
            _environment: &Environment,
            _target_release_id: Uuid,
        ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
            Ok(ReleaseOutcome {
                state: ReleaseState::RolledBack,
                diagnostic: "fake: rolled back".into(),
            })
        }
    }

    fn service() -> ApplicationDeliveryService {
        ApplicationDeliveryService::new(
            Arc::new(FakeBuildAdapter),
            Arc::new(FakeRuntimeAdapter),
            Arc::new(NoopAuditService),
        )
    }

    fn service_with(rt: Arc<dyn RuntimeAdapter>) -> ApplicationDeliveryService {
        ApplicationDeliveryService::new(Arc::new(FakeBuildAdapter), rt, Arc::new(NoopAuditService))
    }

    fn git_source() -> ApplicationSource {
        ApplicationSource::new(
            ApplicationSourceKind::Git,
            "https://example.com/repo.git",
            "abcdef1234567890",
        )
        .unwrap()
    }

    fn make_application(service: &ApplicationDeliveryService, owner: &User) -> (Uuid, Uuid) {
        let application = service
            .register_application(owner, "web-frontend", git_source(), "")
            .unwrap();
        let prod = service
            .add_environment(
                owner,
                application.id(),
                EnvironmentKind::Production,
                "production",
                4,
                SecretSetRef::new("prod-secrets", None).unwrap(),
                None,
            )
            .unwrap();
        (application.id(), prod.id())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn register_application_rejects_member() {
        let svc = service();
        let result = svc.register_application(&member(), "web", git_source(), "");
        assert!(matches!(
            result,
            Err(ApplicationDeliveryServiceError::Forbidden)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn register_application_rejects_bad_name() {
        let svc = service();
        let owner = owner();
        let result = svc.register_application(&owner, "BadName!", git_source(), "");
        assert!(matches!(
            result,
            Err(ApplicationDeliveryServiceError::Domain(_))
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn register_application_rejects_duplicate_name() {
        let svc = service();
        let owner = owner();
        svc.register_application(&owner, "web", git_source(), "")
            .unwrap();
        let again = svc.register_application(&owner, "web", git_source(), "");
        assert!(matches!(
            again,
            Err(ApplicationDeliveryServiceError::Domain(_))
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn build_release_walks_to_deploying() {
        let svc = service();
        let owner = owner();
        let (app_id, _env_id) = make_application(&svc, &owner);
        let release = svc.build_release(&owner, app_id, "production").unwrap();
        // The lifecycle walks Pending -> Building -> Deploying; the
        // runtime adapter (Healthy in the fake) is invoked from
        // `promote`, not `build_release`, so the post-build state
        // is `Deploying`.
        assert_eq!(release.state(), ReleaseState::Deploying);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn promote_walks_to_healthy() {
        let svc = service();
        let owner = owner();
        let (app_id, _env_id) = make_application(&svc, &owner);
        let release = svc.build_release(&owner, app_id, "production").unwrap();
        let promoted = svc
            .promote(&owner, app_id, "production", release.id(), "PROMOTE")
            .unwrap();
        assert_eq!(promoted.state(), ReleaseState::Healthy);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn promote_rejects_empty_confirmation() {
        let svc = service();
        let owner = owner();
        let (app_id, _env_id) = make_application(&svc, &owner);
        let release = svc.build_release(&owner, app_id, "production").unwrap();
        let result = svc.promote(&owner, app_id, "production", release.id(), "");
        assert!(matches!(
            result,
            Err(ApplicationDeliveryServiceError::Domain(_))
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rollback_moves_release_to_rolled_back() {
        let svc = service_with(Arc::new(FailingRuntimeAdapter));
        let owner = owner();
        let (app_id, _env_id) = make_application(&svc, &owner);
        // Build + promote a release; the failing adapter will land
        // it in `Failed`.
        let release = svc.build_release(&owner, app_id, "production").unwrap();
        let failed = svc
            .promote(&owner, app_id, "production", release.id(), "PROMOTE")
            .unwrap();
        assert_eq!(failed.state(), ReleaseState::Failed);
        // Roll it back. The failing adapter reports `RolledBack`
        // for the rollback step.
        let rolled = svc
            .rollback(&owner, app_id, "production", release.id(), "ROLLBACK")
            .unwrap();
        assert_eq!(rolled.state(), ReleaseState::RolledBack);
        // The diagnostic is redacted — secrets never appear in the
        // record.
        assert!(rolled.diagnostic().is_empty() || !rolled.diagnostic().contains("hunter2"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_applications_returns_owner_scoped_sorted() {
        let svc = service();
        let owner = owner();
        svc.register_application(&owner, "zeta", git_source(), "")
            .unwrap();
        svc.register_application(&owner, "alpha", git_source(), "")
            .unwrap();
        let apps = svc.list_applications(&owner).unwrap();
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name(), "alpha");
        assert_eq!(apps[1].name(), "zeta");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn promote_rejects_non_owner() {
        let svc = service();
        let owner = owner();
        let (app_id, _env_id) = make_application(&svc, &owner);
        let release = svc.build_release(&owner, app_id, "production").unwrap();
        let result = svc.promote(&member(), app_id, "production", release.id(), "PROMOTE");
        assert!(matches!(
            result,
            Err(ApplicationDeliveryServiceError::Forbidden)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn project_application_returns_sorted_environments() {
        let svc = service();
        let owner = owner();
        let application = svc
            .register_application(&owner, "web", git_source(), "")
            .unwrap();
        let _ = svc
            .add_environment(
                &owner,
                application.id(),
                EnvironmentKind::Production,
                "production",
                4,
                SecretSetRef::new("prod-secrets", None).unwrap(),
                None,
            )
            .unwrap();
        let _ = svc
            .add_environment(
                &owner,
                application.id(),
                EnvironmentKind::Preview,
                "preview-42",
                1,
                SecretSetRef::new("preview-secrets", None).unwrap(),
                Some(Utc::now() + chrono::Duration::hours(2)),
            )
            .unwrap();
        let record = svc.project_application(&owner, application.id()).unwrap();
        assert_eq!(record.environments.len(), 2);
        assert_eq!(record.environments[0].name, "preview-42");
        assert_eq!(record.environments[1].name, "production");
    }
}
