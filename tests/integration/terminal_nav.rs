//! Terminal navigation + saved-commands HTTP integration tests.
//!
//! Capabilities under test: `terminal-nav-and-saved-commands` (new;
//! homed on `terminal-host-fleet`) and the `capability-navigation`
//! discoverability delta for `/terminal`: Owner renders the three
//! panels with CSRF; unauthenticated redirects; User role is denied
//! without leaking secrets; server/command mutations are
//! Owner + CSRF gated with metadata-only rendering.

use openpanel_domain::Role;
use openpanel_web::capability_registry;

use crate::common::*;

fn session_cookie(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("Set-Cookie")
        .to_str()
        .expect("cookie str")
        .split(';')
        .next()
        .expect("name=value")
        .to_string()
}

async fn login(server: &TestServer, username: &str, password: &str) -> String {
    let resp = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[("username_or_email", username), ("password", password)])
        .send()
        .await
        .expect("POST /login");
    assert_eq!(resp.status(), 303);
    session_cookie(&resp)
}

async fn seed_owner(server: &TestServer) -> String {
    server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    login(server, "owner", "correct horse battery staple").await
}

fn csrf_token_from_html(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html.find(marker).expect("csrf field in html") + marker.len();
    let end = html[start..].find('"').expect("csrf value end") + start;
    html[start..end].to_string()
}

async fn owner_cookie_and_csrf(server: &TestServer) -> (String, String) {
    let cookie = seed_owner(server).await;
    let body = server
        .client()
        .get(format!("{}/terminal", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /terminal")
        .text()
        .await
        .expect("body");
    let csrf = csrf_token_from_html(&body);
    (cookie, csrf)
}

/// GET /terminal as Owner renders the three panels with a CSRF
/// token; unauthenticated callers are redirected to /login.
#[tokio::test]
async fn terminal_page_renders() {
    let server = TestServer::new().await;
    let (cookie, _) = owner_cookie_and_csrf(&server).await;
    let resp = server
        .client()
        .get(format!("{}/terminal", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /terminal");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("<h1>Terminal</h1>"), "page title: {body}");
    assert!(body.contains("Local session"), "session panel: {body}");
    assert!(body.contains("Server List"), "server panel: {body}");
    assert!(body.contains("Add server"), "server form: {body}");
    assert!(body.contains("Add command"), "command form: {body}");
    assert!(body.contains("name=\"_csrf\""), "csrf field: {body}");

    let anon = server
        .client()
        .get(format!("{}/terminal", server.base_url()))
        .send()
        .await
        .expect("anon GET /terminal");
    assert_eq!(anon.status(), 302, "unauthenticated redirects");
    let location = anon
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("location")
        .to_str()
        .expect("location str");
    assert_eq!(location, "/login");
}

/// POST /terminal/servers requires Owner role and valid CSRF; the
/// list never echoes secrets.
#[tokio::test]
async fn terminal_servers_gated_and_redacted() {
    let server = TestServer::new().await;
    let (cookie, csrf) = owner_cookie_and_csrf(&server).await;

    let resp = server
        .client()
        .post(format!("{}/terminal/servers", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("ip", "192.0.2.10"),
            ("port", "2222"),
            ("account", "deploy"),
            ("password", "s3cr3t-password-value"),
            ("private_key", "fake-key-bytes-0000"),
            ("remarks", "production web node"),
        ])
        .send()
        .await
        .expect("POST /terminal/servers");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("192.0.2.10"), "ip shown: {body}");
    assert!(body.contains("deploy"), "account shown: {body}");
    assert!(
        !body.contains("s3cr3t-password-value"),
        "password leaked: {body}"
    );
    assert!(!body.contains("fake-key-bytes-0000"), "key leaked: {body}");

    // Bad CSRF is refused.
    let bad = server
        .client()
        .post(format!("{}/terminal/servers", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", "wrong-token"),
            ("ip", "192.0.2.11"),
            ("port", "22"),
            ("account", "deploy"),
            ("password", ""),
            ("private_key", ""),
            ("remarks", ""),
        ])
        .send()
        .await
        .expect("bad csrf POST");
    assert_eq!(bad.status(), 403, "bad csrf refused");

    // A User-role caller is denied without leaking anything.
    server
        .identity()
        .create_user(
            "member",
            "member@example.test",
            "correct horse battery staple",
            Role::User,
            "test",
        )
        .await
        .expect("create user");
    let user_cookie = login(&server, "member", "correct horse battery staple").await;
    let denied = server
        .client()
        .get(format!("{}/terminal", server.base_url()))
        .header(reqwest::header::COOKIE, &user_cookie)
        .send()
        .await
        .expect("user GET /terminal");
    assert_eq!(denied.status(), 403, "user denied");
    let denied_body = denied.text().await.expect("body");
    assert!(
        !denied_body.contains("s3cr3t-password-value"),
        "no leak on deny: {denied_body}"
    );
    let denied_post = server
        .client()
        .post(format!("{}/terminal/servers", server.base_url()))
        .header(reqwest::header::COOKIE, &user_cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("ip", "192.0.2.12"),
            ("port", "22"),
            ("account", "deploy"),
            ("password", ""),
            ("private_key", ""),
            ("remarks", ""),
        ])
        .send()
        .await
        .expect("user POST servers");
    assert_eq!(denied_post.status(), 403, "user post denied");
}

/// POST /terminal/commands stores the classification and the list
/// renders it; dangerous rows carry the confirmation gate.
#[tokio::test]
async fn terminal_commands_classified() {
    let server = TestServer::new().await;
    let (cookie, csrf) = owner_cookie_and_csrf(&server).await;

    let resp = server
        .client()
        .post(format!("{}/terminal/commands", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("label", "restart panel"),
            ("command", "systemctl restart openpanel"),
        ])
        .send()
        .await
        .expect("POST /terminal/commands");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("Controls a system service"), "label: {body}");
    assert!(
        body.contains("I confirm this dangerous command"),
        "confirm gate: {body}"
    );

    let safe = server
        .client()
        .post(format!("{}/terminal/commands", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("label", "list"),
            ("command", "ls -la"),
        ])
        .send()
        .await
        .expect("safe command POST");
    assert_eq!(safe.status(), 200);
    let safe_body = safe.text().await.expect("body");
    assert!(safe_body.contains("Safe"), "safe label: {safe_body}");
}

/// Registry/route agreement: `find_by_route("/terminal")` resolves
/// and the static router-mount guard covers `/terminal`.
#[tokio::test]
async fn terminal_discoverable() {
    let entry = capability_registry::find_by_route("/terminal").expect("terminal entry");
    assert_eq!(entry.key, "terminal");
    assert_eq!(entry.capability, "terminal");
    assert_eq!(entry.label, "Terminal");
    assert!(matches!(
        entry.scope,
        capability_registry::RouteScope::Global
    ));
    assert!(capability_registry::template_matches(
        "/terminal",
        "/terminal"
    ));
    assert!(!capability_registry::role_may_see(Role::User, entry));
    assert!(capability_registry::role_may_see(Role::Owner, entry));

    // Mounted-route guard: every global registry route is mounted.
    const ROUTER: &str = include_str!("../../crates/openpanel-web/src/router.rs");
    assert!(
        ROUTER.contains("\"/terminal\""),
        "terminal route mounted in router.rs"
    );
}
