//! Layer overlay surface: the single owner of every overlay on the panel —
//! modal, confirm (destructive actions only), toast, tip, and load.
//!
//! Each route returns a server-rendered HTMX fragment mounted into the
//! shell's `#layer-root` container. No feature route hand-rolls its own
//! modal or toast markup.

use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    http::{StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// Content type contract for every layer fragment.
fn html_fragment(markup: Markup) -> Response {
    let mut response = markup.into_string().into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

/// HTTP verb used by a confirm form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmMethod {
    /// `POST` (or HTMX `hx-post`).
    Post,
    /// `DELETE` (or HTMX `hx-delete`).
    Delete,
}

/// Static description of one destructive action served by `/layer/confirm`.
#[derive(Debug, Clone, Copy)]
pub struct ConfirmSpec {
    /// Form verb.
    pub method: ConfirmMethod,
    /// Whether the form submits via HTMX swap (`true`) or navigates
    /// (`false`, for handlers that redirect).
    pub htmx: bool,
    /// Action path template; `{name}` placeholders are substituted from
    /// the confirm query parameters.
    pub action: &'static str,
    /// HTMX swap target when `htmx` is true.
    pub target: &'static str,
    /// Modal heading.
    pub title: &'static str,
    /// Destructive description shown above the form.
    pub body: &'static str,
    /// Extra query parameters forwarded as hidden form inputs.
    pub hidden: &'static [&'static str],
}

const fn post(
    action: &'static str,
    target: &'static str,
    title: &'static str,
    body: &'static str,
    hidden: &'static [&'static str],
) -> ConfirmSpec {
    ConfirmSpec {
        method: ConfirmMethod::Post,
        htmx: true,
        action,
        target,
        title,
        body,
        hidden,
    }
}

const fn delete(
    action: &'static str,
    target: &'static str,
    title: &'static str,
    body: &'static str,
) -> ConfirmSpec {
    delete_with_hidden(action, target, title, body, &[])
}

const fn delete_with_hidden(
    action: &'static str,
    target: &'static str,
    title: &'static str,
    body: &'static str,
    hidden: &'static [&'static str],
) -> ConfirmSpec {
    ConfirmSpec {
        method: ConfirmMethod::Delete,
        htmx: true,
        action,
        target,
        title,
        body,
        hidden,
    }
}

const fn navigate(
    action: &'static str,
    title: &'static str,
    body: &'static str,
    hidden: &'static [&'static str],
) -> ConfirmSpec {
    ConfirmSpec {
        method: ConfirmMethod::Post,
        htmx: false,
        action,
        target: "",
        title,
        body,
        hidden,
    }
}

/// Every destructive action the panel routes through `/layer/confirm`.
fn confirm_spec(action: &str) -> Option<ConfirmSpec> {
    Some(match action {
        "delete-site" => delete(
            "/sites/{id}",
            "#site-list",
            "Delete site",
            "This permanently removes the site and its configuration. This cannot be undone.",
        ),
        "delete-database" => delete(
            "/databases/{id}",
            "#databases-list",
            "Delete database",
            "This permanently drops the database. This cannot be undone.",
        ),
        "reveal-database-password" => post(
            "/databases/{id}/reveal",
            "#password-panel",
            "Reveal database password",
            "The plaintext password is shown once. Store it now; OpenPanel cannot display it again.",
            &[],
        ),
        "revoke-cert" => post(
            "/ssl/{domain}/revoke",
            "#ssl-list",
            "Revoke certificate",
            "This revokes and deletes the certificate. This cannot be undone.",
            &[],
        ),
        "delete-entry" => delete_with_hidden(
            "/sites/{site_id}/files/remove",
            "#files-listing",
            "Delete file",
            "This permanently deletes the file. This cannot be undone.",
            &["path"],
        ),
        "delete-user" => delete(
            "/users/{id}",
            "#user-list",
            "Delete user",
            "This permanently removes the user account. This cannot be undone.",
        ),
        "delete-job" => navigate(
            "/cron/jobs/{id}/delete",
            "Delete cron job",
            "This permanently deletes the cron job.",
            &[],
        ),
        "delete-ftp-user" => navigate(
            "/sites/{site_id}/ftp/{account_id}/delete",
            "Delete FTP account",
            "This permanently deletes the FTP account.",
            &[],
        ),
        "delete-record" => navigate(
            "/dns/zones/{zone}/records/{record}/delete",
            "Delete DNS record",
            "This permanently deletes the DNS record.",
            &["zone", "record", "expected_version"],
        ),
        _ => return None,
    })
}

