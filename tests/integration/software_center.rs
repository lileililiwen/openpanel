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
async fn web_install_button_for_a_tar_gz_web_entry_also_lands_on_disk() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let phpmyadmin_url =
        "https://files.phpmyadmin.net/phpMyAdmin/5.2.2/phpMyAdmin-5.2.2-all-languages.tar.gz";
    server.stage_artifact(phpmyadmin_url, build_tar_gz_fixture());

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
        .unwrap()
        .to_owned();

    let storefront = server
        .client()
        .get(format!("{}/software", server.base_url()))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    let storefront_body = storefront.text().await.unwrap();
    assert!(
        storefront_body.contains(r#"action="/software/components/phpmyadmin/install""#),
        "storefront should render an Install button that POSTs to the new install route for phpmyadmin"
    );
    let csrf = extract_csrf(&storefront_body);

    let install = server
        .client()
        .post(format!(
            "{}/software/components/phpmyadmin/install",
            server.base_url()
        ))
        .header("cookie", &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(install.status(), 200, "tar.gz install should not 422");
    let body = install.text().await.unwrap();
    assert!(body.contains("Software installed"), "body={body}");
    let fetched = server.fetched_artifacts();
    assert_eq!(fetched, vec![phpmyadmin_url.to_owned()]);
    let placed = walk_files(server.webapps_root())
        .into_iter()
        .find(|path| path.is_file() && path.file_name().is_some_and(|n| n == "index.html"))
        .expect("tar.gz install should extract the test fixture into the webapps root");
    assert_eq!(
        std::fs::read_to_string(placed).unwrap(),
        "<p>phpmyadmin fixture</p>"
    );
}

/// Build a tiny but valid `tar.gz` fixture with the structure the
/// installer expects: one top-level directory named after `archive_root`
/// and a single file inside it.
fn build_tar_gz_fixture() -> Vec<u8> {
    use flate2::{Compression, write::GzEncoder};
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let body = b"<p>phpmyadmin fixture</p>";
    let mut header = tar::Header::new_gnu();
    header
        .set_path("phpMyAdmin-5.2.2-all-languages/index.html")
        .expect("path");
    header.set_size(body.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, &body[..]).expect("append");
    let encoder = builder.into_inner().expect("finish builder");
    encoder.finish().expect("finish gzip")
}

#[tokio::test]
async fn web_preview_renders_styled_confirm_button_and_executes_end_to_end() {
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
        .unwrap()
        .to_owned();

    let csrf_from_storefront = {
        let page = server
            .client()
            .get(format!("{}/software", server.base_url()))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), 200);
        extract_csrf(&page.text().await.unwrap())
    };

    let preview = server
        .client()
        .post(format!(
            "{}/software/components/redis/preview",
            server.base_url()
        ))
        .header("cookie", &cookie)
        .form(&[("_csrf", csrf_from_storefront.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), 200, "preview should not 422");
    let preview_body = preview.text().await.unwrap();
    assert!(
        preview_body.contains(r#"action="/software/plans/"#) && preview_body.contains("/execute"),
        "preview page should render a form posting to /software/plans/{{digest}}/execute, body={preview_body}",
    );
    assert!(
        preview_body.contains(r#"class="button""#) && preview_body.contains("Confirm installation"),
        "Confirm button must carry the .button class so it is visible",
    );
    let execute_csrf = extract_csrf(&preview_body);
    let digest = extract_digest(&preview_body);
    let token = extract_confirmation_token(&preview_body);

    let executed = server
        .client()
        .post(format!(
            "{}/software/plans/{digest}/execute",
            server.base_url()
        ))
        .header("cookie", &cookie)
        .form(&[
            ("_csrf", execute_csrf.as_str()),
            ("confirmation_token", token.as_str()),
        ])
        .send()
        .await
        .unwrap();
    let executed_status = executed.status();
    let executed_body = executed.text().await.unwrap();
    assert_eq!(
        executed_status, 200,
        "execute should render a confirmation page, not 422. body={executed_body}"
    );
    assert!(
        executed_body.contains("Installation complete")
            || executed_body.contains("Software Center unavailable"),
        "execute should render the success or error page, got body={executed_body}"
    );
}

#[tokio::test]
async fn web_execute_with_stale_token_renders_an_error_page_not_blank_422() {
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
        .unwrap()
        .to_owned();

    let csrf_from_storefront = {
        let page = server
            .client()
            .get(format!("{}/software", server.base_url()))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), 200);
        extract_csrf(&page.text().await.unwrap())
    };

    let response = server
        .client()
        .post(format!(
            "{}/software/plans/00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aaaa00aa00aa/execute",
            server.base_url()
        ))
        .header("cookie", &cookie)
        .form(&[
            ("_csrf", csrf_from_storefront.as_str()),
            ("confirmation_token", "definitely-not-a-real-token"),
        ])
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, 200, "error page should render, body={body}");
    assert!(
        body.contains("Software Center unavailable")
            && (body.contains("confirmation token") || body.contains("plan is no longer")),
        "stale token error page should explain the cause, got body={body}"
    );
}

