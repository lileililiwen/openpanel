//! Pure container-management aggregates and validation boundaries.

use std::{collections::BTreeMap, path::PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Domain failures for container management.
#[derive(Debug, Error)]
pub enum DockerError {
    /// Caller lacks the required role.
    #[error("forbidden")]
    Forbidden,
    /// Input failed a typed safety boundary.
    #[error("invalid docker request: {0}")]
    Invalid(String),
    /// Image is outside the configured trust root.
    #[error("image is not allowlisted: {0}")]
    ImageDenied(String),
    /// Runtime adapter failed.
    #[error("container runtime failed: {0}")]
    Adapter(String),
    /// Stored object was not found.
    #[error("docker object not found: {0}")]
    NotFound(String),
    /// A container create would exceed the per-user quota.
    #[error("container quota exceeded")]
    QuotaExceeded,
    /// Persistence failed.
    #[error("docker persistence failed: {0}")]
    Persistence(String),
}

/// A trusted image-reference pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageAllowlistEntry {
    /// Prefix or `*`-suffix pattern.
    pub pattern: String,
    /// Whether pulls are enabled.
    pub allow_pull: bool,
    /// Whether port-binding containers require an `@sha256:` pin.
    pub pin_digest_required: bool,
}

impl ImageAllowlistEntry {
    /// Test a normalized image reference against this entry.
    pub fn matches(&self, image: &str) -> bool {
        self.allow_pull
            && self
                .pattern
                .strip_suffix('*')
                .map_or_else(|| image == self.pattern, |prefix| image.starts_with(prefix))
    }

    /// Validate a bounded, non-injectable pattern.
    pub fn validate(&self) -> Result<(), DockerError> {
        if self.pattern.is_empty()
            || self.pattern.len() > 255
            || self.pattern.matches('*').count() > 1
            || (self.pattern.contains('*') && !self.pattern.ends_with('*'))
            || !self
                .pattern
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-/:@*".contains(&byte))
        {
            return Err(DockerError::Invalid("invalid image pattern".into()));
        }
        Ok(())
    }
}

/// Container resource ceilings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResourceLimits {
    /// Docker CPU share weight.
    pub cpu_shares: u64,
    /// Hard memory ceiling in MiB.
    pub mem_mb: u64,
    /// Maximum process count.
    pub pids_max: i64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_shares: 1024,
            mem_mb: 512,
            pids_max: 256,
        }
    }
}

/// Supported daemon restart policies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    /// Never restart.
    No,
    /// Restart on failure with an optional retry ceiling.
    OnFailure {
        /// Optional retry ceiling.
        max_retries: Option<u64>,
    },
    /// Always restart.
    Always,
    /// Restart unless explicitly stopped.
    UnlessStopped,
}

/// One host-to-container port mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortBinding {
    /// Host TCP port.
    pub host: u16,
    /// Container TCP port.
    pub container: u16,
}

/// One constrained bind mount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindMount {
    /// Absolute host source.
    pub source: PathBuf,
    /// Absolute container destination.
    pub target: PathBuf,
    /// Mount read-only flag.
    #[serde(default)]
    pub read_only: bool,
}

/// Strict create request accepted by every adapter surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerSpec {
    /// Panel identifier used for names and storage roots.
    pub id: Uuid,
    /// Unique bounded name.
    pub name: String,
    /// Trusted image reference.
    pub image: String,
    /// Optional owning site.
    pub site_id: Option<Uuid>,
    /// Environment values; persistence/adapters must redact these.
    #[serde(default, skip_serializing)]
    pub env: BTreeMap<String, String>,
    /// Optional direct argv used instead of the image default command.
    #[serde(default)]
    pub command: Vec<String>,
    /// Port bindings.
    #[serde(default)]
    pub ports: Vec<PortBinding>,
    /// Bind mounts.
    #[serde(default)]
    pub mounts: Vec<BindMount>,
    /// Explicit capability additions.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Resource ceilings.
    #[serde(default)]
    pub limits: ResourceLimits,
    /// Restart behavior.
    pub restart_policy: RestartPolicy,
    /// Numeric non-root user in `uid:gid` form.
    pub user_namespace: String,
}

