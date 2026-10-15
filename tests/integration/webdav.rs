//! WebDAV account API and HTML surface integration tests.

use openpanel_test_support::TestServer;

async fn fixture() -> (TestServer, String, uuid::Uuid) {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("davowner", "correct horse battery staple")
        .await;
    let user = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|user| user.username().as_str() == "davowner")
        .unwrap();
    let site = server
        .sites()
        .create_site(
            &user,
            user.id(),
            "dav.example.test",
            vec![],
            false,
            None,
            None,
        )
        .await
        .unwrap();
    (server, token, site.id())
}

fn csrf(body: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = body.find(marker).unwrap() + marker.len();
    body[start..].split('"').next().unwrap().to_owned()
}

#[tokio::test]
async fn webdav_rest_routes_cover_create_list_patch_delete_and_duplicate() {
    let (server, token, site_id) = fixture().await;
    let base = format!(
        "{}/api/v1/sites/{site_id}/webdav/accounts",
        server.base_url()
    );
    let input = serde_json::json!({"username":"uploads","password":"correct horse battery staple","read_only":false});
    let created = server
        .client()
        .post(&base)
        .bearer_auth(&token)
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let body: serde_json::Value = created.json().await.unwrap();
    let id = body["id"].as_str().unwrap().to_owned();
    assert_eq!(body["plaintext_password_shown_once"], true);
    assert!(!body.to_string().contains("correct horse"));
    assert!(!body.to_string().contains("argon2"));
    let duplicate = server
        .client()
        .post(&base)
        .bearer_auth(&token)
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), 409);
    let listed = server
        .client()
        .get(&base)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let text = listed.text().await.unwrap();
    assert!(text.contains("uploads"));
    assert!(!text.contains("password_hash"));
    let patched = server
        .client()
        .patch(format!("{base}/{id}"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"enabled":false,"password":null,"read_only":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(patched.status(), 200);
    assert_eq!(
        patched.json::<serde_json::Value>().await.unwrap()["enabled"],
        false
    );
    let deleted = server
        .client()
        .delete(format!("{base}/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);
    let missing = server
        .client()
        .patch(format!("{base}/{id}"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"enabled":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

#[tokio::test]
async fn dav_mount_challenges_anonymous_requests() {
    let (server, token, site_id) = fixture().await;
    let created = server
        .client()
        .post(format!(
            "{}/api/v1/sites/{site_id}/webdav/accounts",
            server.base_url()
        ))
        .bearer_auth(&token)
        .json(&serde_json::json!({"username":"dave","password":"correct horse battery staple","read_only":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let body: serde_json::Value = created.json().await.unwrap();
    let account_id = body["id"].as_str().unwrap();
    let url = format!("{}/dav/{account_id}/notes.txt", server.base_url());
    let anonymous = server.client().get(&url).send().await.unwrap();
    assert_eq!(anonymous.status(), 401);
    assert!(
        anonymous
            .headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .starts_with("Basic realm="),
        "401 must carry a Basic challenge"
    );
    let options = server
        .client()
        .request(reqwest::Method::OPTIONS, &url)
        .send()
        .await
        .unwrap();
    assert_eq!(options.status(), 200);
    assert_eq!(
        options
            .headers()
            .get("dav")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default(),
        "1"
    );
}

#[tokio::test]
async fn webdav_web_requires_csrf_and_shows_password_once_banner() {
    let (server, token, site_id) = fixture().await;
    let page = server
        .client()
        .get(format!("{}/sites/{site_id}/webdav", server.base_url()))
        .header("cookie", format!("openpanel_session={token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    let body = page.text().await.unwrap();
    assert!(body.contains("WebDAV accounts"));
    let token_csrf = csrf(&body);
    let denied = server
        .client()
        .post(format!("{}/sites/{site_id}/webdav", server.base_url()))
        .header("cookie", format!("openpanel_session={token}"))
        .form(&[
            ("_csrf", "wrong"),
            ("username", "webdav"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    let created = server
        .client()
        .post(format!("{}/sites/{site_id}/webdav", server.base_url()))
        .header("cookie", format!("openpanel_session={token}"))
        .form(&[
            ("_csrf", token_csrf.as_str()),
            ("username", "webdav"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 200);
    let html = created.text().await.unwrap();
    assert!(html.contains("Password shown once for webdav"));
    assert!(!html.contains("correct horse battery staple"));
}
