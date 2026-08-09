use crate::common::*;

/// Extract the `openpanel_session` cookie value from a response's Set-Cookie header.
fn session_cookie(resp: &reqwest::Response) -> String {
    let value = resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("Set-Cookie header")
        .to_str()
        .expect("Set-Cookie string")
        .to_string();
    value
        .split(';')
        .next()
        .expect("cookie name=value")
        .to_string()
}

/// Extract the CSRF token from a rendered form: `name="_csrf" value="..."`.
fn csrf_token_from_html(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html.find(marker).expect("csrf field in html") + marker.len();
    let end = html[start..].find('"').expect("closing quote");
    html[start..start + end].to_string()
}

/// Unauthenticated `GET /` redirects (302) to `/login`.
#[tokio::test]
async fn web_unauthenticated_root_redirects_to_login() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/", server.base_url()))
        .send()
        .await
        .expect("GET /");
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );
}

/// Valid login sets the session cookie, redirects to `/`, and the shell
/// renders the logged-in user's name.
#[tokio::test]
async fn web_valid_login_sets_cookie_and_renders_shell() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;

    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "admin"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .expect("POST /login");

    assert_eq!(login.status(), 303, "successful login redirects");
    let cookie = session_cookie(&login);
    assert!(
        cookie.starts_with("openpanel_session="),
        "cookie set: {cookie}"
    );
    assert_eq!(
        login
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/")
    );

    let home = server
        .client()
        .get(format!("{}/", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET / with cookie");
    assert_eq!(home.status(), 200);
    let body = home.text().await.expect("home body");
    assert!(body.contains("admin"), "shell shows user name: {body}");
    assert!(body.contains("Log out"));
}

/// Invalid login returns 401, renders the error form, and sets no cookie.
#[tokio::test]
async fn web_invalid_login_returns_401_no_cookie() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;

    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "admin"),
            ("password", "wrong-password"),
        ])
        .send()
        .await
        .expect("POST /login");

    assert_eq!(login.status(), 401);
    assert!(
        login.headers().get(reqwest::header::SET_COOKIE).is_none(),
        "no cookie on failure"
    );
    let body = login.text().await.expect("error body");
    assert!(
        body.contains("invalid credentials"),
        "error form rendered: {body}"
    );
}

