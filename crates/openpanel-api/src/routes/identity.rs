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
        BeginWebAuthnLoginRequest, BeginWebAuthnLoginResponse, BeginWebAuthnRegisterResponse,
        ChangePasswordRequest, ChangeRoleRequest, CreateUserRequest, EnrollTotpResponse, FactorDto,
        FactorListResponse, FinishWebAuthnRegisterRequest, LoginFactorRequest, LoginFactorRequired,
        LoginRequest, LoginResponse, RegenerateRecoveryResponse, UserDto,
        VerifyTotpEnrollmentRequest, VerifyTotpEnrollmentResponse,
    },
    error::{ApiError, ApiResult},
    extract::{AuthUser, RequireOwner},
    middleware::session::SESSION_COOKIE,
};

const CHALLENGE_COOKIE: &str = "openpanel_challenge";

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
        .route("/login/webauthn/begin", post(begin_webauthn_login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", delete(delete_user).patch(change_role))
        .route("/users/{id}/disable", post(disable_user))
        .route("/users/{id}/password", post(change_password))
        .route(
            "/users/{user_id}/factors/{factor_id}",
            delete(owner_revoke_factor),
        )
        .route("/factors", get(list_factors))
        .route("/factors/totp/enroll", post(enroll_totp))
        .route("/factors/totp/verify", post(verify_totp_enrollment))
        .route(
            "/factors/webauthn/register/begin",
            post(begin_webauthn_register),
        )
        .route(
            "/factors/webauthn/register/finish",
            post(finish_webauthn_register),
        )
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
    let ip = forwarded
        .map(|v| v.to_string())
        .or_else(|| Some(peer.to_string()));
    let ua = user_agent(&headers_in);
    let remember = remember_device_cookie(&headers_in);
    let outcome = state
        .identity
        .login(
            &req.username_or_email,
            &req.password,
            ip,
            ua.clone(),
            remember.as_deref(),
        )
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
            let cookie = format!(
                "{}={}; HttpOnly; Path=/; SameSite=Lax; Max-Age={}",
                CHALLENGE_COOKIE,
                challenge.challenge_token,
                state.two_factor.challenge_lifetime_seconds()
            );
            let mut headers = HeaderMap::new();
            headers.insert(
                SET_COOKIE,
                HeaderValue::from_str(&cookie)
                    .map_err(|_| ApiError::Internal("bad challenge cookie".into()))?,
            );
            Ok((headers, Json(body)).into_response())
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
    let forwarded = forwarded_ip(&headers_in);
    let ip = forwarded
        .map(|v| v.to_string())
        .or_else(|| Some(peer.ip().to_string()));
    let ua = user_agent(&headers_in);
    let challenge_token = if req.challenge_token.is_empty() {
        cookie_value(&headers_in, CHALLENGE_COOKIE).ok_or(ApiError::Unauthorized)?
    } else {
        req.challenge_token
    };
    let response = match req.kind.as_str() {
        "totp" => openpanel_app::identity::two_factor::FactorResponse::Totp(req.code),
        "recovery" => openpanel_app::identity::two_factor::FactorResponse::Recovery(req.code),
        "webauthn" => openpanel_app::identity::two_factor::FactorResponse::WebAuthn {
            ceremony_challenge_id: req
                .webauthn_challenge_id
                .ok_or_else(|| ApiError::Unprocessable("missing WebAuthn challenge id".into()))?,
            credential: req
                .credential
                .ok_or_else(|| ApiError::Unprocessable("missing WebAuthn credential".into()))?,
        },
        _ => return Err(ApiError::Unprocessable("unknown factor kind".into())),
    };
    let (user, factor_id, token) = state
        .identity
        .verify_login_factor_with_factor(
            req.challenge_id,
            &challenge_token,
            response,
            ip.clone(),
            ua.clone(),
        )
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
    headers.append(
        SET_COOKIE,
        HeaderValue::from_static("openpanel_challenge=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0"),
    );
    if req.remember_device {
        // Remember-device binds to a specific factor; recovery codes
        // are account-scoped, so we only issue when the factor is known.
        if let Some(factor_id) = factor_id {
            let ip_str = ip.as_deref().unwrap_or("");
            let ua_str = ua.as_deref().unwrap_or("");
            let remember = state.two_factor.issue_remember_device_cookie(
                user.id(),
                factor_id,
                ua_str,
                ip_str,
                chrono::Utc::now(),
            );
            let max_age = state.two_factor.remember_device_lifetime_seconds();
            let remember_cookie = format!(
                "{}={}; HttpOnly; Path=/; SameSite=Lax; Max-Age={}",
                openpanel_domain::REMEMBER_DEVICE_COOKIE,
                remember,
                max_age
            );
            headers.append(
                SET_COOKIE,
                HeaderValue::from_str(&remember_cookie)
                    .map_err(|_| ApiError::Internal("bad remember cookie".into()))?,
            );
        }
    }
    Ok((headers, Json(resp)))
}

async fn begin_webauthn_login(
    State(state): State<IdentityRouteState>,
    headers: HeaderMap,
    Json(req): Json<BeginWebAuthnLoginRequest>,
) -> ApiResult<Json<BeginWebAuthnLoginResponse>> {
    let token = if req.challenge_token.is_empty() {
        cookie_value(&headers, CHALLENGE_COOKIE).ok_or(ApiError::Unauthorized)?
    } else {
        req.challenge_token
    };
    let (challenge_id, public_key) = state
        .two_factor
        .begin_webauthn_login(req.challenge_id, &token, chrono::Utc::now())
        .await
        .map_err(map_two_factor_err)?;
    Ok(Json(BeginWebAuthnLoginResponse {
        challenge_id,
        public_key,
    }))
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

/// Extract the remember-device cookie value (if any) from the request.
fn remember_device_cookie(headers: &HeaderMap) -> Option<String> {
    cookie_value(headers, openpanel_domain::REMEMBER_DEVICE_COOKIE)
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|raw| {
            for part in raw.split(';') {
                let trimmed = part.trim();
                if let Some(rest) = trimmed.strip_prefix(&format!("{name}=")) {
                    return Some(rest.to_string());
                }
            }
            None
        })
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
    AuthUser(user, _session): AuthUser,
) -> ApiResult<Json<EnrollTotpResponse>> {
    let now = chrono::Utc::now();
    let enrollment = state
        .two_factor
        .enroll_totp(
            user.id(),
            user.username().as_str(),
            user.username().as_str(),
            now,
        )
        .await
        .map_err(map_two_factor_err)?;
    Ok(Json(EnrollTotpResponse {
        enrollment_id: enrollment.enrollment_id,
        secret_base32: enrollment.secret.to_base32(),
        provisioning_uri: enrollment.provisioning_uri,
    }))
}

async fn verify_totp_enrollment(
    State(state): State<IdentityRouteState>,
    AuthUser(user, _session): AuthUser,
    Json(req): Json<VerifyTotpEnrollmentRequest>,
) -> ApiResult<Json<VerifyTotpEnrollmentResponse>> {
    let verified = state
        .two_factor
        .verify_totp_enrollment(
            user.username().as_str(),
            user.id(),
            req.enrollment_id,
            &req.code,
            chrono::Utc::now(),
        )
        .await
        .map_err(map_two_factor_err)?;
    Ok(Json(VerifyTotpEnrollmentResponse {
        factor: FactorDto::from_factor(&verified.factor),
        recovery_codes: verified.recovery_codes,
    }))
}

async fn begin_webauthn_register(
    State(state): State<IdentityRouteState>,
    AuthUser(user, _session): AuthUser,
) -> ApiResult<Json<BeginWebAuthnRegisterResponse>> {
    let now = chrono::Utc::now();
    let (challenge_id, public_key, registration) = state
        .two_factor
        .begin_webauthn_register(user.id(), user.username().as_str())
        .map_err(map_two_factor_err)?;
    state
        .two_factor
        .persist_webauthn_register(challenge_id, user.id(), &registration, now)
        .await
        .map_err(map_two_factor_err)?;
    Ok(Json(BeginWebAuthnRegisterResponse {
        challenge_id,
        public_key,
    }))
}

async fn finish_webauthn_register(
    State(state): State<IdentityRouteState>,
    AuthUser(user, _session): AuthUser,
    Json(req): Json<FinishWebAuthnRegisterRequest>,
) -> ApiResult<Json<FactorDto>> {
    let factor_id = state
        .two_factor
        .finish_webauthn_register(
            user.username().as_str(),
            req.challenge_id,
            user.id(),
            &req.credential,
            chrono::Utc::now(),
        )
        .await
        .map_err(map_two_factor_err)?;
    let factor = state
        .two_factor
        .list_factors(user.id())
        .await
        .map_err(map_two_factor_err)?
        .into_iter()
        .find(|factor| factor.id() == factor_id)
        .ok_or_else(|| ApiError::Internal("enrolled factor missing".into()))?;
    Ok(Json(FactorDto::from_factor(&factor)))
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

async fn owner_revoke_factor(
    State(state): State<IdentityRouteState>,
    AuthUser(actor, _session): AuthUser,
    RequireOwner: RequireOwner,
    Path((user_id, factor_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .two_factor
        .revoke_factor(
            actor.username().as_str(),
            user_id,
            factor_id,
            chrono::Utc::now(),
        )
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
        TwoFactorError::RememberDeviceInvalid
        | TwoFactorError::RememberDeviceExpired
        | TwoFactorError::RememberDeviceMismatch
        | TwoFactorError::WebAuthnUnavailable
        | TwoFactorError::WebAuthnCeremony(_) => ApiError::Unprocessable(e.to_string()),
        TwoFactorError::WebAuthnCeremonyExpired => {
            ApiError::Identity(IdentityError::InvalidCredentials)
        }
    }
}