fn extract_csrf(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html
        .find(marker)
        .expect("csrf hidden input should be present")
        + marker.len();
    let rest = &html[start..];
    let end = rest.find('"').expect("csrf value should be quoted");
    rest[..end].to_owned()
}

fn extract_digest(html: &str) -> String {
    let marker = r#"action="/software/plans/"#;
    let start = html
        .find(marker)
        .expect("digest in plan action should be present")
        + marker.len();
    let rest = &html[start..];
    let end = rest.find('/').expect("digest should end before /execute");
    rest[..end].to_owned()
}

fn extract_confirmation_token(html: &str) -> String {
    let marker = "name=\"confirmation_token\" value=\"";
    let start = html
        .find(marker)
        .expect("confirmation_token hidden input should be present")
        + marker.len();
    let rest = &html[start..];
    let end = rest.find('"').expect("token value should be quoted");
    rest[..end].to_owned()
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

#[tokio::test]
async fn web_install_button_for_adminer_downloads_and_places_the_php_file() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let adminer_url =
        "https://github.com/vrana/adminer/releases/download/v4.8.1/adminer-4.8.1-en.php";
    let staged = b"<?php // staged adminer bytes for the install test".to_vec();
    server.stage_artifact(adminer_url, staged.clone());

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
        .unwrap()
        .to_owned();

    let storefront = server
        .client()
        .get(format!("{}/software", server.base_url()))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(storefront.status(), 200);
    let storefront_body = storefront.text().await.unwrap();
    assert!(
        storefront_body.contains(r#"action="/software/components/adminer/install""#),
        "storefront should render an Install button that POSTs to the new install route, body={storefront_body}"
    );
    let csrf = extract_csrf(&storefront_body);

    let install = server
        .client()
        .post(format!(
            "{}/software/components/adminer/install",
            server.base_url()
        ))
        .header("cookie", &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(install.status(), 200, "install should not 422");
    let install_body = install.text().await.unwrap();
    assert!(
        install_body.contains("Software installed"),
        "expected the success page, body={install_body}"
    );
    assert!(
        install_body.contains(adminer_url) || install_body.contains("adminer-4.8.1-en.php"),
        "success page should name the URL or the placed file, body={install_body}"
    );

    let fetched = server.fetched_artifacts();
    assert_eq!(
        fetched,
        vec![adminer_url.to_owned()],
        "the in-process fetcher must receive the exact URL the adminer recipe pins"
    );

    let webapps_root = server.webapps_root().to_path_buf();
    let placed_root_searched = walk_files(&webapps_root);
    let placed = placed_root_searched
        .iter()
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("php"))
        .expect("the downloaded adminer file should land somewhere under the webapps root");
    let read_back = std::fs::read(placed).expect("read placed file");
    assert_eq!(
        read_back, staged,
        "placed bytes must match the staged payload"
    );
}

fn walk_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    out
}
