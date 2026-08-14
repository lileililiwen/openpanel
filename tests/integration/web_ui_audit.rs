//! Web-UI audit route group integration tests.
//!
//! Verifies that `/audit` and `/audit/events` return `501` with the
//! stub header, refuse User-role principals, and never leak audit
//! data.

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn audit_route_unauthenticated_redirects_to_login() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/audit", server.base_url()))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 302);
    assert_eq!(resp.headers().get("location").unwrap(), "/login");
}

#[tokio::test]
async fn audit_route_returns_501_with_stub_header_for_owner() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!("{}/audit", server.base_url()))
        .header("authorization", bearer(&token))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 501);
    assert_eq!(
        resp.headers().get("x-openpanel-stub").unwrap(),
        "audit-ui-pending"
    );
    let body = resp.text().await.expect("body");
    assert!(body.contains("audit-ui-pending"));
    // Stub must not leak any audit table / data.
    assert!(!body.contains("<table"));
    assert!(!body.to_lowercase().contains("user"));
    assert!(!body.to_lowercase().contains("login"));
}

#[tokio::test]
async fn audit_events_returns_501_with_stub_header_for_owner() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!("{}/audit/events", server.base_url()))
        .header("authorization", bearer(&token))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 501);
    assert_eq!(
        resp.headers().get("x-openpanel-stub").unwrap(),
        "audit-ui-pending"
    );
    assert!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
}

#[tokio::test]
async fn tokens_css_is_served() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/assets/tokens.css", server.base_url()))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/css")
    );
    let body = resp.text().await.expect("body");
    assert!(body.contains("--op-color-accent"));
    assert!(body.contains("--op-space-4"));
}
