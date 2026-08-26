//! Runtime environment REST integration tests.

use crate::common::*;

#[tokio::test]
async fn runtime_env_secret_masking_and_audit() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let base = format!(
        "{}/api/v1/runtimes/{}/env",
        server.base_url(),
        uuid::Uuid::new_v4()
    );

    // PUT a set with one secret and one plain var.
    let put = server
        .client()
        .put(&base)
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "vars": [
                { "key": "DATABASE_URL", "value": "postgres://s3cret", "secret": true },
                { "key": "LOG_LEVEL", "value": "debug", "secret": false }
            ]
        }))
        .send()
        .await
        .expect("put");
    assert_eq!(put.status(), 204, "{}", put.text().await.expect("body"));

    // GET returns the secret's key + secret flag but no value; the
    // non-secret value round-trips verbatim.
    let got = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get");
    assert_eq!(got.status(), 200);
    let body: serde_json::Value = got.json().await.expect("json");
    assert_eq!(body.as_array().expect("vars").len(), 2);
    for var in body.as_array().unwrap() {
        match var["key"].as_str().unwrap() {
            "DATABASE_URL" => {
                assert_eq!(var["secret"], serde_json::json!(true));
                assert!(var.get("value").is_none(), "secret value leaked");
            }
            "LOG_LEVEL" => {
                assert_eq!(var["secret"], serde_json::json!(false));
                assert_eq!(var["value"], serde_json::json!("debug"));
            }
            other => panic!("unexpected key {other}"),
        }
    }

    // The audit event lists changed keys only — never values.
    let events = server.audit_events().await;
    let audit = events
        .iter()
        .find(|event| {
            event.action == openpanel_core::AuditAction::RuntimeChanged
                && event.metadata["changed_keys"].is_array()
        })
        .expect("runtime env audit");
    let transcript = serde_json::to_string(&audit.metadata).expect("metadata");
    assert!(!transcript.contains("s3cret"));
}
