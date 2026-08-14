//! Plugin marketplace HTTP integration tests.

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn marketplace_list_returns_503_when_catalog_not_yet_discovered() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/marketplace/plugins", server.base_url()))
        .header("authorization", bearer(&token))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 503);
}

#[tokio::test]
async fn marketplace_detail_returns_503_when_catalog_not_yet_discovered() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/marketplace/plugins/com.example.demo",
            server.base_url()
        ))
        .header("authorization", bearer(&token))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 503);
}

#[tokio::test]
async fn marketplace_install_requires_authentication() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/marketplace/plugins/com.example.demo/install",
            server.base_url()
        ))
        .json(&serde_json::json!({"manifest": {}, "publisher_key_b64": ""}))
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn marketplace_install_rejects_unverifiable_publisher() {
    use base64::Engine;
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    // 32-byte all-zero "key" that is unlikely to be a valid ed25519 point.
    let bad_key = [7u8; 32];
    let key_b64 = base64::engine::general_purpose::STANDARD.encode(bad_key);
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/marketplace/plugins/com.example.demo/install",
            server.base_url()
        ))
        .header("authorization", bearer(&token))
        .json(&serde_json::json!({
            "publisher_key_b64": key_b64,
            "manifest": {
                "id": "com.example.demo",
                "version": "1.0.0",
                "runtime": "json-rpc",
                "entrypoint": "/usr/lib/openpanel/plugins/demo/bin",
                "permissions": [],
                "publisher": "publisher-demo",
                "signature": "AA=="
            }
        }))
        .send()
        .await
        .expect("post");
    // Either 400 (bad signature/key) or 403 (publisher unverified)
    // depending on which check fires first. Both are acceptable.
    assert!(resp.status() == 400 || resp.status() == 403);
}