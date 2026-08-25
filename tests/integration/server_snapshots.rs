//! Whole-server snapshot integration tests: create from a verified
//! backup run, preflight with single-use confirmation, and restore
//! delegation.

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn owner_and_run(server: &TestServer) -> (String, String) {
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let created = server
        .client()
        .post(format!("{}/api/v1/backups/plans", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "name": "snapshot source",
            "resources": [{"kind": "panel_metadata"}],
            "schedule": "0 2 * * *",
            "timezone": "UTC",
            "retention_copies": 3
        }))
        .send()
        .await
        .expect("create plan");
    assert_eq!(created.status(), 201);
    let plan: serde_json::Value = created.json().await.expect("plan json");
    let plan_id = plan["id"].as_str().expect("plan id").to_owned();
    let run = server
        .client()
        .post(format!(
            "{}/api/v1/backups/plans/{plan_id}/run",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("run");
    assert_eq!(run.status(), 202);
    let run: serde_json::Value = run.json().await.expect("run json");
    (token, run["id"].as_str().expect("run id").to_owned())
}

#[tokio::test]
async fn snapshot_routes_require_authentication() {
    let server = TestServer::new().await;
    for path in [
        "/api/v1/server/snapshots".to_string(),
        format!(
            "/api/v1/server/snapshots/{}/preflight",
            uuid::Uuid::new_v4()
        ),
    ] {
        assert_eq!(
            server
                .client()
                .post(format!("{}{}", server.base_url(), path))
                .send()
                .await
                .expect("unauth")
                .status(),
            401
        );
    }
}

#[tokio::test]
async fn snapshot_create_preflight_and_single_use_restore() {
    let server = TestServer::new().await;
    let (token, run_id) = owner_and_run(&server).await;
    let auth = bearer(&token);
    let base = format!("{}/api/v1/server/snapshots", server.base_url());

    // Create a snapshot from the verified run.
    let created = server
        .client()
        .post(&base)
        .header("authorization", &auth)
        .json(&serde_json::json!({ "run_id": run_id }))
        .send()
        .await
        .expect("create snapshot");
    assert_eq!(
        created.status(),
        200,
        "{}",
        created.text().await.expect("body")
    );
    let manifest: serde_json::Value = created.json().await.expect("manifest json");
    assert_eq!(manifest["manifest_schema"], 1);
    assert!(manifest["entries"].as_array().expect("entries").len() >= 2);

    // Unknown snapshot id → 404.
    assert_eq!(
        server
            .client()
            .get(format!("{base}/{}", uuid::Uuid::new_v4()))
            .header("authorization", &auth)
            .send()
            .await
            .expect("unknown")
            .status(),
        404
    );

    // Preflight mints a confirmation token and reports readiness.
    let listed = server
        .client()
        .get(&base)
        .header("authorization", &auth)
        .send()
        .await
        .expect("list snapshots")
        .json::<serde_json::Value>()
        .await
        .expect("list json");
    let snapshot_id = listed["snapshots"][0]["id"]
        .as_str()
        .expect("snapshot id")
        .to_owned();

    let preflight = server
        .client()
        .post(format!("{base}/{snapshot_id}/preflight"))
        .header("authorization", &auth)
        .send()
        .await
        .expect("preflight");
    assert_eq!(
        preflight.status(),
        200,
        "{}",
        preflight.text().await.expect("body")
    );
    let body: serde_json::Value = preflight.json().await.expect("preflight json");
    let confirm_token = body["confirm_token"].as_str().expect("token").to_owned();
    assert_eq!(body["report"]["ready"], true);

    // Restore without the token is refused.
    assert_eq!(
        server
            .client()
            .post(format!("{base}/{snapshot_id}/restore"))
            .header("authorization", &auth)
            .json(&serde_json::json!({ "confirm_token": "", "overwrite": false }))
            .send()
            .await
            .expect("no token")
            .status(),
        422
    );

    // First restore with the token succeeds.
    let restored = server
        .client()
        .post(format!("{base}/{snapshot_id}/restore"))
        .header("authorization", &auth)
        .json(&serde_json::json!({ "confirm_token": confirm_token, "overwrite": false }))
        .send()
        .await
        .expect("restore");
    assert_eq!(
        restored.status(),
        200,
        "{}",
        restored.text().await.expect("body")
    );

    // Token replay is refused.
    assert_eq!(
        server
            .client()
            .post(format!("{base}/{snapshot_id}/restore"))
            .header("authorization", &auth)
            .json(&serde_json::json!({ "confirm_token": confirm_token, "overwrite": false }))
            .send()
            .await
            .expect("replay")
            .status(),
        422
    );
}
