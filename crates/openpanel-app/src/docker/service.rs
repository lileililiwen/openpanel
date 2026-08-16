//! Owner-only container lifecycle orchestration.

use std::{ops::RangeInclusive, sync::Arc};

use async_trait::async_trait;
use base64::Engine;
use chrono::Utc;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    RepoError, Role, User,
    docker::{
        ComposeStack, Container, ContainerSpec, DockerError, DockerRepository, ImageAllowlistEntry,
        StoredComposeStack,
    },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::container_runtime::{ContainerRuntimeService, ProposedContainer, UsageSnapshot};

/// Bounded runtime inspection result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeContainerState {
    /// Runtime status.
    pub status: String,
    /// Whether the kernel OOM-killed the process.
    pub oom_killed: bool,
}

/// Bounded exec result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecResult {
    /// Exit code.
    pub exit_code: i64,
    /// Bounded and redacted combined output.
    pub output: String,
    /// Whether output was truncated.
    pub truncated: bool,
}

/// Stack reconciliation diff returned by adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyReport {
    /// Runtime containers created or replaced.
    pub created: Vec<String>,
    /// Runtime containers removed as drift.
    pub removed: Vec<String>,
}

/// Typed site-network provisioning boundary.
#[async_trait]
pub trait NetworkAdapter: Send + Sync + 'static {
    /// Ensure the dedicated site or system bridge exists.
    async fn ensure(&self, site_id: Option<Uuid>) -> Result<(), DockerError>;
    /// Remove the bridge when no active endpoints remain.
    async fn remove_if_unused(&self, site_id: Option<Uuid>) -> Result<(), DockerError>;
}

/// The only runtime verbs exposed to the application service.
#[async_trait]
pub trait DockerAdapter: Send + Sync + 'static {
    /// Check daemon availability.
    async fn ping(&self) -> Result<(), DockerError>;
    /// Pull an image and return its digest.
    async fn pull(&self, image: &str) -> Result<String, DockerError>;
    /// Create a validated container and its site network.
    async fn create(&self, spec: &ContainerSpec) -> Result<String, DockerError>;
    /// Start a container.
    async fn start(&self, id: &str) -> Result<(), DockerError>;
    /// Stop a container.
    async fn stop(&self, id: &str) -> Result<(), DockerError>;
    /// Restart a container.
    async fn restart(&self, id: &str) -> Result<(), DockerError>;
    /// Remove a container.
    async fn remove(&self, id: &str, force: bool) -> Result<(), DockerError>;
    /// Inspect runtime state.
    async fn inspect(&self, id: &str) -> Result<RuntimeContainerState, DockerError>;
    /// Read bounded tail logs.
    async fn logs(&self, id: &str, tail: u64) -> Result<Vec<String>, DockerError>;
    /// Execute one argv without a shell.
    async fn exec(
        &self,
        id: &str,
        command: &[String],
        user: &str,
    ) -> Result<ExecResult, DockerError>;
    /// Reconcile one verified canonical compose document.
    async fn apply_stack(&self, stack: &ComposeStack) -> Result<ApplyReport, DockerError>;
    /// Remove every runtime container owned by a compose stack.
    async fn remove_stack(&self, stack: &ComposeStack) -> Result<Vec<String>, DockerError>;
}

/// Owner-only service that enforces trust before runtime I/O.
pub struct DockerService {
    repo: Arc<dyn DockerRepository>,
    adapter: Arc<dyn DockerAdapter>,
    network: Arc<dyn NetworkAdapter>,
    audit: Arc<dyn AuditService>,
    allowed_ports: RangeInclusive<u16>,
    quota_gate: std::sync::RwLock<Option<Arc<ContainerRuntimeService>>>,
}

