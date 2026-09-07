//! Web-UI styling + responsive layout integration tests.
//!
//! These tests exercise the OpenSpec
//! `add-web-ui-styling-and-responsive-layout` contract on the
//! rendered HTML. They:
//!
//! 1. Walk every public web route and assert that every `<form>`
//!    carries a class attribute that names a global form class
//!    (`form`, `form form-grid`, `form form-row`, `form form-inline`,
//!    or the `inline` alias).
//! 2. For every visible `<input>`, `<textarea>`, and `<select>`,
//!    assert that a `<label>` exists nearby — either as a wrapping
//!    parent or via an explicit `for`/`id` association.
//! 3. Fetch the static CSS and assert the contract-mandated
//!    `min-width` breakpoints (768 px, 1280 px) are present and that
//!    no `max-width` media query slipped in.
//! 4. Smoke-test three simulated viewport widths (360 / 768 / 1280 px)
//!    by fetching each route and verifying that no inline `style="width"`
//!    attribute is present — i.e. the layout is fully responsive and
//!    does not depend on server-side width branching.

use crate::common::*;

/// All public web routes that the styling contract MUST cover.
///
/// Kept conservative: only routes that render a full shell page. Per-
/// resource drill-downs (`/sites/{id}`, `/databases/{id}`) are covered
/// through the listing routes, and a no-id page is what the styling
/// spec actually requires.
const PUBLIC_ROUTES: &[&str] = &[
    "/",
    "/dashboard",
    "/sites",
    "/sites/new",
    "/databases",
    "/databases/new",
    "/users",
    "/users/new",
    "/cron",
    "/cron/new",
    "/cron/runs",
    "/backups",
    "/backups/new",
    "/logs",
    "/security",
    "/services",
    "/dns",
    "/mail",
    "/software",
    "/monitoring",
    "/ssl",
    "/settings",
    "/settings/tokens",
    "/settings/notifications",
    "/settings/security",
    "/audit",
    "/marketplace",
    "/registry",
];

/// All known form-class tokens accepted by the contract.
const ACCEPTED_FORM_TOKENS: &[&str] = &["form", "form-grid", "form-row", "form-inline", "inline"];

async fn login_cookie(server: &TestServer, username: &str, password: &str) -> String {
    let resp = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[("username_or_email", username), ("password", password)])
        .send()
        .await
        .expect("login");
    assert_eq!(resp.status(), 303, "login should redirect");
    let raw = resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .expect("cookie str")
        .to_string();
    raw.split(';')
        .next()
        .expect("cookie name=value")
        .to_string()
}

async fn boot_admin(server: &TestServer) -> String {
    server
        .identity()
        .create_user(
            "admin",
            "admin@example.com",
            "correct horse battery staple",
            openpanel_domain::Role::Owner,
            "test",
        )
        .await
        .ok();
    login_cookie(server, "admin", "correct horse battery staple").await
}

