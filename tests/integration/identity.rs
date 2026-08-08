use serde_json::json;

use crate::common::*;

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
