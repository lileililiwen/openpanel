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
