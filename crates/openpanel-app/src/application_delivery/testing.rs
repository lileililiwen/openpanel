//! Test doubles for the application-delivery ports.
//!
//! Kept beside the port they implement (and separate from
//! [`crate::docker::testing`], which doubles the docker engine port)
//! so every suite that needs a delivery double — the app unit tests,
//! the integration suite, the CLI suite — shares one definition.

// Test-double setup may fail fast: an unbuildable double is a
// broken test environment, not a production condition. This mirrors
// the crate-level allow in `openpanel-test-support`.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use openpanel_core::audit::{AuditService, NoopAuditService};
use openpanel_domain::ApplicationDeliveryError;
use std::sync::Arc;
use uuid::Uuid;

use super::docker_runtime::{ConvergeTarget, PlanTargetResolver};

/// Shared no-op audit sink so the delivery doubles and the provider
/// harness do not each carry their own.
pub fn noop_audit_sink() -> Arc<dyn AuditService> {
    Arc::new(NoopAuditService)
}

/// Plan-target resolver that answers with one fixed target, or with
/// nothing when composed empty. It exists so a deployment-adapter
/// suite can prove plan behavior without a delivery graph.
pub struct FixedTargetResolver {
    target: Option<ConvergeTarget>,
    refuse: bool,
}

impl FixedTargetResolver {
    /// Resolver that always answers with `target`.
    pub fn with_target(target: ConvergeTarget) -> Self {
        Self {
            target: Some(target),
            refuse: false,
        }
    }

    /// Resolver that never resolves a target.
    pub fn new() -> Self {
        Self {
            target: None,
            refuse: false,
        }
    }

    /// Resolver that fails with a typed delivery error.
    pub fn refusing() -> Self {
        Self {
            target: None,
            refuse: true,
        }
    }
}

impl Default for FixedTargetResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanTargetResolver for FixedTargetResolver {
    fn resolve(
        &self,
        _plan_target: &str,
        _release_digest: &str,
    ) -> Result<Option<ConvergeTarget>, ApplicationDeliveryError> {
        if self.refuse {
            return Err(ApplicationDeliveryError::RuntimeUnavailable);
        }
        Ok(self.target.clone())
    }
}

/// A converge target for `application_name` / `environment_name`
/// backed by fresh ids, so a suite never has to invent uuids.
pub fn target_for(application_name: &str, environment_name: &str, digest: &str) -> ConvergeTarget {
    ConvergeTarget::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        application_name,
        environment_name,
        Uuid::new_v4(),
        digest,
    )
    .expect("static")
}
