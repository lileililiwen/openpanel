//! Browser notification settings with explicit CSRF checks.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use openpanel_app::notifications::{CreateSmtpChannel, CreateSubscription, CreateWebhookChannel};
use openpanel_domain::{
    Role,
    notifications::{ChannelMetadata, EventKind, TlsMode},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// Render channel, subscription, and health controls.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    render(&state, &user, session.id()).await
}

#[derive(Deserialize)]
/// Browser channel creation form.
pub struct ChannelForm {
    _csrf: String,
    kind: String,
    name: String,
    host_or_url: String,
    port: Option<u16>,
    username: Option<String>,
    credential: String,
    from_addr: Option<String>,
    tls_mode: Option<String>,
    allowlist: String,
}

/// Create an SMTP or webhook channel.
pub async fn create_channel(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<ChannelForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let allowlist = csv(&form.allowlist);
    let result = if form.kind == "webhook" {
        state
            .notifications
            .create_webhook(
                &user,
                CreateWebhookChannel {
                    name: form.name,
                    url: form.host_or_url,
                    signing_secret: form.credential,
                    allowlist,
                },
            )
            .await
    } else {
        let tls_mode = match form.tls_mode.as_deref().unwrap_or("start_tls") {
            "tls" => TlsMode::Tls,
            "none" => TlsMode::None,
            _ => TlsMode::StartTls,
        };
        state
            .notifications
            .create_smtp(
                &user,
                CreateSmtpChannel {
                    name: form.name,
                    host: form.host_or_url,
                    port: form.port.unwrap_or(587),
                    username: form.username.unwrap_or_default(),
                    password: form.credential,
                    from_addr: form.from_addr.unwrap_or_default(),
                    tls_mode,
                    allowlist,
                },
            )
            .await
    };
    match result {
        Ok(_) => render(&state, &user, session.id()).await,
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
/// Browser channel test/disable form.
pub struct ChannelActionForm {
    _csrf: Option<String>,
    destination: Option<String>,
}

/// Test-send or disable one channel.
pub async fn channel_action(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path((id, action)): Path<(Uuid, String)>,
    Form(form): Form<ChannelActionForm>,
) -> Response {
    if user.role() != Role::Owner
        || !state
            .csrf
            .verify(session.id(), form._csrf.as_deref().unwrap_or(""))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result = match action.as_str() {
        "test" => {
            state
                .notifications
                .test_channel(&user, id, form.destination.as_deref().unwrap_or(""))
                .await
        }
        "disable" => state.notifications.disable_channel(&user, id).await,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match result {
        Ok(()) => render(&state, &user, session.id()).await,
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
/// Browser subscription creation form.
pub struct SubscriptionForm {
    _csrf: String,
    channel_id: Uuid,
    destination: String,
    kind: String,
    filter_json: String,
}

/// Create a caller-owned subscription.
pub async fn create_subscription(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<SubscriptionForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let kind = match form.kind.as_str() {
        "alert" => EventKind::Alert,
        "audit" => EventKind::Audit,
        "job-terminal" => EventKind::JobTerminal,
        _ => return (StatusCode::BAD_REQUEST, "invalid event kind").into_response(),
    };
    let filter = match serde_json::from_str(&form.filter_json) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid filter JSON").into_response(),
    };
    match state
        .notifications
        .create_subscription(
            &user,
            CreateSubscription {
                channel_id: form.channel_id,
                destination: form.destination,
                kind,
                filter,
            },
        )
        .await
    {
        Ok(_) => render(&state, &user, session.id()).await,
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
/// Browser CSRF-only action form.
pub struct CsrfForm {
    _csrf: String,
}

/// Disable a subscription without deleting history.
pub async fn disable_subscription(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> Response {
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match state.notifications.disable_subscription(&user, id).await {
        Ok(()) => render(&state, &user, session.id()).await,
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn render(state: &WebState, user: &openpanel_domain::User, session_id: Uuid) -> Response {
    let csrf = state.csrf.token_for(session_id);
    let channels = match state.notifications.channels(user).await {
        Ok(value) => value,
        Err(_error) if user.role() != Role::Owner => Vec::new(),
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };
    let subscriptions = match state.notifications.subscriptions(user).await {
        Ok(value) => value,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };
    state
        .render_shell(
            user,
            &csrf,
            "/settings",
            content(&channels, &subscriptions, &csrf, user.role() == Role::Owner),
        )
        .await
        .into_response()
}

fn content(
    channels: &[ChannelMetadata],
    subscriptions: &[openpanel_app::notifications::SubscriptionView],
    csrf: &str,
    owner: bool,
) -> Markup {
    html! {
        h1 { "Notifications" }
        @if owner {
            h2 { "Add channel" }
            form method="post" action="/settings/notifications" class="form" {
                (csrf_field(csrf))
                label { "Kind" select name="kind" { option value="smtp" { "SMTP" } option value="webhook" { "Webhook" } } }
                label { "Name" input name="name" required; }
                label { "SMTP host or HTTPS URL" input name="host_or_url" required; }
                label { "Port" input name="port" type="number" value="587"; }
                label { "Username" input name="username"; }
                label { "Password or signing secret" input name="credential" type="password" required; }
                label { "From address" input name="from_addr"; }
                input name="tls_mode" type="hidden" value="start_tls";
                label { "Allowlist (comma separated recipients or CIDRs)" input name="allowlist" required; }
                button type="submit" { "Add channel" }
            }
        }
        h2 { "Channels" }
        @for channel in channels {
            article { strong { (&channel.name) } " " (channel.kind.to_string()) " " (&channel.endpoint)
                form method="post" action=(format!("/settings/notifications/channels/{}/test", channel.id)) class="form form-inline-row" { (csrf_field(csrf)) label { "Destination" input name="destination" placeholder="allowed destination" required; button type="submit" { "Test send" } } }
                form method="post" action=(format!("/settings/notifications/channels/{}/disable", channel.id)) class="form form-inline" { (csrf_field(csrf)) button type="submit" { "Disable" } }
            }
        }
        h2 { "Add subscription" }
        form method="post" action="/settings/notifications/subscriptions" class="form" {
            (csrf_field(csrf))
            label { "Channel ID" input name="channel_id" required; }
            label { "Destination" input name="destination" required; }
            label { "Kind" select name="kind" { option value="alert" { "Alert" } option value="audit" { "Audit" } option value="job-terminal" { "Job terminal" } } }
            label { "Filter JSON" input name="filter_json" value="{}" required; }
            button type="submit" { "Subscribe" }
        }
        @for subscription in subscriptions {
            article { code { (subscription.id) } " " (&subscription.destination) " " (if subscription.enabled { "enabled" } else { "disabled" })
                @if subscription.enabled { form method="post" action=(format!("/settings/notifications/subscriptions/{}/disable", subscription.id)) class="form form-inline" { (csrf_field(csrf)) button type="submit" { "Disable" } } }
            }
        }
    }
}

fn csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}
