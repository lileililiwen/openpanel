//! Owner-only per-site WAF editor.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use openpanel_domain::{
    Role,
    waf::{DefaultAction, Rule, RuleSet},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    layout::csrf_field,
    router::{WebState, WebUser},
};

/// Render the rule document, hit totals, and dry-run form.
pub async fn page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if user.role() != Role::Owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let set = match state.waf.get(&user, id).await {
        Ok(set) => set,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    render(&state, &user, &csrf, id, &set, None).await
}

/// Browser replacement form carrying a strict JSON rule array.
#[derive(Deserialize)]
pub struct WafForm {
    _csrf: String,
    version: u64,
    default_action: DefaultAction,
    rules_json: String,
}

/// Browser dry-run input containing strict rule and request JSON.
#[derive(Deserialize)]
pub struct WafTestForm {
    _csrf: String,
    rule_json: String,
    request_json: String,
}

/// Replace the complete rule set after CSRF verification.
pub async fn save(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<WafForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let rules = match serde_json::from_str::<Vec<Rule>>(&form.rules_json) {
        Ok(rules) => rules,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    let set = match RuleSet::new(id, form.version, form.default_action, rules) {
        Ok(set) => set,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    match state.waf.put(&user, set).await {
        Ok(saved) => {
            render(
                &state,
                &user,
                &form._csrf,
                id,
                &saved,
                Some("WAF policy saved"),
            )
            .await
        }
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

/// Compile and simulate one rule without changing the live policy.
pub async fn test_rule(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<WafTestForm>,
) -> Response {
    if user.role() != Role::Owner || !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let rule = match serde_json::from_str(&form.rule_json) {
        Ok(rule) => rule,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    let request = match serde_json::from_str(&form.request_json) {
        Ok(request) => request,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    let result = match state
        .waf
        .dry_run(
            &user,
            id,
            openpanel_domain::waf::DryRunRequest { rule, request },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    let set = match state.waf.get(&user, id).await {
        Ok(set) => set,
        Err(error) => return (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    };
    let outcome = format!(
        "Dry run: would_match={}, action={:?}",
        result.matched.would_match, result.matched.action
    );
    render(&state, &user, &form._csrf, id, &set, Some(&outcome)).await
}

async fn render(
    state: &WebState,
    user: &openpanel_domain::User,
    csrf: &str,
    id: Uuid,
    set: &RuleSet,
    notice: Option<&str>,
) -> Response {
    let rules_json = serde_json::to_string_pretty(set.rules()).unwrap_or_else(|_| "[]".to_owned());
    let hits = state.waf.hits(user, id).await.unwrap_or_default();
    let content = html! {
        h1 { "Web application firewall" }
        p { "Typed rules are compiled into the managed nginx site configuration and validated before activation." }
        @if let Some(notice) = notice { p class="banner banner--ok" { (notice) } }
        form method="post" action=(format!("/sites/{id}/waf")) class="form" {
            (csrf_field(csrf))
            label { "Version" input type="number" min="1" name="version" value=(set.version()); }
            label { "Default action" select name="default_action" {
                option value="allow" selected[set.default_action() == DefaultAction::Allow] { "Allow" }
                option value="challenge" selected[set.default_action() == DefaultAction::Challenge] { "Challenge" }
                option value="deny" selected[set.default_action() == DefaultAction::Deny] { "Deny" }
            } }
            label { "Rules (strict JSON)" textarea name="rules_json" rows="18" { (rules_json) } }
            button type="submit" { "Save WAF policy" }
        }
        h2 { "Rule hits" }
        @if hits.is_empty() { p { "No rule hits recorded." } } @else { ul { @for hit in hits { li { (hit.kind) ": " (hit.count) } } } }
        h2 { "Test one rule" }
        form method="post" action=(format!("/sites/{id}/waf/test")) class="form" {
            (csrf_field(csrf))
            label { "Rule (strict JSON)" textarea name="rule_json" rows="8" { "{}" } }
            label { "Request (strict JSON)" textarea name="request_json" rows="6" { r#"{"path":"/","method":"GET","user_agent":"Browser","country":null}"# } }
            button type="submit" { "Dry-run rule" }
        }
    };
    state
        .render_shell(user, csrf, "/sites", content)
        .await
        .into_response()
}
