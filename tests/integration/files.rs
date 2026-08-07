use crate::common::*;

#[tokio::test]
async fn files_write_read_delete_roundtrip() {
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

    // Create a site with a sandboxed document root.
    let doc_root =
        server.sandbox_path(&format!("sites/files-{}/public_html", uuid::Uuid::new_v4()));
    let site = server
        .sites()
        .create_site(
            &user,
            user.id(),
            "files.example.com",
            vec![],
            false,
            None,
            Some(doc_root.clone()),
        )
        .await
        .unwrap();
    let site_id = site.id();

    // PUT a file
    let put = server
        .client()
        .put(format!(
            "{}/api/v1/files/{site_id}/index.html",
            server.base_url()
        ))
        .bearer_auth(&token)
        .body("hello world")
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 200);

    // GET it back
    let get = server
        .client()
        .get(format!(
            "{}/api/v1/files/{site_id}/index.html",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), 200);
    assert_eq!(get.text().await.unwrap(), "hello world");

    // PATCH rename
    let rename = server
        .client()
        .patch(format!(
            "{}/api/v1/files/{site_id}/index.html",
            server.base_url()
        ))
        .bearer_auth(&token)
        .json(&serde_json::json!({"to": "renamed.txt"}))
        .send()
        .await
        .unwrap();
    assert_eq!(rename.status(), 200);

    let get_renamed = server
        .client()
        .get(format!(
            "{}/api/v1/files/{site_id}/renamed.txt",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(get_renamed.status(), 200);
    assert_eq!(get_renamed.text().await.unwrap(), "hello world");

    // DELETE it
    let del = server
        .client()
        .delete(format!(
            "{}/api/v1/files/{site_id}/renamed.txt",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);

    let get_deleted = server
        .client()
        .get(format!(
            "{}/api/v1/files/{site_id}/renamed.txt",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(get_deleted.status(), 404);
}

#[tokio::test]
async fn files_rejects_chroot_escape() {
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

    let doc_root = server.sandbox_path(&format!(
        "sites/escape-{}/public_html",
        uuid::Uuid::new_v4()
    ));
    let site = server
        .sites()
        .create_site(
            &user,
            user.id(),
            "escape.example.com",
            vec![],
            false,
            None,
            Some(doc_root),
        )
        .await
        .unwrap();
    let site_id = site.id();

    // URL-encoded `..` must be rejected (400), not escape the chroot.
    let resp = server
        .client()
        .get(format!(
            "{}/api/v1/files/{site_id}/%2E%2E%2F%2E%2E%2Fetc%2Fpasswd",
            server.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}
