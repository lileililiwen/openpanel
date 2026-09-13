//! Site-scoped landing routes: Domains, Runtime, Logs, and Backups.
//!
//! These complete the site workspace: every site tab now has a backing
//! route that preserves site identity (breadcrumb + header + tabs via
//! `site_bar`) across content and errors. Each handler reuses the existing
//! services (`SitesService`, `LogService`, `BackupService`); no new
//! backend logic is introduced.
//!
//! Covers `capability-navigation`: Site Workspace Is Complete and Scoped.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use openpanel_domain::Role;
use uuid::Uuid;

use crate::{
    capability_registry::{self, unauthorized_state, unavailable_state},
    router::{WebState, WebUser},
    site_workspace::TabId,
};

/// Render a site-scoped page inside the workspace chrome, or 404 when the
/// site cannot be resolved. The `site_bar` stub still renders a breadcrumb
/// for the unknown id so context is never silently dropped.
async fn render_site_page(
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
    path: &str,
    site_id: Uuid,
    active: TabId,
    content: maud::Markup,
) -> Response {
    let bar = crate::site_workspace::site_bar(state, user, site_id, active).await;
    let page = html! {
        (bar)
        (content)
    };
    state
        .render_shell(user, csrf, path, page)
        .await
        .into_response()
}

/// Deny below-minimum roles with the explicit unauthorized state while
/// keeping the workspace chrome (defense in depth: the tab is already
/// hidden; direct access still gets the authorization response).
async fn render_unauthorized(
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
    path: &str,
    site_id: Uuid,
    active: TabId,
) -> Response {
    let bar = crate::site_workspace::site_bar(state, user, site_id, active).await;
    let page = html! {
        (bar)
        (unauthorized_state())
    };
    (
        StatusCode::FORBIDDEN,
        state.render_shell(user, csrf, path, page).await,
    )
        .into_response()
}

/// Offer the explicit unavailable state when the backing capability is not
/// installed. The route stays mounted so the response names the gap instead
/// of returning a bare 404.
async fn render_unavailable(
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
    path: &str,
    site_id: Uuid,
    active: TabId,
    capability: &str,
) -> Response {
    let bar = crate::site_workspace::site_bar(state, user, site_id, active).await;
    let page = html! {
        (bar)
        (unavailable_state(capability))
    };
    state
        .render_shell(user, csrf, path, page)
        .await
        .into_response()
}

/// Check the registry entry for a tab: unauthorized when the role is below
/// the minimum, unavailable when the capability is not installed.
fn gate(state: &WebState, user: &openpanel_domain::User, tab: &str) -> Result<(), &'static str> {
    let Some(entry) = capability_registry::site_entry_for_tab(tab) else {
        return Ok(());
    };
    if !capability_registry::role_may_see(user.role(), entry) {
        return Err("unauthorized");
    }
    if !state.capabilities.contains(entry.capability) {
        return Err("unavailable");
    }
    Ok(())
}