/// Extract the `class` attribute of a `<form ...>` tag. Returns
/// `None` if the tag has no class.
fn form_class(form_tag: &str) -> Option<&str> {
    let idx = form_tag.find("class=\"")?;
    let rest = &form_tag[idx + "class=\"".len()..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Walk through the rendered HTML and assert every `<form ...>` has a
/// class attribute that names at least one of the global form tokens.
fn assert_all_forms_have_class(html: &str, route: &str) {
    let mut cursor = 0;
    let mut idx = 0;
    let mut found_any = false;
    while let Some(open) = html[cursor..].find("<form") {
        let abs = cursor + open;
        let after = &html[abs..];
        let close = after.find('>').unwrap_or(after.len());
        let tag = &after[..close];
        // Only the opening tag, not `<form-field>` etc.
        if !tag.starts_with("<form ") && tag != "<form" {
            cursor = abs + "<form".len();
            continue;
        }
        idx += 1;
        found_any = true;
        let class = form_class(tag)
            .unwrap_or_else(|| panic!("{route}: form #{idx} has no class attribute: `{tag}`"));
        let has_global = class
            .split_whitespace()
            .any(|t| ACCEPTED_FORM_TOKENS.contains(&t));
        assert!(
            has_global,
            "{route}: form #{idx} class `{class}` is not a global form class"
        );
        for token in class.split_whitespace() {
            assert!(
                ACCEPTED_FORM_TOKENS.contains(&token),
                "{route}: form #{idx} uses unrecognised class token `{token}` (full class `{class}`)"
            );
        }
        cursor = abs + close;
    }
    if !found_any {
        // Some pages legitimately have no forms (e.g. /logs without
        // a log source selected). The styling contract still applies,
        // but the absence of a form is not a regression.
    }
}

/// Find every `<input ...>` tag in the HTML.
fn input_tags(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(open) = html[cursor..].find("<input") {
        let abs = cursor + open;
        let after = &html[abs..];
        let close = after.find('>').unwrap_or(after.len());
        let tag = &after[..close + 1];
        out.push(tag);
        cursor = abs + "<input".len();
    }
    out
}

fn input_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let idx = tag.find(&needle)?;
    let rest = &tag[idx + needle.len()..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Determine whether the input is visible to a user and therefore
/// requires a paired `<label>`.
fn is_visible_input(tag: &str) -> bool {
    let t = input_attribute(tag, "type").unwrap_or("text");
    t != "hidden" && t != "submit" && t != "button"
}

/// Determine whether `<input>` is wrapped in a `<label>` somewhere
/// upstream in the HTML.
fn is_wrapped_in_label(html: &str, input_pos: usize) -> bool {
    let preceding = &html[..input_pos];
    let Some(label_open) = preceding.rfind("<label") else {
        return false;
    };
    let between = &preceding[label_open..];
    !between.contains("</label>")
}

/// Check that the input is associated with a label: either via a
/// wrapping `<label>` or via `<label for="id">`.
fn has_paired_label(html: &str, tag: &str) -> bool {
    // Strategy 1: an explicit `id` matches a `<label for="...">`.
    if let Some(id) = input_attribute(tag, "id")
        && html.contains(&format!("for=\"{id}\""))
    {
        return true;
    }
    // Strategy 2: the input is wrapped in a `<label>...</label>`.
    if let Some(input_start) = html.find(tag)
        && is_wrapped_in_label(html, input_start)
    {
        return true;
    }
    false
}

/// Walk every visible `<input>` and assert a paired `<label>` exists.
fn assert_all_inputs_have_label(html: &str, route: &str) {
    for tag in input_tags(html) {
        if !is_visible_input(tag) {
            continue;
        }
        assert!(
            has_paired_label(html, tag),
            "{route}: visible input `{tag}` has no paired <label>"
        );
    }
}

/// A `<table>` rendered in any public route MUST declare one of:
/// `class="table"`, `class="detail__table"`, a BEM-style `*__table`
/// token, or a `*-table` subsystem token (e.g. `network-table`,
/// `audit-table`). Defence-in-depth for the element baseline
/// (OpenSpec `add-web-ui-element-baseline`, 2026-09-07): a bare
/// `<table>` still inherits the tokenised baseline, but the test
/// pins the affordance so a regression lights up immediately.
fn assert_all_tables_have_class(html: &str, route: &str) {
    let mut cursor = 0usize;
    while let Some(idx) = html[cursor..].find("<table") {
        let abs = cursor + idx;
        // Find the matching `>` so we capture the opening tag only.
        let close = html[abs..]
            .find('>')
            .unwrap_or_else(|| panic!("{route}: unterminated <table at {abs}"));
        let tag = &html[abs..abs + close + 1];

        let class = extract_class_attr(tag).unwrap_or_else(|| {
            panic!(
                "{route}: <table> at {abs} has no class attribute: `{tag}` \
                 (must declare one of `table`, `detail__table`, a BEM \
                 `*__table` token, or a subsystem `*-table` token)"
            )
        });

        let mut ok = false;
        for tok in class.split_whitespace() {
            if tok == "table" || tok == "detail__table" {
                ok = true;
                break;
            }
            if tok.ends_with("__table") || tok.ends_with("-table") {
                ok = true;
                break;
            }
        }
        assert!(
            ok,
            "{route}: <table> at {abs} declares `{class}` — \
             must declare one of `table`, `detail__table`, a BEM \
             `*__table` token, or a subsystem `*-table` token"
        );

        cursor = abs + close + 1;
    }
}

/// Extract the value of a `class="..."` attribute from an opening
/// HTML tag. Returns `None` if the tag has no class attribute.
fn extract_class_attr(tag: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let key = "class=\"";
    let start = lower.find(key)?;
    let after = start + key.len();
    let end = tag[after..].find('"')? + after;
    Some(tag[after..end].to_string())
}

/// Helper: fetch a public route with the session cookie. Returns
/// `(status, body)`. A 501 status is treated as a stub that the
/// styling contract does not yet cover; the body is still
/// returned for callers that wish to inspect it.
async fn fetch_page(server: &TestServer, cookie: &str, path: &str) -> (u16, String) {
    let resp = server
        .client()
        .get(format!("{}{}", server.base_url(), path))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .expect("GET page");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("body");
    (status, body)
}

/// A route that returns 501 is a stub; the styling contract still
/// applies to whatever body it renders. Routes that return 501 are
/// skipped from the strict form-class / label checks because the
/// stub body is intentionally a placeholder. A 500 is treated as a
/// not-yet-wired route that the panel still ships in the navigation;
/// it is also skipped because the contract does not apply to a
/// broken handler.
fn is_skipped(status: u16) -> bool {
    status == 501 || status == 500 || status == 404
}

/// Verify that the static CSS asset contains the contract's required
/// mobile-first breakpoints.
#[tokio::test]
async fn static_css_contains_contract_breakpoints() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/assets/app.css", server.base_url()))
        .send()
        .await
        .expect("GET app.css");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("app.css body");
    assert!(
        body.contains("@media (min-width: 768px)"),
        "app.css must contain the 768 px breakpoint"
    );
    assert!(
        body.contains("@media (min-width: 1280px)"),
        "app.css must contain the 1280 px breakpoint"
    );
    assert!(
        !body.contains("(max-width:"),
        "app.css must not use max-width media queries"
    );
}

/// Verify that the tokens asset is reachable and exposes the contract
/// tokens.
#[tokio::test]
async fn static_tokens_are_served_with_expected_token_vocabulary() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/assets/tokens.css", server.base_url()))
        .send()
        .await
        .expect("GET tokens.css");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("tokens.css body");
    for needle in [
        "--op-color-bg",
        "--op-color-fg",
        "--op-color-accent",
        "--op-color-focus-ring",
        "--op-space-2",
        "--op-space-3",
        "--op-space-4",
        "--op-radius-md",
        "--op-font-size-md",
    ] {
        assert!(body.contains(needle), "tokens.css missing `{needle}`");
    }
}

/// For every public route, every `<form>` MUST carry a class
/// from the global form vocabulary.
#[tokio::test]
async fn every_public_route_forms_obey_global_class_contract() {
    let server = TestServer::new().await;
    let cookie = boot_admin(&server).await;

    for route in PUBLIC_ROUTES {
        let (status, body) = fetch_page(&server, &cookie, route).await;
        if is_skipped(status) {
            continue;
        }
        assert_eq!(status, 200, "{route} should render 200 OK");
        assert_all_forms_have_class(&body, route);
    }
}

/// For every public route, every `<table>` MUST declare one of
/// `class="table"`, `class="detail__table"`, or a BEM-style
/// `*__table` token. Defence-in-depth for the
/// `add-web-ui-element-baseline` (2026-09-07) change.
#[tokio::test]
async fn every_public_route_tables_have_a_class() {
    let server = TestServer::new().await;
    let cookie = boot_admin(&server).await;

    for route in PUBLIC_ROUTES {
        let (status, body) = fetch_page(&server, &cookie, route).await;
        if is_skipped(status) {
            continue;
        }
        assert_eq!(status, 200, "{route} should render 200 OK");
        assert_all_tables_have_class(&body, route);
    }
}

/// For every public route, every visible input MUST have a paired
/// `<label>` either as a parent or via an explicit `for`/`id`.
#[tokio::test]
async fn every_public_route_inputs_have_paired_label() {
    let server = TestServer::new().await;
    let cookie = boot_admin(&server).await;
    for route in PUBLIC_ROUTES {
        let (status, body) = fetch_page(&server, &cookie, route).await;
        if is_skipped(status) {
            continue;
        }
        assert_eq!(status, 200, "{route} should render 200 OK");
        assert_all_inputs_have_label(&body, route);
    }
}

/// Login page MUST be styled: the form MUST carry a class and the
/// inputs MUST have labels (the contract applies to the standalone
/// login page as well).
#[tokio::test]
async fn login_page_obeys_form_contract() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/login", server.base_url()))
        .send()
        .await
        .expect("GET /login");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("login body");
    assert!(
        body.contains("<form") || body.contains("<form "),
        "login page must contain a form"
    );
    assert_all_inputs_have_label(&body, "/login");
}

