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

// =====================================================================
// User management pages
// =====================================================================

/// Unauthenticated `GET /users` redirects to `/login` (1.12).
#[tokio::test]
async fn web_unauthenticated_users_redirects_to_login() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/users", server.base_url()))
        .send()
        .await
        .expect("GET /users");
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );
}

/// Non-owner `GET /users` renders a forbidden message, not the table (1.6).
#[tokio::test]
async fn web_users_non_owner_forbidden_message() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    server
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
    let cookie = login(&server, "alice", "alice-password-1").await;
    let body = authed_get(&server, &cookie, "/users").await;
    assert!(
        body.to_lowercase().contains("forbidden"),
        "forbidden msg: {body}"
    );
    assert!(
        !body.contains("<table"),
        "no user table for non-owner: {body}"
    );
}

/// Owner creates a user and the row appears in `GET /users` (1.5).
#[tokio::test]
async fn web_users_owner_creates_user_appears_in_list() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/users").await);

    let create = server
        .client()
        .post(format!("{}/users", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("username", "bob"),
            ("email", "bob@example.com"),
            ("password", "bob-password-1"),
            ("role", "user"),
        ])
        .send()
        .await
        .expect("POST /users");
    assert_eq!(create.status(), 303, "create redirects");

    let body = authed_get(&server, &cookie, "/users").await;
    assert!(body.contains("bob"), "row visible: {body}");
    assert!(body.contains("bob@example.com"), "email visible: {body}");
    assert!(body.contains("user"), "role visible: {body}");
}

/// A role change updates the row; demoting the last owner is rejected (1.7).
#[tokio::test]
async fn web_users_role_change_and_last_owner_protection() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    server
        .identity()
        .create_user(
            "carol",
            "carol@example.com",
            "carol-password-1",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .expect("create carol");
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/users").await);

    let carol = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "carol")
        .expect("carol");

    let promote = server
        .client()
        .post(format!("{}/users/{}/role", server.base_url(), carol.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str()), ("role", "admin")])
        .send()
        .await
        .expect("POST role");
    assert_eq!(promote.status(), 200, "promote swaps row");

    // The user is now an admin; the list page re-renders with the new role.
    let body = authed_get(&server, &cookie, "/users").await;
    assert!(body.contains("admin"), "new role visible: {body}");

    // Demote the only owner (admin) — should be rejected inline.
    let admin_user = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin");
    let demote = server
        .client()
        .post(format!(
            "{}/users/{}/role",
            server.base_url(),
            admin_user.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str()), ("role", "user")])
        .send()
        .await
        .expect("POST role demote last owner");
    assert_eq!(demote.status(), 200, "rejected but swap returned");
    let body = demote.text().await.expect("demote body");
    assert!(
        body.contains("cannot demote the last owner"),
        "inline error: {body}"
    );
}

