//! Account hierarchy HTTP routes: tree traversal, child-account
//! creation, detach, and pool CRUD.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use openpanel_app::{HierarchyService, account_hierarchy::CreateChildRequest};
use openpanel_domain::{PoolAxis, Role};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser, RequireOwner};

#[derive(Clone)]
struct HierarchyRouteState {
    service: Arc<HierarchyService>,
}

/// Build the account-hierarchy routes; the caller wires the
/// service into the composition root.
pub fn router(service: Arc<HierarchyService>) -> Router {
    Router::new()
        .route(
            "/users/{id}/children",
            get(list_children).post(create_child),
        )
        .route("/users/{id}/tree", get(tree))
        .route("/users/{id}/quota-pool", get(pools).post(set_pool))
        .route("/users/{id}/quota-pool/usage", get(pool_usage))
        .route("/users/{id}/quota-pool/claim", post(claim))
        .route("/users/{id}/quota-pool/release", post(release))
        .with_state(HierarchyRouteState { service })
}

/// A child account row in the children list.
#[derive(Debug, Serialize)]
pub struct ChildView {
    /// The child account id.
    pub id: Uuid,
    /// The child's login username.
    pub username: String,
    /// The child's contact email.
    pub email: String,
    /// The child's role in the hierarchy.
    pub role: Role,
}

impl ChildView {
    fn from_user(user: &openpanel_domain::User) -> Self {
        Self {
            id: user.id(),
            username: user.username().as_str().to_string(),
            email: user.email().as_str().to_string(),
            role: user.role(),
        }
    }
}

/// A node in the hierarchy tree.
#[derive(Debug, Serialize)]
pub struct TreeNode {
    /// The account id at this node.
    pub id: Uuid,
    /// Depth below the queried root.
    pub depth: u32,
    /// Child nodes one level below this one.
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    fn from(node: &openpanel_domain::HierarchyNode) -> Self {
        Self {
            id: node.user_id(),
            depth: node.depth(),
            children: node.children().iter().map(Self::from).collect(),
        }
    }
}

/// Body for creating a child account.
#[derive(Debug, Deserialize)]
pub struct CreateChildBody {
    /// Login username for the child.
    pub username: String,
    /// Contact email for the child.
    pub email: String,
    /// Initial password for the child.
    pub password: String,
    /// Role granted to the child.
    pub role: Role,
    /// Optional hosting plan to assign on creation.
    #[serde(default)]
    pub initial_plan_id: Option<openpanel_domain::HostingPlanId>,
}

/// Body for setting an account's quota pool.
#[derive(Debug, Deserialize)]
pub struct SetPoolBody {
    /// The pool axis being configured.
    pub axis: PoolAxis,
    /// Total bytes allocated to the pool.
    pub total_bytes: u64,
}

/// Body for claiming pool capacity for a child.
#[derive(Debug, Deserialize)]
pub struct ClaimBody {
    /// The child account claiming capacity.
    pub child_id: Uuid,
    /// The axis the claim applies to.
    pub axis: PoolAxis,
    /// Bytes claimed from the pool.
    pub share_bytes: u64,
}

/// Body for releasing a pool claim.
#[derive(Debug, Deserialize)]
pub struct ReleaseBody {
    /// The child account releasing capacity.
    pub child_id: Uuid,
    /// The axis the release applies to.
    pub axis: PoolAxis,
}

/// A pool usage snapshot.
#[derive(Debug, Serialize)]
pub struct PoolUsageView {
    /// The pool axis.
    pub axis: PoolAxis,
    /// Total bytes in the pool.
    pub total_bytes: u64,
    /// Bytes currently claimed/used.
    pub used_bytes: u64,
}

async fn list_children(
    State(state): State<HierarchyRouteState>,
    AuthUser(user, _session): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ChildView>>> {
    let _ = user;
    let children = state.service.children(id).await.map_err(map_err)?;
    Ok(Json(children.iter().map(ChildView::from_user).collect()))
}

async fn create_child(
    State(state): State<HierarchyRouteState>,
    AuthUser(_user, session): AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateChildBody>,
) -> ApiResult<Json<ChildView>> {
    let actor = session_label(&session);
    let actor_id = session.user_id();
    let user = state
        .service
        .create_child(
            id,
            CreateChildRequest {
                username: req.username,
                email: req.email,
                password: req.password,
                role: req.role,
                initial_plan_id: req.initial_plan_id,
            },
            &actor,
            actor_id,
        )
        .await
        .map_err(map_err)?;
    Ok(Json(ChildView::from_user(&user)))
}

async fn tree(
    State(state): State<HierarchyRouteState>,
    AuthUser(_user, _session): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<TreeNode>> {
    let node = state.service.tree(id).await.map_err(map_err)?;
    Ok(Json(TreeNode::from(&node)))
}

async fn pools(
    State(state): State<HierarchyRouteState>,
    AuthUser(_user, _session): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<PoolUsageView>>> {
    let usage = state.service.pool_usage(id).await.map_err(map_err)?;
    Ok(Json(
        usage
            .into_iter()
            .map(|u| PoolUsageView {
                axis: u.axis,
                total_bytes: u.total_bytes,
                used_bytes: u.used_bytes,
            })
            .collect(),
    ))
}

async fn pool_usage(
    State(state): State<HierarchyRouteState>,
    AuthUser(_user, _session): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<PoolUsageView>>> {
    pools(State(state), AuthUser(_user, _session), Path(id)).await
}

async fn set_pool(
    State(state): State<HierarchyRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<Uuid>,
    Json(req): Json<SetPoolBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let _ = state
        .service
        .set_pool(id, req.axis, req.total_bytes, "owner")
        .await
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn claim(
    State(state): State<HierarchyRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<Uuid>,
    Json(req): Json<ClaimBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let _ = state
        .service
        .claim_pool(id, req.child_id, req.axis, req.share_bytes, "owner")
        .await
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn release(
    State(state): State<HierarchyRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<Uuid>,
    Json(req): Json<ReleaseBody>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .service
        .release_claim(id, req.child_id, req.axis, "owner")
        .await
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

fn map_err(e: openpanel_domain::AccountHierarchyError) -> ApiError {
    match e {
        openpanel_domain::AccountHierarchyError::Cycle => ApiError::Unprocessable(e.to_string()),
        openpanel_domain::AccountHierarchyError::AlreadyAttached => {
            ApiError::Conflict(e.to_string())
        }
        openpanel_domain::AccountHierarchyError::UserNotFound => ApiError::NotFound(e.to_string()),
        openpanel_domain::AccountHierarchyError::InvalidPoolTotal
        | openpanel_domain::AccountHierarchyError::InvalidClaim => {
            ApiError::Unprocessable(e.to_string())
        }
        openpanel_domain::AccountHierarchyError::PoolExhausted => ApiError::Conflict(e.to_string()),
        openpanel_domain::AccountHierarchyError::PoolNotFound => ApiError::NotFound(e.to_string()),
        openpanel_domain::AccountHierarchyError::Forbidden => ApiError::Forbidden,
        openpanel_domain::AccountHierarchyError::Persistence(_) => {
            ApiError::Internal(e.to_string())
        }
    }
}

fn session_label(session: &openpanel_domain::Session) -> String {
    session.role().to_string()
}
