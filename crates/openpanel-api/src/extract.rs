//! Request extractors: `AuthUser`, `RequireRole`, `RequireOwner`.
//!
//! These wrap the session middleware's extensions so handlers can request
//! an authenticated user (or a specific role) directly as a function arg.

use std::future::Future;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use openpanel_domain::{Role, Session, User};

use crate::error::ApiError;

#[derive(Clone)]
pub struct AuthSession {
    pub user: User,
    pub session: Session,
}

/// Internal extension trait used by the session middleware.
pub trait AuthSessionExt {
    fn auth_session(&self) -> Option<AuthSession>;
}

/// Authenticated user extractor.
pub struct AuthUser(pub User, pub Session);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let auth = parts.extensions.get::<AuthSession>().cloned();
        async move {
            let auth = auth.ok_or(ApiError::Unauthorized)?;
            Ok(AuthUser(auth.user, auth.session))
        }
    }
}

/// Requires the authenticated user to have at least the specified role level.
/// For now, `Role::Owner > Admin > User` — higher roles satisfy lower checks.
pub struct RequireRole(pub User, pub Session);

impl<S> FromRequestParts<S> for RequireRole
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let inner = AuthUser::from_request_parts(parts, state);
        async move {
            let AuthUser(user, session) = inner.await?;
            if matches!(user.role(), Role::Owner) {
                Ok(RequireRole(user, session))
            } else {
                Err(ApiError::Forbidden)
            }
        }
    }
}

pub struct RequireOwner;

impl<S> FromRequestParts<S> for RequireOwner
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let inner = AuthUser::from_request_parts(parts, state);
        async move {
            let AuthUser(user, _session) = inner.await?;
            if matches!(user.role(), Role::Owner) {
                Ok(RequireOwner)
            } else {
                Err(ApiError::Forbidden)
            }
        }
    }
}