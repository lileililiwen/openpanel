//! Owner-only host-security browser page and rule creation action.

use axum::{
    Form,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use maud::html;
use openpanel_domain::{
    Role,
    security::{FirewallRule, NetworkCidr, PortRange, Protocol, RuleAction},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::router::{WebState, WebUser};

/// Render firewall posture, managed rules, and active blocks.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    if !matches!(user.role(), Role::Owner) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let rules = state.security.rules().await.unwrap_or_default();
    let blocks = state.security.blocks().await.unwrap_or_default();
    let content = html! {
        h1 { "Host security" }
        p { "OpenPanel manages only the isolated nftables table " code { "inet openpanel" } "." }
        h2 { "Firewall rules" }
        ul { @for rule in rules { li { (rule.comment()) " — " (rule.source()) ":" (rule.ports()) } } }
        form method="post" action="/security/rules" class="form form-grid" {
            (crate::layout::csrf_field(&csrf))
            label { "Port" input name="port" type="number" min="1" max="65535" required; }
            label { "Source CIDR" input name="source" value="0.0.0.0/0" required; }
            label { "Comment" input name="comment" required; }
            button type="submit" { "Add TCP allow rule" }
        }
        h2 { "Login abuse blocks" }
        p { (blocks.len()) " recorded block(s)" }
    };
    state
        .render_shell(&user, &csrf, "/security", content)
        .await
        .into_response()
}

#[derive(Default, Deserialize)]
#[serde(default)]
/// Minimal safe browser rule fields.
pub struct RuleForm {
    _csrf: String,
    port: u16,
    source: String,
    comment: String,
}

/// Validate CSRF and create an enabled TCP allow rule.
pub async fn create(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<RuleForm>,
) -> Response {
    if !matches!(user.role(), Role::Owner) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let rule = PortRange::new(form.port, form.port).and_then(|ports| {
        NetworkCidr::parse(&form.source).and_then(|source| {
            FirewallRule::new(
                Uuid::new_v4(),
                Protocol::Tcp,
                ports,
                source,
                RuleAction::Allow,
                form.comment,
                true,
            )
        })
    });
    match rule {
        Ok(rule) => match state.security.save_rule(rule).await {
            Ok(_) => Redirect::to("/security").into_response(),
            Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
        },
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}
