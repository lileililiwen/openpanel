//! End-to-end SSL HTTP tests.
//!
//! Covers self-signed generation (the only issuance path that's
//! safe to run in CI without network/ACME access), force-HTTPS
//! toggling, listing / fetching, and the metadata-only contract
//! (no private-key bytes in any response).

use serde_json::json;

use crate::common::*;

#[tokio::test]
async fn ssl_list_empty_for_fresh_db() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/ssl/certificates", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["certificates"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn ssl_self_signed_create_list_delete() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;

    // 1. Generate a self-signed cert.
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/ssl/certificates/self-signed",
            server.base_url()
        ))
        .bearer_auth(&token)
        .json(&json!({
            "domain": "self-signed.example.com",
            "valid_for_days": 30,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["certificate"]["domain"], "self-signed.example.com");
    assert_eq!(body["certificate"]["source"], "self_signed");
    assert_eq!(body["certificate"]["force_https"], true);
    // NO private key in the response.
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(
        !body_str.contains("PRIVATE KEY"),
        "response must not contain private-key material"
    );

    // 2. List returns the row.
    let resp = server
        .client()
        .get(format!("{}/api/v1/ssl/certificates", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let arr = body["certificates"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["domain"], "self-signed.example.com");

    // 3. Fetch by domain.
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/ssl/certificates/self-signed.example.com",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["certificate"]["source"], "self_signed");

    // 4. Force-HTTPS toggle off → on.
    let resp = server
        .client()
        .patch(format!(
            "{}/api/v1/ssl/certificates/self-signed.example.com/force-https",
            server.base_url()
        ))
        .bearer_auth(&token)
        .json(&json!({"enabled": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["certificate"]["force_https"], false);

    // 5. Delete.
    let resp = server
        .client()
        .delete(format!(
            "{}/api/v1/ssl/certificates/self-signed.example.com",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    // 6. List is empty again.
    let resp = server
        .client()
        .get(format!("{}/api/v1/ssl/certificates", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["certificates"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn ssl_unauthenticated_get_is_rejected() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/ssl/certificates", server.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn ssl_get_unknown_domain_returns_404() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/ssl/certificates/does-not-exist.example.com",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
