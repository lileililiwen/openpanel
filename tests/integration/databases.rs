use crate::common::*;

/// Detect the `mysql` CLI; skip the test if it's absent.
fn mysql_available() -> bool {
    std::process::Command::new("which")
        .arg("mysql")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn databases_list_empty_for_fresh_db() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/databases", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body, serde_json::json!([]));
}

#[tokio::test]
async fn databases_create_roundtrip_when_mysql_present() {
    if !mysql_available() {
        eprintln!("skipped: mysql not installed");
        return;
    }
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;

    let user = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .unwrap();

    let (db, password) = server
        .databases()
        .create_database(&user, user.id(), "admin", "app", Some("utf8mb4".into()))
        .await
        .unwrap();
    assert!(db.name().starts_with("admin_"));
    assert!(password.len() >= 20);

    // List returns the row
    let resp = server
        .client()
        .get(format!("{}/api/v1/databases", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Change password
    let new_pw = server
        .databases()
        .change_password(&user, db.id())
        .await
        .unwrap();
    assert_ne!(new_pw, password);

    // Delete
    server
        .databases()
        .delete_database(&user, db.id())
        .await
        .unwrap();
}
