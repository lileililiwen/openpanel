//! In-memory `HostingPlanRepository` used by tests that exercise
//! the hosting-plans service without spinning up SQLite.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    PlanId, RepoError,
    hosting_plans::{HostingPlan, HostingPlanRepository, HostingPlansError, PlanAssignment},
};

/// In-memory hosting plan and assignment store.
#[derive(Default, Clone)]
pub struct MemoryHostingPlanRepository {
    inner: Arc<Mutex<MemoryState>>,
}

#[derive(Default)]
struct MemoryState {
    plans: HashMap<uuid::Uuid, HostingPlan>,
    /// Current assignment per user, indexed by user_id.
    current: HashMap<uuid::Uuid, PlanAssignment>,
    /// Append-only history.
    #[allow(dead_code)]
    history: Vec<PlanAssignment>,
}

impl MemoryHostingPlanRepository {
    /// Empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl HostingPlanRepository for MemoryHostingPlanRepository {
    async fn insert(&self, plan: &HostingPlan) -> Result<(), HostingPlansError> {
        let mut state = self.inner.lock().expect("memory poisoned");
        if state.plans.values().any(|p| p.name() == plan.name()) {
            return Err(HostingPlansError::DuplicateName);
        }
        state.plans.insert(plan.id().as_uuid(), plan.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: PlanId) -> Result<Option<HostingPlan>, HostingPlansError> {
        let state = self.inner.lock().expect("memory poisoned");
        Ok(state.plans.get(&id.as_uuid()).cloned())
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<HostingPlan>, HostingPlansError> {
        let state = self.inner.lock().expect("memory poisoned");
        Ok(state.plans.values().find(|p| p.name() == name).cloned())
    }

    async fn list(&self) -> Result<Vec<HostingPlan>, HostingPlansError> {
        let state = self.inner.lock().expect("memory poisoned");
        let mut plans: Vec<_> = state.plans.values().cloned().collect();
        plans.sort_by_key(|p| p.created_at());
        Ok(plans)
    }

    async fn update(&self, plan: &HostingPlan) -> Result<(), HostingPlansError> {
        let mut state = self.inner.lock().expect("memory poisoned");
        if let Some(existing) = state.plans.get(&plan.id().as_uuid())
            && existing.name() != plan.name()
            && state.plans.values().any(|p| p.name() == plan.name())
        {
            return Err(HostingPlansError::DuplicateName);
        }
        state.plans.insert(plan.id().as_uuid(), plan.clone());
        Ok(())
    }

    async fn delete(&self, id: PlanId) -> Result<(), HostingPlansError> {
        let mut state = self.inner.lock().expect("memory poisoned");
        let in_use = state.current.values().any(|a| a.plan_id() == id);
        if in_use {
            return Err(HostingPlansError::PlanInUse);
        }
        state.plans.remove(&id.as_uuid());
        Ok(())
    }

    async fn insert_assignment(
        &self,
        assignment: &PlanAssignment,
    ) -> Result<(), HostingPlansError> {
        let mut state = self.inner.lock().expect("memory poisoned");
        state
            .current
            .insert(assignment.user_id(), assignment.clone());
        state.history.push(assignment.clone());
        Ok(())
    }

    async fn replace_current_assignment(
        &self,
        user_id: uuid::Uuid,
        _now: DateTime<Utc>,
    ) -> Result<Option<PlanId>, HostingPlansError> {
        let mut state = self.inner.lock().expect("memory poisoned");
        Ok(state.current.remove(&user_id).map(|a| a.plan_id()))
    }

    async fn remove_current_assignment(
        &self,
        user_id: uuid::Uuid,
        _now: DateTime<Utc>,
    ) -> Result<Option<PlanId>, HostingPlansError> {
        let mut state = self.inner.lock().expect("memory poisoned");
        Ok(state.current.remove(&user_id).map(|a| a.plan_id()))
    }

    async fn current_assignment(
        &self,
        user_id: uuid::Uuid,
    ) -> Result<Option<PlanAssignment>, HostingPlansError> {
        let state = self.inner.lock().expect("memory poisoned");
        Ok(state.current.get(&user_id).cloned())
    }

    async fn count_assignments(&self, plan_id: PlanId) -> Result<i64, HostingPlansError> {
        let state = self.inner.lock().expect("memory poisoned");
        Ok(state
            .current
            .values()
            .filter(|a| a.plan_id() == plan_id)
            .count() as i64)
    }

    async fn exists(&self, id: PlanId) -> Result<bool, RepoError> {
        let state = self.inner.lock().expect("memory poisoned");
        Ok(state.plans.contains_key(&id.as_uuid()))
    }
}
