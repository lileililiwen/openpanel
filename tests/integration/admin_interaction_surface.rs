//! Interaction surface integration tests: the layer overlay routes,
//! inline form validation, the ui-state vocabulary, and the feedback
//! widget — the `add-admin-interaction-surface` change.
//!
//! Covers the four capabilities: `modal-toast-confirm-surface`,
//! `inline-form-validation`, `ui-state-vocabulary`, `feedback-widget`.

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

/// Log in via the web form and return the `openpanel_session` cookie value.
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

/// Authenticated GET returning the body; asserts 200.
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

// =====================================================================
// modal-toast-confirm-surface
// =====================================================================

/// Every `/layer/*` route requires a valid session.
#[tokio::test]
async fn layer_routes_require_session() {
    let server = TestServer::new().await;
    for path in [
        "/layer/modal",
        "/layer/confirm?action=delete-site&id=00000000-0000-0000-0000-000000000000",
        "/layer/toast?kind=success&msg=Saved",
        "/layer/tip?id=help",
        "/layer/load?label=Working",
    ] {
        let resp = server
            .client()
            .get(format!("{}{}", server.base_url(), path))
            .send()
            .await
            .expect("unauthenticated GET");
        assert_eq!(resp.status(), 302, "unauthenticated {path} redirects");
        assert_eq!(
            resp.headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok()),
            Some("/login")
        );
    }
}

/// `GET /layer/toast` returns the dismissable auto-clearing fragment.
#[tokio::test]
async fn layer_toast_renders_dismissable_fragment() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .get(format!(
            "{}/layer/toast?kind=success&msg=Saved",
            server.base_url()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /layer/toast");
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("text/html; charset=utf-8")
    );
    let body = resp.text().await.expect("toast body");
    assert!(
        body.contains("class=\"op-toast op-toast--success\""),
        "toast class: {body}"
    );
    assert!(
        body.contains("data-op-auto-dismiss=\"4000\""),
        "auto-dismiss marker: {body}"
    );
    assert!(body.contains("role=\"status\""), "status role: {body}");
    assert!(body.contains("Saved"), "message: {body}");
}

/// `GET /layer/confirm` embeds the session CSRF token in a destructive form.
#[tokio::test]
async fn layer_confirm_embeds_csrf_and_action_form() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .get(format!(
            "{}/layer/confirm?action=delete-site&id={}",
            server.base_url(),
            uuid::Uuid::new_v4()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /layer/confirm");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("confirm body");
    assert!(body.contains("role=\"dialog\""), "modal: {body}");
    assert!(body.contains("aria-modal=\"true\""), "modal aria: {body}");
    assert!(body.contains("<form"), "confirm form: {body}");
    assert!(
        body.contains("hx-delete=\"/sites/"),
        "destructive verb: {body}"
    );
    assert!(
        body.contains("name=\"_csrf\" value=\""),
        "csrf embedded: {body}"
    );
    assert!(
        body.contains("type=\"submit\" class=\"danger\""),
        "danger submit: {body}"
    );
}

/// `GET /layer/confirm` rejects a caller-supplied CSRF token mismatch.
#[tokio::test]
async fn layer_confirm_rejects_wrong_csrf_token() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .get(format!(
            "{}/layer/confirm?action=delete-site&id={}&csrf_token=wrong-token",
            server.base_url(),
            uuid::Uuid::new_v4()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /layer/confirm with wrong csrf");
    assert_eq!(resp.status(), 403, "wrong csrf_token rejected");
}

/// `GET /layer/{modal,tip,load}` each render their fragment.
#[tokio::test]
async fn layer_modal_tip_and_load_render() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let modal = authed_get(
        &server,
        &cookie,
        "/layer/modal?title=Careful&body=This+removes+it",
    )
    .await;
    assert!(modal.contains("class=\"op-modal\""), "modal: {modal}");
    assert!(modal.contains("Careful"), "modal title: {modal}");

    let tip = authed_get(&server, &cookie, "/layer/tip?id=help&label=Inline+help").await;
    assert!(tip.contains("class=\"op-tip\""), "tip: {tip}");
    assert!(tip.contains("Inline help"), "tip label: {tip}");

    let load = authed_get(&server, &cookie, "/layer/load?label=Working").await;
    assert!(load.contains("class=\"op-load\""), "load: {load}");
    assert!(load.contains("Working"), "load label: {load}");
}

/// Destructive row actions route through `/layer/confirm`.
#[tokio::test]
async fn destructive_actions_route_through_confirm() {
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
            "confirm.example",
            vec![],
            false,
            None,
            None,
        )
        .await
        .expect("create site");

    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(
        body.contains("hx-get=\"/layer/confirm?action=delete-site"),
        "delete action routes through confirm: {body}"
    );
    assert!(
        !body.contains("hx-delete="),
        "no plain destructive button: {body}"
    );
}

// =====================================================================
// inline-form-validation
// =====================================================================

/// Invalid blur validation returns `422` with the OOB error fragment.
#[tokio::test]
async fn forms_validate_invalid_email_returns_422_with_oob() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .post(format!("{}/forms/validate", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("field", "email"),
            ("value", "not-an-email"),
            ("kind", "email"),
        ])
        .send()
        .await
        .expect("POST /forms/validate");
    assert_eq!(resp.status(), 422, "invalid field rejected");
    assert_eq!(
        resp.headers()
            .get("HX-Trigger")
            .and_then(|v| v.to_str().ok()),
        Some("form-validation-failed"),
        "focus-management trigger"
    );
    assert!(
        resp.headers().get("X-Form-Errors").is_some(),
        "JSON error contract header present"
    );
    let body = resp.text().await.expect("validation body");
    assert!(
        body.contains("class=\"field-error\"") && body.contains("role=\"alert\""),
        "inline error fragment: {body}"
    );
    assert!(body.contains("field-error-email"), "field id: {body}");
}

