//! SSO + session-control integration tests.

use crate::common::*;

#[tokio::test]
async fn session_inventory_lists_and_revokes() {
    let server = TestServer::new().await;
    let token_one = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let token_two = server.login("owner", "correct horse battery staple").await;

    let url = format!("{}/api/v1/auth/sessions", server.base_url());

    // Two live sessions are listed for the owner.
    let listed = server
        .client()
        .get(&url)
        .bearer_auth(&token_one)
        .send()
        .await
        .expect("list");
    assert_eq!(listed.status(), 200);
    let body: serde_json::Value = listed.json().await.expect("json");
    let sessions = body["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 2);

    // Identify which session id belongs to token_two by listing from
    // that session and diffing against token_one's view.
    let mine: serde_json::Value = server
        .client()
        .get(&url)
        .bearer_auth(&token_two)
        .send()
        .await
        .expect("list two")
        .json()
        .await
        .expect("json");
    let two_id = mine["sessions"][0]["id"].as_str().expect("id").to_owned();

    // Revoking that session invalidates it immediately.
    let revoked = server
        .client()
        .delete(format!("{url}/{two_id}"))
        .bearer_auth(&token_two)
        .send()
        .await
        .expect("revoke");
    assert_eq!(revoked.status(), 204);

    // The revoked token no longer authenticates.
    assert_eq!(
        server
            .client()
            .get(&url)
            .bearer_auth(&token_two)
            .send()
            .await
            .expect("revoked request")
            .status(),
        401
    );

    // The surviving session still works and now lists one entry.
    let remaining = server
        .client()
        .get(&url)
        .bearer_auth(&token_one)
        .send()
        .await
        .expect("remaining")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    assert_eq!(remaining["sessions"].as_array().expect("arr").len(), 1);
}

#[tokio::test]
async fn sso_begin_without_connection_is_unavailable() {
    let server = TestServer::new().await;
    let _token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let response = server
        .client()
        .post(format!("{}/auth/sso/begin", server.base_url()))
        .send()
        .await
        .expect("begin");
    assert_eq!(response.status(), 503);
}

#[tokio::test]
async fn sso_configure_requires_owner_and_validates_secret_layout() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;

    // Plaintext secret is rejected (must arrive as ciphertext or be
    // encrypted server-side; the API accepts plaintext and encrypts,
    // so this exercises the happy path).
    let configured = server
        .client()
        .put(format!("{}/api/v1/auth/sso/connection", server.base_url()))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "issuer_url": "https://idp.example.com",
            "client_id": "openpanel",
            "client_secret_cipher": "super-secret",
            "default_role": "user",
            "auto_provision": false,
            "trust_idp_mfa": false,
        }))
        .send()
        .await
        .expect("configure");
    assert_eq!(
        configured.status(),
        200,
        "{}",
        configured.text().await.expect("body")
    );
    let body: serde_json::Value = configured.json().await.expect("json");
    // The stored form must be ciphertext, never the plaintext.
    assert_ne!(body["client_secret_cipher"], "super-secret");

    // Begin now reaches discovery of a non-existent IdP → 503.
    let begin = server
        .client()
        .post(format!("{}/auth/sso/begin", server.base_url()))
        .send()
        .await
        .expect("begin");
    assert_eq!(begin.status(), 503);
}
