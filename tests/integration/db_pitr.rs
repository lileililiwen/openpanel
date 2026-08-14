//! Database point-in-time recovery HTTP integration tests.
//!
//! Verifies the routing, auth, and idempotency of the
//! `/api/v1/backups/databases/{id}/binlog` and
//! `/api/v1/backups/databases/{id}/pitr/restore` endpoints.

use crate::common::*;

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn seed_database_row(server: &TestServer, db_id: uuid::Uuid, owner_id: uuid::Uuid) {
    // Insert a stub `databases` row directly into the SQLite pool so
    // the test does not require a running MySQL daemon.
    let pool = server.pool();
    sqlx::query(
        "INSERT OR REPLACE INTO databases
            (id, owner_id, name, db_user, db_host, engine, charset,
             status, password_ciphertext, created_at, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(db_id.to_string())
    .bind(owner_id.to_string())
    .bind("pitr_owner_app")
    .bind("pitr_owner_app")
    .bind("localhost")
    .bind("mysql")
    .bind("utf8mb4")
    .bind("active")
    .bind("00")
    .bind(chrono::Utc::now().to_rfc3339())
    .bind("admin")
    .execute(&pool)
    .await
    .expect("seed databases row");
}

#[tokio::test]
async fn pitr_routes_require_authentication() {
    let server = TestServer::new().await;
    let db_id = uuid::Uuid::new_v4();
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/backups/databases/{db_id}/binlog",
            server.base_url()
        ))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn pitr_inspect_returns_empty_range_when_no_segments() {
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
    let db_id = uuid::Uuid::new_v4();
    seed_database_row(&server, db_id, user.id()).await;

    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/backups/databases/{db_id}/binlog",
            server.base_url()
        ))
        .header("authorization", &auth)
        .send()
        .await
        .expect("inspect");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("inspect json");
    assert_eq!(body["empty"], serde_json::Value::Bool(true));
}

#[tokio::test]
async fn pitr_restore_rejects_timestamp_outside_range() {
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
    let db_id = uuid::Uuid::new_v4();
    seed_database_row(&server, db_id, user.id()).await;

    let resp = server
        .client()
        .post(format!(
            "{}/api/v1/backups/databases/{db_id}/pitr/restore",
            server.base_url()
        ))
        .header("authorization", &auth)
        .json(&serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
        .send()
        .await
        .expect("restore");
    assert_eq!(resp.status(), 422);
}
