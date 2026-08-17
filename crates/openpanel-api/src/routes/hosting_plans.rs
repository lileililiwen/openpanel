//! Hosting plans HTTP routes: CRUD, lifecycle, and assignment.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use openpanel_app::HostingPlansService;
use openpanel_domain::{
    PlanId,
    hosting_plans::{
        HostingPlan, HostingPlansError, PlanFeature, PlanFeatureState, PlanQuotas, PlanStatus,
    },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser, RequireOwner, dto::common::ErrorBody};

#[derive(Clone)]
struct HostingPlansRouteState {
    plans: Arc<HostingPlansService>,
}

/// Build the hosting-plans routes; the caller wires the service
/// into the composition root.
pub fn router(plans: Arc<HostingPlansService>) -> Router {
    Router::new()
        .route("/hosting-plans", get(list).post(create))
        .route(
            "/hosting-plans/{id}",
            get(detail).put(update).delete(remove),
        )
        .route("/hosting-plans/{id}/clone", post(clone))
        .route("/hosting-plans/{id}/assign", post(assign))
        .route("/hosting-plans/{id}/unassign", post(unassign))
        .with_state(HostingPlansRouteState { plans })
}

/// A hosting plan as returned to clients.
#[derive(Debug, Serialize)]
pub struct HostingPlanView {
    /// The plan id.
    pub id: PlanId,
    /// Display name of the plan.
    pub name: String,
    /// Free-form plan description.
    pub description: String,
    /// Whether the plan is active.
    pub status: PlanStatus,
    /// Resource quota caps imposed by the plan.
    pub quota_caps: PlanQuotas,
    /// App kinds the plan permits.
    pub allowed_apps: Vec<Uuid>,
    /// PHP runtime versions the plan permits.
    pub allowed_php_runtimes: Vec<String>,
}

impl HostingPlanView {
    fn from(plan: &HostingPlan) -> Self {
        Self {
            id: plan.id(),
            name: plan.name().to_string(),
            description: plan.description().to_string(),
            status: plan.status(),
            quota_caps: plan.quota_caps().clone(),
            allowed_apps: plan.allowed_apps().iter().map(|a| a.as_uuid()).collect(),
            allowed_php_runtimes: plan
                .allowed_php_runtimes()
                .iter()
                .map(|r| r.as_str().to_string())
                .collect(),
        }
    }
}

/// Body for creating a hosting plan.
#[derive(Debug, Deserialize)]
pub struct CreatePlanRequest {
    /// Display name of the plan.
    pub name: String,
    /// Free-form plan description.
    pub description: String,
    /// Resource quota caps imposed by the plan.
    pub quota_caps: PlanQuotas,
}

/// Body for updating a hosting plan; absent fields are unchanged.
#[derive(Debug, Deserialize)]
pub struct UpdatePlanRequest {
    /// New plan description.
    pub description: Option<String>,
    /// New quota caps.
    pub quota_caps: Option<PlanQuotas>,
    /// Feature toggles to apply.
    pub features: Option<std::collections::BTreeMap<PlanFeature, PlanFeatureState>>,
    /// New plan status.
    pub status: Option<PlanStatus>,
}

/// Body for cloning a hosting plan.
#[derive(Debug, Deserialize)]
pub struct ClonePlanRequest {
    /// Name for the cloned plan.
    pub name: String,
}

/// Body for assigning a plan to a user.
#[derive(Debug, Deserialize)]
pub struct AssignPlanRequest {
    /// The user to assign the plan to.
    pub user_id: Uuid,
}

async fn list(
    State(state): State<HostingPlansRouteState>,
    AuthUser(_user, _session): AuthUser,
) -> ApiResult<Json<Vec<HostingPlanView>>> {
    let plans = state.plans.list_plans().await.map_err(map_plan_err)?;
    Ok(Json(plans.iter().map(HostingPlanView::from).collect()))
}

