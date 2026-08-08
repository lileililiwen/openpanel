//! HTTP error type with `IntoResponse` mapping for typed domain errors.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use openpanel_domain::{IdentityError, MonitoringError, SslError};
use thiserror::Error;

use crate::dto::ErrorBody;

/// Convenience alias for `Result<T, ApiError>` used by every route handler.
pub type ApiResult<T> = Result<T, ApiError>;

/// All errors that may be returned from an API handler, mapped to HTTP responses.
#[derive(Debug, Error)]
pub enum ApiError {
    /// Wraps a typed [`IdentityError`] from the domain layer.
    #[error(transparent)]
    Identity(#[from] IdentityError),

    /// Wraps a typed [`SslError`] from the ssl bounded context.
    #[error(transparent)]
    Ssl(#[from] SslError),

    /// Wraps a typed [`MonitoringError`] from the monitoring context.
    #[error(transparent)]
    Monitoring(#[from] MonitoringError),

    /// A request could not be processed due to malformed input.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// The requested resource does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// The caller is not authenticated.
    #[error("unauthorized")]
    Unauthorized,

    /// The caller is authenticated but lacks the required permission.
    #[error("forbidden")]
    Forbidden,

    /// The request conflicts with the current resource state (e.g. duplicate).
    #[error("conflict: {0}")]
    Conflict(String),

    /// The request body exceeded the configured size limit.
    #[error("payload too large: {0} bytes")]
    PayloadTooLarge(u64),

    /// An unexpected internal failure. Avoid leaking details to clients.
    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    /// Converts this error into a stable `(<status>, <machine_code>)` JSON response.
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::Identity(e) => match e {
                IdentityError::InvalidCredentials => {
                    (StatusCode::UNAUTHORIZED, "invalid_credentials")
                }
                IdentityError::AccountDisabled => (StatusCode::UNAUTHORIZED, "account_disabled"),
                IdentityError::PasswordTooShort => (StatusCode::BAD_REQUEST, "password_too_short"),
                IdentityError::UsernameTaken => (StatusCode::CONFLICT, "username_taken"),
                IdentityError::EmailTaken => (StatusCode::CONFLICT, "email_taken"),
                IdentityError::UserNotFound => (StatusCode::NOT_FOUND, "user_not_found"),
                IdentityError::SessionExpired => (StatusCode::UNAUTHORIZED, "session_expired"),
                IdentityError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
                IdentityError::LastOwner => (StatusCode::CONFLICT, "last_owner"),
                IdentityError::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),
                IdentityError::Persistence(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
            },
            ApiError::Ssl(e) => match e {
                SslError::NotFound(_) => (StatusCode::NOT_FOUND, "ssl_not_found"),
                SslError::AcmeChallenge(_) | SslError::Acme(_) => {
                    (StatusCode::BAD_GATEWAY, "ssl_acme_failed")
                }
                SslError::KeyMismatch => (StatusCode::BAD_REQUEST, "ssl_key_mismatch"),
                SslError::Expired => (StatusCode::BAD_REQUEST, "ssl_expired"),
                SslError::InvalidPem(_) | SslError::InvalidCert(_) | SslError::InvalidKey(_) => {
                    (StatusCode::BAD_REQUEST, "ssl_invalid_pem")
                }
                SslError::Repo(_)
                | SslError::Io(_)
                | SslError::Encryption(_)
                | SslError::Decryption(_) => (StatusCode::INTERNAL_SERVER_ERROR, "ssl_internal"),
            },
            ApiError::Monitoring(e) => match e {
                MonitoringError::InvalidKind(_) => (StatusCode::BAD_REQUEST, "invalid_metric"),
                MonitoringError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
                MonitoringError::Repo(_) | MonitoringError::Io(_) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "monitoring_internal")
                }
                MonitoringError::InvalidValue(_)
                | MonitoringError::TimestampInFuture
                | MonitoringError::EmptySnapshot => (StatusCode::BAD_REQUEST, "bad_request"),
            },
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::PayloadTooLarge(_) => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        let body = Json(ErrorBody::new(code));
        (status, body).into_response()
    }
}
