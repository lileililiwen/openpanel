//! HTTP error type with `IntoResponse` mapping for typed domain errors.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use openpanel_domain::IdentityError;
use thiserror::Error;

use crate::dto::ErrorBody;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Identity(#[from] IdentityError),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("payload too large: {0} bytes")]
    PayloadTooLarge(u64),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::Identity(e) => match e {
                IdentityError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
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