//! Operator security control-plane HTTP integration tests.
//!
//! Capability under test: `operator-security-control-plane` (findings
//! are normalized and prioritized; remediation is previewed and
//! verified; suppression expires; operators see safe evidence).

use crate::common::*;
use serde_json::json;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn auto_firewall_finding(resource: &str, evidence: &str) -> serde_json::Value {
    json!({
        "source": "firewall",
        "severity": "high",
        "resource": resource,
        "rule": "login-blocks-active",
        "title": "Login-abuse blocks are active",
        "evidence": evidence,
        "remediation_mode": "automatic",
    })
}

fn manual_finding() -> serde_json::Value {
    json!({
        "source": "compliance",
        "severity": "medium",
        "resource": "compliance:00000000-0000-0000-0000-000000000000",
        "rule": "cis-5.2.1",
        "title": "SSH root login",
        "evidence": "CIS 5.2.1 must be reviewed by hand",
        "remediation_mode": "manual",
    })
}

fn session_cookie(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("Set-Cookie header")
        .to_str()
        .expect("Set-Cookie string")
        .split(';')
        .next()
        .expect("cookie name=value")
        .to_string()
}

async fn login(server: &TestServer, username: &str, password: &str) -> String {
    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[("username_or_email", username), ("password", password)])
        .send()
        .await
        .expect("POST /login");
    assert_eq!(login.status(), 303, "login redirects");
    session_cookie(&login)
}

/// Unauthenticated API access is rejected.
#[tokio::test]
async fn operator_security_control_plane_api_requires_authentication() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/security/findings", server.base_url()))
        .send()
        .await
        .expect("GET");
    assert_eq!(resp.status(), 401);
}

