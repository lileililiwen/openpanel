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