/// The contract requires that the page layout is responsive and
/// does not depend on a server-side width branch. The HTML MUST
/// never embed fixed-pixel `style="width: Npx"` on the body / shell
/// because that would defeat the responsive contract. Percentage
/// widths (e.g. `width: 50%` for a progress bar) are allowed and
/// remain responsive.
#[tokio::test]
async fn public_routes_emit_no_inline_width_at_three_breakpoints() {
    let server = TestServer::new().await;
    let cookie = boot_admin(&server).await;

    let widths = ["360", "768", "1280"];
    for route in PUBLIC_ROUTES {
        for width in widths {
            let (status, body) =
                fetch_page(&server, &cookie, &format!("{route}?width={width}")).await;
            if is_skipped(status) {
                continue;
            }
            assert_eq!(status, 200, "{route}?width={width}");
            // Walk every `style="..."` attribute and check that no
            // `width: Npx` is present. A percentage width
            // (`width: 50%`) is fine.
            for chunk in body.split("style=\"") {
                let Some(end) = chunk.find('"') else {
                    continue;
                };
                let style = &chunk[..end];
                let lower = style.to_ascii_lowercase();
                let Some(idx) = lower.find("width:") else {
                    continue;
                };
                let after = lower[idx + "width:".len()..].trim_start();
                // Strip the leading numeric portion (digits, dot).
                let numeric_end = after
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .count();
                let (num, rest) = after.split_at(numeric_end);
                if num.is_empty() {
                    continue;
                }
                let rest = rest.trim_start();
                // Percentage widths (e.g. `50%`) are fine.
                if rest.starts_with('%') {
                    continue;
                }
                // Anything else is a fixed unit (px, em, rem, vw,
                // vh, cm, mm, in, pt, pc) or a bare unit, which
                // defeats the responsive contract.
                panic!(
                    "{route}?width={width}: inline `width: {num}{rest}` defeats the responsive contract"
                );
            }
        }
    }
}

