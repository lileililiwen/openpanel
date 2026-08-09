//! Web router: shell, login/logout, and static assets, all behind the same
//! session middleware used by the API so there is one auth system.

use std::sync::Arc;

use axum::{
    Router,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::LOCATION},
    middleware::from_fn_with_state,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use openpanel_api::{
    extract::AuthSession,
    middleware::session::{SESSION_COOKIE, session_middleware},
};
use openpanel_app::{
    DatabasesService, FilesService, IdentityService, MonitoringService, SitesService, SslService,
};
use openpanel_domain::{Session, SessionToken, User};

use crate::{
    assets,
    csrf::{CsrfStore, ValidateCsrf},
    dashboard, login, sites,
};

/// Shared state for every web handler.
#[derive(Clone)]
pub struct WebState {
    /// Identity service for login/logout/session resolution.
    pub identity: Arc<IdentityService>,
    /// Sites service for the dashboard quick-count cards.
    pub sites: Arc<SitesService>,
    /// Databases service for the dashboard quick-count cards.
    pub databases: Arc<DatabasesService>,
    /// Files service for the dashboard quick-count cards.
    pub files: Arc<FilesService>,
    /// SSL service for the dashboard quick-count cards.
    pub ssl: Arc<SslService>,
    /// Monitoring service for the host gauges and alert feed.
    pub monitoring: Arc<MonitoringService>,
    /// Per-session CSRF token store.
    pub csrf: Arc<CsrfStore>,
}

/// Authenticated web user; rejects unauthenticated requests with a 302 to `/login`.
pub struct WebUser(pub User, pub Session);

impl<S> FromRequestParts<S> for WebUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let auth = parts.extensions.get::<AuthSession>().cloned();
        std::future::ready(match auth {
            Some(a) => Ok(WebUser(a.user, a.session)),
            None => Err(unauth_redirect()),
        })
    }
}

/// 302 Found → `/login`, used when an authenticated web route is hit without a
/// valid session.
fn unauth_redirect() -> Response {
    (StatusCode::FOUND, [(LOCATION, "/login")]).into_response()
}

/// POST /logout — invalidate the session, clear the cookie, redirect to login.
async fn logout(
    State(state): State<WebState>,
    WebUser(user, _session): WebUser,
    headers: HeaderMap,
    _csrf: ValidateCsrf,
) -> Response {
    if let Some(tok) = cookie_token(&headers)
        && let Ok(token) = SessionToken::from_string(tok)
    {
        let _ = state
            .identity
            .logout(&token, user.username().as_str())
            .await;
    }
    let mut resp = Redirect::to("/login").into_response();
    let cookie = format!("{SESSION_COOKIE}=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0");
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        resp.headers_mut()
            .insert(axum::http::header::SET_COOKIE, value);
    }
    resp
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

/// Build the web router. Returns a `Router<()>` ready to merge into the API app.
pub fn router(
    identity: Arc<IdentityService>,
    sites: Arc<SitesService>,
    databases: Arc<DatabasesService>,
    files: Arc<FilesService>,
    ssl: Arc<SslService>,
    monitoring: Arc<MonitoringService>,
) -> Router {
    let state = WebState {
        identity: identity.clone(),
        sites,
        databases,
        files,
        ssl,
        monitoring,
        csrf: Arc::new(CsrfStore::new()),
    };
    Router::new()
        .route("/", get(dashboard::home))
        .route("/dashboard", get(dashboard::home))
        .route("/dashboard/gauges", get(dashboard::gauges_partial))
        .route(
            "/login",
            get(login::login_page_handler).post(login::login_handler),
        )
        .route("/logout", post(logout))
        .route("/sites", get(sites::list).post(sites::create))
        .route("/sites/new", get(sites::new_form))
        .route("/sites/{id}", get(sites::detail).delete(sites::delete))
        .route("/sites/{id}/enable", post(sites::enable))
        .route("/sites/{id}/disable", post(sites::disable))
        .route("/assets/htmx.min.js", get(assets::htmx_min_js))
        .route("/assets/app.css", get(assets::app_css))
        .layer(from_fn_with_state(identity, session_middleware))
        .with_state(state)
}