/// Ingest normalizes and deduplicates; the queue is prioritized.
#[tokio::test]
async fn operator_security_control_plane_ingest_deduplicates_and_prioritizes() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);

    let low = json!({
        "source": "waf",
        "severity": "low",
        "resource": "waf:00000000-0000-0000-0000-000000000000",
        "rule": "xss",
        "title": "WAF note",
        "evidence": "low signal",
        "remediation_mode": "automatic",
    });
    let body = vec![
        auto_firewall_finding("firewall:rules", "first"),
        auto_firewall_finding("firewall:rules", "first"),
        low,
    ];
    let ingested = server
        .client()
        .post(format!("{}/api/v1/security/findings", server.base_url()))
        .header("authorization", &auth)
        .json(&body)
        .send()
        .await
        .expect("POST ingest");
    assert_eq!(ingested.status(), 201);

    let queue = server
        .client()
        .get(format!("{}/api/v1/security/findings", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .expect("GET queue");
    assert_eq!(queue.status(), 200);
    let items: Vec<serde_json::Value> = queue.json().await.expect("queue json");
    assert_eq!(items.len(), 2, "duplicate merged: {items:?}");
    assert_eq!(items[0]["severity"], "high", "critical first: {items:?}");
    assert_eq!(items[1]["severity"], "low");
}

/// Manual findings explain themselves instead of executing.
#[tokio::test]
async fn operator_security_control_plane_manual_preview_is_guided() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let ingested = server
        .client()
        .post(format!("{}/api/v1/security/findings", server.base_url()))
        .header("authorization", &auth)
        .json(&vec![manual_finding()])
        .send()
        .await
        .expect("POST ingest");
    assert_eq!(ingested.status(), 201);
    let stored: Vec<serde_json::Value> = ingested.json().await.expect("json");
    let id = stored[0]["id"].as_str().expect("id").to_string();

    let preview = server
        .client()
        .get(format!(
            "{}/api/v1/security/findings/{id}/preview",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("GET preview");
    assert_eq!(preview.status(), 422, "manual stays manual");
}

/// Remediation requires confirmation, is idempotent, and verifies.
#[tokio::test]
async fn operator_security_control_plane_remediate_confirms_idempotent_verifies() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let ingested = server
        .client()
        .post(format!("{}/api/v1/security/findings", server.base_url()))
        .header("authorization", &auth)
        .json(&vec![auto_firewall_finding("firewall:rules", "blocks")])
        .send()
        .await
        .expect("POST ingest");
    assert_eq!(ingested.status(), 201);
    let stored: Vec<serde_json::Value> = ingested.json().await.expect("json");
    let id = stored[0]["id"].as_str().expect("id").to_string();

    let preview = server
        .client()
        .get(format!(
            "{}/api/v1/security/findings/{id}/preview",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("GET preview");
    assert_eq!(preview.status(), 200);
    let preview_body: serde_json::Value = preview.json().await.expect("preview json");
    assert_eq!(preview_body["adapter"], "firewall_review");
    assert_eq!(preview_body["requires_confirmation"], true);

    let unconfirmed = server
        .client()
        .post(format!(
            "{}/api/v1/security/findings/{id}/remediate",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&json!({"idempotency_key": "key-1", "confirmed": false}))
        .send()
        .await
        .expect("POST remediate");
    assert_eq!(unconfirmed.status(), 409, "confirmation required");

    let remediated = server
        .client()
        .post(format!(
            "{}/api/v1/security/findings/{id}/remediate",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&json!({"idempotency_key": "key-1", "confirmed": true}))
        .send()
        .await
        .expect("POST remediate");
    assert_eq!(remediated.status(), 200);
    let done: serde_json::Value = remediated.json().await.expect("json");
    assert_eq!(done["state"], "resolved", "post-check passed: {done:?}");

    let retry = server
        .client()
        .post(format!(
            "{}/api/v1/security/findings/{id}/remediate",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&json!({"idempotency_key": "key-1", "confirmed": true}))
        .send()
        .await
        .expect("POST remediate retry");
    assert_eq!(retry.status(), 200);
    let again: serde_json::Value = retry.json().await.expect("json");
    assert_eq!(again["state"], "resolved", "idempotent retry: {again:?}");

    let queue = server
        .client()
        .get(format!("{}/api/v1/security/findings", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .expect("GET queue");
    let items: Vec<serde_json::Value> = queue.json().await.expect("queue json");
    assert!(
        items.iter().all(|item| item["id"] != id),
        "resolved leaves the queue: {items:?}"
    );
}

/// Suppression hides the finding until expiry, then it reopens.
#[tokio::test]
async fn operator_security_control_plane_suppression_expires_and_reopens() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let ingested = server
        .client()
        .post(format!("{}/api/v1/security/findings", server.base_url()))
        .header("authorization", &auth)
        .json(&vec![auto_firewall_finding("firewall:rules", "blocks")])
        .send()
        .await
        .expect("POST ingest");
    let stored: Vec<serde_json::Value> = ingested.json().await.expect("json");
    let id = stored[0]["id"].as_str().expect("id").to_string();

    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(1);
    let suppressed = server
        .client()
        .post(format!(
            "{}/api/v1/security/findings/{id}/suppress",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&json!({
            "reason": "known maintenance window",
            "scope": "firewall:rules",
            "expires_at": expires_at.to_rfc3339(),
        }))
        .send()
        .await
        .expect("POST suppress");
    assert_eq!(suppressed.status(), 200);

    let queue = server
        .client()
        .get(format!("{}/api/v1/security/findings", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .expect("GET queue");
    let items: Vec<serde_json::Value> = queue.json().await.expect("queue json");
    assert!(items.is_empty(), "suppressed hides: {items:?}");

    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let queue = server
        .client()
        .get(format!("{}/api/v1/security/findings", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .expect("GET queue");
    let items: Vec<serde_json::Value> = queue.json().await.expect("queue json");
    assert_eq!(items.len(), 1, "expiry reopens: {items:?}");
    assert_eq!(items[0]["state"], "open");
}

/// Secrets never reach API projections or the web detail page.
#[tokio::test]
async fn operator_security_control_plane_redacts_secrets_everywhere() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let ingested = server
        .client()
        .post(format!("{}/api/v1/security/findings", server.base_url()))
        .header("authorization", &auth)
        .json(&vec![auto_firewall_finding(
            "firewall:rules",
            "deny 203.0.113.7 password=hunter2 token abc",
        )])
        .send()
        .await
        .expect("POST ingest");
    assert_eq!(ingested.status(), 201);
    let stored: Vec<serde_json::Value> = ingested.json().await.expect("json");
    let id = stored[0]["id"].as_str().expect("id").to_string();
    assert!(
        !stored[0]["evidence"]
            .as_str()
            .unwrap_or("")
            .contains("hunter2"),
        "api projection redacted: {stored:?}"
    );
    assert!(
        stored[0]["evidence"]
            .as_str()
            .unwrap_or("")
            .contains("203.0.113.7"),
        "actionable context kept: {stored:?}"
    );

    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let page = server
        .client()
        .get(format!("{}/security/findings/{id}", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET detail");
    assert_eq!(page.status(), 200);
    let body = page.text().await.expect("body");
    assert!(!body.contains("hunter2"), "web detail redacted");
    assert!(body.contains("203.0.113.7"), "web keeps context");
}

/// Web queue is owner-gated and renders the shipped table vocabulary.
#[tokio::test]
async fn operator_security_control_plane_web_queue_is_owner_gated() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;

    let unauth = server
        .client()
        .get(format!("{}/security/findings", server.base_url()))
        .send()
        .await
        .expect("GET");
    assert_eq!(unauth.status(), 302, "unauth redirects to login");

    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let page = server
        .client()
        .get(format!("{}/security/findings", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET queue");
    assert_eq!(page.status(), 200);
    let body = page.text().await.expect("body");
    assert!(body.contains("Security findings"), "heading: {body}");
    assert!(
        body.contains("op-empty-state") || body.contains("class=\"table\""),
        "shipped vocabulary: {body}"
    );
}
