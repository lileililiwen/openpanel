//! Site HTTP-controls integration tests: REST round-trip, path
//! containment, secret hygiene, and rendered nginx output.

use crate::common::*;

async fn owner_and_site(server: &TestServer) -> (String, openpanel_domain::Site) {
    let token = server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    let owner = server
        .identity()
        .list_users()
        .await
        .expect("users")
        .into_iter()
        .find(|user| user.username().as_str() == "owner")
        .expect("owner");
    let site = server
        .sites()
        .create_site(
            &owner,
            owner.id(),
            "http-controls.example.test",
            vec![],
            false,
            None,
            None,
        )
        .await
        .expect("site");
    (token, site)
}

fn controls_body() -> serde_json::Value {
    serde_json::json!({
        "site_id": uuid::Uuid::new_v4(),
        "version": 1,
        "error_pages": [
            {"status": 404, "document_path": "/errors/404.html"}
        ],
        "redirects": [
            {"ordinal": 1, "source_prefix": "/old",
             "destination": "/new", "status": "moved_permanently"}
        ],
        "protected_dirs": [
            {"path_prefix": "/private", "realm": "restricted",
             "accounts": [{"name": "alice",
                           "password_hash": "$2b$12$ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFGHIJKLMNOPQR"}]}
        ],
        "hotlink": null,
        "ip_rules": [],
        "mime_overrides": [],
        "index_policy": null
    })
}

