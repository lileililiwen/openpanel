//! Browser API-token lifecycle with CSRF and show-once plaintext.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use maud::{Markup, html};
use openpanel_app::api_tokens::CreateApiToken;
use openpanel_domain::ApiTokenMetadata;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// Render token metadata and lifecycle forms.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    render(&state, &user, session.id(), None).await
}

#[derive(Debug, Deserialize)]
/// Browser token creation form.
pub struct CreateForm {
    _csrf: String,
    label: String,
    scopes: String,
    expires_in_days: i64,
    cidr_allowlist: Option<String>,
}

/// Create a token and render its plaintext once in the response.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CreateForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let scopes = form
        .scopes
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect();
    let cidr_allowlist = form
        .cidr_allowlist
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|cidr| !cidr.is_empty())
        .map(str::to_owned)
        .collect();
    match state
        .api_tokens
        .create(
            &user,
            CreateApiToken {
                user_id: user.id(),
                label: form.label,
                scopes,
                expires_at: Utc::now() + Duration::days(form.expires_in_days),
                cidr_allowlist,
            },
        )
        .await
    {
        Ok(created) => render(&state, &user, session.id(), Some(&created.plaintext_token)).await,
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
/// CSRF form used by rotate and revoke buttons.
pub struct ActionForm {
    _csrf: String,
}

/// Rotate or revoke a token.
pub async fn action(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path((id, action)): Path<(Uuid, String)>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match action.as_str() {
        "rotate" => match state.api_tokens.rotate(&user, user.id(), id).await {
            Ok(created) => {
                render(&state, &user, session.id(), Some(&created.plaintext_token)).await
            }
            Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
        },
        "revoke" => match state.api_tokens.revoke(&user, user.id(), id).await {
            Ok(()) => render(&state, &user, session.id(), None).await,
            Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
        },
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn render(
    state: &WebState,
    user: &openpanel_domain::User,
    session_id: Uuid,
    plaintext: Option<&str>,
) -> Response {
    let csrf = state.csrf.token_for(session_id);
    match state.api_tokens.list(user, user.id()).await {
        Ok(tokens) => state
            .render_shell(user, &csrf, "/settings", content(&tokens, &csrf, plaintext))
            .await
            .into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

fn content(tokens: &[ApiTokenMetadata], csrf: &str, plaintext: Option<&str>) -> Markup {
    html! {
        h1 { "API tokens" }
        @if let Some(value) = plaintext {
            aside class="notice" role="status" {
                strong { "This token is shown exactly once. Copy it now." }
                code { (value) }
            }
        }
        form method="post" action="/settings/tokens" {
            (csrf_field(csrf))
            label { "Label" input name="label" required maxlength="120"; }
            label { "Scopes (comma separated)" input name="scopes" required placeholder="sites:read"; }
            label { "Allowed CIDRs (optional)" input name="cidr_allowlist" placeholder="192.0.2.0/24"; }
            label { "Lifetime in days" input name="expires_in_days" type="number" min="1" max="365" value="90"; }
            button type="submit" { "Create token" }
        }
        table {
            thead { tr { th { "Label" } th { "Scopes" } th { "Expires" } th { "Status" } th { "Actions" } } }
            tbody {
                @for token in tokens {
                    tr {
                        td { (&token.label) }
                        td { @for scope in &token.scopes { code { (scope) } " " } }
                        td { (token.expires_at) }
                        td { @if token.revoked_at.is_some() { "revoked" } @else { "active" } }
                        td {
                            @if token.revoked_at.is_none() {
                                form method="post" action=(format!("/settings/tokens/{}/rotate", token.id)) {
                                    (csrf_field(csrf))
                                    button type="submit" { "Rotate" }
                                }
                                form method="post" action=(format!("/settings/tokens/{}/revoke", token.id)) {
                                    (csrf_field(csrf))
                                    button type="submit" { "Revoke" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