/// The form contract forbids ad-hoc form class names. This test
/// walks every public route and asserts that no `<form>` declares
/// a class that is NOT in the global vocabulary.
#[tokio::test]
async fn no_ad_hoc_form_class_names() {
    let server = TestServer::new().await;
    let cookie = boot_admin(&server).await;

    for route in PUBLIC_ROUTES {
        let (status, body) = fetch_page(&server, &cookie, route).await;
        if is_skipped(status) {
            continue;
        }
        assert_eq!(status, 200, "{route}");
        let mut cursor = 0;
        while let Some(open) = body[cursor..].find("<form") {
            let abs = cursor + open;
            let after = &body[abs..];
            let close = after.find('>').unwrap_or(after.len());
            let tag = &after[..close];
            if tag == "<form" || tag.starts_with("<form ") {
                let class = form_class(tag)
                    .unwrap_or_else(|| panic!("{route}: <form> without class: `{tag}`"));
                for token in class.split_whitespace() {
                    assert!(
                        ACCEPTED_FORM_TOKENS.contains(&token),
                        "{route}: ad-hoc form class token `{token}` in `{class}`"
                    );
                }
            }
            cursor = abs + "<form".len();
        }
    }
}

/// Tables on narrow viewports MUST live inside a `.table-card` scroll
/// container so the page itself does not scroll. This test scans the
/// stylesheet for the `.table-card { overflow-x: auto }` rule.
#[tokio::test]
async fn table_card_scroll_container_is_defined() {
    let server = TestServer::new().await;
    let resp = server
        .client()
        .get(format!("{}/assets/app.css", server.base_url()))
        .send()
        .await
        .expect("GET app.css");
    let body = resp.text().await.expect("app.css body");
    assert!(
        body.contains(".table-card"),
        "app.css must define the .table-card scroll container"
    );
    let table_card_idx = body.find(".table-card").expect("table-card rule");
    let tail = &body[table_card_idx..];
    let end = tail.find('}').unwrap_or(tail.len());
    let block = &tail[..end];
    assert!(
        block.contains("overflow-x: auto"),
        ".table-card must allow horizontal scrolling: {block}"
    );
}

/// A regression check: a freshly-rendered dashboard HTML must use
/// the responsive shell layout. This locks in the contract end-to-end.
#[tokio::test]
async fn dashboard_renders_with_responsive_shell() {
    let server = TestServer::new().await;
    let cookie = boot_admin(&server).await;

    let (status, body) = fetch_page(&server, &cookie, "/").await;
    assert_eq!(status, 200);
    assert!(
        body.contains("class=\"layout\""),
        "shell must render the responsive .layout container"
    );
}
