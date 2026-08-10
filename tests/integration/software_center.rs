use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn software_routes_require_authentication() {
    let server = TestServer::new().await;
    for path in [
        "/api/v1/software/catalog",
        "/api/v1/software/inventory",
        "/api/v1/software/diagnostics",
        "/api/v1/software/jobs",
    ] {
        assert_eq!(
            server
                .client()
                .get(format!("{}{}", server.base_url(), path))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
    }
}

#[tokio::test]
async fn owner_can_preview_install_and_observe_a_secret_free_job() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let catalog = server
        .client()
        .get(format!("{}/api/v1/software/catalog", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(catalog.status(), 200);
    assert!(catalog.text().await.unwrap().contains("wordpress"));
    let inventory = server
        .client()
        .get(format!("{}/api/v1/software/inventory", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(inventory.status(), 200);
    assert!(inventory.text().await.unwrap().contains("available"));
    let preview = server
        .client()
        .post(format!(
            "{}/api/v1/software/components/redis/preview",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), 200);
    let preview: serde_json::Value = preview.json().await.unwrap();
    let digest = preview["plan"]["digest"].as_str().unwrap();
    let confirmation = preview["confirmation_token"].as_str().unwrap();
    let installed = server
        .client()
        .post(format!(
            "{}/api/v1/software/plans/{digest}/execute",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({"confirmation_token":confirmation}))
        .send()
        .await
        .unwrap();
    assert_eq!(installed.status(), 200);
    let installed: serde_json::Value = installed.json().await.unwrap();
    let job_id = installed["id"].as_str().unwrap();
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/api/v1/software/jobs/{job_id}/cancel",
                server.base_url()
            ))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    for action in ["update", "remove"] {
        let preview = server
            .client()
            .post(format!(
                "{}/api/v1/software/components/redis/{action}/preview",
                server.base_url()
            ))
            .header("authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(preview.status(), 200, "{action}");
        let preview: serde_json::Value = preview.json().await.unwrap();
        let digest = preview["plan"]["digest"].as_str().unwrap();
        let confirmation = preview["confirmation_token"].as_str().unwrap();
        let executed = server
            .client()
            .post(format!(
                "{}/api/v1/software/plans/{digest}/execute",
                server.base_url()
            ))
            .header("authorization", &auth)
            .json(&serde_json::json!({"confirmation_token":confirmation}))
            .send()
            .await
            .unwrap();
        assert_eq!(executed.status(), 200, "{action}");
    }
    let inventory: serde_json::Value = server
        .client()
        .get(format!("{}/api/v1/software/inventory", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        inventory
            .as_array()
            .unwrap()
            .iter()
            .any(|component| { component["id"] == "redis" && component["state"] == "available" })
    );
    let jobs = server
        .client()
        .get(format!("{}/api/v1/software/jobs", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(jobs.contains("succeeded"));
    assert!(!jobs.to_ascii_lowercase().contains("password"));
}

#[tokio::test]
async fn software_web_is_owner_only_and_mutations_require_csrf() {
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
        .get(format!("{}/software", server.base_url()))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    let page = page.text().await.unwrap();
    assert!(
        page.contains("WordPress"),
        "page did not contain 'WordPress' (length={})",
        page.len()
    );
    assert!(page.contains("Software Center"));
    assert!(page.contains("Install"));
    assert_eq!(
        server
            .client()
            .post(format!(
                "{}/software/components/redis/preview",
                server.base_url()
            ))
            .header("cookie", cookie)
            .form(&[("_csrf", "wrong")])
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
}

#[tokio::test]
async fn owner_can_deploy_wordpress_and_credentials_are_returned_once() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let preview = server
        .client()
        .post(format!(
            "{}/api/v1/software/applications/preview",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "application":"wordpress",
            "domain":"example.test",
            "php_version":"8.3",
            "locale":"en_US",
            "enable_dns":false,
            "enable_tls":false,
            "enable_backups":false
        }))
        .send()
        .await
        .unwrap();
    let preview_status = preview.status();
    let preview: serde_json::Value = preview.json().await.unwrap();
    assert_eq!(preview_status, 200);
    let digest = preview["plan"]["digest"].as_str().unwrap();
    let confirmation = preview["confirmation_token"].as_str().unwrap();
    let deployed = server
        .client()
        .post(format!(
            "{}/api/v1/software/applications/plans/{digest}/execute",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({"confirmation_token":confirmation}))
        .send()
        .await
        .unwrap();
    assert_eq!(deployed.status(), 200);
    let body = deployed.text().await.unwrap();
    assert!(body.contains("admin_password"));
    let deployments = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM software_deployments")
        .fetch_one(&server.database_pool())
        .await
        .unwrap();
    assert_eq!(deployments, 1);
    let jobs = server
        .client()
        .get(format!("{}/api/v1/software/jobs", server.base_url()))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!jobs.contains("admin_password"));
}

#[tokio::test]
async fn invalid_catalog_refresh_retains_the_last_known_good_snapshot() {
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    use openpanel_app::software_center::{CatalogVerifier, SignedCatalogEnvelope};

    let server = TestServer::new().await;
    let signing = SigningKey::from_bytes(&[9_u8; 32]);
    let verifier = CatalogVerifier::new(signing.verifying_key().to_bytes());
    let payload = r#"{"entries":[{"id":"redis","name":"Redis"}]}"#;
    let signed = format!("1\n2000\n{payload}");
    let envelope = SignedCatalogEnvelope {
        schema: 1,
        expires_at: 2000,
        payload: payload.to_owned(),
        signature: base64::engine::general_purpose::STANDARD
            .encode(signing.sign(signed.as_bytes()).to_bytes()),
    };
    let activated = server
        .software_center()
        .activate_catalog(
            uuid::Uuid::new_v4(),
            openpanel_domain::Role::Owner,
            &verifier,
            &envelope,
            1000,
        )
        .await
        .unwrap();
    let mut invalid = envelope;
    invalid.signature = base64::engine::general_purpose::STANDARD.encode([0_u8; 64]);
    assert!(
        server
            .software_center()
            .activate_catalog(
                uuid::Uuid::new_v4(),
                openpanel_domain::Role::Owner,
                &verifier,
                &invalid,
                1000
            )
            .await
            .is_err()
    );
    let active = sqlx::query_scalar::<_, String>(
        "SELECT digest FROM software_catalog_snapshots WHERE active=1",
    )
    .fetch_one(&server.database_pool())
    .await
    .unwrap();
    assert_eq!(active, activated.digest);
}
