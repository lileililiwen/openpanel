//! Hosting plans bounded context: plan definitions, quota caps, and the
//! `HostingPlanId` reference used by the `User` aggregate.
//!
//! This module starts with a placeholder `HostingPlanId` newtype plus
//! a stub repository trait. The full plan domain (limits, subscription
//! lifecycle, plan resolution) ships in the follow-on `add-hosting-plans`
//! change. The placeholder exists so the `refine-identity-with-hierarchy-and-plan-fields`
//! change can wire the `user.hosting_plan_id` field without inventing
//! a temporary type that the follow-on change would have to replace.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Stable identifier for a hosting plan.
///
/// Plans are referenced by `User.hosting_plan_id` and resolve into
/// quota caps and allowed features. The full plan lifecycle
/// (creation, modification, deletion) is owned by the follow-on
/// `add-hosting-plans` change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostingPlanId(pub Uuid);

impl HostingPlanId {
    /// Construct a fresh random plan id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for HostingPlanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for HostingPlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Persistence operations for hosting plans.
///
/// The follow-on `add-hosting-plans` change provides the concrete
/// implementation. The placeholder trait ships with the
/// `refine-identity-with-hierarchy-and-plan-fields` change so the
/// `UserRepository` trait can reference it without depending on the
/// full plan domain.
#[async_trait]
pub trait HostingPlanRepository: Send + Sync + 'static {
    /// Return `true` when a plan with the given id exists.
    async fn exists(&self, _id: HostingPlanId) -> Result<bool, RepoError> {
        Ok(false)
    }
}
