//! Host capability port, in-memory adapter, and service.
//!
//! Per the `Host Capability Contract` requirement of
//! `host-migration-interoperability`, an operation that requires
//! a capability absent from the host profile MUST be rejected
//! before mutation with a remediation that names the missing
//! capability. This module is the application-layer surface over
//! the domain `HostCapability` value object.
//!
//! The port is `HostAdapter` (the runtime adapter that returns
//! the host's capability profile); the in-memory test double is
//! `MemoryHostAdapter`; the façade that asks the adapter whether
//! a capability is present and raises
//! `AgentError::CapabilityMismatch` when it is not is
//! `HostAdapterService`. The real adapter (added by the
//! `add-multi-host-agent-crypto` follow-on) talks to the
//! per-host agent over the existing mTLS channel.

use std::sync::Arc;

use openpanel_domain::AgentError;
use openpanel_domain::agent::HostCapability;

/// Runtime adapter that returns a host's capability profile.
///
/// The trait is intentionally minimal: the only primitive an
/// application-layer caller needs is `capability_profile`. The
/// `requires_capability` query is built on top of it in
/// `HostAdapterService` so all callers go through the same
/// remediation path.
#[async_trait::async_trait]
pub trait HostAdapter: Send + Sync {
    /// Return the host's current capability profile.
    async fn capability_profile(&self) -> Result<HostCapability, AgentError>;
}

/// In-memory test double for `HostAdapter`.
///
/// The memory adapter carries a single `HostCapability` and
/// returns it from every call. It is the conformance fixture
/// for the `Host Capability Contract` requirement and the test
/// surface for any port consumer that needs a deterministic
/// host profile.
#[derive(Clone)]
pub struct MemoryHostAdapter {
    capability: HostCapability,
}

impl MemoryHostAdapter {
    /// Build an adapter with the given capability profile.
    pub fn with_capability(capability: HostCapability) -> Self {
        Self { capability }
    }

    /// Build an adapter for a healthy `linux/x86_64` host with
    /// the given runtime adapters and panel version.
    pub fn healthy(runtime_adapters: Vec<String>, panel_version: impl Into<String>) -> Self {
        Self::with_capability(HostCapability::new(
            "linux",
            "x86_64",
            runtime_adapters,
            panel_version,
        ))
    }
}

#[async_trait::async_trait]
impl HostAdapter for MemoryHostAdapter {
    async fn capability_profile(&self) -> Result<HostCapability, AgentError> {
        Ok(self.capability.clone())
    }
}

/// Façade that asks the adapter whether a capability is present
/// and raises `AgentError::CapabilityMismatch` when it is not.
///
/// All application-layer callers MUST go through this service
/// (not directly through `HostAdapter::capability_profile`) so
/// the remediation path is uniform: a missing capability is
/// always rejected before any mutation with a string that names
/// the missing capability and tells the operator how to add it.
#[derive(Clone)]
pub struct HostAdapterService {
    adapter: Arc<dyn HostAdapter>,
}

impl HostAdapterService {
    /// Build a service over the given adapter.
    pub fn new(adapter: impl HostAdapter + 'static) -> Self {
        Self {
            adapter: Arc::new(adapter),
        }
    }

    /// Return the host's current capability profile.
    pub async fn capability_profile(&self) -> Result<HostCapability, AgentError> {
        self.adapter.capability_profile().await
    }

    /// Reject the operation before any mutation if the host
    /// profile does not declare the given capability.
    ///
    /// Returns `Ok(())` when the capability is present and
    /// `Err(AgentError::CapabilityMismatch { capability, remediation })`
    /// when it is not. The remediation string names the missing
    /// capability and points the operator at the `host_capability`
    /// record they need to amend.
    pub async fn requires_capability(&self, capability: &str) -> Result<(), AgentError> {
        let profile = self.adapter.capability_profile().await?;
        if profile.requires(capability) {
            Ok(())
        } else {
            let remediation = format!(
                "host declares adapters {:?}; add `{capability}` to the host's \
                 `host_capability.runtime_adapters` record and re-register the agent",
                profile.runtime_adapters(),
            );
            Err(AgentError::CapabilityMismatch {
                capability: capability.to_string(),
                remediation,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpanel_domain::agent::HostHealth;

    #[tokio::test]
    async fn memory_adapter_returns_constructed_profile() {
        let adapter = MemoryHostAdapter::healthy(
            vec!["docker".to_string(), "systemd".to_string()],
            "openpanel-1.2.3",
        );
        let profile = adapter.capability_profile().await.expect("profile");
        assert_eq!(profile.os(), "linux");
        assert_eq!(profile.arch(), "x86_64");
        assert!(profile.requires("docker"));
        assert!(!profile.requires("kubernetes"));
        assert_eq!(profile.health(), &HostHealth::Healthy);
    }

    #[tokio::test]
    async fn service_rejects_missing_capability_with_remediation() {
        let adapter = MemoryHostAdapter::healthy(vec!["docker".to_string()], "openpanel-1.2.3");
        let service = HostAdapterService::new(adapter);
        service
            .requires_capability("docker")
            .await
            .expect("docker is declared");
        let err = service
            .requires_capability("kubernetes")
            .await
            .expect_err("kubernetes is not declared");
        match err {
            AgentError::CapabilityMismatch {
                capability,
                remediation,
            } => {
                assert_eq!(capability, "kubernetes");
                assert!(remediation.contains("kubernetes"));
                assert!(remediation.contains("docker"));
            }
            other => panic!("expected CapabilityMismatch, got {other:?}"),
        }
    }
}
