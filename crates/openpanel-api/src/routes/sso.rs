//! SSO login routes plus session inventory/revocation surfaces.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use openpanel_app::SsoService;
use openpanel_domain::{SsoError, identity::sso::SsoConnection};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AuthUser};

/// Public login-flow routes (mounted at the router root, outside the
/// authenticated `/api/v1` nest).
pub fn public_router(service: Arc<SsoService>) -> Router {
    Router::new()
        .route("/auth/sso/begin", post(begin))
        .route("/auth/sso/callback", post(callback))
        .with_state(service)
}

/// Authenticated session-control + connection-config routes (nested
/// under `/api/v1`, behind the session middleware).
pub fn router(service: Arc<SsoService>) -> Router {
    Router::new()
        .route(
            "/auth/sso/connection",
            axum::routing::put(configure).get(get_connection),
        )
        .route("/auth/sessions", get(list_own_sessions))
        .route("/auth/sessions/{id}", delete(revoke_own_session))
        .route(
            "/users/{user_id}/sessions/{session_id}",
            delete(revoke_user_session),
        )
        .with_state(service)
}

async fn begin(State(service): State<Arc<SsoService>>) -> Result<Response, ApiErrorResponse> {
    let url = service.begin_login().await.map_err(map)?;
    Ok(Json(serde_json::json!({ "authorize_url": url })).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CallbackInput {
    code: String,
    state: String,
}

async fn callback(
    State(service): State<Arc<SsoService>>,
    Json(input): Json<CallbackInput>,
) -> Result<Response, ApiErrorResponse> {
    match service
        .callback(&input.code, &input.state, None, None)
        .await
        .map_err(map)?
    {
        openpanel_app::CallbackOutcome::Session {
            token,
            mfa_satisfied,
        } => Ok(Json(serde_json::json!({
            "token": token.expose().to_owned(),
            "mfa_satisfied": mfa_satisfied,
        }))
        .into_response()),
        openpanel_app::CallbackOutcome::FactorRequired { challenge } => {
            Ok(Json(serde_json::json!({
                "status": "factor_required",
                "challenge_id": challenge.challenge_id,
                "expires_at": challenge.expires_at.to_rfc3339(),
            }))
            .into_response())
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ConfigureInput {
    issuer_url: String,
    client_id: String,
    client_secret_cipher: String,
    default_role: openpanel_domain::Role,
    auto_provision: bool,
    trust_idp_mfa: bool,
}

impl Default for ConfigureInput {
    fn default() -> Self {
        Self {
            issuer_url: String::new(),
            client_id: String::new(),
            client_secret_cipher: String::new(),
            default_role: openpanel_domain::Role::User,
            auto_provision: false,
            trust_idp_mfa: false,
        }
    }
}

async fn configure(
    State(service): State<Arc<SsoService>>,
    AuthUser(user, _): AuthUser,
    Json(input): Json<ConfigureInput>,
) -> Result<Response, ApiErrorResponse> {
    let saved = service
        .configure(
            &user,
            SsoConnection {
                issuer_url: input.issuer_url,
                client_id: input.client_id,
                client_secret_cipher: input.client_secret_cipher,
                default_role: input.default_role,
                auto_provision: input.auto_provision,
                trust_idp_mfa: input.trust_idp_mfa,
            },
        )
        .await
        .map_err(map)?;
    Ok(Json(serde_json::json!({
        "issuer_url": saved.issuer_url,
        "client_id": saved.client_id,
        // Never echo the secret material — ciphertext or not.
        "client_secret_cipher": "<redacted>",
        "default_role": format!("{:?}", saved.default_role).to_lowercase(),
        "auto_provision": saved.auto_provision,
        "trust_idp_mfa": saved.trust_idp_mfa,
    }))
    .into_response())
}

async fn get_connection(
    State(service): State<Arc<SsoService>>,
    AuthUser(_user, _): AuthUser,
) -> Result<Response, ApiErrorResponse> {
    let connection = service.connection().await.map_err(map)?;
    Ok(Json(match connection {
        Some(connection) => serde_json::json!({
            "issuer_url": connection.issuer_url,
            "client_id": connection.client_id,
            "client_secret_cipher": "<redacted>",
            "auto_provision": connection.auto_provision,
            "trust_idp_mfa": connection.trust_idp_mfa,
        }),
        None => serde_json::Value::Null,
    })
    .into_response())
}

async fn list_own_sessions(
    State(service): State<Arc<SsoService>>,
    AuthUser(user, _): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let sessions = service.list_sessions(&user).await.map_err(map)?;
    let items: Vec<serde_json::Value> = sessions
        .iter()
        .map(|session| {
            serde_json::json!({
                "id": session.id.to_string(),
                "created_at": session.created_at.to_rfc3339(),
                "last_seen_at": session.last_seen_at.to_rfc3339(),
                "absolute_expires_at": session.absolute_expires_at.to_rfc3339(),
                "source_ip": session.source_ip,
                "user_agent": session.user_agent,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "sessions": items })))
}

async fn revoke_own_session(
    State(service): State<Arc<SsoService>>,
    AuthUser(user, _): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    service.revoke_own_session(&user, id).await.map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_user_session(
    State(service): State<Arc<SsoService>>,
    AuthUser(user, _): AuthUser,
    Path((target_user, session_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    service
        .revoke_user_session(&user, target_user, session_id)
        .await
        .map_err(map)?;
    Ok(StatusCode::NO_CONTENT)
}

struct ApiErrorResponse(ApiError);

impl axum::response::IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        self.0.into_response()
    }
}

impl From<ApiError> for ApiErrorResponse {
    fn from(value: ApiError) -> Self {
        Self(value)
    }
}

impl From<ApiErrorResponse> for Response {
    fn from(value: ApiErrorResponse) -> Self {
        value.0.into_response()
    }
}

fn map(error: SsoError) -> ApiError {
    match error {
        SsoError::Forbidden => ApiError::Forbidden,
        SsoError::ProvisionDisabled | SsoError::LinkConflict => {
            ApiError::Unprocessable(error.to_string())
        }
        SsoError::StateMismatch | SsoError::StateExpired | SsoError::NonceMismatch => {
            ApiError::Unprocessable(error.to_string())
        }
        SsoError::Discovery => ApiError::ServiceUnavailable("provider unavailable".into()),
        SsoError::InvalidConfig(message) => ApiError::Unprocessable(message),
        SsoError::Persistence(message) => ApiError::Internal(message),
    }
}
