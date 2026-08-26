//! Per-site transport tuning REST integration tests.

use crate::common::*;

#[tokio::test]
async fn transport_put_get_roundtrip_and_guards() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let owner = server
        .identity()
        .list_users()
        .await
        .expect("users")
        .into_iter()
        .find(|user| user.username().as_str() == "owner")
        .expect("owner");
    let site = server
        .sites()
        .create_site(
            &owner,
            owner.id(),
            "transport.example.test",
            vec![],
            false,
            None,
            Some("/var/www/transport.example.test/public_html".to_owned()),
        )
        .await
        .expect("site");
    let url = format!("{}/api/v1/sites/{}/transport", server.base_url(), site.id());

    // Unauthenticated callers are rejected.
    assert_eq!(
        server
            .client()
            .get(&url)
            .send()
            .await
            .expect("anon")
            .status(),
        401
    );

    // Default policy round-trips.
    let got = server
        .client()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get");
    assert_eq!(got.status(), 200);
    let body: serde_json::Value = got.json().await.expect("json");
    assert_eq!(body["http3_enabled"], serde_json::json!(false));
    assert_eq!(body["tls_min_version"], serde_json::json!("V1_2"));

    // Valid PUT updates and round-trips.
    let put = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "http3_enabled": true,
            "tls_min_version": "1.3",
            "hsts": { "max_age_secs": 31536000, "include_subdomains": true, "preload": true },
            "compression": { "kind": "brotli", "level": 5 },
            "body_size_cap_bytes": 33554432
        }))
        .send()
        .await
        .expect("put");
    assert_eq!(put.status(), 200, "{}", put.text().await.expect("body"));
    let body: serde_json::Value = put.json().await.expect("json");
    assert_eq!(body["http3_enabled"], serde_json::json!(true));

    let got = server
        .client()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get after put");
    let body: serde_json::Value = got.json().await.expect("json");
    assert_eq!(body["http3_enabled"], serde_json::json!(true));
    assert_eq!(body["tls_min_version"], serde_json::json!("V1_3"));
    assert_eq!(body["hsts"]["max_age_secs"], serde_json::json!(31536000));

    // Invalid HSTS combo (preload below one year) → 422.
    let invalid = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "http3_enabled": false,
            "tls_min_version": "1.2",
            "hsts": { "max_age_secs": 60, "include_subdomains": true, "preload": true },
            "compression": { "kind": "off" },
            "body_size_cap_bytes": 1048576
        }))
        .send()
        .await
        .expect("invalid put");
    assert_eq!(invalid.status(), 422);

    // Successful mutations are audited.
    let events = server.audit_events().await;
    assert!(
        events
            .iter()
            .any(|event| event.action == openpanel_core::AuditAction::SiteTransportChanged),
        "transport change must be audited"
    );
}
