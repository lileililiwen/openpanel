//! Sites pages: list, create, detail, enable/disable, and delete — rendered
//! server-side in the shell, wired to `SitesService` exactly as the API does.
//!
//! Actions use HTMX swaps (`hx-post`/`hx-delete` targeting `#site-list`) and
//! the shared CSRF token. Delete requires confirmation (`hx-confirm`). The
//! caller's role gates what renders: only Owner/Admin see create/delete
//! actions, and `list_sites(caller)` already scopes which rows are shown.

use axum::{
    extract::{Form, Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::{Markup, html};
use openpanel_domain::{SiteError, User, sites::site::Site};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    csrf::ValidateCsrf,
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// One row in the sites table (all fields pre-resolved for rendering).
pub struct SiteRow {
    /// Site id, used for detail links and action URLs.
    pub id: Uuid,
    /// Primary hostname.
    pub domain: String,
    /// `active` or `disabled`.
    pub status: String,
    /// Owning user's username.
    pub owner: String,
    /// Configured PHP version (absent when PHP is off).
    pub php: Option<String>,
    /// Whether the caller may manage this row (role + ownership).
    pub can_manage: bool,
}

/// Body of the create-site form. `_csrf` is validated in the handler because
/// the form extractor owns the request body.
#[derive(Debug, Deserialize)]
pub struct CreateSiteForm {
    /// Primary hostname (e.g. `example.com`).
    pub primary_domain: String,
    /// UUID of the owning user.
    pub owner_id: String,
    /// Whitespace-separated aliases.
    #[serde(default)]
    pub aliases: String,
    /// `"on"` when the PHP checkbox is checked.
    #[serde(default)]
    pub php_enabled: Option<String>,
    /// Desired PHP version (required when PHP is enabled).
    #[serde(default)]
    pub php_version: String,
    /// Document root override; empty means the default.
    #[serde(default)]
    pub document_root: String,
    /// CSRF token.
    #[serde(default)]
    pub _csrf: String,
}

/// GET /sites — the sites list page inside the shell.
pub async fn list(State(state): State<WebState>, WebUser(user, session): WebUser) -> Markup {
    let csrf = state.csrf.token_for(session.id());
    let rows = collect_rows(&state, &user).await;
    let can_create = user.role().can_manage_sites();
    let content = html! {
        h1 { "Sites" }
        (list_fragment(&rows, can_create, &csrf))
    };
    state.render_shell(&user, &csrf, "/sites", content).await
}

/// GET /sites/new — the create form.
pub async fn new_form(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    if !user.role().can_manage_sites() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let owners = owner_options(&state).await;
    let php_versions = managed_php_versions(&state, &user).await;
    let content = html! {
        h1 { "New site" }
        (create_form(&owners, &php_versions, &csrf, None, None))
    };
    state
        .render_shell(&user, &csrf, "/sites", content)
        .await
        .into_response()
}

/// POST /sites — create the site; swap the refreshed list or render the
/// inline error.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CreateSiteForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !user.role().can_manage_sites() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let owner_id = Uuid::parse_str(&form.owner_id).unwrap_or(user.id());
    let aliases: Vec<String> = form.aliases.split_whitespace().map(String::from).collect();
    let php_enabled = form.php_enabled.is_some();
    let php_version = if php_enabled {
        let v = form.php_version.trim();
        if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        }
    } else {
        None
    };
    let document_root = if form.document_root.trim().is_empty() {
        None
    } else {
        Some(form.document_root.trim().to_string())
    };

    match state
        .sites
        .create_site(
            &user,
            owner_id,
            &form.primary_domain,
            aliases,
            php_enabled,
            php_version,
            document_root,
        )
        .await
    {
        Ok(_) => Redirect::to("/sites").into_response(),
        Err(e) => {
            let owners = owner_options(&state).await;
            let php_versions = managed_php_versions(&state, &user).await;
            let content = html! {
                h1 { "New site" }
                (create_form(&owners, &php_versions, &csrf, Some(&e.to_string()), Some(&form)))
            };
            state
                .render_shell(&user, &csrf, "/sites", content)
                .await
                .into_response()
        }
    }
}