/// Disable/enable toggles the user status (1.8).
#[tokio::test]
async fn web_users_disable_enable_toggles_status() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    server
        .identity()
        .create_user(
            "dave",
            "dave@example.com",
            "dave-password-1",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .expect("create dave");
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/users").await);

    let dave = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "dave")
        .expect("dave");

    let disable = server
        .client()
        .post(format!("{}/users/{}/disable", server.base_url(), dave.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .expect("POST disable");
    assert_eq!(disable.status(), 200, "disable swaps row");
    let body = authed_get(&server, &cookie, "/users").await;
    assert!(body.contains("disabled"), "user shown as disabled: {body}");

    let enable = server
        .client()
        .post(format!("{}/users/{}/enable", server.base_url(), dave.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .expect("POST enable");
    assert_eq!(enable.status(), 200, "enable swaps row");
    let body = authed_get(&server, &cookie, "/users").await;
    assert!(body.contains("active"), "user shown as active: {body}");
}

/// Password reset succeeds without the value being displayed (1.9).
#[tokio::test]
async fn web_users_password_reset_does_not_echo_value() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    server
        .identity()
        .create_user(
            "eve",
            "eve@example.com",
            "eve-password-1",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .expect("create eve");
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/users").await);

    let eve = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "eve")
        .expect("eve");

    let secret = "the-new-secret-1234";
    let resp = server
        .client()
        .post(format!("{}/users/{}/password", server.base_url(), eve.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str()), ("password", secret)])
        .send()
        .await
        .expect("POST password");
    assert_eq!(resp.status(), 200, "password swap returned");
    let body = resp.text().await.expect("password body");
    assert!(!body.contains(secret), "secret never rendered: {body}");
    assert!(
        body.contains("password updated") || body.contains("updated"),
        "success note: {body}"
    );
}

/// Delete requires confirmation and removes the user (1.10).
#[tokio::test]
async fn web_users_delete_removes_user() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    server
        .identity()
        .create_user(
            "frank",
            "frank@example.com",
            "frank-password-1",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .expect("create frank");
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/users").await);

    let frank = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "frank")
        .expect("frank");

    let listed = authed_get(&server, &cookie, "/users").await;
    assert!(
        listed.contains(&format!("hx-delete=\"/users/{}\"", frank.id()))
            && listed.contains("hx-confirm"),
        "delete row is confirmable: {listed}"
    );

    let del = server
        .client()
        .delete(format!("{}/users/{}", server.base_url(), frank.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .expect("DELETE user");
    assert_eq!(del.status(), 200, "delete swaps row");

    let body = authed_get(&server, &cookie, "/users").await;
    assert!(!body.contains("frank"), "user removed: {body}");
}

/// CSRF mismatch on a mutation returns 403 (1.11).
#[tokio::test]
async fn web_users_bad_csrf_rejected() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    server
        .identity()
        .create_user(
            "gina",
            "gina@example.com",
            "gina-password-1",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .expect("create gina");
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let gina = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "gina")
        .expect("gina");

    let resp = server
        .client()
        .post(format!("{}/users/{}/disable", server.base_url(), gina.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", "not-the-token")])
        .send()
        .await
        .expect("POST disable bad csrf");
    assert_eq!(resp.status(), 403, "bad csrf rejected");

    // gina is still enabled.
    let gina_after = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "gina")
        .expect("gina");
    assert!(!gina_after.is_disabled(), "no state change");
}

// =====================================================================
// Monitoring pages
// =====================================================================

/// Unauthenticated `GET /monitoring` redirects to `/login` (1.8).
#[tokio::test]
async fn web_unauthenticated_monitoring_redirects_to_login() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/monitoring", server.base_url()))
        .send()
        .await
        .expect("GET /monitoring");
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );
}

/// Authenticated `GET /monitoring` renders the page chrome, the metric + range
/// selectors, and the `#history-chart` / `#alert-feed` swap targets.
#[tokio::test]
async fn web_monitoring_page_renders_selectors_and_targets() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let body = authed_get(&server, &cookie, "/monitoring").await;
    for needle in [
        "Monitoring",
        "id=\"history-chart\"",
        "id=\"alert-feed\"",
        "name=\"metric\"",
        "name=\"range\"",
        "Cpu",
        "Memory",
        "Disk",
        "Network",
    ] {
        assert!(body.contains(needle), "missing {needle}: {body}");
    }
}

/// Seeded samples render as an SVG polyline (1.5).
#[tokio::test]
async fn web_monitoring_history_returns_svg_polylines() {
    use openpanel_domain::monitoring::{MetricKind, MetricSample, Unit};

    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    // Seed three CPU samples via the service repo handle.
    let now = chrono::Utc::now();
    let samples = vec![
        MetricSample::new(MetricKind::Cpu, Unit::Percent, 10.0, now).expect("sample"),
        MetricSample::new(
            MetricKind::Cpu,
            Unit::Percent,
            50.0,
            now + chrono::Duration::seconds(30),
        )
        .expect("sample"),
        MetricSample::new(
            MetricKind::Cpu,
            Unit::Percent,
            80.0,
            now + chrono::Duration::seconds(60),
        )
        .expect("sample"),
    ];
    for s in &samples {
        server
            .monitoring()
            .repo()
            .insert(s)
            .await
            .expect("insert sample");
    }

    let resp = server
        .client()
        .get(format!(
            "{}/monitoring/history?metric=Cpu&range=3600",
            server.base_url()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET history");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("history body");
    assert!(body.contains("<svg"), "svg: {body}");
    assert!(body.contains("<polyline"), "polyline: {body}");
    assert!(body.contains("points="), "points attribute: {body}");
}

/// Empty history returns the placeholder (1.6).
#[tokio::test]
async fn web_monitoring_history_empty_renders_placeholder() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let body = authed_get(
        &server,
        &cookie,
        "/monitoring/history?metric=Memory&range=3600",
    )
    .await;
    assert!(
        body.contains("no samples") || body.contains("No samples"),
        "placeholder: {body}"
    );
}

/// Alerts feed renders the audit-driven alert events or empty state (1.7).
#[tokio::test]
async fn web_monitoring_alerts_feed_renders_empty_state() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let body = authed_get(&server, &cookie, "/monitoring/alerts").await;
    // Empty state: no alert events recorded yet.
    assert!(body.contains("No alerts"), "empty state: {body}");
}