async fn detail(
    State(state): State<HostingPlansRouteState>,
    AuthUser(_user, _session): AuthUser,
    Path(id): Path<PlanId>,
) -> ApiResult<Json<HostingPlanView>> {
    let plan = state.plans.find_plan(id).await.map_err(map_plan_err)?;
    Ok(Json(HostingPlanView::from(&plan)))
}

async fn create(
    State(state): State<HostingPlansRouteState>,
    RequireOwner: RequireOwner,
    Json(req): Json<CreatePlanRequest>,
) -> ApiResult<Json<HostingPlanView>> {
    let plan = state
        .plans
        .create_plan(&req.name, &req.description, req.quota_caps, "owner")
        .await
        .map_err(map_plan_err)?;
    Ok(Json(HostingPlanView::from(&plan)))
}

async fn update(
    State(state): State<HostingPlansRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<PlanId>,
    Json(req): Json<UpdatePlanRequest>,
) -> ApiResult<Json<HostingPlanView>> {
    let mut plan = state.plans.find_plan(id).await.map_err(map_plan_err)?;
    if let Some(description) = req.description {
        plan.set_description(description);
    }
    if let Some(caps) = req.quota_caps.clone() {
        plan.set_quota_caps(caps);
    }
    if let Some(features) = req.features.clone() {
        for (feature, state) in features {
            plan.set_feature(feature, state);
        }
    }
    if let Some(status) = req.status {
        match status {
            PlanStatus::Active => plan.enable(),
            PlanStatus::Disabled => plan.disable(),
        }
    }
    state
        .plans
        .update_plan(
            id,
            req.quota_caps.unwrap_or_else(|| plan.quota_caps().clone()),
            req.features.unwrap_or_default(),
            "owner",
        )
        .await
        .map_err(map_plan_err)?;
    Ok(Json(HostingPlanView::from(&plan)))
}

async fn remove(
    State(state): State<HostingPlansRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<PlanId>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .plans
        .delete_plan(id, "owner")
        .await
        .map_err(map_plan_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn clone(
    State(state): State<HostingPlansRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<PlanId>,
    Json(req): Json<ClonePlanRequest>,
) -> ApiResult<Json<HostingPlanView>> {
    let plan = state
        .plans
        .clone_plan(id, &req.name, "owner")
        .await
        .map_err(map_plan_err)?;
    Ok(Json(HostingPlanView::from(&plan)))
}

async fn assign(
    State(state): State<HostingPlansRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<PlanId>,
    Json(req): Json<AssignPlanRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = state.plans.resolve_actor("owner");
    let actor_id = Uuid::new_v4();
    let _ = state
        .plans
        .assign_plan(id, req.user_id, &actor, actor_id)
        .await
        .map_err(map_plan_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn unassign(
    State(state): State<HostingPlansRouteState>,
    RequireOwner: RequireOwner,
    Path(_id): Path<PlanId>,
    Json(req): Json<AssignPlanRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let _ = req;
    state
        .plans
        .unassign_plan(req.user_id, "owner")
        .await
        .map_err(map_plan_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

fn map_plan_err(e: HostingPlansError) -> ApiError {
    match e {
        HostingPlansError::PlanNotFound | HostingPlansError::UserNotFound => {
            ApiError::NotFound(e.to_string())
        }
        HostingPlansError::PlanInUse | HostingPlansError::DuplicateName => {
            ApiError::Conflict(e.to_string())
        }
        HostingPlansError::PlanDisabled => ApiError::Unprocessable(e.to_string()),
        HostingPlansError::InvalidName(_)
        | HostingPlansError::InvalidPrice(_)
        | HostingPlansError::InvalidPhpRuntime(_) => ApiError::Unprocessable(e.to_string()),
        HostingPlansError::WouldShrinkBelowUsage => ApiError::Unprocessable(e.to_string()),
        HostingPlansError::Persistence(_) => ApiError::Internal(e.to_string()),
    }
}

#[allow(dead_code)]
fn _body() -> ErrorBody {
    ErrorBody::new("plans")
}
