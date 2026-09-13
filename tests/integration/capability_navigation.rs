//! Site workspace completion HTTP integration tests.
//!
//! Capability under test: `capability-navigation` (one discoverability
//! inventory; navigation and router agree; site workspace is complete and
//! scoped; role filtering is defense in depth).

use crate::common::*;

/// Extract the `openpanel_session` cookie value from a response's Set-Cookie header.
fn session_cookie(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("Set-Cookie header")
        .to_str()
        .expect("Set-Cookie string")
        .split(';')
        .next()
        .expect("cookie name=value")
        .to_string()
}

async fn login(server: &TestServer, username: &str, password: &str) -> String {
    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[("username_or_email", username), ("password", password)])
        .send()
        .await
        .expect("POST /login");
    assert_eq!(login.status(), 303, "login redirects");
    session_cookie(&login)
}

async fn authed_get(server: &TestServer, cookie: &str, path: &str) -> reqwest::Response {
    server
        .client()
        .get(format!("{}{}", server.base_url(), path))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .expect("authed GET")
}

async fn seed_site(server: &TestServer) -> uuid::Uuid {
    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list users")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin user");
    server
        .sites()
        .create_site(
            &admin,
            admin.id(),
            "workspace.example.com",
            vec!["www.workspace.example.com".into()],
            true,
            Some("8.3".into()),
            None,
        )
        .await
        .expect("create site")
        .id()
}

/// Every new site tab is reachable and preserves site context.
#[tokio::test]
async fn capability_navigation_site_tabs_are_reachable() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let site_id = seed_site(&server).await;

    for (tab, heading) in [
        ("domains", "Domains"),
        ("runtime", "Runtime"),
        ("logs", "Logs"),
        ("backups", "Backups"),
    ] {
        let resp = authed_get(&server, &cookie, &format!("/sites/{site_id}/{tab}")).await;
        assert_eq!(resp.status(), 200, "GET /sites/{{id}}/{tab}");
        let body = resp.text().await.expect("page body");
        assert!(
            body.contains("workspace.example.com"),
            "site identity preserved on {tab}: {body}"
        );
        assert!(
            body.contains("role=\"tablist\""),
            "workspace tab strip on {tab}: {body}"
        );
        assert!(
            body.contains("aria-current=\"page\""),
            "active tab marked on {tab}: {body}"
        );
        assert!(body.contains(heading), "heading on {tab}: {body}");
    }
}

/// Domains tab lists the primary domain and aliases; runtime tab shows PHP.
#[tokio::test]
async fn capability_navigation_domains_and_runtime_content() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let site_id = seed_site(&server).await;

    let resp = authed_get(&server, &cookie, &format!("/sites/{site_id}/domains")).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("workspace.example.com"), "primary: {body}");
    assert!(body.contains("www.workspace.example.com"), "alias: {body}");

    let resp = authed_get(&server, &cookie, &format!("/sites/{site_id}/runtime")).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("8.3"), "php version: {body}");
}

/// Unauthenticated requests redirect to `/login` (direct-access protection).
#[tokio::test]
async fn capability_navigation_site_tabs_require_login() {
    let server = TestServer::new().await;
    let site_id = uuid::Uuid::new_v4();
    for tab in ["domains", "runtime", "logs", "backups"] {
        let resp = server
            .client()
            .get(format!("{}/sites/{site_id}/{tab}", server.base_url()))
            .send()
            .await
            .expect("GET");
        assert_eq!(resp.status(), 302, "unauth {tab} redirects");
        assert_eq!(
            resp.headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok()),
            Some("/login"),
            "unauth {tab} targets login"
        );
    }
}

/// Unknown sites are 404 (distinct from unavailable/unauthorized states).
#[tokio::test]
async fn capability_navigation_unknown_site_is_not_found() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let missing = uuid::Uuid::new_v4();
    for tab in ["domains", "runtime", "logs", "backups"] {
        let resp = authed_get(&server, &cookie, &format!("/sites/{missing}/{tab}")).await;
        assert_eq!(resp.status(), 404, "missing site {tab} is 404");
    }
}
