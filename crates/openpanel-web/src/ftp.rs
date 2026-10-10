//! Per-site FTP account HTML surface.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::{Markup, html};
use openpanel_app::CreateFtpAccount;
use openpanel_domain::Role;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
    site_workspace::TabId,
};

/// Render a site's FTP accounts and creation form.
pub async fn page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if user.role() != Role::Owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    match state.ftp.list(&user, id).await {
        Ok(accounts) => {
            let body = html! {
                (crate::site_workspace::site_bar(&state, &user, id, TabId::Ftp).await)
                (content(id, &accounts, &csrf, None))
            };
            state
                .render_shell(&user, &csrf, "/sites", body)
                .await
                .into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
/// Browser form for creating a site FTP account.
pub struct CreateForm {
    _csrf: String,
    username: String,
    password: String,
    read_only: Option<String>,
    bandwidth_kb_per_session: Option<u64>,
    max_concurrent_connections: Option<u16>,
}

/// Create an FTP account after CSRF verification.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<CreateForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state
        .ftp
        .create(
            &user,
            id,
            CreateFtpAccount {
                username: form.username,
                password: form.password,
                read_only: form.read_only.is_some(),
                bandwidth_kb_per_session: form.bandwidth_kb_per_session,
                max_concurrent_connections: form.max_concurrent_connections,
            },
        )
        .await
    {
        Ok(created) => {
            let csrf = state.csrf.token_for(session.id());
            let accounts = state.ftp.list(&user, id).await.unwrap_or_default();
            state.render_shell(&user,&csrf,"/sites",content(id,&accounts,&csrf,Some(&format!("Password shown once for {}. Store it now; OpenPanel cannot display it again.",created.account.username)))).await.into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
/// CSRF-protected account lifecycle action form.
pub struct ActionForm {
    _csrf: String,
}

/// Enable, disable, or delete an FTP account after CSRF verification.
pub async fn action(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path((site_id, account_id, action)): Path<(Uuid, Uuid, String)>,
    Form(form): Form<ActionForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result = match action.as_str() {
        "enable" => state
            .ftp
            .enable(&user, site_id, account_id)
            .await
            .map(|_| ()),
        "disable" => state
            .ftp
            .disable(&user, site_id, account_id)
            .await
            .map(|_| ()),
        "delete" => state.ftp.delete(&user, site_id, account_id).await,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match result {
        Ok(()) => Redirect::to(&format!("/sites/{site_id}/ftp")).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

/// One row of the global FTP aggregate: a secret-free account view
/// plus the identity of the site that owns it.
///
/// Secret-free by construction: [`openpanel_app::FtpAccountView`]
/// carries no password material, so rendering this struct cannot
/// leak credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtpGlobalRow {
    /// Site that owns the account.
    pub site_id: Uuid,
    /// Primary domain of the owning site.
    pub site_domain: String,
    /// Account username (site-scoped).
    pub username: String,
    /// Chroot home (the site document root).
    pub home: String,
    /// Read-only flag.
    pub read_only: bool,
    /// Enabled flag.
    pub enabled: bool,
}

impl FtpGlobalRow {
    /// Build an aggregate row from a site identity and an account view.
    pub fn new(site_id: Uuid, site_domain: &str, account: &openpanel_app::FtpAccountView) -> Self {
        Self {
            site_id,
            site_domain: site_domain.to_string(),
            username: account.username.clone(),
            home: account.home.clone(),
            read_only: account.read_only,
            enabled: account.enabled,
        }
    }
}

/// Render the global FTP aggregate: one row per account across all
/// caller-visible sites, each linking back to its site-scoped
/// `/sites/{id}/ftp` page. Shared `table` / `btn` /
/// `op-empty-state` tokens only; no new CSS.
pub fn aggregate_view(rows: &[FtpGlobalRow], csrf: &str) -> Markup {
    let _ = csrf;
    html! {
        h1 { "FTP" }
        p { "Every FTP account on sites you can access." }
        @if rows.is_empty() {
            (crate::ui_states::EmptyState::new(
                "No FTP accounts yet",
                "Create an account from a site's FTP page to see it here."
            ).with_cta("/sites", "Back to Sites").render())
        } @else {
            table class="table" {
                thead {
                    tr {
                        th { "Site" }
                        th { "Username" }
                        th { "Home" }
                        th { "Mode" }
                        th { "Status" }
                        th { "Site page" }
                    }
                }
                tbody {
                    @for row in rows {
                        tr {
                            td { (row.site_domain) }
                            td { (row.username) }
                            td { (row.home) }
                            td { @if row.read_only { "read only" } @else { "read/write" } }
                            td { @if row.enabled { "enabled" } @else { "disabled" } }
                            td {
                                a class="btn btn-ghost"
                                    href=(format!("/sites/{}/ftp", row.site_id)) {
                                    "Open site FTP"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// GET /ftp — aggregate FTP accounts across all caller-visible
/// sites. Any authenticated role may read; per-site visibility is
/// enforced by `SitesService::list_sites` + `FtpService::list`
/// (Owner sees all, others see owned sites only). Per-site failures
/// are skipped so one unreadable site cannot hide the rest.
pub async fn global_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let mut rows: Vec<FtpGlobalRow> = Vec::new();
    if let Ok(sites) = state.sites.list_sites(&user).await {
        for site in &sites {
            if let Ok(accounts) = state.ftp.list(&user, site.id()).await {
                for account in &accounts {
                    rows.push(FtpGlobalRow::new(site.id(), site.primary_domain(), account));
                }
            }
        }
    }
    rows.sort_by(|a, b| {
        a.site_domain
            .cmp(&b.site_domain)
            .then_with(|| a.username.cmp(&b.username))
    });
    let content = aggregate_view(&rows, &csrf);
    state
        .render_shell(&user, &csrf, "/ftp", content)
        .await
        .into_response()
}

fn content(
    site_id: Uuid,
    accounts: &[openpanel_app::FtpAccountView],
    csrf: &str,
    banner: Option<&str>,
) -> Markup {
    html! {
        h1 { "FTP accounts" }
        @if let Some(message)=banner { p class="notice" { (message) } }
        form method="post" action=(format!("/sites/{site_id}/ftp")) class="form" {
            (csrf_field(csrf))
            label { "Username" input name="username" required; }
            label { "Password" input name="password" type="password" minlength="12" required; }
            label class="checkbox" { input name="read_only" type="checkbox" value="true"; " Read only" }
            label { "Bandwidth KiB/session" input name="bandwidth_kb_per_session" type="number" min="1" value="1048576"; }
            label { "Max connections" input name="max_concurrent_connections" type="number" min="1" max="256" value="4"; }
            button type="submit" { "Create account" }
        }
        @if accounts.is_empty() {
            (crate::ui_states::EmptyState::new("No FTP accounts yet", "Create an account to let people connect to this site.").render())
        } @else {
            table class="table" {
                thead { tr { th { "Username" } th { "Home" } th { "Mode" } th { "Status" } th { "Actions" } } }
                tbody { @for account in accounts { tr { td { (&account.username) } td { (&account.home) } td { @if account.read_only { "read only" } @else { "read/write" } } td { @if account.enabled { "enabled" } @else { "disabled" } } td {
                    @if account.enabled {
                        form method="post" action=(format!("/sites/{site_id}/ftp/{}/disable", account.id)) class="form form-inline" { (csrf_field(csrf)) button { "Disable" } }
                    } @else {
                        form method="post" action=(format!("/sites/{site_id}/ftp/{}/enable", account.id)) class="form form-inline" { (csrf_field(csrf)) button { "Enable" } }
                    }
                    a class="btn danger" hx-get=(format!("/layer/confirm?action=delete-ftp-user&site_id={site_id}&account_id={}", account.id))
                        hx-target="#layer-root" href=(format!("/layer/confirm?action=delete-ftp-user&site_id={site_id}&account_id={}", account.id)) {
                        "Delete"
                    }
                } } } }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(site: &str, username: &str) -> FtpGlobalRow {
        FtpGlobalRow {
            site_id: Uuid::new_v4(),
            site_domain: site.to_string(),
            username: username.to_string(),
            home: format!("/var/www/{site}"),
            read_only: false,
            enabled: true,
        }
    }

    #[test]
    fn aggregate_view_lists_accounts_from_n_sites_with_site_links() {
        let rows = vec![
            row("a.example.test", "uploads"),
            row("b.example.test", "deploy"),
        ];
        let site_ids: Vec<String> = rows.iter().map(|r| r.site_id.to_string()).collect();
        let out = aggregate_view(&rows, "csrf-token").into_string();
        assert!(out.contains("<h1>FTP</h1>"), "single h1: {out}");
        assert_eq!(out.matches("<h1>").count(), 1, "exactly one h1: {out}");
        assert!(out.contains("uploads"), "first account: {out}");
        assert!(out.contains("deploy"), "second account: {out}");
        assert!(out.contains("a.example.test"), "first site: {out}");
        assert!(out.contains("b.example.test"), "second site: {out}");
        for id in &site_ids {
            assert!(
                out.contains(&format!("/sites/{id}/ftp")),
                "site link for {id}: {out}"
            );
        }
    }

    #[test]
    fn aggregate_view_empty_renders_empty_state() {
        let out = aggregate_view(&[], "csrf-token").into_string();
        assert!(out.contains("No FTP accounts yet"), "empty text: {out}");
        assert!(out.contains("/sites"), "sites link: {out}");
        assert!(!out.contains("<table"), "no table when empty: {out}");
    }

    #[test]
    fn aggregate_view_redacts_secrets() {
        let password = "s3cr3t-ftp-password-value";
        let key = "argon2$fake-hash-bytes";
        let rows = vec![row("a.example.test", "uploads")];
        let out = aggregate_view(&rows, "csrf-token").into_string();
        assert!(!out.contains(password), "no password material: {out}");
        assert!(!out.contains(key), "no hash material: {out}");
        assert!(!out.contains("password_hash"), "no hash field: {out}");
    }

    mod prop {
        use super::*;

        #[test]
        fn prop_ftp_global_never_leaks_secrets() {
            // Deterministic sweep over credential-shaped inputs: no
            // credential substring may appear in aggregate HTML. The
            // view carries no secret field by construction
            // (FtpAccountView is secret-free), so this pins the renderer.
            for (secret, username) in [
                ("s3cr3t-Pass-0001", "uploader-0001"),
                ("correct horse battery staple", "deploy-0002"),
                ("$argon2id$v=19$m=19456,t=2,p=1$c2FsdA", "backup-0003"),
                ("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig", "mirror-0004"),
            ] {
                let rows = vec![FtpGlobalRow {
                    site_id: Uuid::new_v4(),
                    site_domain: "a.example.test".to_string(),
                    username: username.to_string(),
                    home: "/var/www/a".to_string(),
                    read_only: false,
                    enabled: true,
                }];
                let out = aggregate_view(&rows, "csrf-token").into_string();
                assert!(!out.contains(secret), "credential leaked: {out}");
                assert!(out.contains(username), "row still renders: {out}");
            }
        }
    }
}
