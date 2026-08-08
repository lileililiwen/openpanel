//! Identity HTTP routes: login, logout, me, user CRUD.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header::SET_COOKIE},
    response::IntoResponse,
    routing::{delete, get, post},
};
use openpanel_app::IdentityService;
use openpanel_domain::IdentityError;
use uuid::Uuid;

use crate::{
    dto::{
        ChangePasswordRequest, ChangeRoleRequest, CreateUserRequest, LoginRequest, LoginResponse,
        UserDto,
    },
    error::{ApiError, ApiResult},
    extract::{AuthUser, RequireOwner},
    middleware::session::SESSION_COOKIE,
};

/// Builds the Axum sub-router for `/identity` routes (login, logout, me, user CRUD).
pub fn router(svc: Arc<IdentityService>) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", delete(delete_user).patch(change_role))
        .route("/users/{id}/disable", post(disable_user))
        .route("/users/{id}/password", post(change_password))
        .with_state(svc)
}

async fn login(
    State(svc): State<Arc<IdentityService>>,
    headers_in: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    let ip = client_ip(&headers_in);
    let ua = user_agent(&headers_in);
    let (user, token) = svc
        .login(&req.username_or_email, &req.password, ip, ua)
        .await?;
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);

    let cookie = format!(
        "{}={}; HttpOnly; Path=/; SameSite=Lax; Max-Age=86400",
        SESSION_COOKIE,
        token.expose()
    );

    let resp = LoginResponse {
        token: token.expose().to_string(),
        user: UserDto::from_user(&user),
        expires_at,
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| ApiError::Internal("bad cookie".into()))?,
    );
    Ok((headers, Json(resp)))
}

async fn logout(
    State(svc): State<Arc<IdentityService>>,
    AuthUser(user, _session): AuthUser,
    headers_in: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let _ip = client_ip(&headers_in);
    let _ua = user_agent(&headers_in);
    let token_str = headers_in
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers_in
                .get(axum::http::header::COOKIE)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| {
                    s.split(';').find_map(|p| {
                        let p = p.trim();
                        p.strip_prefix(&format!("{SESSION_COOKIE}="))
                            .map(|v| v.to_string())
                    })
                })
        })
        .ok_or(ApiError::Unauthorized)?;

    let token = openpanel_domain::SessionToken::from_string(token_str)
        .map_err(|_| ApiError::Unauthorized)?;
    svc.logout(&token, user.username().as_str()).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn me(AuthUser(user, _): AuthUser) -> Json<UserDto> {
    Json(UserDto::from_user(&user))
}

async fn list_users(
    State(svc): State<Arc<IdentityService>>,
    AuthUser(_user, _session): AuthUser,
) -> ApiResult<Json<Vec<UserDto>>> {
    let users = svc.list_users().await?;
    Ok(Json(users.iter().map(UserDto::from_user).collect()))
}

async fn create_user(
    State(svc): State<Arc<IdentityService>>,
    RequireOwner: RequireOwner,
    Json(req): Json<CreateUserRequest>,
) -> ApiResult<Json<UserDto>> {
    let actor = "owner"; // Could pull from request extensions
    let user = svc
        .create_user(&req.username, &req.email, &req.password, req.role, actor)
        .await
        .map_err(map_identity_err)?;
    Ok(Json(UserDto::from_user(&user)))
}

async fn change_role(
    State(svc): State<Arc<IdentityService>>,
    RequireOwner: RequireOwner,
    Path(id): Path<Uuid>,
    Json(req): Json<ChangeRoleRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = "owner";
    svc.change_role(id, req.role, actor).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn disable_user(
    State(svc): State<Arc<IdentityService>>,
    RequireOwner: RequireOwner,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = "owner";
    svc.disable_user(id, actor).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn delete_user(
    State(svc): State<Arc<IdentityService>>,
    RequireOwner: RequireOwner,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = "owner";
    svc.delete_user(id, actor).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn change_password(
    State(svc): State<Arc<IdentityService>>,
    AuthUser(_user, _session): AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = "self";
    svc.change_password(id, &req.new_password, actor).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

fn map_identity_err(e: IdentityError) -> ApiError {
    ApiError::Identity(e)
}

fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
}