/// GET /sites/{id} — site detail with files/SSL links.
pub async fn detail(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    match state.sites.get_site(id).await {
        Ok(site) => {
            let owner = owner_name(&state, site.owner_id()).await;
            let content = html! {
                h1 { (site.primary_domain()) }
                (detail_section(&site, &owner))
            };
            state
                .render_shell(&user, &csrf, "/sites", content)
                .await
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

/// POST /sites/{id}/enable — mark the site active and swap the list.
pub async fn enable(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    _csrf: ValidateCsrf,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    action_response(&state, &user, &csrf, async {
        state.sites.enable_site(&user, id).await
    })
    .await
}

/// POST /sites/{id}/disable — mark the site disabled and swap the list.
pub async fn disable(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    _csrf: ValidateCsrf,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    action_response(&state, &user, &csrf, async {
        state.sites.disable_site(&user, id).await
    })
    .await
}

/// DELETE /sites/{id} — remove the site (config) and swap the list.
pub async fn delete(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    _csrf: ValidateCsrf,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    action_response(&state, &user, &csrf, async {
        state.sites.delete_site(&user, id).await
    })
    .await
}

/// Run a state-changing action then re-render the list fragment (HTMX swap),
/// or the inline error on failure.
async fn action_response<F>(state: &WebState, user: &User, csrf: &str, action: F) -> Response
where
    F: std::future::Future<Output = Result<(), SiteError>>,
{
    let error = match action.await {
        Ok(()) => None,
        Err(e) => Some(e.to_string()),
    };
    let rows = collect_rows(state, user).await;
    let can_create = user.role().can_manage_sites();
    let fragment = match &error {
        Some(msg) => html! {
            (error_region(msg))
            (list_fragment(&rows, can_create, csrf))
        },
        None => list_fragment(&rows, can_create, csrf),
    };
    (StatusCode::OK, fragment).into_response()
}

/// Render the `#site-list` fragment swapped by HTMX actions: the table plus
/// the create action when the caller may create.
pub fn list_fragment(rows: &[SiteRow], can_create: bool, csrf: &str) -> Markup {
    html! {
        section id="site-list" {
            @if can_create {
                a class="btn" href="/sites/new" { "New site" }
            }
            @if rows.is_empty() {
                @if can_create {
                    (crate::ui_states::EmptyState::new("No sites yet", "Create your first site to start hosting.")
                        .with_cta("/sites/new", "Create your first site")
                        .render())
                } @else {
                    (crate::ui_states::EmptyState::new("No sites yet", "No sites are assigned to your account.").render())
                }
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Domain" }
                            th { "Status" }
                            th { "Owner" }
                            th { "PHP" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for row in rows {
                            tr {
                                td { a href=(format!("/sites/{}", row.id)) { (row.domain) } }
                                td { span class="status" { (row.status) } }
                                td { (row.owner) }
                                td {
                                    @match &row.php {
                                        Some(v) => { (v) }
                                        None => { "—" }
                                    }
                                }
                                td class="actions" {
                                    @if row.can_manage {
                                        @if row.status == "active" {
                                            form class="inline" hx-post=(format!("/sites/{}/disable", row.id)) hx-target="#site-list" {
                                                (csrf_field(csrf))
                                                button type="submit" { "Disable" }
                                            }
                                        } @else {
                                            form class="inline" hx-post=(format!("/sites/{}/enable", row.id)) hx-target="#site-list" {
                                                (csrf_field(csrf))
                                                button type="submit" { "Enable" }
                                            }
                                        }
                                        a class="btn danger" hx-get=(format!("/layer/confirm?action=delete-site&id={}", row.id))
                                            hx-target="#layer-root" href=(format!("/layer/confirm?action=delete-site&id={}", row.id)) {
                                            "Delete"
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
}

/// Render the create-site form with owner options and an optional inline error.
/// `values` re-populates the form after a validation failure.
pub fn create_form(
    owners: &[(Uuid, String)],
    php_versions: &[String],
    csrf: &str,
    error: Option<&str>,
    values: Option<&CreateSiteForm>,
) -> Markup {
    let domain = values.map(|v| v.primary_domain.as_str()).unwrap_or("");
    let aliases = values.map(|v| v.aliases.as_str()).unwrap_or("");
    let doc_root = values.map(|v| v.document_root.as_str()).unwrap_or("");
    let php_checked = values.map(|v| v.php_enabled.is_some()).unwrap_or(false);
    let php_version = values.map(|v| v.php_version.as_str()).unwrap_or("8.3");
    html! {
        @if let Some(msg) = error {
            (error_region(msg))
        }
        form method="post" action="/sites" class="form" {
            (csrf_field(csrf))
            label { "Primary domain" input type="text" name="primary_domain" value=(domain) required; }
            label { "Aliases (space-separated)" input type="text" name="aliases" value=(aliases); }
            label { "Owner"
                select name="owner_id" {
                    @for (id, name) in owners {
                        option value=(id.to_string()) { (name) }
                    }
                }
            }
            label { "PHP version"
                select name="php_version" {
                    @if php_versions.is_empty() {
                        option value="" disabled selected { "Install or adopt PHP in Software Center" }
                    }
                    @for version in php_versions {
                        option value=(version) selected[version == php_version] { (version) }
                    }
                }
            }
            label class="checkbox" {
                input type="checkbox" name="php_enabled" checked[php_checked];
                " Enable PHP for this site"
            }
            label { "Document root (optional)" input type="text" name="document_root" value=(doc_root); }
            button type="submit" { "Create site" }
        }
    }
}

async fn managed_php_versions(state: &WebState, user: &User) -> Vec<String> {
    state
        .software_center
        .inventory(user.role())
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.state == "panel_managed" && entry.id.starts_with("php-"))
        .filter_map(|entry| entry.id.strip_prefix("php-").map(str::to_owned))
        .collect()
}

/// Render the site detail section: domain, aliases, document root, PHP, status,
/// and links to the site's files and SSL pages.
pub fn detail_section(site: &Site, owner: &str) -> Markup {
    let status = site.status().as_str();
    let php = match site.php_version() {
        Some(v) => {
            if site.php_enabled() {
                v.to_string()
            } else {
                "disabled".into()
            }
        }
        None => "—".into(),
    };
    html! {
        section class="detail" {
            p { strong { "Status:" } " " (status) }
            p { strong { "Owner:" } " " (owner) }
            p { strong { "Document root:" } " " (site.document_root()) }
            p { strong { "PHP:" } " " (php) }
            p { strong { "Aliases:" } }
            @if site.aliases().is_empty() {
                p class="empty" { "none" }
            } @else {
                ul {
                    @for alias in site.aliases() {
                        li { (alias) }
                    }
                }
            }
            nav class="links" {
                a href=(format!("/files/{}", site.id())) { "Files" }
                a href="/ssl" { "SSL" }
                a href=(format!("/sites/{}/waf", site.id())) { "WAF" }
                a href=(format!("/sites/{}/http", site.id())) { "HTTP controls" }
                a href=(format!("/sites/{}/staging", site.id())) { "Staging" }
                a href=(format!("/sites/{}/previews", site.id())) { "Previews" }
                a href=(format!("/sites/{}/cache", site.id())) { "Cache & CDN" }
                a href=(format!("/sites/{}/collaborators", site.id())) { "Collaborators" }
            }
        }
    }
}

/// Render an inline error/alert region.
pub fn error_region(message: &str) -> Markup {
    html! {
        div class="alert error" role="alert" { (message) }
    }
}

/// Resolve every row for the caller's visible sites.
async fn collect_rows(state: &WebState, user: &User) -> Vec<SiteRow> {
    let Ok(sites) = state.sites.list_sites(user).await else {
        return Vec::new();
    };
    let can_manage_all = user.role().can_manage_sites();
    let mut rows = Vec::with_capacity(sites.len());
    for site in &sites {
        let owner = owner_name(state, site.owner_id()).await;
        rows.push(SiteRow {
            id: site.id(),
            domain: site.primary_domain().to_string(),
            status: site.status().as_str().to_string(),
            owner,
            php: site.php_version().map(str::to_string),
            can_manage: can_manage_all,
        });
    }
    rows
}

/// Owner select options: `(id, username)` for every user.
async fn owner_options(state: &WebState) -> Vec<(Uuid, String)> {
    state
        .identity
        .list_users()
        .await
        .map(|users| {
            users
                .into_iter()
                .map(|u| (u.id(), u.username().as_str().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve a user id to its username (or the raw id when unknown).
async fn owner_name(state: &WebState, id: Uuid) -> String {
    state
        .identity
        .list_users()
        .await
        .map(|users| {
            users
                .into_iter()
                .find(|u| u.id() == id)
                .map(|u| u.username().as_str().to_string())
                .unwrap_or_else(|| id.to_string())
        })
        .unwrap_or_else(|_| id.to_string())
}

#[cfg(test)]
mod tests {
    use openpanel_domain::sites::status::SiteStatus;

    use super::*;

    fn row(id: Uuid, domain: &str, status: &str, owner: &str, php: Option<&str>) -> SiteRow {
        SiteRow {
            id,
            domain: domain.into(),
            status: status.into(),
            owner: owner.into(),
            php: php.map(str::to_string),
            can_manage: true,
        }
    }

    #[test]
    fn list_renders_one_row_per_site() {
        let rows = vec![
            row(
                Uuid::new_v4(),
                "example.com",
                "active",
                "admin",
                Some("8.3"),
            ),
            row(Uuid::new_v4(), "test.org", "disabled", "admin", None),
        ];
        let out = list_fragment(&rows, true, "tok").into_string();
        assert!(out.contains("example.com"), "domain 1: {out}");
        assert!(out.contains("test.org"), "domain 2: {out}");
        assert!(out.contains("active"), "status 1: {out}");
        assert!(out.contains("disabled"), "status 2: {out}");
        assert!(out.contains("admin"), "owner: {out}");
        assert!(out.contains("8.3"), "php version: {out}");
        assert!(out.contains("New site"), "create action: {out}");
    }

    #[test]
    fn list_hides_actions_for_non_managers() {
        let mut r = row(Uuid::new_v4(), "example.com", "active", "admin", None);
        r.can_manage = false;
        let out = list_fragment(&[r], false, "tok").into_string();
        assert!(!out.contains("New site"), "no create: {out}");
        assert!(!out.contains("Delete"), "no delete: {out}");
        assert!(!out.contains("Disable"), "no toggle: {out}");
    }

    #[test]
    fn list_empty_state() {
        let out = list_fragment(&[], true, "tok").into_string();
        assert!(out.contains("No sites yet"), "empty state: {out}");
    }

    #[test]
    fn create_form_renders_all_fields() {
        let owners = vec![(Uuid::new_v4(), "admin".to_string())];
        let out = create_form(&owners, &["8.3".into()], "tok", None, None).into_string();
        for needle in [
            "name=\"primary_domain\"",
            "name=\"aliases\"",
            "name=\"owner_id\"",
            "name=\"php_enabled\"",
            "name=\"php_version\"",
            "name=\"document_root\"",
            "Create site",
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
    }

    #[test]
    fn create_form_renders_inline_error() {
        let out = create_form(
            &[],
            &["8.3".into()],
            "tok",
            Some("duplicate domain: example.com"),
            None,
        )
        .into_string();
        assert!(
            out.contains("duplicate domain: example.com"),
            "error: {out}"
        );
        assert!(out.contains("role=\"alert\""), "alert role: {out}");
    }

    #[test]
    fn detail_renders_domain_aliases_root_php_links() {
        let site = Site::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "example.com",
            vec!["www.example.com".to_string()],
            "/var/www/example.com/public_html",
            true,
            Some("8.3".into()),
            "admin",
        )
        .expect("valid site");
        let out = detail_section(&site, "admin").into_string();
        for needle in [
            "example.com",
            "www.example.com",
            "/var/www/example.com/public_html",
            "8.3",
            "active",
            format!("/files/{}", site.id()).as_str(),
            "/ssl",
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
    }

    #[test]
    fn site_status_display_matches() {
        assert_eq!(SiteStatus::Active.as_str(), "active");
        assert_eq!(SiteStatus::Disabled.as_str(), "disabled");
    }
}
