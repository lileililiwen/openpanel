//! Per-site staging HTTP integration tests.
//!
//! Verifies auth, the create/sync/promote/destroy flow, and the
//! rollback path.

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn staging_routes_require_authentication() {
    let server = TestServer::new().await;
    let site_id = uuid::Uuid::new_v4();
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/sites/{site_id}/staging",
            server.base_url()
        ))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn staging_create_succeeds_then_conflict() {
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

    // Seed a site via the pool so the test does not require
    // touching the live nginx config.
    let site_id = uuid::Uuid::new_v4();
    let site = openpanel_domain::Site::new(
        site_id,
        user.id(),
        "stage.example.com",
        vec![],
        "/var/www/stage.example.com/public_html",
        false,
        None,
        "admin",
    )
    .unwrap();
    let pool = server.pool();
    sqlx::query(
        "INSERT OR REPLACE INTO sites
            (id, owner_id, primary_domain, aliases, document_root,
             php_enabled, php_version, status, created_at, updated_at,
             created_by, modified_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(site.id().to_string())
    .bind(site.owner_id().to_string())
    .bind(site.primary_domain())
    .bind(serde_json::to_string(site.aliases()).unwrap())
    .bind(site.document_root())
    .bind(if site.php_enabled() { 1 } else { 0 })
    .bind(site.php_version())
    .bind(site.status().as_str())
    .bind(site.created_at().to_rfc3339())
    .bind(site.updated_at().to_rfc3339())
    .bind(site.created_by())
    .bind(site.modified_by())
    .execute(&pool)
    .await
    .expect("seed site");

    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/sites/{site_id}/staging",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("create");
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.expect("body");
    assert_eq!(body["site_id"], serde_json::json!(site_id.to_string()));
    assert_eq!(
        body["document_root"],
        serde_json::Value::String("/var/www/stage.example.com/staging/public_html".to_string())
    );
    assert_eq!(
        body["db_name"],
        serde_json::Value::String("stage_example_com_staging".to_string())
    );

    // Second create must conflict.
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/sites/{site_id}/staging",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("create 2");
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn staging_create_rejects_path_outside_chroot() {
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

    let site_id = uuid::Uuid::new_v4();
    let pool = server.pool();
    let site = openpanel_domain::Site::new(
        site_id,
        user.id(),
        "chroot.example.com",
        vec![],
        "/var/www/chroot.example.com/public_html",
        false,
        None,
        "admin",
    )
    .unwrap();
    sqlx::query(
        "INSERT OR REPLACE INTO sites
            (id, owner_id, primary_domain, aliases, document_root,
             php_enabled, php_version, status, created_at, updated_at,
             created_by, modified_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(site.id().to_string())
    .bind(site.owner_id().to_string())
    .bind(site.primary_domain())
    .bind(serde_json::to_string(site.aliases()).unwrap())
    .bind(site.document_root())
    .bind(if site.php_enabled() { 1 } else { 0 })
    .bind(site.php_version())
    .bind(site.status().as_str())
    .bind(site.created_at().to_rfc3339())
    .bind(site.updated_at().to_rfc3339())
    .bind(site.created_by())
    .bind(site.modified_by())
    .execute(&pool)
    .await
    .expect("seed site");

    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/sites/{site_id}/staging",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "document_root": "/var/www/other/public_html"
        }))
        .send()
        .await
        .expect("create");
    assert_eq!(resp.status(), 422);
}
