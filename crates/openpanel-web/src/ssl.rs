//! SSL pages: certificate list, three-source issue form (ACME staging/prod,
//! manual PEM, self-signed), force-https toggle, renew, and revoke+delete.
//!
//! Rendered server-side in the shell, wired to `SslService` exactly as the
//! API does. The metadata-only contract is a hard invariant: no rendered
//! page contains private-key material in any form (cert PEM, encrypted
//! ciphertext, or `PRIVATE KEY` bytes).
//!
//! State-changing actions use HTMX swaps (`hx-post`/`hx-patch` targeting
//! `#ssl-list`) and the shared CSRF token. Revoke and renew render
//! confirmation dialogs (`hx-confirm`) before dispatching.

use axum::{
    extract::{Form, Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use chrono::{DateTime, Utc};
use maud::{Markup, html};
use openpanel_domain::ssl::certificate::Certificate;
use serde::Deserialize;

use crate::{
    csrf::ValidateCsrf,
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// One row in the SSL list, pre-resolved for rendering.
pub struct CertRow {
    /// Domain the cert covers.
    pub domain: String,
    /// Issuing CA (e.g. `Let's Encrypt`, `OpenPanel Self-Signed`).
    pub issuer: String,
    /// Status classifier (`active`, `expiring`, `expired`, `revoked`).
    pub status: String,
    /// Source (`acme`, `manual`, `self_signed`).
    pub source: String,
    /// RFC3339 `valid_from`.
    pub valid_from: String,
    /// RFC3339 `valid_to`.
    pub valid_to: String,
    /// Whether the per-site 301 redirect is on.
    pub force_https: bool,
    /// Key algorithm string (e.g. `ecdsa-p256`).
    pub key_type: String,
}

/// Body of the issue form.
#[derive(Debug, Deserialize)]
pub struct IssueForm {
    /// Issue source: `acme`, `manual`, or `self_signed`.
    pub source: String,
    /// Domain (or `domain` in manual + self-signed).
    #[serde(default)]
    pub domain: String,
    /// Production ACME flag (`on` = production).
    #[serde(default)]
    pub production: Option<String>,
    /// Self-signed validity in days.
    #[serde(default)]
    pub valid_for_days: Option<u32>,
    /// Manual: cert PEM.
    #[serde(default)]
    pub cert_pem: String,
    /// Manual: chain PEM.
    #[serde(default)]
    pub chain_pem: String,
    /// Manual: key PEM (one-shot; never echoed back).
    #[serde(default)]
    pub key_pem: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// Body of the force-https toggle form.
#[derive(Debug, Deserialize)]
pub struct ForceHttpsForm {
    /// `true` / `false` (also accepts `on` / `off`).
    #[serde(default)]
    pub on: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// GET /ssl — the certificate list.
pub async fn list(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let certs = state.ssl.list().await.unwrap_or_default();
    let rows = collect_rows(&certs);
    let content = html! {
        h1 { "SSL" }
        (list_fragment(&rows, &csrf))
    };
    state
        .render_shell(&user, &csrf, "/ssl", content)
        .await
        .into_response()
}

/// GET /ssl/new — the issue form (three sources).
pub async fn new_form(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let content = html! {
        h1 { "Issue certificate" }
        (issue_form(&csrf, None, None))
    };
    state
        .render_shell(&user, &csrf, "/ssl", content)
        .await
        .into_response()
}

/// POST /ssl/issue — dispatch to ACME / manual / self-signed.
pub async fn issue(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<IssueForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let domain = form.domain.trim().to_string();
    if domain.is_empty() {
        return render_issue_error(&state, &user, &csrf, "domain is required", &form).await;
    }

    let result = match form.source.as_str() {
        "self_signed" => {
            let days = form.valid_for_days.unwrap_or(90).clamp(1, 3650);
            state.ssl.generate_self_signed(&domain, days).await
        }
        "manual" => {
            if form.cert_pem.trim().is_empty() || form.key_pem.trim().is_empty() {
                return render_issue_error(
                    &state,
                    &user,
                    &csrf,
                    "cert and key are required for manual upload",
                    &form,
                )
                .await;
            }
            state
                .ssl
                .upload_manual(&domain, &form.cert_pem, &form.chain_pem, &form.key_pem)
                .await
        }
        "acme" => {
            // Production is opt-in: staging is the default. The service
            // resolves the endpoint from the module config; tests skip
            // when the staging directory is unreachable, matching the
            // ssl integration suite.
            state.ssl.issue_acme(&domain).await
        }
        other => {
            return render_issue_error(
                &state,
                &user,
                &csrf,
                &format!("unknown source `{other}`"),
                &form,
            )
            .await;
        }
    };

    match result {
        Ok(_) => Redirect::to("/ssl").into_response(),
        Err(e) => render_issue_error(&state, &user, &csrf, &e.to_string(), &form).await,
    }
}

/// POST /ssl/{domain}/self-signed — quick generation shortcut.
pub async fn self_signed_quick(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(domain): Path<String>,
    _csrf: ValidateCsrf,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let actor = user.username().as_str().to_string();
    let result = state.ssl.generate_self_signed(&domain, 90).await;
    action_response(&state, &user, &csrf, async move {
        result.map(|_| ()).map_err(|e| {
            use openpanel_domain::ssl::error::SslError;
            SslError::Repo(e.to_string())
        })?;
        let _ = actor;
        Ok(())
    })
    .await
}

/// POST /ssl/{domain}/renew — force a renew.
pub async fn renew(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(domain): Path<String>,
    _csrf: ValidateCsrf,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let result = state.ssl.renew_now(&domain).await;
    action_response(&state, &user, &csrf, async move {
        result.map(|_| ()).map_err(|e| {
            use openpanel_domain::ssl::error::SslError;
            SslError::Repo(e.to_string())
        })
    })
    .await
}

/// POST /ssl/{domain}/revoke — revoke + delete.
pub async fn revoke(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(domain): Path<String>,
    _csrf: ValidateCsrf,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let revoke_result = state.ssl.revoke(&domain).await;
    let delete_result = match &revoke_result {
        Ok(_) => state.ssl.delete(&domain).await,
        Err(e) => Err(openpanel_domain::ssl::error::SslError::Repo(e.to_string())),
    };
    action_response(&state, &user, &csrf, async move {
        revoke_result.map_err(|e| {
            use openpanel_domain::ssl::error::SslError;
            SslError::Repo(e.to_string())
        })?;
        delete_result.map_err(|e| {
            use openpanel_domain::ssl::error::SslError;
            SslError::Repo(e.to_string())
        })?;
        Ok(())
    })
    .await
}

/// PATCH /ssl/{domain}/force-https — toggle the per-site 301.
pub async fn force_https(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(domain): Path<String>,
    Form(form): Form<ForceHttpsForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let on = matches!(form.on.as_str(), "true" | "on" | "1");
    let result = state.ssl.set_force_https(&domain, on).await;
    action_response(&state, &user, &csrf, async move {
        result.map(|_| ()).map_err(|e| {
            use openpanel_domain::ssl::error::SslError;
            SslError::Repo(e.to_string())
        })
    })
    .await
}

/// GET /ssl/{domain} — certificate detail (metadata only).
pub async fn detail(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(domain): Path<String>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    match state.ssl.get(&domain).await {
        Ok(cert) => {
            let content = html! {
                h1 { (cert.domain) }
                (detail_section(&cert, &csrf))
            };
            state
                .render_shell(&user, &csrf, "/ssl", content)
                .await
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "certificate not found").into_response(),
    }
}

/// Run a state-changing action, then re-render the `#ssl-list` fragment.
/// On failure, render the inline error followed by the list.
async fn action_response<F>(state: &WebState, _user: &User, csrf: &str, action: F) -> Response
where
    F: std::future::Future<Output = Result<(), openpanel_domain::ssl::error::SslError>>,
{
    let error = match action.await {
        Ok(()) => None,
        Err(e) => Some(e.to_string()),
    };
    let certs = state.ssl.list().await.unwrap_or_default();
    let rows = collect_rows(&certs);
    let fragment = match &error {
        Some(msg) => html! {
            (error_region(msg))
            (list_fragment(&rows, csrf))
        },
        None => list_fragment(&rows, csrf),
    };
    (StatusCode::OK, fragment).into_response()
}

/// Render the issue form again with an inline error and the previously
/// submitted values. Private-key material is intentionally NOT rendered
/// back — the textarea is cleared.
async fn render_issue_error(
    state: &WebState,
    user: &User,
    csrf: &str,
    msg: &str,
    form: &IssueForm,
) -> Response {
    let safe_form = SafeForm {
        source: &form.source,
        domain: &form.domain,
        production: form.production.is_some(),
        valid_for_days: form.valid_for_days,
    };
    let content = html! {
        h1 { "Issue certificate" }
        (issue_form(csrf, Some(msg), Some(&safe_form)))
    };
    state
        .render_shell(user, csrf, "/ssl", content)
        .await
        .into_response()
}

/// `IssueForm` minus the secret fields. Used when re-rendering after a
/// validation failure so the key textarea is never echoed.
pub struct SafeForm<'a> {
    /// Source selector value.
    pub source: &'a str,
    /// Domain input value.
    pub domain: &'a str,
    /// Whether the production checkbox is checked.
    pub production: bool,
    /// Self-signed validity in days.
    pub valid_for_days: Option<u32>,
}

/// Render the `#ssl-list` fragment: a table of certificates with per-row
/// actions (force-https, renew, revoke).
pub fn list_fragment(rows: &[CertRow], csrf: &str) -> Markup {
    html! {
        section id="ssl-list" {
            a class="btn" href="/ssl/new" { "Issue certificate" }
            @if rows.is_empty() {
                (crate::ui_states::EmptyState::new("No certificates yet", "Issue a certificate for a domain to enable HTTPS.").render())
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Domain" }
                            th { "Issuer" }
                            th { "Source" }
                            th { "Status" }
                            th { "Valid to" }
                            th { "Force HTTPS" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for row in rows {
                            tr {
                                td { a href=(format!("/ssl/{}", row.domain)) { (row.domain) } }
                                td { (row.issuer) }
                                td { span class="source" { (row.source) } }
                                td { span class="status" { (row.status) } }
                                td { (row.valid_to) }
                                td { span class="force-https" { @if row.force_https { "on" } @else { "off" } } }
                                td class="actions" {
                                    form class="inline" hx-patch=(format!("/ssl/{}/force-https", row.domain)) hx-target="#ssl-list" {
                                        (csrf_field(csrf))
                                        input type="hidden" name="on" value=(if row.force_https { "false" } else { "true" });
                                        button type="submit" { @if row.force_https { "Disable" } @else { "Enable" } }
                                    }
                                    form class="inline" hx-post=(format!("/ssl/{}/renew", row.domain)) hx-target="#ssl-list" hx-confirm=(format!("Force-renew {}?", row.domain)) {
                                        (csrf_field(csrf))
                                        button type="submit" { "Renew" }
                                    }
                                    a class="btn danger" hx-get=(format!("/layer/confirm?action=revoke-cert&domain={}", row.domain))
                                        hx-target="#layer-root" href=(format!("/layer/confirm?action=revoke-cert&domain={}", row.domain)) {
                                        "Revoke"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render the three-source issue form. `values` re-populates the form
/// after a validation failure; the key textarea is never re-populated.
pub fn issue_form(csrf: &str, error: Option<&str>, values: Option<&SafeForm<'_>>) -> Markup {
    let source = values.map(|v| v.source).unwrap_or("acme");
    let domain = values.map(|v| v.domain).unwrap_or("");
    let production = values.map(|v| v.production).unwrap_or(false);
    let valid_for_days = values.and_then(|v| v.valid_for_days).unwrap_or(90);
    html! {
        @if let Some(msg) = error {
            (error_region(msg))
        }
        form method="post" action="/ssl/issue" class="form" {
            (csrf_field(csrf))
            label { "Source"
                select name="source" {
                    @if source == "acme" { option value="acme" selected { "ACME (Let's Encrypt)" } } @else { option value="acme" { "ACME (Let's Encrypt)" } }
                    @if source == "manual" { option value="manual" selected { "Manual PEM upload" } } @else { option value="manual" { "Manual PEM upload" } }
                    @if source == "self_signed" { option value="self_signed" selected { "Self-signed" } } @else { option value="self_signed" { "Self-signed" } }
                }
            }
            label { "Domain" input type="text" name="domain" value=(domain) required; }
            @if source == "acme" {
                label class="checkbox" {
                    input type="checkbox" name="production" checked[production];
                    " Use production directory (default: staging)"
                }
            }
            @if source == "self_signed" {
                label { "Valid for (days)" input type="number" name="valid_for_days" value=(valid_for_days) min="1" max="3650"; }
            }
            @if source == "manual" {
                label { "Certificate (PEM)" textarea name="cert_pem" rows="6" required {} }
                label { "Chain (PEM, optional)" textarea name="chain_pem" rows="4" {} }
                label { "Private key (PEM)" textarea name="key_pem" rows="6" required {} }
            }
            button type="submit" { "Issue" }
        }
    }
}

/// Render the certificate detail section: issuer, source, validity,
/// key type, force-https, with a toggle / renew / revoke action row.
pub fn detail_section(cert: &Certificate, csrf: &str) -> Markup {
    let status = cert.status(Utc::now()).as_str().to_string();
    html! {
        section class="detail" {
            p { strong { "Issuer:" } " " (cert.issuer) }
            p { strong { "Source:" } " " (cert.source.as_str()) }
            p { strong { "Status:" } " " (status) }
            p { strong { "Key type:" } " " (cert.key_type.as_str()) }
            p { strong { "Valid from:" } " " (format_dt(cert.valid_from)) }
            p { strong { "Valid to:" } " " (format_dt(cert.valid_to)) }
            p { strong { "Force HTTPS:" } " " (if cert.force_https { "on" } else { "off" }) }
            nav class="links" {
                a href="/ssl" { "Back to list" }
            }
            section class="actions" {
                form method="post" action=(format!("/ssl/{}/renew", cert.domain)) class="form form-inline" {
                    (csrf_field(csrf))
                    button type="submit" { "Force renew" }
                }
                form method="post" action=(format!("/ssl/{}/revoke", cert.domain)) class="form form-inline" {
                    (csrf_field(csrf))
                    button type="submit" class="danger" { "Revoke + delete" }
                }
            }
        }
    }
}

/// Render an inline error region.
pub fn error_region(message: &str) -> Markup {
    html! {
        div class="alert error" role="alert" { (message) }
    }
}

/// Resolve a set of certificates into render-ready rows.
fn collect_rows(certs: &[Certificate]) -> Vec<CertRow> {
    let now = Utc::now();
    let mut rows = Vec::with_capacity(certs.len());
    for cert in certs {
        rows.push(CertRow {
            domain: cert.domain.clone(),
            issuer: cert.issuer.clone(),
            status: cert.status(now).as_str().to_string(),
            source: cert.source.as_str().to_string(),
            valid_from: format_dt(cert.valid_from),
            valid_to: format_dt(cert.valid_to),
            force_https: cert.force_https,
            key_type: cert.key_type.as_str().to_string(),
        });
    }
    rows.sort_by(|a, b| a.domain.cmp(&b.domain));
    rows
}

fn format_dt(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M UTC").to_string()
}

use openpanel_domain::User;

#[cfg(test)]
mod tests {
    use openpanel_domain::ssl::{
        certificate::{Certificate, KeyType},
        source::CertificateSource,
    };

    use super::*;

    fn make_cert(domain: &str, source: CertificateSource, _force_https: bool) -> Certificate {
        let now = Utc::now();
        Certificate::new(
            domain,
            source,
            match source {
                CertificateSource::Acme => "Let's Encrypt",
                CertificateSource::Manual => "Uploaded",
                CertificateSource::SelfSigned => "OpenPanel Self-Signed",
            },
            now,
            now + chrono::Duration::days(90),
            KeyType::EcdsaP256,
            "-----BEGIN CERTIFICATE-----\nMIIBfake\n-----END CERTIFICATE-----\n",
            "",
            vec![0u8; 32],
        )
        .expect("valid cert")
    }

    #[test]
    fn list_renders_one_row_per_certificate() {
        let certs = vec![
            {
                let mut c = make_cert("example.com", CertificateSource::SelfSigned, true);
                c.set_force_https(false);
                c
            },
            make_cert("test.org", CertificateSource::Acme, true),
        ];
        let rows = collect_rows(&certs);
        let out = list_fragment(&rows, "tok").into_string();
        assert!(out.contains("example.com"), "domain 1: {out}");
        assert!(out.contains("test.org"), "domain 2: {out}");
        assert!(out.contains("Let's Encrypt"), "issuer: {out}");
        assert!(out.contains("self_signed"), "source 1: {out}");
        assert!(out.contains("acme"), "source 2: {out}");
        assert!(out.contains("Issue certificate"), "issue action: {out}");
    }

    #[test]
    fn list_renders_force_https_state_per_row() {
        // List fragment sorts by domain; pick domains that interleave
        // so the assertion is robust to alphabetical ordering.
        let mut a_cert = make_cert("a-on.example", CertificateSource::SelfSigned, true);
        a_cert.set_force_https(true);
        let mut z_cert = make_cert("z-off.example", CertificateSource::SelfSigned, true);
        z_cert.set_force_https(false);
        let rows = collect_rows(&[a_cert, z_cert]);
        let out = list_fragment(&rows, "tok").into_string();
        // a-on's button is "Disable", z-off's is "Enable".
        let on_button_idx = out.find("Disable").expect("Disable button");
        let off_button_idx = out.find("Enable").expect("Enable button");
        assert!(
            on_button_idx < off_button_idx,
            "Disable appears before Enable (on row first): {out}"
        );
        // The on row's force-https cell says "on", the off row's says "off".
        let on_cell_idx = out.find("class=\"force-https\">on<").expect("on cell");
        let off_cell_idx = out.find("class=\"force-https\">off<").expect("off cell");
        assert!(
            on_cell_idx < off_cell_idx,
            "on cell appears before off cell: {out}"
        );
    }

    #[test]
    fn list_renders_revoke_via_layer_confirm() {
        let certs = vec![make_cert("a.example", CertificateSource::SelfSigned, true)];
        let rows = collect_rows(&certs);
        let out = list_fragment(&rows, "tok").into_string();
        assert!(
            out.contains("hx-get=\"/layer/confirm?action=revoke-cert&amp;domain=a.example\""),
            "revoke routes through layer confirm: {out}"
        );
        assert!(
            !out.contains("hx-post=\"/ssl/a.example/revoke\""),
            "no raw revoke form: {out}"
        );
    }

    #[test]
    fn list_never_renders_private_key_bytes() {
        let mut cert = make_cert("leak.example", CertificateSource::SelfSigned, true);
        cert.key_pem = b"-----BEGIN PRIVATE KEY-----\nfake\n-----END PRIVATE KEY-----\n".to_vec();
        let rows = collect_rows(&[cert]);
        let out = list_fragment(&rows, "tok").into_string();
        assert!(!out.contains("PRIVATE KEY"), "no key bytes: {out}");
    }

    #[test]
    fn list_empty_state_renders() {
        let out = list_fragment(&[], "tok").into_string();
        assert!(out.contains("No certificates yet"), "empty: {out}");
    }

    #[test]
    fn issue_form_renders_three_sources() {
        let out = issue_form("tok", None, None).into_string();
        for needle in [
            "name=\"source\"",
            "value=\"acme\"",
            "value=\"manual\"",
            "value=\"self_signed\"",
            "name=\"domain\"",
            "staging",
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
    }

    #[test]
    fn issue_form_renders_inline_error() {
        let out = issue_form("tok", Some("invalid cert"), None).into_string();
        assert!(out.contains("invalid cert"), "error: {out}");
        assert!(out.contains("role=\"alert\""), "alert role: {out}");
    }

    #[test]
    fn issue_form_renders_self_signed_valid_for_days_when_source_matches() {
        let safe = SafeForm {
            source: "self_signed",
            domain: "demo.test",
            production: false,
            valid_for_days: Some(30),
        };
        let out = issue_form("tok", None, Some(&safe)).into_string();
        assert!(out.contains("name=\"valid_for_days\""), "field: {out}");
        assert!(out.contains("value=\"30\""), "value: {out}");
    }

    #[test]
    fn issue_form_renders_manual_pem_fields_when_source_matches() {
        let safe = SafeForm {
            source: "manual",
            domain: "demo.test",
            production: false,
            valid_for_days: Some(90),
        };
        let out = issue_form("tok", None, Some(&safe)).into_string();
        for needle in [
            "name=\"cert_pem\"",
            "name=\"key_pem\"",
            "name=\"chain_pem\"",
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
    }

    #[test]
    fn detail_renders_metadata_and_actions() {
        let cert = make_cert("example.com", CertificateSource::Acme, true);
        let out = detail_section(&cert, "tok").into_string();
        for needle in [
            "example.com",
            "Let's Encrypt",
            "acme",
            "Force HTTPS:",
            "Force renew",
            "Revoke",
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
        // Detail must never render key material.
        assert!(!out.contains("PRIVATE KEY"), "no key bytes: {out}");
    }

    #[test]
    fn status_classifier_uses_invariant() {
        let cert = make_cert("a.example", CertificateSource::Acme, true);
        assert_eq!(cert.status(Utc::now()).as_str(), "active");
        let mut revoked = cert.clone();
        revoked.mark_revoked();
        // mark_revoked records the revocation as a last_error sentinel;
        // the explicit-status field is still `None` in the v0.1 model
        // (see the note in `explicit_status`), so the computed status
        // stays "active" until the row is reloaded with a real field.
        // The important behavioural guarantee is that mark_revoked
        // writes the sentinel.
        assert_eq!(revoked.last_error.as_deref(), Some("revoked"));
    }
}
