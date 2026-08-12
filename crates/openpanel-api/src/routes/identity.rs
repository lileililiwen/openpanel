//! Identity HTTP routes: login, logout, me, user CRUD.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, header::SET_COOKIE},
    response::IntoResponse,
    routing::{delete, get, post},
};
use openpanel_app::{
    IdentityService,
    identity::{LoginOutcome, TwoFactorService},
    security::LoginThrottleService,
};
use openpanel_domain::IdentityError;
use uuid::Uuid;

use crate::{
    dto::{
        ChangePasswordRequest, ChangeRoleRequest, CreateUserRequest, EnrollTotpResponse, FactorDto,
        FactorListResponse, LoginFactorRequest, LoginFactorRequired, LoginRequest, LoginResponse,
        RegenerateRecoveryResponse, UserDto,
    },
    error::{ApiError, ApiResult},
    extract::{AuthUser, RequireOwner},
    middleware::session::SESSION_COOKIE,
};

/// Builds the Axum sub-router for `/identity` routes (login, logout, me, user CRUD).
#[derive(Clone)]
struct IdentityRouteState {
    identity: Arc<IdentityService>,
    two_factor: Arc<TwoFactorService>,
    throttle: Arc<LoginThrottleService>,
}

/// Build identity routes with durable pre-authentication throttling.
pub fn router(
    svc: Arc<IdentityService>,
    two_factor: Arc<TwoFactorService>,
    throttle: Arc<LoginThrottleService>,
) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/login/factor", post(login_factor))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", delete(delete_user).patch(change_role))
        .route("/users/{id}/disable", post(disable_user))
        .route("/users/{id}/password", post(change_password))
        .route("/factors", get(list_factors))
        .route("/factors/totp/enroll", post(enroll_totp))
        .route("/factors/recovery/regenerate", post(regenerate_recovery))
        .route("/factors/{id}", delete(revoke_factor))
        .with_state(IdentityRouteState {
            identity: svc,
            two_factor,
            throttle,
        })
}

async fn login(
    State(state): State<IdentityRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers_in: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    let peer = peer.ip();
    let forwarded = forwarded_ip(&headers_in);
    if let Some(decision) = state
        .throttle
        .check(&req.username_or_email, peer, forwarded)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
    {
        return Err(rate_limited(decision.retry_after_seconds));
    }
    let ip = Some(peer.to_string());
    let ua = user_agent(&headers_in);
    let outcome = state
        .identity
        .login(&req.username_or_email, &req.password, ip, ua)
        .await;
    match outcome {
        Ok(LoginOutcome::Authenticated { user, token }) => {
            state
                .throttle
                .record_success(&req.username_or_email)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
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
                HeaderValue::from_str(&cookie)
                    .map_err(|_| ApiError::Internal("bad cookie".into()))?,
            );
            Ok((headers, Json(resp)).into_response())
        }
        Ok(LoginOutcome::FactorRequired { user: _, challenge }) => {
            state
                .throttle
                .record_success(&req.username_or_email)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
            let body = LoginFactorRequired {
                status: "factor_required",
                challenge_id: challenge.challenge_id,
                expires_at: challenge.expires_at,
            };
            Ok(Json(body).into_response())
        }
        Err(error) => {
            let decision = state
                .throttle
                .record_failure(&req.username_or_email, peer, forwarded)
                .await
                .map_err(|failure| ApiError::Internal(failure.to_string()))?;
            if decision.retry_after_seconds.is_some() {
                return Err(rate_limited(decision.retry_after_seconds));
            }
            Err(ApiError::Identity(error))
        }
    }
}

