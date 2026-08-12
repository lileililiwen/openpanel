use serde_json::json;

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// Boot a server with an owner user and return `(server, token)`.
async fn owner_server() -> (TestServer, String) {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    (server, token)
}

#[tokio::test]
async fn identity_login_success() {
    let server = TestServer::new().await;
    server
        .identity()
        .create_user(
            "admin",
            "admin@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::Owner,
            "test",
        )
        .await
        .unwrap();

    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .json(&json!({
            "username_or_email": "admin",
            "password": "correct horse battery staple",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["token"].as_str().unwrap().len() > 20);
    assert_eq!(body["user"]["username"], "admin");
    assert!(body["expires_at"].is_string());
}

#[tokio::test]
async fn identity_login_bad_password() {
    let server = TestServer::new().await;
    server
        .identity()
        .create_user(
            "admin",
            "admin@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::Owner,
            "test",
        )
        .await
        .unwrap();

    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .json(&json!({
            "username_or_email": "admin",
            "password": "wrong password",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "invalid_credentials");
}

#[tokio::test]
async fn identity_login_unknown_user() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .json(&json!({
            "username_or_email": "ghost",
            "password": "correct horse battery staple",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn identity_login_disabled_user() {
    let server = TestServer::new().await;
    let user = server
        .identity()
        .create_user(
            "admin",
            "admin@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::Owner,
            "test",
        )
        .await
        .unwrap();
    server
        .identity()
        .disable_user(user.id(), "test")
        .await
        .unwrap();

    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .json(&json!({
            "username_or_email": "admin",
            "password": "correct horse battery staple",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "account_disabled");
}

#[tokio::test]
async fn identity_me_with_token() {
    let (server, token) = owner_server().await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/identity/me", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["username"], "admin");
}

#[tokio::test]
async fn identity_me_without_token() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/identity/me", server.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "unauthorized");
}

#[tokio::test]
async fn identity_admin_create_user() {
    let (server, token) = owner_server().await;
    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/users", server.base_url()))
        .bearer_auth(&token)
        .json(&json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "correct horse battery staple",
            "role": "user",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["username"], "alice");
}

#[tokio::test]
async fn identity_list_users() {
    let (server, token) = owner_server().await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/identity/users", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["username"], "admin");
}

/// An Admin (non-Owner) attempting to create a user MUST be rejected
/// with 403. Only the Owner role can manage users — enforced by the
/// `RequireOwner` extractor on the route.
#[tokio::test]
async fn identity_admin_create_user_is_forbidden() {
    let server = TestServer::new().await;
    // Bootstrap the Owner.
    server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    // Create an Admin user directly via the service, then login as them.
    server
        .identity()
        .create_user(
            "admin",
            "admin@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::Admin,
            "owner",
        )
        .await
        .unwrap();

    let admin_token = login_token(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/users", server.base_url()))
        .bearer_auth(&admin_token)
        .json(&json!({
            "username": "bob",
            "email": "bob@example.com",
            "password": "correct horse battery staple",
            "role": "user",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "forbidden");
}

async fn login_token(server: &TestServer, username: &str, password: &str) -> String {
    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .json(&json!({
            "username_or_email": username,
            "password": password,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "login as {username} should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn identity_two_factor_login_requires_factor_then_succeeds() {
    use openpanel_app::identity::two_factor::FactorResponse;

    let server = TestServer::new().await;
    server
        .identity()
        .create_user(
            "alice",
            "alice@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::Owner,
            "test",
        )
        .await
        .expect("create alice");

    // Enroll TOTP for alice via the service so she has an active factor.
    let two_factor = server.software_center(); // placeholder; correct accessor below
    let _ = two_factor;
    let identity_svc = server.identity();
    let user_id = identity_svc
        .users()
        .find_by_username("alice")
        .await
        .expect("find user")
        .expect("alice exists")
        .id();
    let enrollment = identity_svc
        .two_factor()
        .enroll_totp(
            user_id,
            "test",
            "OpenPanel",
            "alice@example.com",
            chrono::Utc::now(),
        )
        .await
        .expect("enroll totp");

    // Step 1: password-only login returns factor_required.
    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .json(&json!({
            "username_or_email": "alice",
            "password": "correct horse battery staple",
        }))
        .send()
        .await
        .expect("password login");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("factor_required body");
    assert_eq!(body["status"], "factor_required");
    let challenge_id = body["challenge_id"]
        .as_str()
        .expect("challenge_id")
        .to_string();
    assert!(body["expires_at"].is_string());

    // The challenge token isn't returned by the API (it lives in the
    // cookie / web flow). For the JSON API we read it back via the
    // service: re-issue a fresh challenge so we have its plaintext.
    let challenge = identity_svc
        .two_factor()
        .issue_login_challenge(
            user_id,
            Some("127.0.0.1".into()),
            Some("integration-test".into()),
            chrono::Utc::now(),
        )
        .await
        .expect("issue challenge");
    let _ = (challenge_id, &enrollment.secret); // silence unused

    // Compute the current TOTP code from the enrolled secret.
    let now = chrono::Utc::now();
    let step = enrollment.secret.current_step(now);
    let step_u = u64::try_from(step).expect("step");
    let code = enrollment
        .secret
        .totp("OpenPanel", "alice@example.com")
        .generate(step_u);
    let _ = &identity_svc; // silence

    // Step 2: verify the factor and assert we get a session token.
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/identity/login/factor",
            server.base_url()
        ))
        .json(&json!({
            "challenge_id": challenge.challenge_id,
            "challenge_token": challenge.challenge_token,
            "kind": "totp",
            "code": code,
        }))
        .send()
        .await
        .expect("factor login");
    assert_eq!(resp.status(), 200, "factor login should succeed");
    let body: serde_json::Value = resp.json().await.expect("login body");
    assert!(body["token"].as_str().expect("token").len() > 20);

    // Step 3: replaying the same code on a fresh challenge fails (replay protection).
    let challenge2 = identity_svc
        .two_factor()
        .issue_login_challenge(
            user_id,
            Some("127.0.0.1".into()),
            Some("integration-test".into()),
            chrono::Utc::now(),
        )
        .await
        .expect("second challenge");
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/identity/login/factor",
            server.base_url()
        ))
        .json(&json!({
            "challenge_id": challenge2.challenge_id,
            "challenge_token": challenge2.challenge_token,
            "kind": "totp",
            "code": code,
        }))
        .send()
        .await
        .expect("replay");
    assert_eq!(
        resp.status(),
        401,
        "replaying an already-used TOTP code must fail"
    );
    let _ = FactorResponse::Totp(String::new()); // silence unused import
}

#[tokio::test]
async fn identity_two_factor_recovery_code_round_trip() {
    let server = TestServer::new().await;
    server
        .identity()
        .create_user(
            "bob",
            "bob@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::Owner,
            "test",
        )
        .await
        .expect("create bob");
    let identity_svc = server.identity();
    let user_id = identity_svc
        .users()
        .find_by_username("bob")
        .await
        .expect("find")
        .expect("bob exists")
        .id();
    let enrollment = identity_svc
        .two_factor()
        .enroll_totp(
            user_id,
            "test",
            "OpenPanel",
            "bob@example.com",
            chrono::Utc::now(),
        )
        .await
        .expect("enroll bob");
    assert_eq!(enrollment.recovery_codes.len(), 10);

    // Login → factor_required → consume one recovery code.
    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .json(&json!({
            "username_or_email": "bob",
            "password": "correct horse battery staple",
        }))
        .send()
        .await
        .expect("password login");
    assert_eq!(resp.status(), 200);
    let challenge = identity_svc
        .two_factor()
        .issue_login_challenge(
            user_id,
            Some("127.0.0.1".into()),
            Some("integration-test".into()),
            chrono::Utc::now(),
        )
        .await
        .expect("challenge");
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/identity/login/factor",
            server.base_url()
        ))
        .json(&json!({
            "challenge_id": challenge.challenge_id,
            "challenge_token": challenge.challenge_token,
            "kind": "recovery",
            "code": enrollment.recovery_codes[0],
        }))
        .send()
        .await
        .expect("recovery login");
    assert_eq!(resp.status(), 200, "recovery code should log bob in");

    // A second consume of the same recovery code on a fresh challenge fails.
    let challenge2 = identity_svc
        .two_factor()
        .issue_login_challenge(
            user_id,
            Some("127.0.0.1".into()),
            Some("integration-test".into()),
            chrono::Utc::now(),
        )
        .await
        .expect("second challenge");
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/identity/login/factor",
            server.base_url()
        ))
        .json(&json!({
            "challenge_id": challenge2.challenge_id,
            "challenge_token": challenge2.challenge_token,
            "kind": "recovery",
            "code": enrollment.recovery_codes[0],
        }))
        .send()
        .await
        .expect("replay recovery");
    assert_eq!(resp.status(), 401, "a consumed recovery code must not work");
}

#[tokio::test]
async fn identity_two_factor_management_enroll_list_revoke() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let auth = bearer(&token);

    // Enroll TOTP via the API.
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/identity/factors/totp/enroll",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("enroll");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("enroll body");
    let factor_id = body["factor"]["id"]
        .as_str()
        .expect("factor id")
        .to_string();
    assert_eq!(body["factor"]["kind"], "totp");
    assert!(body["secret_base32"].as_str().unwrap().len() > 16);
    assert!(
        body["provisioning_uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://")
    );
    assert_eq!(body["recovery_codes"].as_array().unwrap().len(), 10);

    // List factors.
    let resp = server
        .client()
        .get(format!("{}/api/v1/identity/factors", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .expect("list factors");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("list body");
    assert_eq!(body["factors"].as_array().unwrap().len(), 1);
    assert_eq!(body["recovery_codes_remaining"], 10);

    // Revoke the factor.
    let resp = server
        .client()
        .delete(format!(
            "{}/api/v1/identity/factors/{factor_id}",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("revoke");
    assert_eq!(resp.status(), 200);

    // Factor still listed but revoked.
    let resp = server
        .client()
        .get(format!("{}/api/v1/identity/factors", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .expect("list after revoke");
    let body: serde_json::Value = resp.json().await.expect("list body");
    assert!(body["factors"][0]["revoked_at"].is_string());

    // Regenerate recovery codes.
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/identity/factors/recovery/regenerate",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("regen");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("regen body");
    assert_eq!(body["recovery_codes"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn identity_remember_device_skips_factor_on_next_login() {
    use openpanel_app::identity::two_factor::FactorResponse;

    let server = TestServer::new().await;
    server
        .identity()
        .create_user(
            "carol",
            "carol@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::Owner,
            "test",
        )
        .await
        .expect("create carol");
    let identity_svc = server.identity();
    let user_id = identity_svc
        .users()
        .find_by_username("carol")
        .await
        .expect("find")
        .expect("carol exists")
        .id();
    identity_svc
        .two_factor()
        .enroll_totp(
            user_id,
            "test",
            "OpenPanel",
            "carol@example.com",
            chrono::Utc::now(),
        )
        .await
        .expect("enroll");

    // First login → factor_required → correct TOTP + remember_device.
    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .json(&json!({
            "username_or_email": "carol",
            "password": "correct horse battery staple",
        }))
        .send()
        .await
        .expect("first password login");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("first body");
    let challenge_id = body["challenge_id"]
        .as_str()
        .expect("challenge")
        .to_string();

    let challenge = identity_svc
        .two_factor()
        .issue_login_challenge(
            user_id,
            Some("203.0.113.42".into()),
            Some("TestUA/1.0".into()),
            chrono::Utc::now(),
        )
        .await
        .expect("issue challenge");
    let enrollment = identity_svc
        .two_factor()
        .list_factors(user_id)
        .await
        .expect("list factors")
        .into_iter()
        .find(|f| f.revoked_at().is_none())
        .expect("active factor");
    let secret_bytes = identity_svc
        .two_factor()
        .active_totp_factor(user_id)
        .await
        .expect("active factor")
        .expect("factor exists")
        .1;
    let step = secret_bytes.current_step(chrono::Utc::now());
    let step_u = u64::try_from(step).expect("step");
    let code = secret_bytes
        .totp("OpenPanel", "carol@example.com")
        .generate(step_u);

    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/identity/login/factor",
            server.base_url()
        ))
        .header("x-forwarded-for", "203.0.113.42")
        .header("user-agent", "TestUA/1.0")
        .json(&json!({
            "challenge_id": challenge.challenge_id,
            "challenge_token": challenge.challenge_token,
            "kind": "totp",
            "code": code,
            "remember_device": true,
        }))
        .send()
        .await
        .expect("factor login");
    assert_eq!(resp.status(), 200);
    let cookies: Vec<String> = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap_or("").to_string())
        .collect();
    let cookie_header = cookies
        .iter()
        .find(|v| v.starts_with("openpanel_2fa_remember="))
        .cloned()
        .expect("remember-device cookie must be set");
    let cookie_value = cookie_header
        .split_once('=')
        .map(|(_, v)| v)
        .expect("cookie value")
        .to_string();

    // Second login: present the remember-device cookie. Password step
    // should authenticate directly (Authenticated, not FactorRequired).
    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .header(
            reqwest::header::COOKIE,
            format!(
                "{}={}",
                openpanel_domain::REMEMBER_DEVICE_COOKIE,
                cookie_value
            ),
        )
        .header("x-forwarded-for", "203.0.113.42")
        .header("user-agent", "TestUA/1.0")
        .json(&json!({
            "username_or_email": "carol",
            "password": "correct horse battery staple",
        }))
        .send()
        .await
        .expect("second login");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("body");
    assert!(
        body["token"].as_str().is_some(),
        "second login must authenticate directly, got {body}"
    );

    // Third login: same UA but a different IP prefix (mismatched
    // device) must NOT skip the factor step.
    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .header(
            reqwest::header::COOKIE,
            format!(
                "{}={}",
                openpanel_domain::REMEMBER_DEVICE_COOKIE,
                cookie_value
            ),
        )
        .header("x-forwarded-for", "10.20.30.40")
        .header("user-agent", "TestUA/1.0")
        .json(&json!({
            "username_or_email": "carol",
            "password": "correct horse battery staple",
        }))
        .send()
        .await
        .expect("third login");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("body");
    assert_eq!(
        body["status"], "factor_required",
        "IP mismatch must NOT skip the factor step: {body}"
    );

    // Fourth login: tampered cookie must NOT skip the factor step.
    // Flip one char in the middle of the payload base64 portion.
    let dot = cookie_value.find('.').expect("dot");
    let mut tampered_chars: Vec<char> = cookie_value.chars().collect();
    let mid = dot / 2;
    tampered_chars[mid] = if tampered_chars[mid] == 'A' { 'B' } else { 'A' };
    let tampered: String = tampered_chars.iter().collect();
    let resp = server
        .client()
        .post(format!("{}/api/v1/identity/login", server.base_url()))
        .header(
            reqwest::header::COOKIE,
            format!("{}={}", openpanel_domain::REMEMBER_DEVICE_COOKIE, tampered),
        )
        .header("x-forwarded-for", "203.0.113.42")
        .header("user-agent", "TestUA/1.0")
        .json(&json!({
            "username_or_email": "carol",
            "password": "correct horse battery staple",
        }))
        .send()
        .await
        .expect("tampered login");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("body");
    assert_eq!(
        body["status"], "factor_required",
        "tampered cookie must NOT skip the factor step"
    );
    let _ = FactorResponse::Totp(String::new()); // silence
    let _ = challenge_id; // silence
    let _ = enrollment; // silence
}