impl DockerService {
    /// Construct with persistence, runtime, and audit ports.
    pub fn new(
        repo: Arc<dyn DockerRepository>,
        adapter: Arc<dyn DockerAdapter>,
        network: Arc<dyn NetworkAdapter>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo,
            adapter,
            network,
            audit,
            allowed_ports: 8080..=8999,
            quota_gate: std::sync::RwLock::new(None),
        }
    }

    /// Attach the per-user container quota gate. The docker
    /// service is the *consumer* of the gate: every create reads
    /// the caller's effective quota and refuses to start a
    /// container that would exceed any axis.
    pub fn attach_quota_gate(&self, gate: Arc<ContainerRuntimeService>) {
        *self.quota_gate.write().expect("invariant: rwlock poisoned") = Some(gate);
    }

    /// List trusted image patterns.
    pub async fn allowlist(&self, caller: &User) -> Result<Vec<ImageAllowlistEntry>, DockerError> {
        owner(caller)?;
        self.repo.allowlist().await.map_err(persist)
    }

    /// Add or update a trusted image pattern.
    pub async fn put_allowlist(
        &self,
        caller: &User,
        entry: ImageAllowlistEntry,
    ) -> Result<(), DockerError> {
        owner(caller)?;
        entry.validate()?;
        self.repo.put_allowlist(&entry).await.map_err(persist)?;
        self.record(
            caller,
            AuditAction::DockerChanged,
            &entry.pattern,
            AuditOutcome::Success,
        )
        .await;
        Ok(())
    }

    /// Pull only after allowlist evaluation.
    pub async fn pull(&self, caller: &User, image: &str) -> Result<String, DockerError> {
        owner(caller)?;
        if !self
            .repo
            .allowlist()
            .await
            .map_err(persist)?
            .iter()
            .any(|entry| entry.matches(image))
        {
            return Err(DockerError::ImageDenied(image.into()));
        }
        let digest = self.adapter.pull(image).await?;
        self.record(
            caller,
            AuditAction::DockerImagePulled,
            image,
            AuditOutcome::Success,
        )
        .await;
        Ok(digest)
    }

    /// Create desired and runtime state after complete validation.
    pub async fn create(
        &self,
        caller: &User,
        spec: ContainerSpec,
    ) -> Result<Container, DockerError> {
        owner(caller)?;
        if let Err(error) = spec.validate(self.allowed_ports.clone()) {
            let action = if error.to_string().contains("forbidden capability") {
                AuditAction::DockerCapabilityDenied
            } else {
                AuditAction::DockerSpecRejected
            };
            self.record(caller, action, &spec.id.to_string(), AuditOutcome::Failure)
                .await;
            return Err(error);
        }
        let allowlist = self.repo.allowlist().await.map_err(persist)?;
        let trusted = allowlist
            .iter()
            .find(|entry| entry.matches(&spec.image))
            .ok_or_else(|| DockerError::ImageDenied(spec.image.clone()))?;
        if !spec.ports.is_empty() && trusted.pin_digest_required && !spec.image.contains("@sha256:")
        {
            return Err(DockerError::Invalid(
                "production image requires a sha256 digest pin".into(),
            ));
        }
        let gate: Option<Arc<ContainerRuntimeService>> = self
            .quota_gate
            .read()
            .expect("invariant: rwlock poisoned")
            .clone();
        if let Some(gate) = gate {
            let containers = self.repo.list_containers().await.map_err(persist)?;
            let running = containers
                .iter()
                .filter(|container| container.status == "running")
                .count() as u32;
            let usage = UsageSnapshot {
                running_concurrent: running,
                total: containers.len() as u32,
                egress_used: 0,
            };
            let proposed = ProposedContainer {
                count: 1,
                cpu_pct: 0,
                memory_bytes: spec.limits.mem_mb.saturating_mul(1024 * 1024),
                egress_delta: 0,
            };
            gate.check_quota(caller, caller.id(), usage, proposed)
                .await
                .map_err(|_| DockerError::QuotaExceeded)?;
        }
        self.network.ensure(spec.site_id).await?;
        let runtime_id = match self.adapter.create(&spec).await {
            Ok(runtime_id) => runtime_id,
            Err(error) => {
                let _ = self.network.remove_if_unused(spec.site_id).await;
                return Err(error);
            }
        };
        let container = Container {
            spec,
            runtime_id: Some(runtime_id),
            status: "created".into(),
            oom_killed: false,
            created_at: Utc::now(),
        };
        self.repo.put_container(&container).await.map_err(persist)?;
        Ok(container)
    }

    /// List persisted container summaries.
    pub async fn list(&self, caller: &User) -> Result<Vec<Container>, DockerError> {
        owner(caller)?;
        self.repo.list_containers().await.map_err(persist)
    }

    /// Inspect and persist the latest runtime state.
    pub async fn inspect(&self, caller: &User, id: Uuid) -> Result<Container, DockerError> {
        owner(caller)?;
        let mut container = self.require(id).await?;
        let runtime = container
            .runtime_id
            .as_deref()
            .ok_or_else(|| DockerError::NotFound(id.to_string()))?;
        let state = self.adapter.inspect(runtime).await?;
        let newly_oom = state.oom_killed && !container.oom_killed;
        container.status = state.status;
        container.oom_killed = state.oom_killed;
        self.repo.put_container(&container).await.map_err(persist)?;
        if newly_oom {
            self.record(
                caller,
                AuditAction::DockerOomKilled,
                &id.to_string(),
                AuditOutcome::Failure,
            )
            .await;
        }
        Ok(container)
    }

    /// Perform a lifecycle action and persist observed state.
    pub async fn action(
        &self,
        caller: &User,
        id: Uuid,
        action: &str,
    ) -> Result<Container, DockerError> {
        owner(caller)?;
        let mut container = self.require(id).await?;
        let runtime = container
            .runtime_id
            .clone()
            .ok_or_else(|| DockerError::NotFound(id.to_string()))?;
        match action {
            "start" => self.adapter.start(&runtime).await?,
            "stop" => self.adapter.stop(&runtime).await?,
            "restart" => self.adapter.restart(&runtime).await?,
            _ => return Err(DockerError::Invalid("unsupported lifecycle action".into())),
        }
        let state = self.adapter.inspect(&runtime).await?;
        let newly_oom = state.oom_killed && !container.oom_killed;
        container.status = state.status;
        container.oom_killed = state.oom_killed;
        self.repo.put_container(&container).await.map_err(persist)?;
        if newly_oom {
            self.record(
                caller,
                AuditAction::DockerOomKilled,
                &id.to_string(),
                AuditOutcome::Failure,
            )
            .await;
        }
        self.record(
            caller,
            AuditAction::DockerChanged,
            &id.to_string(),
            AuditOutcome::Success,
        )
        .await;
        if action == "stop" {
            self.network
                .remove_if_unused(container.spec.site_id)
                .await?;
        }
        Ok(container)
    }

    /// Read bounded, secret-redacted tail logs.
    pub async fn logs(
        &self,
        caller: &User,
        id: Uuid,
        tail: u64,
    ) -> Result<Vec<String>, DockerError> {
        owner(caller)?;
        let container = self.require(id).await?;
        let runtime = container
            .runtime_id
            .as_deref()
            .ok_or_else(|| DockerError::NotFound(id.to_string()))?;
        let mut lines = self.adapter.logs(runtime, tail.clamp(1, 1000)).await?;
        for line in &mut lines {
            *line = redact(line, &container.spec.env);
            line.truncate(4096);
        }
        Ok(lines)
    }

    /// Execute as the configured non-root container user.
    pub async fn exec(
        &self,
        caller: &User,
        id: Uuid,
        command: Vec<String>,
    ) -> Result<ExecResult, DockerError> {
        owner(caller)?;
        if command.is_empty()
            || command.len() > 32
            || command
                .iter()
                .any(|arg| arg.len() > 4096 || arg.contains('\0'))
        {
            return Err(DockerError::Invalid("invalid exec command".into()));
        }
        let container = self.require(id).await?;
        let runtime = container
            .runtime_id
            .as_deref()
            .ok_or_else(|| DockerError::NotFound(id.to_string()))?;
        let mut result = self
            .adapter
            .exec(runtime, &command, &container.spec.user_namespace)
            .await?;
        result.output = redact(&result.output, &container.spec.env);
        if result.output.len() > 65_536 {
            result.output.truncate(65_536);
            result.truncated = true;
        }
        Ok(result)
    }

    /// Remove runtime state before desired state.
    pub async fn remove(&self, caller: &User, id: Uuid, force: bool) -> Result<(), DockerError> {
        owner(caller)?;
        let container = self.require(id).await?;
        if let Some(runtime) = container.runtime_id {
            self.adapter.remove(&runtime, force).await?;
        }
        self.repo.delete_container(id).await.map_err(persist)?;
        self.network.remove_if_unused(container.spec.site_id).await
    }

    /// Verify and persist one signed canonical compose stack.
    pub async fn put_stack(
        &self,
        caller: &User,
        stack: ComposeStack,
    ) -> Result<StoredComposeStack, DockerError> {
        owner(caller)?;
        stack.validate()?;
        verify_stack_signature(&stack)?;
        if let Some(existing) = self.repo.get_stack(stack.id).await.map_err(persist)?
            && existing.stack.name != stack.name
        {
            return Err(DockerError::Invalid("stack name is immutable".into()));
        }
        let images = stack_images(&stack.compose_yaml_canonical)?;
        let allowlist = self.repo.allowlist().await.map_err(persist)?;
        if images
            .iter()
            .any(|image| !allowlist.iter().any(|entry| entry.matches(image)))
        {
            return Err(DockerError::ImageDenied("stack image".into()));
        }
        let stored = StoredComposeStack {
            stack,
            status: "pending".into(),
            last_applied_at: None,
        };
        self.repo.put_stack(&stored).await.map_err(persist)?;
        Ok(stored)
    }

    /// List persisted compose stacks.
    pub async fn stacks(&self, caller: &User) -> Result<Vec<StoredComposeStack>, DockerError> {
        owner(caller)?;
        self.repo.list_stacks().await.map_err(persist)
    }

    /// Reconcile a verified stack and persist its completion state.
    pub async fn apply_stack(&self, caller: &User, id: Uuid) -> Result<ApplyReport, DockerError> {
        owner(caller)?;
        let mut stored = self
            .repo
            .get_stack(id)
            .await
            .map_err(persist)?
            .ok_or_else(|| DockerError::NotFound(id.to_string()))?;
        self.network.ensure(stored.stack.site_id).await?;
        let report = self.adapter.apply_stack(&stored.stack).await?;
        stored.status = "applied".into();
        stored.last_applied_at = Some(Utc::now());
        self.repo.put_stack(&stored).await.map_err(persist)?;
        Ok(report)
    }

    /// Reconcile every previously applied stack to its signed desired state.
    pub async fn reconcile_stacks(&self) -> Result<(), DockerError> {
        for container in self.repo.list_containers().await.map_err(persist)? {
            self.network.ensure(container.spec.site_id).await?;
        }
        for mut stored in self.repo.list_stacks().await.map_err(persist)? {
            if stored.status != "applied" {
                continue;
            }
            self.network.ensure(stored.stack.site_id).await?;
            self.adapter.apply_stack(&stored.stack).await?;
            stored.last_applied_at = Some(Utc::now());
            self.repo.put_stack(&stored).await.map_err(persist)?;
        }
        Ok(())
    }

    /// Delete a stack record and remove its bridge if no endpoints remain.
    pub async fn delete_stack(&self, caller: &User, id: Uuid) -> Result<(), DockerError> {
        owner(caller)?;
        let stack = self
            .repo
            .get_stack(id)
            .await
            .map_err(persist)?
            .ok_or_else(|| DockerError::NotFound(id.to_string()))?;
        self.adapter.remove_stack(&stack.stack).await?;
        self.repo.delete_stack(id).await.map_err(persist)?;
        self.network.remove_if_unused(stack.stack.site_id).await
    }

    async fn require(&self, id: Uuid) -> Result<Container, DockerError> {
        self.repo
            .get_container(id)
            .await
            .map_err(persist)?
            .ok_or_else(|| DockerError::NotFound(id.to_string()))
    }

    async fn record(
        &self,
        caller: &User,
        action: AuditAction,
        target: &str,
        outcome: AuditOutcome,
    ) {
        let _ = self
            .audit
            .record(AuditEvent::new(caller.username().as_str(), action, outcome).target(target))
            .await;
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComposeDocument {
    services: std::collections::BTreeMap<String, ComposeService>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComposeService {
    image: String,
}

fn stack_images(yaml: &str) -> Result<Vec<String>, DockerError> {
    let document: ComposeDocument = serde_yaml::from_str(yaml)
        .map_err(|_| DockerError::Invalid("unsupported compose document".into()))?;
    if document.services.is_empty() || document.services.len() > 64 {
        return Err(DockerError::Invalid("invalid compose service count".into()));
    }
    Ok(document
        .services
        .into_values()
        .map(|service| service.image)
        .collect())
}

fn verify_stack_signature(stack: &ComposeStack) -> Result<(), DockerError> {
    const TRUST_ROOT: [u8; 32] = [
        0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07,
        0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07,
        0x51, 0x1a,
    ];
    let key = VerifyingKey::from_bytes(&TRUST_ROOT)
        .map_err(|_| DockerError::Invalid("invalid stack trust root".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&stack.signature_b64)
        .map_err(|_| DockerError::Invalid("invalid stack signature".into()))?;
    let signature = Signature::from_slice(&bytes)
        .map_err(|_| DockerError::Invalid("invalid stack signature".into()))?;
    key.verify(stack.compose_yaml_canonical.as_bytes(), &signature)
        .map_err(|_| DockerError::Invalid("invalid stack signature".into()))
}

fn owner(caller: &User) -> Result<(), DockerError> {
    if caller.role() != Role::Owner {
        return Err(DockerError::Forbidden);
    }
    Ok(())
}

fn persist(error: RepoError) -> DockerError {
    DockerError::Persistence(error.to_string())
}

fn redact(value: &str, env: &std::collections::BTreeMap<String, String>) -> String {
    env.values()
        .filter(|secret| !secret.is_empty())
        .fold(value.to_owned(), |text, secret| {
            text.replace(secret, "[REDACTED]")
        })
}
