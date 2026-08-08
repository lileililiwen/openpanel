//! End-to-end monitoring HTTP tests.
//!
//! The `TestServer` wires a real `MonitoringModule` with a real
//! `SystemCollector`, so `/overview` returns actual host values
//! (asserted within sane ranges only, never exact).

use serde_json::json;

use crate::common::*;

async fn authed_server() -> (TestServer, String) {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    (server, token)
}

#[tokio::test]
async fn overview_returns_current_snapshot() {
    let (server, token) = authed_server().await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/monitoring/overview", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "body: {}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["timestamp"].as_str().is_some(), "timestamp present");
    let cpu = body["cpu"].as_f64().unwrap();
    let memory = body["memory"].as_f64().unwrap();
    assert!(
        (0.0..=100.0).contains(&cpu),
        "cpu {} must be in [0,100]",
        cpu
    );
    assert!(
        (0.0..=100.0).contains(&memory),
        "memory {} must be in [0,100]",
        memory
    );
    assert!(body["disk"].is_array());
    assert!(body["network"].is_array());
}

#[tokio::test]
async fn history_returns_array_even_when_empty() {
    let (server, token) = authed_server().await;
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/monitoring/history?metric=Cpu&range=3600",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.is_array(), "history must return a JSON array");
}

#[tokio::test]
async fn history_rejects_invalid_metric() {
    let (server, token) = authed_server().await;
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/monitoring/history?metric=Bogus",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("invalid_metric"));
}

#[tokio::test]
async fn routes_require_auth() {
    let server = TestServer::new().await;
    for path in [
        "/api/v1/monitoring/overview",
        "/api/v1/monitoring/history?metric=Cpu",
        "/api/v1/monitoring/alerts",
    ] {
        let resp = server
            .client()
            .get(format!("{}{path}", server.base_url()))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401, "{path} must require auth");
    }
}

#[tokio::test]
async fn alerts_returns_array() {
    let (server, token) = authed_server().await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/monitoring/alerts", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.is_array(), "alerts must return a JSON array");
}
