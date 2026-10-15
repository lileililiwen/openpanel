//! Per-site WebDAV account HTML surface.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::html;
use openpanel_app::{CreateWebDavAccount, UpdateWebDavAccount};
use openpanel_domain::Role;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
    site_workspace::TabId,
};

/// Render a site's WebDAV accounts and creation form.
pub async fn page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if user.role() != Role::Owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    match state.webdav.list(&user, id).await {
        Ok(accounts) => {
            let body = html! {
                (crate::site_workspace::site_bar(&state, &user, id, TabId::WebDav).await)
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
/// Browser form for creating a site WebDAV account.
pub struct CreateForm {
    _csrf: String,
    username: String,
    password: String,
    read_only: Option<String>,
}

/// Create a WebDAV account after CSRF verification.
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
        .webdav
        .create(
            &user,
            id,
            CreateWebDavAccount {
                username: form.username,
                password: form.password,
                read_only: form.read_only.is_some(),
            },
        )
        .await
    {
        Ok(created) => {
            let csrf = state.csrf.token_for(session.id());
            let accounts = state.webdav.list(&user, id).await.unwrap_or_default();
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

/// Enable, disable, or delete a WebDAV account after CSRF verification.
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
            .webdav
            .update(
                &user,
                site_id,
                account_id,
                UpdateWebDavAccount {
                    enabled: Some(true),
                    password: None,
                    read_only: None,
                },
            )
            .await
            .map(|_| ()),
        "disable" => state
            .webdav
            .update(
                &user,
                site_id,
                account_id,
                UpdateWebDavAccount {
                    enabled: Some(false),
                    password: None,
                    read_only: None,
                },
            )
            .await
            .map(|_| ()),
        "delete" => state.webdav.delete(&user, site_id, account_id).await,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match result {
        Ok(()) => Redirect::to(&format!("/sites/{site_id}/webdav")).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

fn content(
    site_id: Uuid,
    accounts: &[openpanel_app::WebDavAccountView],
    csrf: &str,
    banner: Option<&str>,
) -> maud::Markup {
    html! {
        h1 { "WebDAV accounts" }
        @if let Some(message)=banner { p class="notice" { (message) } }
        form method="post" action=(format!("/sites/{site_id}/webdav")) class="form" {
            (csrf_field(csrf))
            label { "Username" input name="username" required; }
            label { "Password" input name="password" type="password" minlength="12" required; }
            label class="checkbox" { input name="read_only" type="checkbox" value="true"; " Read only" }
            button type="submit" { "Create account" }
        }
        p { "Accounts serve files over HTTP at " code { "/dav/" (site_id) "/…" } " with Basic auth." }
        @if accounts.is_empty() {
            (crate::ui_states::EmptyState::new("No WebDAV accounts yet", "Create an account to expose this site's files over WebDAV.").render())
        } @else {
            table class="table" {
                thead { tr { th { "Username" } th { "Home" } th { "Mode" } th { "Status" } th { "Last login" } th { "Actions" } } }
                tbody { @for account in accounts { tr { td { (&account.username) } td { (&account.home) } td { @if account.read_only { "read only" } @else { "read/write" } } td { @if account.enabled { "enabled" } @else { "disabled" } } td { @if let Some(at)=account.last_login_at { (at.format("%Y-%m-%d %H:%M UTC")) } @else { "never" } } td {
                    @if account.enabled {
                        form method="post" action=(format!("/sites/{site_id}/webdav/{}/disable", account.id)) class="form form-inline" { (csrf_field(csrf)) button { "Disable" } }
                    } @else {
                        form method="post" action=(format!("/sites/{site_id}/webdav/{}/enable", account.id)) class="form form-inline" { (csrf_field(csrf)) button { "Enable" } }
                    }
                    a class="btn danger" hx-get=(format!("/layer/confirm?action=delete-webdav-account&site_id={site_id}&account_id={}", account.id))
                        hx-target="#layer-root" href=(format!("/layer/confirm?action=delete-webdav-account&site_id={site_id}&account_id={}", account.id)) {
                        "Delete"
                    }
                } } } }
            }
        }
    }
}
