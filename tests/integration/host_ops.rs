//! Host-operations and migration interoperability integration tests.
//!
//! Scenarios covered: Capability mismatch, Cutover rejected.
//!
//! This module owns the end-to-end conformance fixture for the
//! `Host Capability Contract` requirement of
//! `host-migration-interoperability`: the in-memory
//! `MemoryHostAdapter` is wired into the `HostAdapterService`,
//! the service is asked for a present and a missing capability,
//! and the missing-capability path MUST be rejected with
//! `AgentError::CapabilityMismatch` and a remediation string
//! that names the missing capability.
//!
//! Capability under test: `host-migration-interoperability`
//! (`crates/openpanel-domain::agent` + `openpanel-app::agent`).
//!
//! In the red phase the integration test is EXPECTED to fail to
//! compile because `HostAdapterService`, `MemoryHostAdapter`,
//! `HostCapability`, and the `CapabilityMismatch` error variant
//! do not exist yet. The green phase lands them in the agent
//! module.

#![allow(clippy::expect_used)]

use openpanel_app::{
    agent::{HostAdapterService, MemoryHostAdapter},
    host_ops::{CutoverService, DnsCutover, MailCutover, SslCutover},
};
use openpanel_domain::{
    AgentError,
    agent::{HostCapability, HostHealth},
};

#[tokio::test]
async fn capability_mismatch_blocks_operation_with_remediation() {
    let adapter = MemoryHostAdapter::with_capability(
        HostCapability::new(
            "linux",
            "x86_64",
            vec!["docker".to_string(), "systemd".to_string()],
            "openpanel-1.2.3",
        )
        .with_health(HostHealth::Healthy),
    );
    let service = HostAdapterService::new(adapter);

    // Present capability: the service allows the operation.
    service
        .requires_capability("docker")
        .await
        .expect("docker is declared on the host profile");

    // Missing capability: the service rejects BEFORE any
    // mutation with a remediation that names the missing cap.
    let err = service
        .requires_capability("kubernetes")
        .await
        .expect_err("kubernetes is not declared on the host profile");
    match err {
        AgentError::CapabilityMismatch {
            capability,
            remediation,
        } => {
            assert_eq!(capability, "kubernetes");
            assert!(
                remediation.contains("kubernetes"),
                "remediation must name the missing capability: {remediation}",
            );
        }
        other => panic!("expected CapabilityMismatch, got {other:?}"),
    }
}

struct PassCutover;
struct FailCutover;

#[async_trait::async_trait]
impl DnsCutover for PassCutover {
    async fn preflight(&self) -> Result<bool, AgentError> {
        Ok(true)
    }
    async fn commit(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        Ok(openpanel_app::host_ops::CutoverEvidence {
            cutover_id: uuid::Uuid::new_v4(),
            kind: "dns".to_string(),
            preflight_passed: true,
            rollback_ttl: chrono::Duration::hours(24),
            diagnostic: "dns committed".to_string(),
        })
    }
    async fn rollback(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        Ok(openpanel_app::host_ops::CutoverEvidence {
            cutover_id: uuid::Uuid::new_v4(),
            kind: "dns".to_string(),
            preflight_passed: true,
            rollback_ttl: chrono::Duration::hours(24),
            diagnostic: "dns rolled back".to_string(),
        })
    }
}
#[async_trait::async_trait]
impl MailCutover for PassCutover {
    async fn preflight(&self) -> Result<bool, AgentError> {
        Ok(true)
    }
    async fn commit(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        Ok(openpanel_app::host_ops::CutoverEvidence {
            cutover_id: uuid::Uuid::new_v4(),
            kind: "mail".to_string(),
            preflight_passed: true,
            rollback_ttl: chrono::Duration::hours(24),
            diagnostic: "mail committed".to_string(),
        })
    }
    async fn rollback(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        Ok(openpanel_app::host_ops::CutoverEvidence {
            cutover_id: uuid::Uuid::new_v4(),
            kind: "mail".to_string(),
            preflight_passed: true,
            rollback_ttl: chrono::Duration::hours(24),
            diagnostic: "mail rolled back".to_string(),
        })
    }
}
#[async_trait::async_trait]
impl SslCutover for PassCutover {
    async fn preflight(&self) -> Result<bool, AgentError> {
        Ok(true)
    }
    async fn commit(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        Ok(openpanel_app::host_ops::CutoverEvidence {
            cutover_id: uuid::Uuid::new_v4(),
            kind: "ssl".to_string(),
            preflight_passed: true,
            rollback_ttl: chrono::Duration::hours(24),
            diagnostic: "ssl committed".to_string(),
        })
    }
    async fn rollback(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        Ok(openpanel_app::host_ops::CutoverEvidence {
            cutover_id: uuid::Uuid::new_v4(),
            kind: "ssl".to_string(),
            preflight_passed: true,
            rollback_ttl: chrono::Duration::hours(24),
            diagnostic: "ssl rolled back".to_string(),
        })
    }
}
#[async_trait::async_trait]
impl DnsCutover for FailCutover {
    async fn preflight(&self) -> Result<bool, AgentError> {
        Ok(false)
    }
    async fn commit(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        unreachable!()
    }
    async fn rollback(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        unreachable!()
    }
}
#[async_trait::async_trait]
impl MailCutover for FailCutover {
    async fn preflight(&self) -> Result<bool, AgentError> {
        Ok(false)
    }
    async fn commit(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        unreachable!()
    }
    async fn rollback(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        unreachable!()
    }
}
#[async_trait::async_trait]
impl SslCutover for FailCutover {
    async fn preflight(&self) -> Result<bool, AgentError> {
        Ok(false)
    }
    async fn commit(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        unreachable!()
    }
    async fn rollback(&self) -> Result<openpanel_app::host_ops::CutoverEvidence, AgentError> {
        unreachable!()
    }
}

#[tokio::test]
async fn cutover_refused_when_preflight_fails() {
    let svc = CutoverService::new(FailCutover, PassCutover, PassCutover);
    let err = svc
        .dns_commit()
        .await
        .expect_err("Cutover rejected: dns preflight failed");
    assert!(matches!(err, AgentError::InvalidManifest(_)));
}

#[tokio::test]
async fn cutover_rollback_within_ttl() {
    let svc = CutoverService::new(PassCutover, PassCutover, PassCutover);
    assert!(svc.mail_rollback(chrono::Utc::now()).await.is_ok());
}
