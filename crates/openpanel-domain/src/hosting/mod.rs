//! Hosting plans bounded context: plan definitions, quota caps, and the
//! `HostingPlanId` reference used by the `User` aggregate.
//!
//! The full hosting-plans domain (aggregate, validator, resolver,
//! assignment table) lives in [`crate::hosting_plans`]. This module
//! is kept only to host the `HostingPlanId` newtype that the user
//! aggregate references and the placeholder `HostingPlanRepository`
//! that the placeholder identity `find_by_plan` lookup calls.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identifier for a hosting plan.
///
/// Plans are referenced by `User.hosting_plan_id` and resolve into
/// quota caps and allowed features. The full plan lifecycle
/// (creation, modification, deletion) is owned by the
/// `hosting_plans` module.
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