async fn login_factor(
    State(state): State<IdentityRouteState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers_in: HeaderMap,
    Json(req): Json<LoginFactorRequest>,
) -> ApiResult<impl IntoResponse> {
    let ip = Some(peer.ip().to_string());
    let ua = user_agent(&headers_in);
    let response = match req.kind.as_str() {
        "totp" => openpanel_app::identity::two_factor::FactorResponse::Totp(req.code),
        "recovery" => openpanel_app::identity::two_factor::FactorResponse::Recovery(req.code),
        _ => return Err(ApiError::Unprocessable("unknown factor kind".into())),
    };
    let (user, token) = state
        .identity
        .verify_login_factor(req.challenge_id, &req.challenge_token, response, ip, ua)
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
    State(state): State<IdentityRouteState>,
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
    state
        .identity
        .logout(&token, user.username().as_str())
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn me(AuthUser(user, _): AuthUser) -> Json<UserDto> {
    Json(UserDto::from_user(&user))
}

async fn list_users(
    State(state): State<IdentityRouteState>,
    AuthUser(_user, _session): AuthUser,
) -> ApiResult<Json<Vec<UserDto>>> {
    let users = state.identity.list_users().await?;
    Ok(Json(users.iter().map(UserDto::from_user).collect()))
}

async fn create_user(
    State(state): State<IdentityRouteState>,
    RequireOwner: RequireOwner,
    Json(req): Json<CreateUserRequest>,
) -> ApiResult<Json<UserDto>> {
    let actor = "owner"; // Could pull from request extensions
    let user = state
        .identity
        .create_user(&req.username, &req.email, &req.password, req.role, actor)
        .await
        .map_err(map_identity_err)?;
    Ok(Json(UserDto::from_user(&user)))
}

async fn change_role(
    State(state): State<IdentityRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<Uuid>,
    Json(req): Json<ChangeRoleRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = "owner";
    state.identity.change_role(id, req.role, actor).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn disable_user(
    State(state): State<IdentityRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = "owner";
    state.identity.disable_user(id, actor).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn delete_user(
    State(state): State<IdentityRouteState>,
    RequireOwner: RequireOwner,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = "owner";
    state.identity.delete_user(id, actor).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn change_password(
    State(state): State<IdentityRouteState>,
    AuthUser(_user, _session): AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = "self";
    state
        .identity
        .change_password(id, &req.new_password, actor)
        .await?;
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

fn forwarded_ip(headers: &HeaderMap) -> Option<IpAddr> {
    client_ip(headers).and_then(|value| value.parse().ok())
}

fn rate_limited(retry_after: Option<u64>) -> ApiError {
    ApiError::LoginThrottled(retry_after.unwrap_or(1))
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
}

// --- Two-factor factor management ---

async fn list_factors(
    State(state): State<IdentityRouteState>,
    AuthUser(user, _session): AuthUser,
) -> ApiResult<Json<FactorListResponse>> {
    let factors = state
        .two_factor
        .list_factors(user.id())
        .await
        .map_err(map_two_factor_err)?;
    let remaining = state
        .two_factor
        .count_recovery_codes(user.id())
        .await
        .map_err(map_two_factor_err)?;
    Ok(Json(FactorListResponse {
        factors: factors.iter().map(FactorDto::from_factor).collect(),
        recovery_codes_remaining: remaining,
    }))
}

async fn enroll_totp(
    State(state): State<IdentityRouteState>,
    AuthUser(user, session): AuthUser,
) -> ApiResult<Json<EnrollTotpResponse>> {
    let now = chrono::Utc::now();
    let enrollment = state
        .two_factor
        .enroll_totp(
            user.id(),
            user.username().as_str(),
            "OpenPanel",
            user.username().as_str(),
            now,
        )
        .await
        .map_err(map_two_factor_err)?;
    let _ = session; // audit attribution could pull from session; left as username.
    Ok(Json(EnrollTotpResponse {
        factor: FactorDto::from_factor(&enrollment.factor),
        secret_base32: enrollment.secret.to_base32(),
        provisioning_uri: enrollment.provisioning_uri,
        recovery_codes: enrollment.recovery_codes,
    }))
}

async fn regenerate_recovery(
    State(state): State<IdentityRouteState>,
    AuthUser(user, _session): AuthUser,
) -> ApiResult<Json<RegenerateRecoveryResponse>> {
    let codes = state
        .two_factor
        .regenerate_recovery_codes(user.username().as_str(), user.id(), chrono::Utc::now())
        .await
        .map_err(map_two_factor_err)?;
    Ok(Json(RegenerateRecoveryResponse {
        recovery_codes: codes,
    }))
}

async fn revoke_factor(
    State(state): State<IdentityRouteState>,
    AuthUser(user, _session): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .two_factor
        .revoke_factor(user.username().as_str(), user.id(), id, chrono::Utc::now())
        .await
        .map_err(map_two_factor_err)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

fn map_two_factor_err(e: openpanel_app::identity::two_factor::TwoFactorError) -> ApiError {
    use openpanel_app::identity::two_factor::TwoFactorError;
    match e {
        TwoFactorError::NoFactor | TwoFactorError::FactorNotFound | TwoFactorError::Revoked => {
            ApiError::Unprocessable(e.to_string())
        }
        TwoFactorError::InvalidCode | TwoFactorError::CodeReplayed => {
            ApiError::Identity(IdentityError::InvalidCredentials)
        }
        TwoFactorError::NoRecoveryCodes => ApiError::Unprocessable(e.to_string()),
        TwoFactorError::Invalid(_) => ApiError::Unprocessable(e.to_string()),
        TwoFactorError::Storage(_) => ApiError::Internal(e.to_string()),
    }
}
