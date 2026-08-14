//! Container registry HTTP integration tests.

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn registry_routes_require_authentication() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/registry/config", server.base_url()))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn registry_config_shows_and_updates() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let resp = server
        .client()
        .get(format!("{}/api/v1/registry/config", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("body");
    assert!(body["scan_on_push"].is_boolean());
    assert!(body["retention"].is_object());

    let resp = server
        .client()
        .put(format!("{}/api/v1/registry/config", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({"scan_on_push": true}))
        .send()
        .await
        .expect("put");
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn registry_namespace_create_list() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let user = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .unwrap();
    let resp = server
        .client()
        .post(format!("{}/api/v1/registry/namespaces", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "namespace": "alpha",
            "owner": user.id(),
            "quota_bytes": 1024
        }))
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.expect("body");
    assert_eq!(body["namespace_id"], "alpha");

    // Listing should contain the new namespace.
    let resp = server
        .client()
        .get(format!("{}/api/v1/registry/namespaces", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .expect("list");
    assert_eq!(resp.status(), 200);
    let arr: Vec<serde_json::Value> = resp.json().await.expect("arr");
    assert_eq!(arr.len(), 1);
}

#[tokio::test]
async fn registry_push_rejects_quota_overflow() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let user = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .unwrap();
    // 50-byte quota.
    let resp = server
        .client()
        .post(format!("{}/api/v1/registry/namespaces", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "namespace": "alpha",
            "owner": user.id(),
            "quota_bytes": 50
        }))
        .send()
        .await
        .expect("ns");
    assert_eq!(resp.status(), 201);

    let resp = server
        .client()
        .post(format!("{}/api/v1/registry/images", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "namespace": "alpha",
            "digest": format!("sha256:{}", "a".repeat(64)),
            "manifest": "x".repeat(200),
            "blobs": []
        }))
        .send()
        .await
        .expect("push");
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn registry_push_rejects_cross_namespace_attempt() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let user = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .unwrap();
    // Bootstrap a second user (this user owns the namespace).
    let token2 = server
        .identity()
        .create_user(
            "dev",
            "dev@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::Admin,
            "test",
        )
        .await
        .expect("create dev user");
    let _ = token2;
    let resp = server
        .client()
        .post(format!("{}/api/v1/registry/namespaces", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "namespace": "alpha",
            "owner": user.id(),
            "quota_bytes": 10_000
        }))
        .send()
        .await
        .expect("ns");
    assert_eq!(resp.status(), 201);

    // `dev` tries to push to a namespace they do NOT own.
    let dev_token = server.login("dev", "correct horse battery staple").await;
    let dev_auth = bearer(&dev_token);
    let resp = server
        .client()
        .post(format!("{}/api/v1/registry/images", server.base_url()))
        .header("authorization", &dev_auth)
        .json(&serde_json::json!({
            "namespace": "alpha",
            "digest": format!("sha256:{}", "b".repeat(64)),
            "manifest": "{}",
            "blobs": []
        }))
        .send()
        .await
        .expect("push");
    assert_eq!(resp.status(), 400);
}