// =====================================================================
// SSL pages
// =====================================================================

/// Unauthenticated `GET /ssl` redirects to `/login` (1.9).
#[tokio::test]
async fn web_unauthenticated_ssl_redirects_to_login() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/ssl", server.base_url()))
        .send()
        .await
        .expect("GET /ssl");
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );
}

/// Empty `GET /ssl` renders the empty state.
#[tokio::test]
async fn web_ssl_empty_list_renders() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let body = authed_get(&server, &cookie, "/ssl").await;
    assert!(
        body.contains("No certificates") || body.contains("no certificates"),
        "empty state: {body}"
    );
    assert!(body.contains("Issue"), "issue action: {body}");
}

/// Self-signed create → certificate appears in `GET /ssl` (1.5).
#[tokio::test]
async fn web_ssl_self_signed_appears_in_list() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    server
        .ssl()
        .generate_self_signed("example.com", 30)
        .await
        .expect("self-signed");
    let body = authed_get(&server, &cookie, "/ssl").await;
    assert!(body.contains("example.com"), "domain: {body}");
    assert!(body.contains("self_signed"), "source: {body}");
}

/// No rendered SSL page contains the bytes `PRIVATE KEY` (1.4).
#[tokio::test]
async fn web_ssl_metadata_only_no_private_key_leak() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    server
        .ssl()
        .generate_self_signed("leaktest.example", 30)
        .await
        .expect("self-signed");

    for path in ["/ssl", "/ssl/new", "/ssl/leaktest.example"] {
        let body = authed_get(&server, &cookie, path).await;
        assert!(
            !body.contains("PRIVATE KEY"),
            "page {path} leaks PRIVATE KEY: {body}"
        );
    }
}

/// Force-https toggle flips the row state (1.6).
#[tokio::test]
async fn web_ssl_force_https_toggle() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/ssl").await);

    server
        .ssl()
        .generate_self_signed("forcetest.example", 30)
        .await
        .expect("self-signed");

    let listed = authed_get(&server, &cookie, "/ssl").await;
    let initial = server
        .ssl()
        .get("forcetest.example")
        .await
        .expect("get cert")
        .force_https;
    assert!(initial, "force_https default true");
    assert!(
        listed.contains("class=\"force-https\">on<"),
        "row shows initial force-https: {listed}"
    );

    let toggle = server
        .client()
        .patch(format!(
            "{}/ssl/forcetest.example/force-https",
            server.base_url()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str()), ("on", "false")])
        .send()
        .await
        .expect("PATCH force-https");
    assert_eq!(toggle.status(), 200, "toggle swaps row");
    let after = server
        .ssl()
        .get("forcetest.example")
        .await
        .expect("get cert after toggle")
        .force_https;
    assert!(!after, "force_https flipped off");
}

