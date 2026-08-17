//! Per-site collaborator HTTP integration tests.

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn seed_site(server: &TestServer) -> uuid::Uuid {
    let user = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .unwrap();
    let site_id = uuid::Uuid::new_v4();
    let site = openpanel_domain::Site::new(
        site_id,
        user.id(),
        "collab.example.com",
        vec![],
        "/var/www/collab.example.com/public_html",
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
    site_id
}

#[tokio::test]
async fn collaborator_invite_create_list_revoke_flow() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let site_id = seed_site(&server).await;

    // Invite
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/sites/{site_id}/collaborators",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "email": "dev@example.com",
            "scopes": ["file", "database"]
        }))
        .send()
        .await
        .expect("invite");
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.expect("body");
    let collaborator_id = body["collaborator_id"].as_str().unwrap().to_string();
    assert_eq!(body["status"], "invited");

    // List grants on site
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/sites/{site_id}/collaborators",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("list");
    assert_eq!(resp.status(), 200);
    let arr: Vec<serde_json::Value> = resp.json().await.expect("arr");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["collaborator_id"], collaborator_id.clone());

    // Resolve via /resolve
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/sites/{site_id}/collaborators/{collaborator_id}/resolve",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("resolve");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("resolve body");
    let scopes = body["scopes"].as_array().unwrap();
    assert!(scopes.iter().any(|v| v == "file"));
    assert!(scopes.iter().any(|v| v == "database"));

    // Revoke
    let resp = server
        .client()
        .delete(format!(
            "{}/api/v1/sites/{site_id}/collaborators/{collaborator_id}",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("revoke");
    assert_eq!(resp.status(), 204);

    // After revoke, resolve must fail (no grant).
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/sites/{site_id}/collaborators/{collaborator_id}/resolve",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("resolve after revoke");
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn collaborator_invite_rejects_unknown_scope() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let auth = bearer(&token);
    let site_id = seed_site(&server).await;
    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/sites/{site_id}/collaborators",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "email": "dev@example.com",
            "scopes": ["file", "root"]
        }))
        .send()
        .await
        .expect("invite");
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn collaborator_routes_require_authentication() {
    let server = TestServer::new().await;
    let site_id = uuid::Uuid::new_v4();
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/sites/{site_id}/collaborators",
            server.base_url()
        ))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 401);
}