#[tokio::test]
async fn site_http_controls_rest_round_trip_and_rendering() {
    let server = TestServer::new().await;
    let (token, site) = owner_and_site(&server).await;
    let url = format!("{}/api/v1/sites/{}/http", server.base_url(), site.id());

    // Initial document is empty.
    let initial = server
        .client()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get initial");
    assert_eq!(initial.status(), 200);
    let body: serde_json::Value = initial.json().await.expect("json");
    assert_eq!(body["error_pages"], serde_json::json!([]));

    // PUT a full document; the path id wins over any body site_id.
    let mut payload = controls_body();
    payload["site_id"] = serde_json::json!(uuid::Uuid::new_v4());
    let saved = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .expect("put controls");
    assert_eq!(saved.status(), 200, "{}", saved.text().await.expect("body"));
    let saved: serde_json::Value = saved.json().await.expect("saved json");
    assert_eq!(
        saved["site_id"].as_str().expect("site id"),
        &site.id().to_string()
    );

    // GET round-trips the stored document.
    let loaded = server
        .client()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("get saved")
        .json::<serde_json::Value>()
        .await
        .expect("loaded json");
    assert_eq!(loaded["redirects"][0]["source_prefix"], "/old");

    // Rendered vhost contains the compiled snippet before `location /`.
    let config = std::fs::read_to_string(
        std::path::Path::new(&server.sandbox_path("active"))
            .join("http-controls.example.test.conf"),
    )
    .expect("rendered site config");
    assert!(config.contains("# openpanel-http-controls"));
    assert!(config.contains("error_page 404 /errors/404.html;"));
    assert!(config.contains("rewrite ^/old(.*)$ /new$1 permanent;"));
    assert!(config.contains("auth_basic \"restricted\";"));
    let snippet_at = config.find("# openpanel-http-controls").expect("snippet");
    let location_at = config.find("location /").expect("location");
    assert!(snippet_at < location_at);

    // htpasswd file exists with the account and mode 0600.
    use std::os::unix::fs::PermissionsExt;
    let sandbox_root = server.sandbox_path("");
    let auth_dir = std::path::Path::new(&sandbox_root)
        .join("http-auth")
        .join(site.id().to_string());
    let htpasswd = auth_dir.join("0.htpasswd");
    let contents = std::fs::read_to_string(&htpasswd).expect("htpasswd");
    assert!(contents.starts_with("alice:$2b$"));
    let mode = std::fs::metadata(&htpasswd)
        .expect("meta")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    // The rendered vhost never contains the hash.
    assert!(!config.contains("$2b$12$ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
}

#[tokio::test]
async fn site_http_controls_rejects_path_escape_and_loops() {
    let server = TestServer::new().await;
    let (token, site) = owner_and_site(&server).await;
    let url = format!("{}/api/v1/sites/{}/http", server.base_url(), site.id());

    let mut escaping = controls_body();
    escaping["site_id"] = serde_json::json!(site.id());
    escaping["error_pages"][0]["document_path"] = serde_json::json!("/../etc/passwd");
    let response = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&escaping)
        .send()
        .await
        .expect("escaping put");
    // The API maps domain validation failures to 422 validation_failed.
    assert_eq!(response.status(), 422);

    let mut looping = controls_body();
    looping["site_id"] = serde_json::json!(site.id());
    looping["error_pages"] = serde_json::json!([]);
    looping["redirects"] = serde_json::json!([
        {"ordinal": 1, "source_prefix": "/a", "destination": "/b",
         "status": "moved_permanently"},
        {"ordinal": 2, "source_prefix": "/b", "destination": "/c",
         "status": "moved_permanently"}
    ]);
    let response = server
        .client()
        .put(&url)
        .bearer_auth(&token)
        .json(&looping)
        .send()
        .await
        .expect("looping put");
    assert_eq!(response.status(), 422);
}

#[tokio::test]
async fn site_http_controls_is_owner_only() {
    let server = TestServer::new().await;
    let (_token, site) = owner_and_site(&server).await;
    server
        .identity()
        .create_user(
            "operator",
            "operator@example.test",
            "correct horse battery staple",
            openpanel_domain::Role::Admin,
            "test",
        )
        .await
        .expect("admin");
    let admin_token = server
        .login("operator", "correct horse battery staple")
        .await;
    let url = format!("{}/api/v1/sites/{}/http", server.base_url(), site.id());
    assert_eq!(
        server
            .client()
            .get(&url)
            .bearer_auth(&admin_token)
            .send()
            .await
            .expect("admin get")
            .status(),
        403
    );

    // Unauthenticated requests are rejected.
    assert_eq!(
        server
            .client()
            .get(&url)
            .send()
            .await
            .expect("anon get")
            .status(),
        401
    );
}

fn csrf_token(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html.find(marker).expect("csrf field") + marker.len();
    let end = html[start..].find('"').expect("csrf close");
    html[start..start + end].to_owned()
}

#[tokio::test]
async fn site_http_controls_web_page_renders_and_saves_with_csrf() {
    let server = TestServer::new().await;
    let (_token, site) = owner_and_site(&server).await;

    // Web login as the owner.
    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "owner"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .expect("web login");
    let cookie = login.headers()[reqwest::header::SET_COOKIE]
        .to_str()
        .expect("cookie")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_owned();

    let page_url = format!("{}/sites/{}/http", server.base_url(), site.id());
    let page = server
        .client()
        .get(&page_url)
        .header("cookie", &cookie)
        .send()
        .await
        .expect("page");
    assert_eq!(page.status(), 200);
    let body = page.text().await.expect("html");
    assert!(body.contains("HTTP controls"));
    let csrf = csrf_token(&body);

    // Wrong CSRF token is rejected.
    assert_eq!(
        server
            .client()
            .post(&page_url)
            .header("cookie", &cookie)
            .form(&[
                ("_csrf", "wrong"),
                (
                    "controls_json",
                    format!(
                        r#"{{"site_id":"{}","version":1,"error_pages":[],"redirects":[],"protected_dirs":[],"hotlink":null,"ip_rules":[],"mime_overrides":[],"index_policy":null}}"#,
                        uuid::Uuid::new_v4()
                    )
                    .as_str(),
                )
            ])
            .send()
            .await
            .expect("bad csrf")
            .status(),
        403
    );

    // Correct CSRF token saves and re-renders with a success banner.
    let controls = format!(
        r#"{{"site_id":"{}","version":1,"error_pages":[{{"status":404,"document_path":"/errors/404.html"}}],"redirects":[{{"ordinal":1,"source_prefix":"/old","destination":"/new","status":"moved_permanently"}}],"protected_dirs":[],"hotlink":null,"ip_rules":[],"mime_overrides":[],"index_policy":null}}"#,
        uuid::Uuid::new_v4()
    );
    let saved = server
        .client()
        .post(&page_url)
        .header("cookie", &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("controls_json", controls.as_str()),
        ])
        .send()
        .await
        .expect("save");
    assert_eq!(saved.status(), 200);
    let html = saved.text().await.expect("saved html");
    assert!(html.contains("HTTP controls saved"));

    // The site sub-navigation links the new tab.
    assert!(html.contains(&format!("/sites/{}/http", site.id())));
}
