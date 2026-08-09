use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn dns_routes_require_authentication() {
    let server = TestServer::new().await;
    for path in ["/api/v1/dns/providers", "/api/v1/dns/zones"] {
        assert_eq!(
            server
                .client()
                .get(format!("{}{}", server.base_url(), path))
                .send()
                .await
                .unwrap()
                .status(),
            401,
            "{path}"
        );
    }
}

#[tokio::test]
async fn owner_provider_sync_record_and_propagation_workflow_is_secret_free() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let created = server
        .client()
        .post(format!("{}/api/v1/dns/providers", server.base_url()))
        .header("authorization", &auth)
        .json(
            &serde_json::json!({"kind":"fake","name":"Primary","credential":"integration-secret"}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let text = created.text().await.unwrap();
    assert!(!text.contains("integration-secret"));
    let account: serde_json::Value = serde_json::from_str(&text).unwrap();
    let account_id = account["id"].as_str().unwrap();
    let synced = server
        .client()
        .post(format!(
            "{}/api/v1/dns/providers/{account_id}/sync",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(synced.status(), 200);
    let zones = server
        .client()
        .get(format!("{}/api/v1/dns/zones", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let zone_id = zones[0]["id"].as_str().unwrap();
    let record = server.client().post(format!("{}/api/v1/dns/zones/{zone_id}/records", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({"name":"www.example.test","kind":"A","value":"192.0.2.5","ttl":300,"expected_version":"v1"}))
        .send().await.unwrap();
    assert_eq!(record.status(), 201);
    let created: serde_json::Value = record.json().await.unwrap();
    let record_id = created["remote_id"].as_str().unwrap();
    assert_eq!(server.client().put(format!("{}/api/v1/dns/zones/{zone_id}/records/{record_id}",server.base_url())).header("authorization",&auth).json(&serde_json::json!({"name":"www.example.test","kind":"A","value":"192.0.2.6","ttl":300,"expected_version":"stale"})).send().await.unwrap().status(),409);
    assert_eq!(server.client().put(format!("{}/api/v1/dns/zones/{zone_id}/records/{record_id}",server.base_url())).header("authorization",&auth).json(&serde_json::json!({"name":"www.example.test","kind":"A","value":"192.0.2.6","ttl":300,"expected_version":"v1"})).send().await.unwrap().status(),200);
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/api/v1/dns/zones/{zone_id}/check",
                server.base_url()
            ))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}

#[tokio::test]
async fn dns_web_requires_csrf_and_renders_no_credentials() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "owner"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .unwrap();
    let cookie = login.headers()[reqwest::header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let page = server
        .client()
        .get(format!("{}/dns", server.base_url()))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.text().await.unwrap().contains("DNS zones"));
    assert_eq!(
        server
            .client()
            .post(format!("{}/dns/providers", server.base_url()))
            .header("cookie", cookie)
            .form(&[
                ("_csrf", "wrong"),
                ("kind", "fake"),
                ("name", "Primary"),
                ("credential", "must-not-leak")
            ])
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
}

#[tokio::test]
async fn owner_tests_rotates_disables_and_deletes_provider_account() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let account: serde_json::Value = server
        .client()
        .post(format!("{}/api/v1/dns/providers", server.base_url()))
        .header("authorization", &auth)
        .json(&serde_json::json!({"kind":"fake","name":"Lifecycle","credential":"first-secret"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = account["id"].as_str().unwrap();
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/api/v1/dns/providers/{id}/test",
                server.base_url()
            ))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let rotated = server
        .client()
        .post(format!(
            "{}/api/v1/dns/providers/{id}/rotate",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({"credential":"second-secret"}))
        .send()
        .await
        .unwrap();
    assert_eq!(rotated.status(), 200);
    assert!(!rotated.text().await.unwrap().contains("second-secret"));
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/api/v1/dns/providers/{id}/disable",
                server.base_url()
            ))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        server
            .client()
            .delete(format!("{}/api/v1/dns/providers/{id}", server.base_url()))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
}
