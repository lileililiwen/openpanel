//! Database remote-access REST integration tests (mocked grant port).

use crate::common::*;

#[tokio::test]
async fn remote_access_put_disable_wildcard_and_audit() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let base = format!(
        "{}/api/v1/databases/{}/remote-access",
        server.base_url(),
        uuid::Uuid::new_v4()
    );

    // Wildcard without opt-in → 422 global_access_locked.
    let locked = server
        .client()
        .put(&base)
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "user": "wp1",
            "database": "wp1db",
            "enabled": true,
            "allow_cidrs": ["0.0.0.0/0"],
            "wildcard_opt_in": false,
        }))
        .send()
        .await
        .expect("locked put");
    assert_eq!(locked.status(), 422);
    let body: serde_json::Value = locked.json().await.expect("json");
    assert_eq!(body["error"], serde_json::json!("global_access_locked"));

    // Valid PUT applies the derived pattern and audits.
    let put = server
        .client()
        .put(&base)
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "user": "wp1",
            "database": "wp1db",
            "enabled": true,
            "allow_cidrs": ["203.0.113.0/24"],
            "wildcard_opt_in": false,
        }))
        .send()
        .await
        .expect("put");
    assert_eq!(put.status(), 200, "{}", put.text().await.expect("body"));
    let body: serde_json::Value = put.json().await.expect("json");
    assert_eq!(body["enabled"], serde_json::json!(true));
    assert_eq!(body["allow_cidrs"], serde_json::json!(["203.0.113.0/24"]));

    // The grant port received exactly one create for the pattern.
    let port = server.db_grant_port();
    assert_eq!(port.recorded(), vec!["create:203.0.113.%".to_string()]);

    // GET never contains password material.
    let got = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get");
    assert_eq!(got.status(), 200);
    let text = got.text().await.expect("body");
    assert!(!text.to_lowercase().contains("password"));
    assert!(!text.contains("GRANT"));

    // Disabling sends DROP for the applied pattern.
    let disabled = server
        .client()
        .put(&base)
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "user": "wp1",
            "database": "wp1db",
            "enabled": false,
            "allow_cidrs": [],
            "wildcard_opt_in": false,
        }))
        .send()
        .await
        .expect("disable");
    assert_eq!(disabled.status(), 200);
    assert_eq!(
        port.recorded(),
        vec![
            "create:203.0.113.%".to_string(),
            "drop:203.0.113.%".to_string()
        ]
    );

    // Audit trail records the change.
    let events = server.audit_events().await;
    assert!(
        events
            .iter()
            .any(|event| event.action == openpanel_core::AuditAction::DbRemoteAccessChanged),
        "remote-access change must be audited"
    );
}