impl ContainerSpec {
    /// Validate all security and resource boundaries before I/O.
    pub fn validate(
        &self,
        allowed_ports: std::ops::RangeInclusive<u16>,
    ) -> Result<(), DockerError> {
        if self.name.is_empty()
            || self.name.len() > 63
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(DockerError::Invalid("invalid container name".into()));
        }
        validate_image(&self.image)?;
        if self.user_namespace == "0"
            || self.user_namespace.starts_with("0:")
            || !self
                .user_namespace
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b':')
        {
            return Err(DockerError::Invalid(
                "container user must be a numeric non-root uid:gid".into(),
            ));
        }
        if self.limits.cpu_shares < 2
            || self.limits.cpu_shares > 262_144
            || self.limits.mem_mb < 16
            || self.limits.mem_mb > 1_048_576
            || self.limits.pids_max < 1
            || self.limits.pids_max > 1_000_000
        {
            return Err(DockerError::Invalid(
                "resource limit is out of range".into(),
            ));
        }
        if self
            .ports
            .iter()
            .any(|port| !allowed_ports.contains(&port.host) || port.container == 0)
        {
            return Err(DockerError::Invalid(
                "port is outside the allowed range".into(),
            ));
        }
        let root = PathBuf::from(format!("/var/lib/openpanel/containers/{}/", self.id));
        if self.mounts.iter().any(|mount| {
            !mount.source.is_absolute()
                || !mount.source.starts_with(&root)
                || !mount.target.is_absolute()
                || mount
                    .target
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
        }) {
            return Err(DockerError::Invalid(
                "bind mount escapes the container root".into(),
            ));
        }
        const FORBIDDEN: [&str; 6] = [
            "cap_sys_admin",
            "cap_sys_ptrace",
            "cap_sys_module",
            "cap_net_admin",
            "cap_net_raw",
            "cap_dac_override",
        ];
        for capability in &self.capabilities {
            let capability = capability.to_ascii_lowercase();
            if FORBIDDEN.contains(&capability.as_str())
                || (capability != "cap_net_bind_service" && capability != "cap_chown")
            {
                return Err(DockerError::Invalid(format!(
                    "forbidden capability {capability}"
                )));
            }
            if capability == "cap_net_bind_service"
                && !self.ports.iter().any(|port| port.host < 1024)
            {
                return Err(DockerError::Invalid(
                    "cap_net_bind_service requires a privileged port".into(),
                ));
            }
        }
        if self.env.len() > 128
            || self.env.iter().any(|(key, value)| {
                key.is_empty()
                    || key.len() > 128
                    || value.len() > 4096
                    || !key
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            })
        {
            return Err(DockerError::Invalid("invalid environment envelope".into()));
        }
        if self.command.len() > 32
            || self
                .command
                .iter()
                .any(|arg| arg.is_empty() || arg.len() > 4096 || arg.contains('\0'))
        {
            return Err(DockerError::Invalid("invalid container command".into()));
        }
        Ok(())
    }
}

fn validate_image(image: &str) -> Result<(), DockerError> {
    if image.is_empty()
        || image.len() > 255
        || !image
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/:@".contains(&b))
    {
        return Err(DockerError::Invalid("invalid image reference".into()));
    }
    Ok(())
}

/// Persisted container lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Container {
    /// Validated desired specification.
    pub spec: ContainerSpec,
    /// Runtime identifier after creation.
    pub runtime_id: Option<String>,
    /// Current status label.
    pub status: String,
    /// Whether the last exit was OOM-driven.
    pub oom_killed: bool,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// Signed desired state for a group of containers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeStack {
    /// Stack identifier.
    pub id: Uuid,
    /// Unique name.
    pub name: String,
    /// Canonical YAML document.
    pub compose_yaml_canonical: String,
    /// Detached trust-root signature.
    pub signature_b64: String,
    /// Optional owning site.
    pub site_id: Option<Uuid>,
}

impl ComposeStack {
    /// Validate name, canonical YAML, and detached-signature envelope.
    pub fn validate(&self) -> Result<(), DockerError> {
        if self.name.is_empty()
            || self.name.len() > 63
            || !self
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            return Err(DockerError::Invalid("invalid stack name".into()));
        }
        if self.compose_yaml_canonical.is_empty()
            || self.compose_yaml_canonical.len() > 1_048_576
            || self.signature_b64.is_empty()
            || self.signature_b64.len() > 256
        {
            return Err(DockerError::Invalid("invalid stack envelope".into()));
        }
        let parsed: serde_yaml::Value = serde_yaml::from_str(&self.compose_yaml_canonical)
            .map_err(|_| DockerError::Invalid("invalid compose YAML".into()))?;
        let canonical = serde_yaml::to_string(&parsed)
            .map_err(|_| DockerError::Invalid("invalid compose YAML".into()))?;
        if canonical != self.compose_yaml_canonical {
            return Err(DockerError::Invalid("compose YAML is not canonical".into()));
        }
        Ok(())
    }
}

/// Persisted stack reconciliation state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredComposeStack {
    /// Signed desired state.
    pub stack: ComposeStack,
    /// Last reconciliation status.
    pub status: String,
    /// Latest successful apply timestamp.
    pub last_applied_at: Option<DateTime<Utc>>,
}

/// Persistence port for Docker desired state and provenance.
#[async_trait]
pub trait DockerRepository: Send + Sync + 'static {
    /// Store or replace a container.
    async fn put_container(&self, container: &Container) -> Result<(), RepoError>;
    /// Load a container.
    async fn get_container(&self, id: Uuid) -> Result<Option<Container>, RepoError>;
    /// List containers.
    async fn list_containers(&self) -> Result<Vec<Container>, RepoError>;
    /// Delete a container row.
    async fn delete_container(&self, id: Uuid) -> Result<(), RepoError>;
    /// List trusted image patterns.
    async fn allowlist(&self) -> Result<Vec<ImageAllowlistEntry>, RepoError>;
    /// Store a trusted image pattern.
    async fn put_allowlist(&self, entry: &ImageAllowlistEntry) -> Result<(), RepoError>;
    /// Store or replace a stack.
    async fn put_stack(&self, stack: &StoredComposeStack) -> Result<(), RepoError>;
    /// Load a stack.
    async fn get_stack(&self, id: Uuid) -> Result<Option<StoredComposeStack>, RepoError>;
    /// List stacks.
    async fn list_stacks(&self) -> Result<Vec<StoredComposeStack>, RepoError>;
    /// Delete a stack.
    async fn delete_stack(&self, id: Uuid) -> Result<(), RepoError>;
}
