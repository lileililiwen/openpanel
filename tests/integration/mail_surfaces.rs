//! Mail end-user surface integration tests: Sieve filters,
//! autoresponders, forwarders, catch-all, mailing lists, and the
//! outbound-queue snapshot.

use crate::common::*;

async fn owner_token(server: &TestServer) -> String {
    server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await
}

#[tokio::test]
async fn mail_sieve_filter_round_trip_and_oversize_rejection() {
    let server = TestServer::new().await;
    let token = owner_token(&server).await;
    let mailbox_id = uuid::Uuid::new_v4();
    let url = format!(
        "{}/api/v1/mail/mailboxes/{mailbox_id}/filters",
        server.base_url()
    );

    // No script yet.
    let initial = server
        .client()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get initial");
    assert_eq!(initial.status(), 200);
    assert_eq!(
        initial.json::<serde_json::Value>().await.expect("json")["script"],
        serde_json::Value::Null
    );

    // Valid script round-trips.
    let script = r#"if header :contains "subject" "spam" { fileinto "Spam"; }"#;
    let saved = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "script": script }))
        .send()
        .await
        .expect("put filters");
    assert_eq!(saved.status(), 200, "{}", saved.text().await.expect("body"));
    let loaded = server
        .client()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get saved")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    assert_eq!(loaded["script"], serde_json::json!(script));

    // Oversize scripts are rejected with 422.
    let big = "x".repeat(17 * 1024);
    let rejected = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "script": big }))
        .send()
        .await
        .expect("oversize");
    assert_eq!(rejected.status(), 422);

    // Unbalanced braces fail the compile gate.
    let broken = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "script": "if true { fileinto \"INBOX\";" }))
        .send()
        .await
        .expect("broken");
    assert_eq!(broken.status(), 422);
}

#[tokio::test]
async fn mail_autoresponder_window_validation_and_disable() {
    let server = TestServer::new().await;
    let token = owner_token(&server).await;
    let mailbox_id = uuid::Uuid::new_v4();
    let url = format!(
        "{}/api/v1/mail/mailboxes/{mailbox_id}/autoresponder",
        server.base_url()
    );
    let now = chrono::Utc::now();

    // Inverted window is rejected.
    let inverted = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "enabled": true,
            "body": "Out of office",
            "mode": "once",
            "window_start": (now + chrono::Duration::days(7)).to_rfc3339(),
            "window_end": now.to_rfc3339(),
        }))
        .send()
        .await
        .expect("inverted");
    assert_eq!(inverted.status(), 422);

    // Valid window is stored and readable.
    let saved = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "enabled": true,
            "body": "Out of office",
            "mode": "once",
            "window_start": now.to_rfc3339(),
            "window_end": (now + chrono::Duration::days(7)).to_rfc3339(),
        }))
        .send()
        .await
        .expect("save");
    assert_eq!(saved.status(), 200, "{}", saved.text().await.expect("body"));

    // Disable clears the enabled flag.
    let disabled = server
        .client()
        .delete(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("disable");
    assert_eq!(disabled.status(), 204);
}

#[tokio::test]
async fn mail_forwarder_loop_is_rejected_and_valid_round_trips() {
    let server = TestServer::new().await;
    let token = owner_token(&server).await;
    let mailbox_id = uuid::Uuid::new_v4();
    let base = format!(
        "{}/api/v1/mail/mailboxes/{mailbox_id}/forwarders",
        server.base_url()
    );

    // A self-loop is refused.
    let looping = server
        .client()
        .post(format!("{base}?source=alice@example.test"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "destination": "alice@example.test", "keep_local": true }))
        .send()
        .await
        .expect("looping");
    assert_eq!(looping.status(), 422);

    // A valid forwarder is listed after creation.
    let saved = server
        .client()
        .post(format!("{base}?source=alice@example.test"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "destination": "bob@example.net", "keep_local": false }))
        .send()
        .await
        .expect("add");
    assert_eq!(saved.status(), 200, "{}", saved.text().await.expect("body"));
    let listed = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .expect("list")
        .json::<serde_json::Value>()
        .await
        .expect("list json");
    assert_eq!(
        listed[0]["destination"],
        serde_json::json!("bob@example.net")
    );
}

#[tokio::test]
async fn mail_lists_and_catchall_surfaces() {
    let server = TestServer::new().await;
    let token = owner_token(&server).await;
    let member = uuid::Uuid::new_v4();

    // Upsert then read back a list.
    let upsert = server
        .client()
        .post(format!("{}/api/v1/mail/lists", server.base_url()))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "address": "team@example.test",
            "members": [member],
        }))
        .send()
        .await
        .expect("upsert");
    assert_eq!(
        upsert.status(),
        200,
        "{}",
        upsert.text().await.expect("body")
    );

    let listed = server
        .client()
        .get(format!("{}/api/v1/mail/lists", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list lists")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    assert_eq!(listed[0]["address"], serde_json::json!("team@example.test"));
    assert_eq!(
        listed[0]["members"],
        serde_json::json!([member.to_string()])
    );

    // Catch-all stores and reads back.
    let catchall_url = format!("{}/api/v1/mail/catchall/example.test", server.base_url());
    let put = server
        .client()
        .put(&catchall_url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "destination_mailbox": member }))
        .send()
        .await
        .expect("put catchall");
    assert_eq!(put.status(), 200, "{}", put.text().await.expect("body"));
    let got = server
        .client()
        .get(&catchall_url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get catchall")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    assert_eq!(
        got["destination_mailbox"],
        serde_json::json!(member.to_string())
    );
}

#[tokio::test]
async fn mail_queue_endpoint_degrades_without_mta() {
    let server = TestServer::new().await;
    let token = owner_token(&server).await;
    let response = server
        .client()
        .get(format!("{}/api/v1/mail/queue", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .expect("queue");
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.expect("json");
    // In the sandbox there is no postqueue; the endpoint must still be
    // a well-formed snapshot (either real data or degraded unknown).
    assert!(body["queue_depth"].is_u64());
    assert!(
        body["health"] == "ok" || body["health"] == "unknown",
        "unexpected health {}",
        body["health"]
    );
}
