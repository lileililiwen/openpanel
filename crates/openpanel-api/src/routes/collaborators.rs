//! Per-site collaborator HTTP routes.
//!
//! Four endpoints, all nested under `/sites/{id}/collaborators`:
//!
//! * `POST   /sites/{id}/collaborators`        invite
//! * `GET    /sites/{id}/collaborators/{uid}`  detail
//! * `PUT    /sites/{id}/collaborators/{uid}`  update permissions
//! * `DELETE /sites/{id}/collaborators/{uid}`  revoke from site

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use openpanel_app::collaborators::{
    CollaboratorService, GrantResolver, InviteCollaboratorError, InviteRequest, UpdateRequest,
};
use openpanel_domain::Permission;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Build the `/sites/{id}/collaborators` routes.
pub fn router(
    service: Arc<CollaboratorService>,
    resolver: Arc<GrantResolver>,
) -> Router {
    Router::new()
        .route(
            "/{id}/collaborators",
            post(invite).get(list_for_site),
        )
        .route(
            "/{id}/collaborators/{uid}",
            get(detail)
                .put(update)
                .delete(revoke),
        )
        .route(
            "/{id}/collaborators/{uid}/resolve",
            get(resolve_for_user),
        )
        .with_state((service, resolver))
}

#[derive(Debug, Deserialize)]
struct InviteBody {
    email: String,
    scopes: Vec<String>,
}

async fn invite(
    State((svc, _resolver)): State<(Arc<CollaboratorService>, Arc<GrantResolver>)>,
    AuthUser(user, _session): AuthUser,
    Path(site_id): Path<Uuid>,
    Json(body): Json<InviteBody>,
) -> ApiResult<(StatusCode, Json<CollaboratorView>)> {
    let permissions = parse_scopes(&body.scopes).map_err(ApiError::BadRequest)?;
    let account_id = user.id();
    let request = InviteRequest {
        email: body.email.clone(),
        site_id,
        permissions,
    };
    let collaborator = svc
        .invite(account_id, request, user.username().as_str())
        .await
        .map_err(map_invite_error)?;
    Ok((StatusCode::CREATED, Json(CollaboratorView::from(&collaborator))))
}

async fn list_for_site(
    State((svc, _resolver)): State<(Arc<CollaboratorService>, Arc<GrantResolver>)>,
    AuthUser(_user, _session): AuthUser,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Vec<SiteGrantView>>> {
    let grants = svc
        .grants_for_site(site_id)
        .await
        .map_err(map_invite_error)?;
    Ok(Json(grants.iter().map(SiteGrantView::from).collect()))
}

async fn detail(
    State((svc, _resolver)): State<(Arc<CollaboratorService>, Arc<GrantResolver>)>,
    AuthUser(_user, _session): AuthUser,
    Path((_site_id, uid)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<CollaboratorView>> {
    let id = parse_collaborator_id(&uid.to_string())?;
    let collaborator = svc
        .find(&id)
        .await
        .map_err(map_invite_error)?
        .ok_or_else(|| ApiError::NotFound(format!("collaborator {uid}")))?;
    Ok(Json(CollaboratorView::from(&collaborator)))
}

#[derive(Debug, Deserialize)]
struct UpdateBody {
    scopes: Vec<String>,
}

async fn update(
    State((svc, _resolver)): State<(Arc<CollaboratorService>, Arc<GrantResolver>)>,
    AuthUser(user, _session): AuthUser,
    Path((site_id, uid)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateBody>,
) -> ApiResult<Json<SiteGrantView>> {
    let id = parse_collaborator_id(&uid.to_string())?;
    let permissions = parse_scopes(&body.scopes).map_err(ApiError::BadRequest)?;
    let grant = svc
        .update_permissions(
            &id,
            site_id,
            UpdateRequest { permissions },
            user.username().as_str(),
        )
        .await
        .map_err(map_invite_error)?;
    Ok(Json(SiteGrantView::from(&grant)))
}

async fn revoke(
    State((svc, _resolver)): State<(Arc<CollaboratorService>, Arc<GrantResolver>)>,
    AuthUser(user, _session): AuthUser,
    Path((site_id, uid)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let id = parse_collaborator_id(&uid.to_string())?;
    svc.revoke(&id, site_id, user.username().as_str())
        .await
        .map_err(map_invite_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
struct ResolveView {
    scopes: Vec<&'static str>,
}

async fn resolve_for_user(
    State((_svc, resolver)): State<(Arc<CollaboratorService>, Arc<GrantResolver>)>,
    AuthUser(_user, _session): AuthUser,
    Path((site_id, uid)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<ResolveView>> {
    let id = parse_collaborator_id(&uid.to_string())?;
    let set = resolver.resolve(&id, site_id).await.map_err(map_invite_error)?;
    let scopes: Vec<&'static str> = set.iter().map(|p| p.as_str()).collect();
    Ok(Json(ResolveView { scopes }))
}

// ---- helpers ----

fn parse_scopes(scopes: &[String]) -> Result<openpanel_domain::PermissionSet, String> {
    let mut set = openpanel_domain::PermissionSet::EMPTY;
    for s in scopes {
        let perm = Permission::parse(s).ok_or_else(|| format!("unknown scope `{s}`"))?;
        set.insert(perm);
    }
    if set.is_empty() {
        return Err("permission set must not be empty".into());
    }
    Ok(set)
}

fn parse_collaborator_id(s: &str) -> ApiResult<openpanel_domain::CollaboratorId> {
    use std::str::FromStr;
    openpanel_domain::CollaboratorId::from_str(s)
        .map_err(|e| ApiError::BadRequest(format!("invalid collaborator id: {e}")))
}

fn map_invite_error(e: InviteCollaboratorError) -> ApiError {
    match e {
        InviteCollaboratorError::Domain(d) => ApiError::BadRequest(d.to_string()),
        InviteCollaboratorError::Persistence(msg) => ApiError::Internal(msg),
    }
}

// ---- views ----

#[derive(Debug, Serialize)]
struct CollaboratorView {
    collaborator_id: String,
    account_id: String,
    email: String,
    status: String,
    invited_at: String,
    accepted_at: Option<String>,
    revoked_at: Option<String>,
}

impl From<&openpanel_domain::Collaborator> for CollaboratorView {
    fn from(c: &openpanel_domain::Collaborator) -> Self {
        Self {
            collaborator_id: c.collaborator_id.to_string(),
            account_id: c.account_id.to_string(),
            email: c.email.as_str().to_string(),
            status: c.status.as_str().to_string(),
            invited_at: c.invited_at.to_rfc3339(),
            accepted_at: c.accepted_at.map(|d| d.to_rfc3339()),
            revoked_at: c.revoked_at.map(|d| d.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize)]
struct SiteGrantView {
    collaborator_id: String,
    site_id: String,
    scopes: Vec<&'static str>,
    granted_at: String,
}

impl From<&openpanel_domain::SiteGrant> for SiteGrantView {
    fn from(g: &openpanel_domain::SiteGrant) -> Self {
        Self {
            collaborator_id: g.collaborator_id.to_string(),
            site_id: g.site_id.to_string(),
            scopes: g.permissions.iter().map(|p| p.as_str()).collect(),
            granted_at: g.granted_at.to_rfc3339(),
        }
    }
}