/// Revoke with confirmation removes the row (1.7).
#[tokio::test]
async fn web_ssl_revoke_removes_row() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/ssl").await);

    server
        .ssl()
        .generate_self_signed("revoketest.example", 30)
        .await
        .expect("self-signed");

    let listed = authed_get(&server, &cookie, "/ssl").await;
    assert!(
        listed.contains("hx-confirm")
            && listed.contains("hx-post=\"/ssl/revoketest.example/revoke\""),
        "row has confirmable revoke: {listed}"
    );

    let del = server
        .client()
        .post(format!(
            "{}/ssl/revoketest.example/revoke",
            server.base_url()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .expect("POST revoke");
    assert_eq!(del.status(), 200, "revoke swaps row");

    let body = authed_get(&server, &cookie, "/ssl").await;
    assert!(!body.contains("revoketest.example"), "row removed: {body}");
}

/// CSRF mismatch on a state-changing POST returns 403 (1.8).
#[tokio::test]
async fn web_ssl_bad_csrf_rejected() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .post(format!(
            "{}/ssl/some-domain.example/revoke",
            server.base_url()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", "not-the-token")])
        .send()
        .await
        .expect("POST revoke bad csrf");
    assert_eq!(resp.status(), 403, "bad csrf rejected");
}

// =====================================================================
// File manager pages
// =====================================================================

/// Unauthenticated `GET /sites/{id}/files` redirects to `/login` (1.9).
#[tokio::test]
async fn web_unauthenticated_files_redirects_to_login() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!(
            "{}/sites/00000000-0000-0000-0000-000000000000/files",
            server.base_url()
        ))
        .send()
        .await
        .expect("GET files");
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );
}

/// Listing + read + write + mkdir + rename + chmod + delete via the web (1.4).
#[tokio::test]
async fn web_files_full_workflow() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin");
    let doc_root = server.sandbox_path("web-files-test");
    let site = server
        .sites()
        .create_site(
            &admin,
            admin.id(),
            "files-test.example",
            vec![],
            false,
            None,
            Some(doc_root.clone()),
        )
        .await
        .expect("create site");
    let csrf = csrf_token_from_html(
        &authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await,
    );

    // Write file
    let write = server
        .client()
        .post(format!(
            "{}/sites/{}/files/write",
            server.base_url(),
            site.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("path", "hello.txt"),
            ("contents", "hello world"),
        ])
        .send()
        .await
        .expect("POST write");
    assert_eq!(write.status(), 200, "write returns refreshed listing");

    // Listing shows the file
    let listing = authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await;
    assert!(listing.contains("hello.txt"), "file listed: {listing}");

    // Read file
    let read = server
        .client()
        .get(format!(
            "{}/sites/{}/files/read?path=hello.txt",
            server.base_url(),
            site.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET read");
    assert_eq!(read.status(), 200, "read ok");
    let body = read.text().await.expect("read body");
    assert!(body.contains("hello world"), "contents rendered: {body}");

    // mkdir
    let csrf = csrf_token_from_html(
        &authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await,
    );
    let mkdir = server
        .client()
        .post(format!(
            "{}/sites/{}/files/mkdir",
            server.base_url(),
            site.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str()), ("path", "subdir")])
        .send()
        .await
        .expect("POST mkdir");
    assert_eq!(mkdir.status(), 200, "mkdir ok");

    // rename
    let csrf = csrf_token_from_html(
        &authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await,
    );
    let rename = server
        .client()
        .post(format!(
            "{}/sites/{}/files/rename",
            server.base_url(),
            site.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("from", "hello.txt"),
            ("to", "renamed.txt"),
        ])
        .send()
        .await
        .expect("POST rename");
    assert_eq!(rename.status(), 200, "rename ok");

    // chmod
    let csrf = csrf_token_from_html(
        &authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await,
    );
    let chmod = server
        .client()
        .post(format!(
            "{}/sites/{}/files/chmod",
            server.base_url(),
            site.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("path", "renamed.txt"),
            ("mode", "600"),
        ])
        .send()
        .await
        .expect("POST chmod");
    assert_eq!(chmod.status(), 200, "chmod ok");

    // delete (with confirmation hint visible in listing)
    let listed = authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await;
    assert!(
        listed.contains("hx-confirm") && listed.contains("hx-delete="),
        "delete is confirmable: {listed}"
    );

    let csrf = csrf_token_from_html(
        &authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await,
    );
    let del = server
        .client()
        .delete(format!(
            "{}/sites/{}/files/remove",
            server.base_url(),
            site.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str()), ("path", "renamed.txt")])
        .send()
        .await
        .expect("DELETE remove");
    assert_eq!(del.status(), 200, "delete ok");

    let final_listing = authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await;
    assert!(
        !final_listing.contains("renamed.txt"),
        "renamed file removed: {final_listing}"
    );
}

