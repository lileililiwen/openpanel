//! Personal API-token REST lifecycle.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use openpanel_app::{
    ApiTokenService,
    api_tokens::{CreateApiToken, TokenAuthError},
};
use openpanel_domain::ApiTokenMetadata;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extract::AuthUser,
};

/// Build routes nested at /api/v1/identity.
pub fn router(service: Arc<ApiTokenService>) -> Router {
    Router::new()
        .route("/tokens", get(list).post(create))
        .route("/tokens/{id}", get(get_one).delete(revoke))
        .route("/tokens/{id}/rotate", post(rotate))
        .with_state(service)
}

#[derive(Debug, Deserialize)]
struct CreateRequest {
    label: String,
    scopes: Vec<String>,
    expires_at: DateTime<Utc>,
    #[serde(default)]
    cidr_allowlist: Vec<String>,
    user_id: Option<Uuid>,
}

async fn create(
    State(service): State<Arc<ApiTokenService>>,
    AuthUser(user, _): AuthUser,
    Json(request): Json<CreateRequest>,
) -> ApiResult<(StatusCode, Json<openpanel_app::api_tokens::CreatedApiToken>)> {
    let user_id = request.user_id.unwrap_or_else(|| user.id());
    let created = service
        .create(
            &user,
            CreateApiToken {
                user_id,
                label: request.label,
                scopes: request.scopes,
                expires_at: request.expires_at,
                cidr_allowlist: request.cidr_allowlist,
            },
        )
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn list(
    State(service): State<Arc<ApiTokenService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<Vec<ApiTokenMetadata>>> {
    Ok(Json(
        service.list(&user, user.id()).await.map_err(map_error)?,
    ))
}

async fn get_one(
    State(service): State<Arc<ApiTokenService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ApiTokenMetadata>> {
    service
        .list(&user, user.id())
        .await
        .map_err(map_error)?
        .into_iter()
        .find(|token| token.id == id)
        .map(Json)
        .ok_or_else(|| ApiError::NotFound("API token".into()))
}

async fn revoke(
    State(service): State<Arc<ApiTokenService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    service
        .revoke(&user, user.id(), id)
        .await
        .map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn rotate(
    State(service): State<Arc<ApiTokenService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<openpanel_app::api_tokens::CreatedApiToken>> {
    Ok(Json(
        service
            .rotate(&user, user.id(), id)
            .await
            .map_err(map_error)?,
    ))
}

fn map_error(error: TokenAuthError) -> ApiError {
    match error {
        TokenAuthError::Unauthorized | TokenAuthError::Expired => ApiError::Unauthorized,
        TokenAuthError::ScopeRejected
        | TokenAuthError::CidrRejected
        | TokenAuthError::Forbidden => ApiError::Forbidden,
        TokenAuthError::RateLimited { .. } => ApiError::Internal("unexpected rate limit".into()),
        TokenAuthError::Invalid(error) => ApiError::BadRequest(error.to_string()),
        TokenAuthError::Internal(error) => ApiError::Internal(error),
    }
}