/// Valid blur validation returns `200` with the ok fragment.
#[tokio::test]
async fn forms_validate_valid_email_returns_ok_fragment() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .post(format!("{}/forms/validate", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("field", "email"),
            ("value", "admin@example.com"),
            ("kind", "email"),
        ])
        .send()
        .await
        .expect("POST /forms/validate");
    assert_eq!(resp.status(), 200, "valid field accepted");
    let body = resp.text().await.expect("ok body");
    assert!(
        body.contains("id=\"field-error-email\"") && body.contains("aria-hidden=\"true\""),
        "ok fragment clears the error: {body}"
    );
}

// =====================================================================
// ui-state-vocabulary
// =====================================================================

/// Empty list routes render the stable `op-empty-state` component.
#[tokio::test]
async fn ui_states_empty_list_renders_empty_state() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let body = authed_get(&server, &cookie, "/sites").await;
    assert!(
        body.contains("class=\"op-empty-state\""),
        "empty state class: {body}"
    );
    assert!(
        body.contains("aria-live=\"polite\""),
        "polite live region: {body}"
    );
    assert!(!body.contains("<table"), "no empty table body: {body}");
}

/// Empty monitoring history renders the empty state, not a blank panel.
#[tokio::test]
async fn ui_states_monitoring_history_renders_empty_state() {
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
        body.contains("class=\"op-empty-state\""),
        "empty state class: {body}"
    );
    assert!(body.contains("No samples"), "empty state title: {body}");
}

// =====================================================================
// feedback-widget
// =====================================================================

/// The shell mounts the feedback gate attribute; a fresh account gets no
/// embedded widget template, and the localStorage gate script is present.
#[tokio::test]
async fn feedback_shell_mounts_widget_gate() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let body = authed_get(&server, &cookie, "/").await;
    assert!(
        body.contains("id=\"feedback-widget-root\" data-account-age-days=\"0\""),
        "gate attribute reflects fresh account: {body}"
    );
    assert!(
        !body.contains("class=\"op-feedback-widget\""),
        "young account gets no widget template: {body}"
    );
    assert!(
        body.contains("openpanel_feedback_seen"),
        "localStorage dismissal gate wired: {body}"
    );
}

/// A valid feedback submission persists, emits the success toast trigger,
/// and the sixth submission in 24h is rate-limited with `429`.
#[tokio::test]
async fn feedback_submission_persists_and_rate_limits() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/").await);

    let submit = server
        .client()
        .post(format!("{}/feedback", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str()), ("sentiment", "up")])
        .send()
        .await
        .expect("POST /feedback");
    assert_eq!(submit.status(), 200, "first submission accepted");
    let trigger = submit
        .headers()
        .get("HX-Trigger")
        .and_then(|v| v.to_str().ok())
        .expect("toast trigger")
        .to_string();
    assert!(
        trigger.starts_with("layer-toast:"),
        "success toast trigger: {trigger}"
    );
    assert!(trigger.contains("Thanks for the feedback"), "toast msg");

    let admin = server
        .identity()
        .list_users()
        .await
        .expect("list users")
        .into_iter()
        .find(|u| u.username().as_str() == "admin")
        .expect("admin user");
    assert_eq!(
        server
            .feedback()
            .count_in_window(admin.id())
            .await
            .expect("count"),
        1,
        "one row persisted"
    );

    // Four more within the budget.
    for _ in 0..4 {
        let resp = server
            .client()
            .post(format!("{}/feedback", server.base_url()))
            .header(reqwest::header::COOKIE, &cookie)
            .form(&[("_csrf", csrf.as_str()), ("sentiment", "down")])
            .send()
            .await
            .expect("POST /feedback within budget");
        assert_eq!(resp.status(), 200);
    }
    let sixth = server
        .client()
        .post(format!("{}/feedback", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str()), ("sentiment", "up")])
        .send()
        .await
        .expect("POST /feedback sixth");
    assert_eq!(sixth.status(), 429, "sixth submission rate limited");
    let body = sixth.text().await.expect("rate-limit body");
    assert!(
        body.contains("class=\"form-error\""),
        "widget shows inline error: {body}"
    );
}

/// Missing sentiment is rejected inline with `422`.
#[tokio::test]
async fn feedback_missing_sentiment_returns_422() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;
    let csrf = csrf_token_from_html(&authed_get(&server, &cookie, "/").await);

    let resp = server
        .client()
        .post(format!("{}/feedback", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str())])
        .send()
        .await
        .expect("POST /feedback without sentiment");
    assert_eq!(resp.status(), 422, "missing sentiment rejected");
    let body = resp.text().await.expect("body");
    assert!(
        body.contains("Choose a rating."),
        "inline error rendered: {body}"
    );
}

/// A state-changing feedback POST with a bad CSRF token is rejected.
#[tokio::test]
async fn feedback_bad_csrf_returns_403() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let cookie = login(&server, "admin", "correct horse battery staple").await;

    let resp = server
        .client()
        .post(format!("{}/feedback", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", "wrong-token"), ("sentiment", "up")])
        .send()
        .await
        .expect("POST /feedback bad csrf");
    assert_eq!(resp.status(), 403, "bad csrf rejected");
}
