//! Web-UI audit route group integration tests.
//!
//! Verifies that `/audit` and `/audit/events` render an owner-only audit
//! center: unauthenticated requests redirect, User principals are
//! refused, owner requests return the page/fragment/JSON, secrets are
//! redacted, filters produce a no-results state, and reads never create
//! audit events.

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome};

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
async fn owner_sees_audit_page_with_nav_and_summary() {
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
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("Audit &amp; activity") || body.contains("Audit & activity"), "heading: {body}");
    // Navigation exposes the owner-only Audit link.
    assert!(body.contains("href=\"/audit\""), "nav link missing: {body}");
    assert!(body.contains("op-empty-state") || body.contains("op-no-results") || body.contains("audit-table"), "a UI state must render: {body}");
}

#[tokio::test]
async fn user_is_forbidden_from_audit() {
    let server = TestServer::new().await;
    let _owner = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    // Create a regular user directly through the identity service.
    server
        .identity()
        .create_user(
            "alice",
            "alice@example.test",
            "correct horse battery staple",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .expect("create user");
    let user_token = server.login("alice", "correct horse battery staple").await;

    let resp = server
        .client()
        .get(format!("{}/audit", server.base_url()))
        .bearer_auth(&user_token)
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn owner_audit_events_returns_json_api() {
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
    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("application/json"), "expected json: {ct}");
    let body: serde_json::Value = resp.json().await.expect("json");
    assert!(body.get("events").is_some(), "events key: {body}");
    assert!(body.get("next_cursor").is_some(), "next_cursor key: {body}");
}

#[tokio::test]
async fn owner_audit_events_htmx_returns_fragment() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!("{}/audit/events", server.base_url()))
        .header("authorization", bearer(&token))
        .header("HX-Request", "true")
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("text/html"), "expected html fragment: {ct}");
    let body = resp.text().await.expect("body");
    assert!(body.contains("audit-events"), "fragment region: {body}");
}

#[tokio::test]
async fn secret_metadata_is_redacted_in_page() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    server
        .seed_audit(
            AuditEvent::new("admin", AuditAction::UserCreated, AuditOutcome::Success)
                .target("victim")
                .metadata(serde_json::json!({
                    "password": "super-secret-value",
                    "target_id": "1234",
                    "note": "benign",
                })),
        )
        .await;

    let resp = server
        .client()
        .get(format!("{}/audit", server.base_url()))
        .header("authorization", bearer(&token))
        .send()
        .await
        .expect("get");
    let body = resp.text().await.expect("body");
    assert!(
        !body.contains("super-secret-value"),
        "secret leaked into audit page: {body}"
    );
    assert!(body.contains("target_id"), "safe metadata shown: {body}");
    assert!(body.contains("benign"), "safe metadata shown: {body}");
}

#[tokio::test]
async fn no_results_state_when_filter_matches_nothing() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!("{}/audit?target=zzzzz-no-such-target", server.base_url()))
        .header("authorization", bearer(&token))
        .send()
        .await
        .expect("get");
    let body = resp.text().await.expect("body");
    assert!(body.contains("op-no-results"), "no-results state: {body}");
    assert!(body.contains("Clear filter"), "clear action: {body}");
}

#[tokio::test]
async fn reading_audit_does_not_create_audit_events() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    // Let the log settle after bootstrap.
    let before = server.audit_events().await.len();

    let _ = server
        .client()
        .get(format!("{}/audit", server.base_url()))
        .header("authorization", bearer(&token))
        .send()
        .await
        .expect("get");
    let _ = server
        .client()
        .get(format!("{}/audit/events", server.base_url()))
        .header("authorization", bearer(&token))
        .send()
        .await
        .expect("get");

    let after = server.audit_events().await.len();
    assert_eq!(before, after, "reading the audit log must not append events");
}
