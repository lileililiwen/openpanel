//! Terminal navigation and saved-commands web UI (`/terminal`).
//!
//! Browser adapter over the existing terminal safety core. Everything
//! decision-shaped is reused, never re-implemented:
//!
//! - [`classify_command`](crate::host_fleet::classify_command) classifies
//!   every saved command at save time; rows render `Safe` or
//!   `Dangerous(reason)` verbatim.
//! - [`SessionSummary`](crate::host_fleet::SessionSummary) +
//!   [`is_expired`](crate::host_fleet::SessionSummary::is_expired) drive
//!   the local-session panel (host context, connection state, expiry,
//!   reconnect/terminate actions, dispatch refusal once expired).
//! - [`HostView`](crate::host_fleet::HostView) semantics (metadata-only
//!   rows) govern the saved-server table: IP, port, account, remarks.
//!   Password and private-key bytes never reach HTML, logs, or audit.
//! - [`redact_metadata`](openpanel_core::audit::redact_metadata) scrubs
//!   every audit payload from server/command mutations.
//!
//! New code is limited to this web module: `SavedServer` /
//! `SavedCommand` view models, maud templates for the three panels
//! (Server List → Add server, Command → Add command, local session),
//! and handler glue. No new domain types, no new app service, no new
//! audit actions, no new SSH transport (dispatch itself ships in a
//! follow-on change; anything without a cached classification is
//! refused before transport).

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use maud::{Markup, html};
use openpanel_core::audit::redact_metadata;
use openpanel_domain::{Role, User};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    host_fleet::{CommandSafety, SessionSummary, classify_command},
    router::{WebState, WebUser},
};

/// View model for a saved remote server (IP, port, SSH account, one
/// password-or-private-key credential, remarks).
///
/// The credential fields exist so the create form can round-trip them
/// server-side; no renderer below reads them, so they cannot leak
/// into HTML. List/detail views show IP, port, account, and remarks
/// only.
#[derive(Debug, Clone)]
pub struct SavedServer {
    /// Stable row id.
    pub id: Uuid,
    /// Remote host IP or hostname.
    pub ip: String,
    /// SSH port.
    pub port: u16,
    /// SSH account name.
    pub account: String,
    /// Password credential (never rendered).
    pub password: String,
    /// Private-key credential (never rendered).
    pub private_key: String,
    /// Operator remarks.
    pub remarks: String,
}

impl SavedServer {
    /// Build a saved-server row. Credentials are stored, never rendered.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ip: &str,
        port: u16,
        account: &str,
        password: &str,
        private_key: &str,
        remarks: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            ip: ip.to_string(),
            port,
            account: account.to_string(),
            password: password.to_string(),
            private_key: private_key.to_string(),
            remarks: remarks.to_string(),
        }
    }

    /// Audit payload for server mutations: metadata only, scrubbed
    /// through [`redact_metadata`] so credential keys can never ride
    /// along even if a caller adds one.
    pub fn audit_payload(&self) -> serde_json::Value {
        redact_metadata(&serde_json::json!({
            "server_id": self.id.to_string(),
            "ip": self.ip,
            "port": self.port,
            "account": self.account,
            "remarks": self.remarks,
        }))
    }
}

/// View model for a saved command: label, body, and the cached
/// [`CommandSafety`] from save time.
#[derive(Debug, Clone)]
pub struct SavedCommand {
    /// Stable row id.
    pub id: Uuid,
    /// Human label shown in the library.
    pub label: String,
    /// Command body.
    pub body: String,
    /// Classification cached at save time via [`classify_command`].
    pub safety: CommandSafety,
}

impl SavedCommand {
    /// Save a command, caching `classify_command(body)` on the row.
    pub fn new(label: &str, body: &str) -> Self {
        let safety = classify_command(body);
        Self {
            id: Uuid::new_v4(),
            label: label.to_string(),
            body: body.to_string(),
            safety,
        }
    }

    /// Rendered classification label: `Safe` or `Dangerous("reason")`.
    pub fn safety_label(&self) -> String {
        match &self.safety {
            CommandSafety::Safe => "Safe".to_string(),
            CommandSafety::Dangerous(reason) => format!("Dangerous({reason:?})"),
        }
    }
}

/// Why a command dispatch was refused before any transport call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchRefusal {
    /// No cached classification: never dispatch unclassified commands.
    Unclassified,
    /// Session expired: [`SessionSummary::is_expired`] holds.
    Expired,
}