/// Authenticated logout invalidates the session and redirects to `/login`.
#[tokio::test]
async fn web_logout_invalidates_session() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;

    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "admin"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .expect("POST /login");
    let cookie = session_cookie(&login);

    // Fetch the shell to get a fresh CSRF token for the session.
    let home = server
        .client()
        .get(format!("{}/", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /");
    let html = home.text().await.expect("home body");
    let csrf = csrf_token_from_html(&html);

    let logout = server
        .client()
        .post(format!("{}/logout", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", &csrf)])
        .send()
        .await
        .expect("POST /logout");
    assert_eq!(logout.status(), 303, "logout redirects");
    assert_eq!(
        logout
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );

    // The old cookie no longer authenticates.
    let after = server
        .client()
        .get(format!("{}/", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET / after logout");
    assert_eq!(after.status(), 302, "session invalidated after logout");
}

/// A state-changing POST with a wrong or missing CSRF token returns 403 and
/// makes no state change (the session survives).
#[tokio::test]
async fn web_bad_csrf_rejected_with_403() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;

    let login = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[
            ("username_or_email", "admin"),
            ("password", "correct horse battery staple"),
        ])
        .send()
        .await
        .expect("POST /login");
    let cookie = session_cookie(&login);

    // Wrong token.
    let wrong = server
        .client()
        .post(format!("{}/logout", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", "definitely-wrong")])
        .send()
        .await
        .expect("POST /logout wrong csrf");
    assert_eq!(wrong.status(), 403, "wrong csrf rejected");

    // Missing token.
    let missing = server
        .client()
        .post(format!("{}/logout", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", "")])
        .send()
        .await
        .expect("POST /logout missing csrf");
    assert_eq!(missing.status(), 403, "missing csrf rejected");

    // Session still valid — no state change happened.
    let still = server
        .client()
        .get(format!("{}/", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET / after rejected csrf");
    assert_eq!(still.status(), 200, "session not invalidated by bad csrf");
}

/// The embedded htmx script is served with the right content type.
#[tokio::test]
async fn web_htmx_asset_served() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/assets/htmx.min.js", server.base_url()))
        .send()
        .await
        .expect("GET htmx.min.js");
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/javascript")
    );
}

/// Log in as the bootstrap owner and return the session cookie.
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

/// Authenticated `GET /` renders the dashboard inside the shell: the gauge
/// region, the quick-count card region, and the alerts region.
#[tokio::test]
async fn web_dashboard_renders_gauges_cards_and_alerts() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .get(format!("{}/", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("dashboard body");
    assert!(body.contains("id=\"host-gauges\""), "gauge region: {body}");
    assert!(body.contains("class=\"cards\""), "card region: {body}");
    assert!(body.contains("Recent alerts"), "alerts region: {body}");
    // The card grid links to every resource page.
    for href in ["/sites", "/databases", "/files", "/ssl", "/users"] {
        assert!(
            body.contains(&format!("href=\"{href}\"")),
            "card link {href}"
        );
    }
    // Shell chrome is present.
    assert!(body.contains("admin"), "shell shows user name");
    assert!(body.contains("Log out"));
}

/// `GET /dashboard/gauges` returns the auto-refresh partial with host values.
#[tokio::test]
async fn web_dashboard_gauges_partial_returns_host_values() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .get(format!("{}/dashboard/gauges", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /dashboard/gauges");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("gauges partial body");
    assert!(
        body.contains("id=\"host-gauges\""),
        "partial region: {body}"
    );
    assert!(
        body.contains("hx-trigger=\"every 30s\""),
        "refresh trigger: {body}"
    );
    // Real host values render as percentages / a load figure.
    assert!(body.contains("CPU"), "cpu gauge label");
    assert!(body.contains("Memory"), "memory gauge label");
    assert!(body.contains("Disk"), "disk gauge label");
    assert!(body.contains("Load"), "load gauge label");
    assert!(body.contains("%"), "percent values rendered");
}

/// `GET /dashboard` is an explicit alias for the dashboard page.
#[tokio::test]
async fn web_dashboard_alias_renders_dashboard() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .get(format!("{}/dashboard", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /dashboard");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("dashboard body");
    assert!(
        body.contains("id=\"host-gauges\""),
        "alias renders dashboard"
    );
}

/// Unauthenticated `GET /dashboard` redirects to `/login`.
#[tokio::test]
async fn web_unauthenticated_dashboard_redirects_to_login() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/dashboard", server.base_url()))
        .send()
        .await
        .expect("GET /dashboard");
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );
}

/// Fetch an authenticated page and return `(cookie, body)`.
async fn authed_get(server: &TestServer, cookie: &str, path: &str) -> String {
    let resp = server
        .client()
        .get(format!("{}{}", server.base_url(), path))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .expect("authed GET");
    assert_eq!(resp.status(), 200, "GET {path}");
    resp.text().await.expect("page body")
}

/// Unauthenticated `GET /sites` redirects to `/login` (1.11).
#[tokio::test]
async fn web_unauthenticated_sites_redirects_to_login() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/sites", server.base_url()))
        .send()
        .await
        .expect("GET /sites");
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );
}

/// Authenticated `GET /sites` with no sites renders the empty state (1.5).
#[tokio::test]
async fn web_sites_empty_state_renders() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(body.contains("No sites yet"), "empty state: {body}");
    assert!(
        body.contains("href=\"/sites/new\""),
        "create action shown for owner: {body}"
    );
}

/// Authenticated `GET /sites` lists each created site with its fields (1.5).
#[tokio::test]
async fn web_sites_lists_created_sites() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

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
            "example.com",
            vec!["www.example.com".into()],
            true,
            Some("8.3".into()),
            None,
        )
        .await
        .expect("create site");

    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(body.contains("example.com"), "domain: {body}");
    assert!(body.contains("8.3"), "php version: {body}");
    assert!(body.contains("active"), "status: {body}");
    assert!(body.contains("admin"), "owner: {body}");
    assert!(
        body.contains("hx-confirm"),
        "delete requires confirmation: {body}"
    );
}

/// `POST /sites` with a valid form creates the site and validates CSRF (1.6).
#[tokio::test]
async fn web_sites_create_valid_form() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/sites").await);

    let resp = server
        .client()
        .post(format!("{}/sites", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("primary_domain", "new-site.test"),
            ("owner_id", ""),
            ("aliases", "www.new-site.test"),
            ("php_enabled", "on"),
            ("php_version", "8.2"),
            ("document_root", ""),
        ])
        .send()
        .await
        .expect("POST /sites");
    assert_eq!(resp.status(), 303, "create redirects to list");

    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(body.contains("new-site.test"), "created site: {body}");
    assert!(body.contains("8.2"), "php version: {body}");
}

/// `POST /sites` with a wrong CSRF token is rejected with 403 (1.6).
#[tokio::test]
async fn web_sites_create_bad_csrf_rejected() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .post(format!("{}/sites", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", "not-the-token"),
            ("primary_domain", "csrf-test.test"),
            ("owner_id", ""),
        ])
        .send()
        .await
        .expect("POST /sites bad csrf");
    assert_eq!(resp.status(), 403, "bad csrf rejected");

    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(!body.contains("csrf-test.test"), "nothing created: {body}");
}

/// A duplicate-domain create shows the inline error and creates nothing (1.7).
#[tokio::test]
async fn web_sites_duplicate_domain_shows_inline_error() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/sites").await);

    let create = |domain: &str| {
        let client = server.client();
        client
            .post(format!("{}/sites", server.base_url()))
            .header(reqwest::header::COOKIE, &cookie)
            .form(&[
                ("_csrf", csrf.as_str()),
                ("primary_domain", domain),
                ("owner_id", ""),
            ])
    };
    assert_eq!(
        create("dup.test")
            .send()
            .await
            .expect("first create")
            .status(),
        303,
        "first create succeeds"
    );

    let dup = create("dup.test")
        .send()
        .await
        .expect("duplicate create")
        .text()
        .await
        .expect("dup body");
    assert!(
        dup.contains("duplicate domain: dup.test"),
        "inline error rendered: {dup}"
    );

    let body = authed_get(&server, &cookie, "/sites").await;
    let rows = body.matches("class=\"status\"").count();
    assert_eq!(rows, 1, "exactly one site row: {body}");
    assert!(
        !body.contains("duplicate domain"),
        "list page itself is clean: {body}"
    );
}

