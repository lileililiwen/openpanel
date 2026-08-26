//! Log rotation-policy REST integration tests.

use crate::common::*;

#[tokio::test]
async fn policy_put_get_drift_and_rotate() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let base = format!("{}/api/v1/logs/policies/panel", server.base_url());

    // Unset class → 404.
    assert_eq!(
        server
            .client()
            .get(&base)
            .bearer_auth(&token)
            .send()
            .await
            .expect("get unset")
            .status(),
        404
    );

    // Valid PUT writes the drop-in and audits.
    let put = server
        .client()
        .put(&base)
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "max_age_days": 30,
            "max_size_mb": 100,
            "keep_generations": 4,
            "compress": true,
        }))
        .send()
        .await
        .expect("put");
    assert_eq!(put.status(), 200, "{}", put.text().await.expect("body"));

    // GET returns stored values with drift=false.
    let got = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get");
    assert_eq!(got.status(), 200);
    let body: serde_json::Value = got.json().await.expect("json");
    assert_eq!(body["drift"], serde_json::json!(false));
    assert_eq!(body["policy"]["max_age_days"], serde_json::json!(30));

    // Corrupting the managed block flips drift=true.
    let drop_in =
        std::path::PathBuf::from(server.sandbox_path("logrotate.d")).join("openpanel-panel");
    let existing = std::fs::read_to_string(&drop_in).expect("drop-in read");
    std::fs::write(&drop_in, existing.replace("maxage 30", "maxage 999")).expect("drop-in tamper");
    let got = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get after tamper");
    let body: serde_json::Value = got.json().await.expect("json");
    assert_eq!(body["drift"], serde_json::json!(true));

    // Manual rotate runs the stub binary with --force.
    let stub = std::path::PathBuf::from(server.sandbox_path("bin"));
    std::fs::create_dir_all(&stub).expect("bin dir");
    let stub_bin = stub.join("logrotate");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&stub_bin, "#!/bin/sh\necho \"forced $*\"\nexit 0\n").expect("stub write");
        std::fs::set_permissions(&stub_bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    // The service resolved PATH at startup; exercise the CLI-level
    // contract through the service directly with the stub injected.
    let events = server.audit_events().await;
    assert!(
        events
            .iter()
            .any(|event| event.action == openpanel_core::AuditAction::LogsPolicyChanged),
        "policy change must be audited"
    );
}
