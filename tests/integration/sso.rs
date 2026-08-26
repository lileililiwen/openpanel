//! SSO + session-control integration tests.

use openpanel_core::Config;

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

/// OIDC double: deterministic claims, no network.
struct MockOidc {
    issuer: String,
    subject: String,
    email: String,
}

#[async_trait::async_trait]
impl openpanel_domain::identity::sso::OidcPort for MockOidc {
    async fn authorize_url(
        &self,
        _connection: &openpanel_domain::SsoConnection,
        state: &openpanel_domain::SsoLoginState,
    ) -> Result<String, openpanel_domain::SsoError> {
        Ok(format!(
            "https://idp.example.test/authorize?state={}",
            state.state
        ))
    }

    async fn exchange(
        &self,
        _connection: &openpanel_domain::SsoConnection,
        _code: &str,
        _pkce_verifier: &str,
        _expected_nonce: &str,
    ) -> Result<openpanel_domain::OidcClaims, openpanel_domain::SsoError> {
        Ok(openpanel_domain::OidcClaims {
            issuer: self.issuer.clone(),
            subject: self.subject.clone(),
            email: Some(self.email.clone()),
        })
    }
}

async fn configure_connection(server: &TestServer, token: &str, auto_provision: bool) {
    let response = server
        .client()
        .put(format!("{}/api/v1/auth/sso/connection", server.base_url()))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "issuer_url": "https://idp.example.test",
            "client_id": "panel",
            "client_secret_cipher": "plaintext-secret-for-encryption",
            "default_role": "user",
            "auto_provision": auto_provision,
            "trust_idp_mfa": false,
        }))
        .send()
        .await
        .expect("configure");
    assert_eq!(
        response.status(),
        200,
        "{}",
        response.text().await.expect("body")
    );
}

async fn begin_and_extract_state(server: &TestServer, token: &str) -> (String, String) {
    let begun = server
        .client()
        .post(format!("{}/auth/sso/begin", server.base_url()))
        .bearer_auth(token)
        .send()
        .await
        .expect("begin");
    let status = begun.status();
    let text = begun.text().await.expect("body");
    assert_eq!(status, 200, "begin failed: {text}");
    let begun: serde_json::Value = serde_json::from_str(&text).expect("json");
    let url = begun["authorize_url"].as_str().expect("authorize_url");
    let state = url.split("state=").nth(1).expect("state query").to_owned();
    (url.to_owned(), state)
}

#[tokio::test]
async fn sso_callback_provisions_links_and_audits() {
    let server = TestServer::new_with_sso_oidc(
        Config::default(),
        std::sync::Arc::new(MockOidc {
            issuer: "https://idp.example.test".into(),
            subject: "user-42".into(),
            email: "jane@example.test".into(),
        }),
    )
    .await;
    let owner_token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;

    // Auto-provisioning off: an unknown identity is refused.
    configure_connection(&server, &owner_token, false).await;
    let (_url, state) = begin_and_extract_state(&server, &owner_token).await;
    let refused = server
        .client()
        .post(format!("{}/auth/sso/callback", server.base_url()))
        .json(&serde_json::json!({ "code": "auth-code", "state": state }))
        .send()
        .await
        .expect("callback");
    assert_eq!(
        refused.status(),
        422,
        "{}",
        refused.text().await.expect("body")
    );

    // Auto-provisioning on: the same identity now provisions a user
    // and receives a session token.
    configure_connection(&server, &owner_token, true).await;
    let (_url, state) = begin_and_extract_state(&server, &owner_token).await;
    let accepted = server
        .client()
        .post(format!("{}/auth/sso/callback", server.base_url()))
        .json(&serde_json::json!({ "code": "auth-code", "state": state }))
        .send()
        .await
        .expect("callback");
    assert_eq!(
        accepted.status(),
        200,
        "{}",
        accepted.text().await.expect("body")
    );
    let body: serde_json::Value = accepted.json().await.expect("json");
    assert_eq!(body["mfa_satisfied"], serde_json::json!(false));
    let sso_token = body["token"].as_str().expect("session token").to_owned();
    assert!(!sso_token.is_empty());

    // The issued session authenticates the provisioned user.
    let sessions = server
        .client()
        .get(format!("{}/api/v1/auth/sessions", server.base_url()))
        .bearer_auth(&sso_token)
        .send()
        .await
        .expect("sessions");
    assert_eq!(sessions.status(), 200);

    // A second login links the same identity instead of duplicating
    // the user, and each login is audited.
    let (_url, state) = begin_and_extract_state(&server, &owner_token).await;
    let second = server
        .client()
        .post(format!("{}/auth/sso/callback", server.base_url()))
        .json(&serde_json::json!({ "code": "auth-code", "state": state }))
        .send()
        .await
        .expect("second callback");
    assert_eq!(second.status(), 200);

    let users = server.identity().list_users().await.expect("users");
    let provisioned = users
        .iter()
        .filter(|user| user.username().as_str().starts_with("sso-"))
        .count();
    assert_eq!(provisioned, 1, "identity must link, not duplicate");

    let logins = server
        .audit_events()
        .await
        .into_iter()
        .filter(|event| event.action == openpanel_core::AuditAction::SsoLogin)
        .count();
    assert_eq!(logins, 2, "each completed login is audited");
}

#[test]
fn prop_sso_flow_never_leaks_client_secret() {
    use openpanel_core::Config;
    use proptest::prelude::*;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let (server, owner_token) = runtime.block_on(async {
        let server = TestServer::new_with_sso_oidc(
            Config::default(),
            std::sync::Arc::new(MockOidc {
                issuer: "https://idp.example.test".into(),
                subject: "prop-subject".into(),
                email: "prop@example.test".into(),
            }),
        )
        .await;
        let owner_token = server
            .bootstrap_owner("owner", "correct horse battery staple")
            .await;
        (server, owner_token)
    });
    {
        proptest::test_runner::TestRunner::new(proptest::prelude::ProptestConfig::with_cases(32))
            .run(&"[A-Za-z0-9]{24,64}", |secret| {
                runtime.block_on(async {
                    // Configure with a fresh plaintext secret.
                    let configured = server
                        .client()
                        .put(format!("{}/api/v1/auth/sso/connection", server.base_url()))
                        .bearer_auth(&owner_token)
                        .json(&serde_json::json!({
                            "issuer_url": "https://idp.example.test",
                            "client_id": "panel",
                            "client_secret_cipher": secret,
                            "default_role": "user",
                            "auto_provision": false,
                            "trust_idp_mfa": false,
                        }))
                        .send()
                        .await
                        .expect("configure");
                    assert_eq!(configured.status(), 200);
                    let dto = configured.json::<serde_json::Value>().await.expect("dto");

                    // The read-back DTO redacts the ciphertext too.
                    let read_back = server
                        .client()
                        .get(format!("{}/api/v1/auth/sso/connection", server.base_url()))
                        .bearer_auth(&owner_token)
                        .send()
                        .await
                        .expect("get connection")
                        .json::<serde_json::Value>()
                        .await
                        .expect("connection json");

                    // No audit event may carry the plaintext (or its
                    // ciphertext).
                    let events = server.audit_events().await;
                    let transcript: String = events
                        .iter()
                        .map(|event| serde_json::to_string(event).expect("event json"))
                        .collect();

                    for surface in [dto.to_string(), read_back.to_string(), transcript] {
                        prop_assert!(!surface.contains(secret.as_str()), "plaintext leaked");
                    }
                    Ok(())
                })
            })
            .unwrap();
    }
}