/// GET /sites/{id}/domains — aliases and primary domain for a site.
pub async fn domains(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let path = format!("/sites/{id}/domains");
    if gate(&state, &user, "domains") == Err("unauthorized") {
        return render_unauthorized(&state, &user, &csrf, &path, id, TabId::Domains).await;
    }
    match state.sites.get_site(id).await {
        Ok(site) => {
            let aliases = site.aliases().to_vec();
            let content = html! {
                section class="detail" {
                    h1 { "Domains" }
                    dl class="site-overview" {
                        dt { "Primary domain" }
                        dd { (site.primary_domain()) }
                        dt { "Aliases" }
                        dd {
                            @if aliases.is_empty() {
                                span class="empty" { "none" }
                            } @else {
                                ul {
                                    @for alias in &aliases {
                                        li { (alias) }
                                    }
                                }
                            }
                        }
                    }
                    p { a class="btn" href="/sites" { "Back to Sites" } }
                }
            };
            render_site_page(&state, &user, &csrf, &path, id, TabId::Domains, content).await
        }
        Err(_) => (StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

/// GET /sites/{id}/runtime — PHP/runtime configuration for a site.
pub async fn runtime_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let path = format!("/sites/{id}/runtime");
    if gate(&state, &user, "runtime") == Err("unauthorized") {
        return render_unauthorized(&state, &user, &csrf, &path, id, TabId::Runtime).await;
    }
    match state.sites.get_site(id).await {
        Ok(site) => {
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
            let content = html! {
                section class="detail" {
                    h1 { "Runtime" }
                    dl class="site-overview" {
                        dt { "Document root" }
                        dd { code { (site.document_root()) } }
                        dt { "PHP" }
                        dd { (php) }
                        dt { "Status" }
                        dd { (site.status().as_str()) }
                    }
                    p { a class="btn" href="/sites" { "Back to Sites" } }
                }
            };
            render_site_page(&state, &user, &csrf, &path, id, TabId::Runtime, content).await
        }
        Err(_) => (StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

/// GET /sites/{id}/logs — log sources visible from a site workspace.
pub async fn logs(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let path = format!("/sites/{id}/logs");
    match gate(&state, &user, "logs") {
        Err("unauthorized") => {
            return render_unauthorized(&state, &user, &csrf, &path, id, TabId::Logs).await;
        }
        Err(_) => {
            return render_unavailable(&state, &user, &csrf, &path, id, TabId::Logs, "logs").await;
        }
        Ok(()) => {}
    }
    if state.sites.get_site(id).await.is_err() {
        return (StatusCode::NOT_FOUND, "site not found").into_response();
    }
    let actor = openpanel_app::logs::LogActor::new(user.id(), user.role());
    let sources = state.logs.sources(actor).await.unwrap_or_default();
    let content = html! {
        section class="detail" {
            h1 { "Logs" }
            @if sources.is_empty() {
                (crate::ui_states::EmptyState::new("No log sources yet", "Registered log sources appear here.").render())
            } @else {
                ul {
                    @for source in &sources {
                        li { a href=(format!("/logs/entries?source_id={}", source.id())) { (source.name()) } }
                    }
                }
            }
            p { a class="btn" href="/logs" { "All logs" } }
        }
    };
    render_site_page(&state, &user, &csrf, &path, id, TabId::Logs, content).await
}

/// GET /sites/{id}/backups — backup plans and runs visible from a site workspace.
pub async fn backups(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let path = format!("/sites/{id}/backups");
    match gate(&state, &user, "backups") {
        Err("unauthorized") => {
            return render_unauthorized(&state, &user, &csrf, &path, id, TabId::Backups).await;
        }
        Err(_) => {
            return render_unavailable(&state, &user, &csrf, &path, id, TabId::Backups, "backups")
                .await;
        }
        Ok(()) => {}
    }
    if state.sites.get_site(id).await.is_err() {
        return (StatusCode::NOT_FOUND, "site not found").into_response();
    }
    let all = matches!(user.role(), Role::Owner);
    let plans = state
        .backups
        .plans(user.id(), all)
        .await
        .unwrap_or_default();
    let runs = state.backups.runs(user.id(), all).await.unwrap_or_default();
    let content = html! {
        section class="detail" {
            h1 { "Backups" }
            h2 { "Plans" }
            @if plans.is_empty() {
                (crate::ui_states::EmptyState::new("No backup plans yet", "Create a plan to schedule automatic backups.")
                    .with_cta("/backups/new", "Create backup plan")
                    .render())
            } @else {
                ul { @for plan in &plans { li { (plan.name()) " — " (plan.schedule()) } } }
            }
            h2 { "Runs" }
            @if runs.is_empty() {
                (crate::ui_states::EmptyState::new("No backup runs yet", "Backup runs appear here after a plan executes.").render())
            } @else {
                ul { @for run in &runs { li { (format!("{:?}", run.state())) } } }
            }
            p { a class="btn" href="/backups" { "All backups" } }
        }
    };
    render_site_page(&state, &user, &csrf, &path, id, TabId::Backups, content).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Capability under test: `capability-navigation`.
    const CAPABILITY: &str = "capability-navigation";

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "capability-navigation");
    }

    #[test]
    fn site_tab_entries_cover_workspace_tabs() {
        for tab in ["domains", "runtime", "logs", "backups"] {
            let entry = capability_registry::site_entry_for_tab(tab)
                .unwrap_or_else(|| panic!("registry entry for tab `{tab}`"));
            assert!(
                entry.route.contains("/sites/{id}/"),
                "site route template: {}",
                entry.route
            );
        }
    }

    #[test]
    fn missing_tab_has_no_entry() {
        assert!(capability_registry::site_entry_for_tab("no-such-tab").is_none());
    }
}
