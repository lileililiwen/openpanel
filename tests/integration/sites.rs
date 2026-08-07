use crate::common::*;

#[tokio::test]
async fn sites_list_empty_for_fresh_db() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let resp = server
        .client()
        .get(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body, serde_json::json!([]));
}

#[tokio::test]
async fn sites_get_bad_uuid_not_found() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let bad = "00000000-0000-0000-0000-000000000000";
    let resp = server
        .client()
        .get(format!("{}/api/v1/sites/{bad}", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn sites_insert_via_repo_then_list() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;

    // Insert a site directly via the service (no nginx required in tests
    // because nginx writes are skipped when the binary is missing).
    let user = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .unwrap();

    let site = server
        .sites()
        .create_site(
            &user,
            user.id(),
            "example.com",
            vec!["www.example.com".into()],
            false,
            None,
            None,
        )
        .await
        .unwrap();

    let resp = server
        .client()
        .get(format!("{}/api/v1/sites", server.base_url()))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], site.id().to_string());
    assert_eq!(arr[0]["primary_domain"], "example.com");
}
