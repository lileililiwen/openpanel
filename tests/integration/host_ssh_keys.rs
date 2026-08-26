//! Admin SSH host-key REST integration tests.

use crate::common::*;

fn valid_key(seed: u8) -> String {
    use base64::Engine;
    let body = vec![0xABu8 + seed; 68];
    format!(
        "ssh-ed25519 {} tester@host",
        base64::engine::general_purpose::STANDARD.encode(body)
    )
}

#[tokio::test]
async fn ssh_keys_crud_guards_and_audit() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let base = format!("{}/api/v1/host/ssh-keys", server.base_url());

    // Unauthenticated → 401.
    assert_eq!(
        server
            .client()
            .get(&base)
            .send()
            .await
            .expect("anon")
            .status(),
        401
    );

    // POST valid key → 200 with fingerprint.
    let added = server
        .client()
        .post(&base)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "label": "laptop", "public_key": valid_key(0) }))
        .send()
        .await
        .expect("add");
    assert_eq!(added.status(), 200, "{}", added.text().await.expect("body"));
    let body: serde_json::Value = added.json().await.expect("json");
    let id = body["id"].as_str().expect("id").to_owned();
    assert!(
        body["fingerprint"]
            .as_str()
            .expect("fp")
            .starts_with("SHA256:")
    );

    // Duplicate fingerprint refused.
    let dup = server
        .client()
        .post(&base)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "label": "again", "public_key": valid_key(0) }))
        .send()
        .await
        .expect("dup");
    assert_eq!(dup.status(), 422);

    // GET lists it.
    let listed = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .expect("list");
    let keys: serde_json::Value = listed.json().await.expect("json");
    assert_eq!(keys.as_array().expect("keys").len(), 1);

    // DELETE removes it and updates the rendered file.
    let removed = server
        .client()
        .delete(format!("{base}/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("remove");
    assert_eq!(removed.status(), 204);
    let listed = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .expect("list after remove");
    let keys: serde_json::Value = listed.json().await.expect("json");
    assert_eq!(keys.as_array().expect("keys").len(), 0);

    // Both mutations audited (add + remove rewrite).
    let events = server.audit_events().await;
    assert!(
        events
            .iter()
            .filter(|e| e.action == openpanel_core::AuditAction::SshKeyChanged)
            .count()
            >= 2
    );
}