/// Substitute `{name}` placeholders in an action template from query params.
fn interpolate(template: &str, params: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find('}') else {
            out.push_str(tail);
            return out;
        };
        let name = &tail[1..end];
        out.push_str(params.get(name).map(String::as_str).unwrap_or(""));
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Percent-encode a query-string component (RFC 3986 unreserved bytes are
/// kept; everything else becomes `%XX`). Route files use this when building
/// `hx-get="/layer/confirm?..."` links whose parameters can contain spaces
/// or punctuation.
pub fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// GET /layer/modal — generic destructive modal fragment.
pub async fn modal(
    State(state): State<WebState>,
    WebUser(_user, session): WebUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let title = params.get("title").map(String::as_str).unwrap_or("Confirm");
    let body = params.get("body").map(String::as_str).unwrap_or("");
    let csrf = state.csrf.token_for(session.id());
    html_fragment(modal_fragment("layer-modal", title, body, &csrf, None))
}

/// GET /layer/confirm — destructive-action confirmation modal.
pub async fn confirm(
    State(state): State<WebState>,
    WebUser(_user, session): WebUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let action = params.get("action").map(String::as_str).unwrap_or("");
    let Some(spec) = confirm_spec(action) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Some(token) = params.get("csrf_token")
        && !state.csrf.verify(session.id(), token)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let url = interpolate(spec.action, &params);
    let id = params
        .get("id")
        .map(String::as_str)
        .unwrap_or("layer-confirm");
    html_fragment(modal_fragment(
        id,
        spec.title,
        spec.body,
        &csrf,
        Some((&spec, &url, &params)),
    ))
}

/// Render a modal fragment. When `confirm` is `Some`, the modal carries a
/// single state-changing form targeting the destructive endpoint.
fn modal_fragment(
    id: &str,
    title: &str,
    body: &str,
    csrf: &str,
    confirm: Option<(&ConfirmSpec, &str, &HashMap<String, String>)>,
) -> Markup {
    let label_id = format!("{id}-title");
    let body_id = format!("{id}-body");
    let fields = html! {
        (csrf_field(csrf))
        @if let Some((spec, _url, params)) = confirm {
            @for name in spec.hidden {
                @if let Some(value) = params.get(*name) {
                    input type="hidden" name=(*name) value=(value);
                }
            }
        }
        div class="form-actions" {
            @if confirm.is_some() {
                button type="submit" class="danger" { "Confirm" }
            }
            button type="button" class="op-modal-close" { "Cancel" }
        }
    };
    html! {
        div class="op-modal" id=(id) role="dialog" aria-modal="true"
            aria-labelledby=(label_id) aria-describedby=(body_id) {
            div class="op-modal-content" {
                h2 id=(label_id) { (title) }
                p id=(body_id) { (body) }
                @if let Some((spec, url, _params)) = confirm {
                    @if spec.htmx {
                        @match spec.method {
                            ConfirmMethod::Post => {
                                form class="form" hx-post=(url) hx-target=(spec.target) { (fields) }
                            }
                            ConfirmMethod::Delete => {
                                form class="form" hx-delete=(url) hx-target=(spec.target) { (fields) }
                            }
                        }
                    } @else {
                        form class="form" method="post" action=(url) { (fields) }
                    }
                } @else {
                    div class="form-actions" {
                        button type="button" class="op-modal-close" { "Close" }
                    }
                }
            }
        }
    }
}

/// GET /layer/toast — non-blocking routine feedback fragment.
pub async fn toast(
    State(_state): State<WebState>,
    WebUser(_user, _session): WebUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let kind = params.get("kind").map(String::as_str).unwrap_or("success");
    let kind = match kind {
        "success" | "error" | "info" | "warning" => kind,
        _ => "success",
    };
    let msg = params.get("msg").map(String::as_str).unwrap_or("");
    html_fragment(html! {
        div class=(format!("op-toast op-toast--{kind}")) data-op-auto-dismiss="4000" role="status" {
            (msg)
        }
    })
}

/// GET /layer/tip — non-critical help tooltip.
pub async fn tip(
    State(_state): State<WebState>,
    WebUser(_user, _session): WebUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let id = params.get("id").map(String::as_str).unwrap_or("tip");
    let label = params.get("label").map(String::as_str).unwrap_or("");
    html_fragment(html! {
        div class="op-tip" id=(format!("op-tip-{id}")) role="tooltip" { (label) }
    })
}

/// GET /layer/load — blocking load overlay fragment.
pub async fn load(
    State(_state): State<WebState>,
    WebUser(_user, _session): WebUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let label = params
        .get("label")
        .map(String::as_str)
        .unwrap_or("Working…");
    html_fragment(html! {
        div class="op-load" role="status" aria-live="polite" {
            span class="op-loading-spinner" aria-hidden="true";
            (label)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn confirm_spec_covers_all_destructive_actions() {
        for action in [
            "delete-site",
            "delete-database",
            "reveal-database-password",
            "revoke-cert",
            "delete-entry",
            "delete-user",
            "delete-job",
            "delete-ftp-user",
            "delete-record",
        ] {
            assert!(confirm_spec(action).is_some(), "missing spec for {action}");
        }
        assert!(confirm_spec("nope").is_none(), "unknown action rejected");
    }

    #[test]
    fn confirm_specs_are_destructive_only() {
        for action in [
            "delete-site",
            "delete-database",
            "revoke-cert",
            "delete-entry",
            "delete-user",
            "delete-job",
            "delete-ftp-user",
            "delete-record",
        ] {
            let spec = confirm_spec(action).expect("spec");
            assert!(
                matches!(spec.method, ConfirmMethod::Post | ConfirmMethod::Delete),
                "{action} must be destructive"
            );
        }
    }

    #[test]
    fn interpolate_substitutes_placeholders() {
        let p = params(&[("id", "abc"), ("site_id", "s1"), ("path", "x.txt")]);
        assert_eq!(interpolate("/sites/{id}", &p), "/sites/abc");
        assert_eq!(
            interpolate("/sites/{site_id}/files/remove", &p),
            "/sites/s1/files/remove"
        );
        assert_eq!(interpolate("/plain", &p), "/plain");
        assert_eq!(interpolate("/x/{missing}", &p), "/x/");
    }

    #[test]
    fn modal_fragment_embeds_csrf_and_close() {
        let out = modal_fragment("m", "Title", "Body", "tok123", None).into_string();
        assert!(out.contains("role=\"dialog\""), "dialog: {out}");
        assert!(out.contains("aria-modal=\"true\""), "modal aria: {out}");
        assert!(out.contains("aria-labelledby=\"m-title\""), "label: {out}");
        assert!(out.contains("op-modal-close"), "close: {out}");
    }

    #[test]
    fn confirm_fragment_carries_form_with_csrf_and_hidden_fields() {
        let spec = confirm_spec("delete-entry").expect("spec");
        let p = params(&[("id", "e1"), ("site_id", "s1"), ("path", "x.txt")]);
        let url = interpolate(spec.action, &p);
        let out = modal_fragment(
            "c",
            spec.title,
            spec.body,
            "tok123",
            Some((&spec, &url, &p)),
        )
        .into_string();
        assert!(out.contains("<form"), "form: {out}");
        assert!(
            out.contains("hx-delete=\"/sites/s1/files/remove\""),
            "delete verb: {out}"
        );
        assert!(
            out.contains("hx-target=\"#files-listing\""),
            "swap target: {out}"
        );
        assert!(
            out.contains("name=\"_csrf\" value=\"tok123\""),
            "csrf: {out}"
        );
        assert!(
            out.contains("name=\"path\" value=\"x.txt\""),
            "hidden path: {out}"
        );
        assert!(
            out.contains("type=\"submit\" class=\"danger\""),
            "danger submit: {out}"
        );
    }

    #[test]
    fn navigate_confirm_renders_plain_post_form() {
        let spec = confirm_spec("delete-job").expect("spec");
        let p = params(&[("id", "j1")]);
        let url = interpolate(spec.action, &p);
        let out = modal_fragment(
            "c",
            spec.title,
            spec.body,
            "tok123",
            Some((&spec, &url, &p)),
        )
        .into_string();
        assert!(
            !out.contains("hx-post"),
            "no htmx for redirect handler: {out}"
        );
        assert!(!out.contains("hx-delete"), "no hx-delete: {out}");
        assert!(
            out.contains("method=\"post\" action=\"/cron/jobs/j1/delete\""),
            "plain post: {out}"
        );
    }

    #[test]
    fn toast_kind_is_whitelisted() {
        for (kind, expected) in [
            ("success", "op-toast--success"),
            ("error", "op-toast--error"),
            ("info", "op-toast--info"),
            ("warning", "op-toast--warning"),
            ("bogus", "op-toast--success"),
        ] {
            let p = params(&[("kind", kind), ("msg", "Saved")]);
            let out = toast_fragment(&p).into_string();
            assert!(out.contains(expected), "{kind} → {expected}: {out}");
            assert!(
                out.contains("data-op-auto-dismiss=\"4000\""),
                "auto-dismiss: {out}"
            );
            assert!(out.contains("role=\"status\""), "status role: {out}");
            assert!(out.contains("Saved"), "message: {out}");
        }
    }

    fn toast_fragment(p: &HashMap<String, String>) -> Markup {
        let kind = p.get("kind").map(String::as_str).unwrap_or("success");
        let kind = match kind {
            "success" | "error" | "info" | "warning" => kind,
            _ => "success",
        };
        let msg = p.get("msg").map(String::as_str).unwrap_or("");
        html! {
            div class=(format!("op-toast op-toast--{kind}")) data-op-auto-dismiss="4000" role="status" {
                (msg)
            }
        }
    }
}
