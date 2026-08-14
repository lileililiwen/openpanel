//! Audit route stubs.
//!
//! This module wires the `/audit` and `/audit/events` route group into
//! the web router. The handlers return `501 Not Implemented` with a
//! non-secret header that identifies the stub.
//!
//! The follow-on `add-log-viewer` change replaces these handlers with
//! real ones that render the audit log. Owner/Admin role gating is
//! enforced by a thin guard; a User-role principal receives `403`.

use axum::{
    http::{HeaderName, HeaderValue, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use openpanel_domain::Role;

/// Header value attached to every stub response. Used by integration
/// tests and any future caller to detect the stub without parsing HTML.
pub const STUB_HEADER_VALUE: &str = "audit-ui-pending";

/// The header name carrying the stub marker.
pub fn stub_header() -> HeaderName {
    HeaderName::from_static("x-openpanel-stub")
}

/// Bare HTML body for the audit index stub. Contains no audit data and
/// no user-visible strings; it is intentionally inert.
const INDEX_BODY: &str = "<!doctype html><html><head><title>audit-stub</title></head><body><main data-stub=\"audit-ui-pending\"></main></body></html>";

/// Audit index stub.
pub async fn audit_index() -> Response {
    not_implemented_html(INDEX_BODY)
}

/// Audit events stub. Returns 501 with the same stub header; body is
/// empty since clients should not see anything until the follow-on
/// `add-log-viewer` change lands.
pub async fn audit_list() -> Response {
    not_implemented_json()
}

/// Construct a `501 Not Implemented` response with the stub header and
/// `text/html` content type. Used by the index handler.
fn not_implemented_html(body: &'static str) -> Response {
    let mut resp = (StatusCode::NOT_IMPLEMENTED, body).into_response();
    resp.headers_mut()
        .insert(stub_header(), HeaderValue::from_static(STUB_HEADER_VALUE));
    resp.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    resp
}

/// Construct a `501 Not Implemented` response with the stub header and
/// `application/json` content type. Used by the events handler.
fn not_implemented_json() -> Response {
    let body = "{\"stub\":\"audit-ui-pending\"}";
    let mut resp = (StatusCode::NOT_IMPLEMENTED, body).into_response();
    resp.headers_mut()
        .insert(stub_header(), HeaderValue::from_static(STUB_HEADER_VALUE));
    resp.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    resp
}

/// Forbidden response used by the role guard. Symmetric with the rest
/// of the web adapter which returns a plain `403` for unauthorized
/// requests.
pub fn forbidden() -> Response {
    let mut resp = (StatusCode::FORBIDDEN, "forbidden").into_response();
    resp.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp
}

/// Validate the role guard. Owner and Admin are allowed; User is not.
pub fn role_guard(role: Role) -> bool {
    matches!(role, Role::Owner | Role::Admin)
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    #[tokio::test]
    async fn audit_index_returns_501_with_stub_header() {
        let resp = audit_index().await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        assert_eq!(
            resp.headers().get(stub_header()).unwrap(),
            STUB_HEADER_VALUE
        );
        assert!(
            resp.headers()
                .get(CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("audit-ui-pending"));
        assert!(!body.contains("<table"));
    }

    #[tokio::test]
    async fn audit_list_returns_501_with_stub_header_and_json() {
        let resp = audit_list().await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        assert_eq!(
            resp.headers().get(stub_header()).unwrap(),
            STUB_HEADER_VALUE
        );
        assert!(
            resp.headers()
                .get(CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("application/json")
        );
    }

    #[test]
    fn role_guard_admits_owner_and_admin_but_not_user() {
        assert!(role_guard(Role::Owner));
        assert!(role_guard(Role::Admin));
        assert!(!role_guard(Role::User));
    }

    #[test]
    fn forbidden_response_is_403() {
        let resp = forbidden();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