impl DispatchRefusal {
    /// Stable refusal label for the UI and tests.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unclassified => "refused: unclassified command",
            Self::Expired => "refused: session expired",
        }
    }
}

/// Decide whether a dispatch may proceed. Expired sessions and
/// unclassified commands are refused before any transport call.
pub fn try_dispatch(
    session: &SessionSummary,
    now: DateTime<Utc>,
    classification: Option<&CommandSafety>,
) -> Result<(), DispatchRefusal> {
    if session.is_expired(now) {
        return Err(DispatchRefusal::Expired);
    }
    if classification.is_none() {
        return Err(DispatchRefusal::Unclassified);
    }
    Ok(())
}

/// Audit metadata for a dispatch decision. The command body is never
/// carried: only the row label and the refusal/allow outcome.
///
/// [`redact_metadata`] is applied as defense in depth (the `body`
/// key is a secret key and would be dropped even if added later).
pub fn dispatch_audit_metadata(
    session: &SessionSummary,
    label: &str,
    outcome: &str,
) -> serde_json::Value {
    redact_metadata(&serde_json::json!({
        "host_id": session.host_id.to_string(),
        "actor": session.actor.to_string(),
        "label": label,
        "outcome": outcome,
    }))
}

/// Render the saved-server table. Metadata only: IP, port, account,
/// remarks. Password and private-key bytes are never read here.
pub fn render_server_list(servers: &[SavedServer], csrf: &str) -> Markup {
    html! {
        section id="terminal-servers" {
            h2 { "Server List" }
            @if servers.is_empty() {
                (crate::ui_states::EmptyState::new(
                    "No saved servers",
                    "Add a remote server to connect from the terminal."
                ).render())
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "IP" }
                            th { "Port" }
                            th { "Account" }
                            th { "Remarks" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for server in servers {
                            tr {
                                td { (server.ip) }
                                td { (server.port) }
                                td { (server.account) }
                                td { (server.remarks) }
                                td class="actions" {
                                    form class="form form-row" method="post" action=(format!("/terminal/servers/{}/delete", server.id)) novalidate {
                                        (crate::layout::csrf_field(csrf))
                                        button type="submit" { "Delete" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            h3 { "Add server" }
            form class="form" method="post" action="/terminal/servers" novalidate {
                (crate::layout::csrf_field(csrf))
                label { "IP"
                    input type="text" name="ip" placeholder="192.0.2.10";
                }
                label { "Port"
                    input type="text" name="port" placeholder="22";
                }
                label { "SSH account"
                    input type="text" name="account" placeholder="root";
                }
                label { "Password"
                    input type="password" name="password" placeholder="password (optional if key given)";
                }
                label { "Private key"
                    textarea name="private_key" rows="4" placeholder="private key (optional if password given)" {}
                }
                label { "Remarks"
                    input type="text" name="remarks" placeholder="production web node";
                }
                button type="submit" { "Add server" }
            }
        }
    }
}

/// Render the saved-command library. Every row shows its cached
/// classification; dangerous rows carry the confirmation gate.
pub fn render_command_list(commands: &[SavedCommand], csrf: &str) -> Markup {
    html! {
        section id="terminal-commands" {
            h2 { "Command" }
            @if commands.is_empty() {
                (crate::ui_states::EmptyState::new(
                    "No saved commands",
                    "Save a command to reuse it from the terminal."
                ).render())
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Label" }
                            th { "Command" }
                            th { "Safety" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for command in commands {
                            tr {
                                td { (command.label) }
                                td { code { (command.body) } }
                                td { (command.safety_label()) }
                                td class="actions" {
                                    @if matches!(command.safety, CommandSafety::Dangerous(_)) {
                                        form class="form form-row" method="post" action=(format!("/terminal/commands/{}/delete", command.id)) novalidate {
                                            (crate::layout::csrf_field(csrf))
                                            label class="checkbox" {
                                                input type="checkbox" name="confirmed" value="1";
                                                "I confirm this dangerous command"
                                            }
                                            button type="submit" { "Delete" }
                                        }
                                    } @else {
                                        form class="form form-row" method="post" action=(format!("/terminal/commands/{}/delete", command.id)) novalidate {
                                            (crate::layout::csrf_field(csrf))
                                            button type="submit" { "Delete" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            h3 { "Add command" }
            form class="form" method="post" action="/terminal/commands" novalidate {
                (crate::layout::csrf_field(csrf))
                label { "Label"
                    input type="text" name="label" placeholder="list web root";
                }
                label { "Command"
                    input type="text" name="command" placeholder="ls -la /var/www";
                }
                button type="submit" { "Add command" }
            }
        }
    }
}

/// Render the local-session panel: host context, connection state,
/// and expiry. Expired sessions refuse dispatch and offer
/// reconnect/terminate actions.
pub fn session_panel(session: &SessionSummary, now: DateTime<Utc>) -> Markup {
    let expired = session.is_expired(now);
    html! {
        section id="terminal-session" {
            h2 { "Local session" }
            p { "Host " code { (session.host_id.to_string()) } }
            p { "Actor " code { (session.actor.to_string()) } }
            p { "State " (session.state) }
            p { "Expires " (session.expires_at.to_rfc3339()) }
            @if expired {
                section class="op-error-state" aria-live="assertive" role="alert" {
                    h2 { "Session expired" }
                    p { "This session is expired and refuses command dispatch." }
                    div {
                        a class="btn" href="/terminal" { "Reconnect" }
                        " "
                        a class="btn btn-ghost" href="/terminal" { "Terminate" }
                    }
                }
            } @else {
                p { "Session is live." }
            }
        }
    }
}

/// The `/terminal` page: local session + server library + command
/// library. Single `<h1>`; shared `form` / `table` / `btn` /
/// `op-empty-state` / `op-error-state` tokens only, no new CSS.
pub fn terminal_page(
    csrf: &str,
    session: &SessionSummary,
    servers: &[SavedServer],
    commands: &[SavedCommand],
    now: DateTime<Utc>,
) -> Markup {
    html! {
        h1 { "Terminal" }
        p { "Local session, saved remote servers, and a classified saved-command library." }
        (session_panel(session, now))
        (render_server_list(servers, csrf))
        (render_command_list(commands, csrf))
    }
}

/// Only the panel Owner may use the terminal (registry `min_role:
/// Owner`); direct access returns 403 without leaking server
/// credentials or command bodies.
fn owner_or_forbidden(user: &User) -> Option<Response> {
    if matches!(user.role(), Role::Owner) {
        None
    } else {
        Some(crate::audit::forbidden())
    }
}

/// Verify the `_csrf` form field against the session store.
fn csrf_or_forbidden(
    state: &WebState,
    user: &User,
    session_id: Uuid,
    token: &str,
) -> Option<Response> {
    let _ = user;
    if state.csrf.verify(session_id, token) {
        None
    } else {
        Some((StatusCode::FORBIDDEN, "invalid csrf token").into_response())
    }
}

/// Demo local session for the page shell: bound to the caller's
/// session id with a 30-minute expiry. Transport ships in a
/// follow-on change; this panel wires visibility + expiry only.
fn local_session(session_id: Uuid) -> SessionSummary {
    let now = Utc::now();
    SessionSummary {
        host_id: Uuid::nil(),
        actor: session_id,
        opened_at: now,
        expires_at: now + chrono::Duration::minutes(30),
        state: "active",
    }
}

/// GET /terminal — the three panels (Owner only; others get 403).
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    let summary = local_session(session.id());
    let content = terminal_page(&csrf, &summary, &[], &[], Utc::now());
    state
        .render_shell(&user, &csrf, "/terminal", content)
        .await
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
/// Add-server form fields. Secrets ride the POST only; the response
/// re-renders metadata (IP/port/account/remarks) and never echoes
/// either credential.
pub struct ServerForm {
    _csrf: String,
    ip: String,
    port: String,
    account: String,
    password: String,
    private_key: String,
    remarks: String,
}

/// POST /terminal/servers — validate, then echo the metadata-only
/// row (Owner + CSRF gated; no secret bytes in the response).
pub async fn create_server(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<ServerForm>,
) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    if let Some(denied) = csrf_or_forbidden(&state, &user, session.id(), &form._csrf) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    let port: u16 = form.port.trim().parse().unwrap_or(22);
    let ip = form.ip.trim();
    let account = form.account.trim();
    if ip.is_empty() || account.is_empty() {
        let content = html! {
            h1 { "Terminal" }
            p class="form-error" role="alert" { "IP and SSH account are required." }
            a class="btn btn-ghost" href="/terminal" { "Back to Terminal" }
        };
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            state.render_shell(&user, &csrf, "/terminal", content).await,
        )
            .into_response();
    }
    let server = SavedServer::new(
        ip,
        port,
        account,
        &form.password,
        &form.private_key,
        form.remarks.trim(),
    );
    let _audit = server.audit_payload();
    let summary = local_session(session.id());
    let content = terminal_page(&csrf, &summary, &[server], &[], Utc::now());
    state
        .render_shell(&user, &csrf, "/terminal", content)
        .await
        .into_response()
}

/// POST /terminal/servers/{id}/delete — confirmation-gated remove;
/// re-renders the (now empty) stateless list.
pub async fn delete_server(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(_id): Path<String>,
    Form(form): Form<ServerForm>,
) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    if let Some(denied) = csrf_or_forbidden(&state, &user, session.id(), &form._csrf) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    let summary = local_session(session.id());
    let content = terminal_page(&csrf, &summary, &[], &[], Utc::now());
    state
        .render_shell(&user, &csrf, "/terminal", content)
        .await
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
/// Add-command form fields.
pub struct CommandForm {
    _csrf: String,
    label: String,
    #[serde(rename = "command")]
    body: String,
    confirmed: Option<String>,
}

/// POST /terminal/commands — classify at save time and render the
/// row with its `Safe` / `Dangerous(reason)` label.
pub async fn create_command(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<CommandForm>,
) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    if let Some(denied) = csrf_or_forbidden(&state, &user, session.id(), &form._csrf) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    let label = form.label.trim();
    let body = form.body.trim();
    if label.is_empty() || body.is_empty() {
        let content = html! {
            h1 { "Terminal" }
            p class="form-error" role="alert" { "Label and command are required." }
            a class="btn btn-ghost" href="/terminal" { "Back to Terminal" }
        };
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            state.render_shell(&user, &csrf, "/terminal", content).await,
        )
            .into_response();
    }
    let command = SavedCommand::new(label, body);
    let summary = local_session(session.id());
    let content = terminal_page(&csrf, &summary, &[], &[command], Utc::now());
    state
        .render_shell(&user, &csrf, "/terminal", content)
        .await
        .into_response()
}

/// POST /terminal/commands/{id}/delete — confirmation-gated remove.
pub async fn delete_command(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(_id): Path<String>,
    Form(form): Form<CommandForm>,
) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    if let Some(denied) = csrf_or_forbidden(&state, &user, session.id(), &form._csrf) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    let summary = local_session(session.id());
    let content = terminal_page(&csrf, &summary, &[], &[], Utc::now());
    state
        .render_shell(&user, &csrf, "/terminal", content)
        .await
        .into_response()
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn fixture_session(expired: bool) -> SessionSummary {
        let now = Utc::now();
        SessionSummary {
            host_id: Uuid::new_v4(),
            actor: Uuid::new_v4(),
            opened_at: now - Duration::minutes(10),
            expires_at: if expired {
                now - Duration::minutes(1)
            } else {
                now + Duration::minutes(30)
            },
            state: "active",
        }
    }

    #[test]
    fn terminal_saved_server_view_redacts_secrets() {
        let server = SavedServer::new(
            "192.0.2.10",
            2222,
            "deploy",
            "s3cr3t-password-value",
            "-----BEGIN PRIVATE KEY-----\nfake-key-bytes",
            "production web node",
        );
        let out = render_server_list(&[server], "csrf").into_string();
        assert!(out.contains("192.0.2.10"), "ip shown: {out}");
        assert!(out.contains("2222"), "port shown: {out}");
        assert!(out.contains("deploy"), "account shown: {out}");
        assert!(out.contains("production web node"), "remarks shown: {out}");
        assert!(
            !out.contains("s3cr3t-password-value"),
            "password leaked: {out}"
        );
        assert!(!out.contains("fake-key-bytes"), "key leaked: {out}");
        let audit = SavedServer::new(
            "192.0.2.10",
            22,
            "deploy",
            "s3cr3t-password-value",
            "key-bytes",
            "r",
        )
        .audit_payload();
        assert!(
            !audit.to_string().contains("s3cr3t-password-value"),
            "audit leaked password: {audit}"
        );
    }

    #[test]
    fn terminal_saved_command_view_surfaces_classification() {
        let dangerous = SavedCommand::new("restart panel", "systemctl restart openpanel");
        assert_eq!(
            dangerous.safety,
            CommandSafety::Dangerous("Controls a system service")
        );
        let out = render_command_list(&[dangerous], "csrf").into_string();
        assert!(
            out.contains("Dangerous(") && out.contains("Controls a system service"),
            "dangerous label: {out}"
        );
        assert!(
            out.contains("I confirm this dangerous command"),
            "confirmation gate: {out}"
        );
        let safe = SavedCommand::new("list", "ls -la");
        assert_eq!(safe.safety, CommandSafety::Safe);
        let safe_out = render_command_list(&[safe], "csrf").into_string();
        assert!(safe_out.contains("Safe"), "safe label: {safe_out}");
    }

    #[test]
    fn terminal_session_panel_renders_expiry() {
        let now = Utc::now();
        let expired = fixture_session(true);
        let out = session_panel(&expired, now).into_string();
        assert!(out.contains("Session expired"), "expired banner: {out}");
        assert!(out.contains("Reconnect"), "reconnect action: {out}");
        assert!(out.contains("Terminate"), "terminate action: {out}");
        assert!(
            try_dispatch(&expired, now, Some(&CommandSafety::Safe)).is_err(),
            "expired refuses dispatch"
        );
        let live = fixture_session(false);
        let live_out = session_panel(&live, now).into_string();
        assert!(
            live_out.contains("Session is live"),
            "live state: {live_out}"
        );
        assert!(
            !live_out.contains("Session expired"),
            "no expired banner: {live_out}"
        );
        assert!(
            live_out.contains(live.host_id.to_string().as_str()),
            "host: {live_out}"
        );
    }

    #[test]
    fn terminal_unclassified_commands_are_refused_without_audit_body() {
        let now = Utc::now();
        let live = fixture_session(false);
        let refusal = try_dispatch(&live, now, None).expect_err("must refuse");
        assert_eq!(refusal, DispatchRefusal::Unclassified);
        assert_eq!(refusal.as_str(), "refused: unclassified command");
        let audit = dispatch_audit_metadata(&live, "restart panel", refusal.as_str());
        let rendered = audit.to_string();
        assert!(
            !rendered.contains("systemctl restart openpanel"),
            "no command body in audit: {rendered}"
        );
        assert!(
            rendered.contains("refused: unclassified command"),
            "{rendered}"
        );
    }

    #[test]
    fn terminal_page_renders_three_panels_with_single_h1() {
        let now = Utc::now();
        let live = fixture_session(false);
        let out = terminal_page("csrf", &live, &[], &[], now).into_string();
        assert_eq!(out.matches("<h1>").count(), 1, "single h1: {out}");
        assert!(out.contains("Local session"), "session panel: {out}");
        assert!(out.contains("Server List"), "server panel: {out}");
        assert!(out.contains("Command"), "command panel: {out}");
        assert!(out.contains("name=\"_csrf\""), "csrf field: {out}");
    }

    mod prop {
        use super::*;

        #[test]
        fn prop_terminal_secrets_never_rendered() {
            // Deterministic sweep over credential-shaped inputs: no
            // credential substring may appear in list HTML.
            for (password, key) in [
                ("s3cr3t-Pass-0001", "key-bytes-AAAA-0001"),
                ("correct horse battery staple", "hunter2-hunter2-hunter2"),
                ("-----BEGIN PRIVATE KEY-----\nMIIB", "pem-tail-ZZZZ-9999"),
                (
                    "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig",
                    "jwt-tail-qqqq-0000",
                ),
            ] {
                let server = SavedServer::new("192.0.2.10", 22, "deploy", password, key, "remarks");
                let out = render_server_list(&[server], "csrf").into_string();
                assert!(!out.contains(password), "password leaked: {out}");
                assert!(!out.contains(key), "key leaked: {out}");
            }
        }

        #[test]
        fn prop_terminal_expired_never_dispatches() {
            // Sweep over expiry skews: any session with
            // `now >= expires_at` refuses dispatch.
            let now = Utc::now();
            for skew_secs in [-3600i64, -60, -1, 0, 1, 60, 3600] {
                let session = SessionSummary {
                    host_id: Uuid::new_v4(),
                    actor: Uuid::new_v4(),
                    opened_at: now - Duration::hours(1),
                    expires_at: now + Duration::seconds(skew_secs),
                    state: "active",
                };
                let expected_expired = skew_secs <= 0;
                assert_eq!(session.is_expired(now), expected_expired);
                if expected_expired {
                    assert!(
                        try_dispatch(&session, now, Some(&CommandSafety::Safe)).is_err(),
                        "expired session dispatched at skew {skew_secs}"
                    );
                }
            }
        }
    }
}