/// Enable/disable flips the site status in the list (1.8).
#[tokio::test]
async fn web_sites_enable_disable_flips_status() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/sites").await);

    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list users")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin user");
    let site = server
        .sites()
        .create_site(&admin, admin.id(), "toggle.test", vec![], false, None, None)
        .await
        .expect("create site");

    let disable = server
        .client()
        .post(format!("{}/sites/{}/disable", server.base_url(), site.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .expect("POST disable");
    assert_eq!(disable.status(), 200, "disable swaps list");
    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(body.contains("disabled"), "site shows disabled: {body}");

    let enable = server
        .client()
        .post(format!("{}/sites/{}/enable", server.base_url(), site.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .expect("POST enable");
    assert_eq!(enable.status(), 200, "enable swaps list");
    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(body.contains("active"), "site shows active: {body}");
}

/// Delete requires confirmation (`hx-confirm`) and removes the site (1.9).
#[tokio::test]
async fn web_sites_delete_removes_site() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/sites").await);

    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list users")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin user");
    let site = server
        .sites()
        .create_site(
            &admin,
            admin.id(),
            "delete-me.test",
            vec![],
            false,
            None,
            None,
        )
        .await
        .expect("create site");

    let listed = authed_get(&server, &cookie, "/sites").await;
    assert!(
        listed.contains(&format!("hx-delete=\"/sites/{}\"", site.id()))
            && listed.contains("hx-confirm"),
        "delete row is confirmable: {listed}"
    );

    let del = server
        .client()
        .delete(format!("{}/sites/{}", server.base_url(), site.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .expect("DELETE site");
    assert_eq!(del.status(), 200, "delete swaps list");

    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(
        !body.contains("delete-me.test"),
        "site removed from list: {body}"
    );
}

/// A non-owner sees only their own sites and no create/delete actions (1.10).
#[tokio::test]
async fn web_sites_non_owner_sees_own_sites_only() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;

    // Second user with the plain `user` role.
    let alice = server
        .identity()
        .create_user(
            "alice",
            "alice@example.com",
            "alice-password-1",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .expect("create alice");
    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list users")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin user");

    // Admin creates one site for alice and one for themselves.
    server
        .sites()
        .create_site(
            &admin,
            alice.id(),
            "alice-site.test",
            vec![],
            false,
            None,
            None,
        )
        .await
        .expect("alice's site");
    server
        .sites()
        .create_site(
            &admin,
            admin.id(),
            "admin-site.test",
            vec![],
            false,
            None,
            None,
        )
        .await
        .expect("admin's site");

    let cookie = login(&server, "alice", "alice-password-1").await;
    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(body.contains("alice-site.test"), "own site shown: {body}");
    assert!(
        !body.contains("admin-site.test"),
        "other's site hidden: {body}"
    );
    assert!(
        !body.contains("href=\"/sites/new\""),
        "no create action for non-owner: {body}"
    );
    assert!(!body.contains("Delete"), "no delete action: {body}");
    assert!(!body.contains("hx-confirm"), "no confirm dialogs: {body}");

    // The new-site page itself is forbidden for non-owners.
    let new_page = server
        .client()
        .get(format!("{}/sites/new", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /sites/new as non-owner");
    assert_eq!(new_page.status(), 403, "new-site forbidden");
}