/// `..` escape attempt rejected by the service and shown inline (1.5).
#[tokio::test]
async fn web_files_escape_attempt_rejected() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin");
    let doc_root = server.sandbox_path("web-files-escape");
    let site = server
        .sites()
        .create_site(
            &admin,
            admin.id(),
            "escape-test.example",
            vec![],
            false,
            None,
            Some(doc_root),
        )
        .await
        .expect("create site");
    let resp = server
        .client()
        .get(format!(
            "{}/sites/{}/files?path=..",
            server.base_url(),
            site.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET files with ..");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(
        body.contains("`..` not allowed") || body.contains("invalid path"),
        "service error rendered: {body}"
    );
}

/// CSRF mismatch on a files mutation returns 403 (1.8).
#[tokio::test]
async fn web_files_bad_csrf_rejected() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin");
    let doc_root = server.sandbox_path("web-files-csrf");
    let site = server
        .sites()
        .create_site(
            &admin,
            admin.id(),
            "csrf-test.example",
            vec![],
            false,
            None,
            Some(doc_root),
        )
        .await
        .expect("create site");
    let resp = server
        .client()
        .post(format!(
            "{}/sites/{}/files/mkdir",
            server.base_url(),
            site.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", "not-the-token"), ("path", "subdir")])
        .send()
        .await
        .expect("POST mkdir bad csrf");
    assert_eq!(resp.status(), 403, "bad csrf rejected");
}

/// A non-owner user cannot access another user's site files (1.10).
#[tokio::test]
async fn web_files_rbac_blocks_other_user() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin");
    let doc_root = server.sandbox_path("web-files-rbac");
    let site = server
        .sites()
        .create_site(
            &admin,
            admin.id(),
            "rbac-test.example",
            vec![],
            false,
            None,
            Some(doc_root),
        )
        .await
        .expect("create site");
    server
        .identity()
        .create_user(
            "mallory",
            "mallory@example.com",
            "mallory-password-1",
            openpanel_domain::Role::User,
            "test",
        )
        .await
        .expect("create mallory");
    let cookie = login(&server, "mallory", "mallory-password-1").await;
    let resp = server
        .client()
        .get(format!("{}/sites/{}/files", server.base_url(), site.id()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET files as other user");
    // RBAC: non-owner user is forbidden.
    assert_eq!(
        resp.status(),
        403,
        "non-owner user blocked: {}",
        resp.text().await.unwrap_or_default()
    );
}

/// Upload a file and confirm it appears in the listing (1.6).
#[tokio::test]
async fn web_files_upload_appears_in_listing() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin");
    let doc_root = server.sandbox_path("web-files-upload");
    let site = server
        .sites()
        .create_site(
            &admin,
            admin.id(),
            "upload-test.example",
            vec![],
            false,
            None,
            Some(doc_root),
        )
        .await
        .expect("create site");
    let csrf = csrf_token_from_html(
        &authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await,
    );

    let form = reqwest::multipart::Form::new()
        .text("_csrf", csrf.clone())
        .text("path", "")
        .text("filename", "upload.txt")
        .part(
            "file",
            reqwest::multipart::Part::text("uploaded body")
                .file_name("upload.txt")
                .mime_str("text/plain")
                .expect("mime"),
        );
    let resp = server
        .client()
        .post(format!(
            "{}/sites/{}/files/upload",
            server.base_url(),
            site.id()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .multipart(form)
        .send()
        .await
        .expect("POST upload");
    assert_eq!(resp.status(), 200, "upload returns refreshed listing");

    let body = authed_get(&server, &cookie, &format!("/sites/{}/files", site.id())).await;
    assert!(body.contains("upload.txt"), "uploaded file listed: {body}");
}
