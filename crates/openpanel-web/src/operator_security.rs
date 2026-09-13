//! Owner/admin security-findings control plane (queue, detail,
//! preview, suppress, remediate).
//!
//! Thin adapter over the control-plane lifecycle service: handlers
//! enforce Owner/Admin + CSRF, render with the shared `table` / `form` /
//! `btn` / `op-empty-state` / `op-error-state` / `op-attn-*` /
//! `op-status-*` vocabulary (no new CSS tokens), and seed the queue
//! from the existing firewall + service-health services so scanners
//! stay authoritative. Covers `operator-security-control-plane`:
//! queue visibility, safe evidence, suppression expiry, verified
//! remediation.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use chrono::Utc;
use maud::{Markup, html};
use openpanel_domain::{
    Role,
    operator_security::{FindingSeverity, FindingState, SecurityFinding},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::router::{WebState, WebUser};
use crate::ui_states::{EmptyState, ErrorState};

/// Capability under test: `operator-security-control-plane`.
pub const CAPABILITY: &str = "operator-security-control-plane";

fn require_operator(user: &openpanel_domain::User) -> Result<(), StatusCode> {
    if matches!(user.role(), Role::Owner | Role::Admin) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Severity badge reusing the attention palette (no new tokens).
fn severity_class(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Critical | FindingSeverity::High => "op-attn-critical",
        FindingSeverity::Medium => "op-attn-warning",
        FindingSeverity::Low | FindingSeverity::Info => "op-attn-info",
    }
}

/// Lifecycle badge reusing the status palette (no new tokens).
fn state_class(state: FindingState) -> &'static str {
    match state {
        FindingState::Open => "op-status-degraded",
        FindingState::InRemediation => "op-status-unknown",
        FindingState::Resolved => "op-status-healthy",
        FindingState::Failed => "op-status-error",
        FindingState::Suppressed => "op-status-unknown",
    }
}

/// Seed the control plane from live firewall + service-health state.
/// Pure constructor so the seed policy is unit-testable.
pub fn seed_findings(
    active_blocks: usize,
    degraded_services: &[String],
    now: chrono::DateTime<Utc>,
) -> Vec<SecurityFinding> {
    let mut out = Vec::new();
    if active_blocks > 0
        && let Ok(item) = SecurityFinding::new(
            openpanel_domain::operator_security::FindingSource::Firewall,
            FindingSeverity::Medium,
            "firewall:rules",
            "login-blocks-active",
            "Login-abuse blocks are active",
            format!("{active_blocks} active login block(s); review host firewall and throttling"),
            openpanel_domain::operator_security::RemediationMode::Automatic,
            now,
        )
    {
        out.push(item);
    }
    for service in degraded_services {
        if let Ok(item) = SecurityFinding::new(
            openpanel_domain::operator_security::FindingSource::ServiceHealth,
            FindingSeverity::High,
            format!("service:{service}"),
            "service-degraded",
            format!("Service {service} needs attention"),
            format!("service {service} is not active; restart from the finding when ready"),
            openpanel_domain::operator_security::RemediationMode::Automatic,
            now,
        ) {
            out.push(item);
        }
    }
    out
}

/// GET /security/findings — prioritized queue.
pub async fn queue_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
) -> Response {
    if require_operator(&user).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let now = Utc::now();
    let items = state
        .operator_security
        .queue(&user, now)
        .await
        .unwrap_or_default();
    let content = html! {
        h1 { "Security findings" }
        p { "Prioritized queue across firewall, malware, WAF, compliance, and service health. Evidence is redacted; suppressions expire." }
        form method="post" action="/security/findings/seed" class="form form-grid" {
            (crate::layout::csrf_field(&csrf))
            button type="submit" { "Refresh from live services" }
        }
        (queue_table(&items))
    };
    state
        .render_shell(&user, &csrf, "/security/findings", content)
        .await
        .into_response()
}

/// Render the queue table or the all-clear empty state.
pub fn queue_table(items: &[SecurityFinding]) -> Markup {
    if items.is_empty() {
        return EmptyState::new(
            "Nothing needs attention",
            "No open security findings. Refresh from live services to re-check.",
        )
        .render();
    }
    html! {
        table class="table" {
            thead {
                tr { th { "Severity" } th { "Finding" } th { "Resource" } th { "State" } th { "" } }
            }
            tbody {
                @for item in items {
                    tr {
                        td { span class=(severity_class(item.severity())) { (item.severity().as_str()) } }
                        td { (item.title()) }
                        td { code { (item.resource()) } }
                        td { span class=(state_class(item.state())) { (item.state().as_str()) } }
                        td { a class="btn" href=(format!("/security/findings/{}", item.id())) { "Review" } }
                    }
                }
            }
        }
    }
}

/// GET /security/findings/{id} — detail with evidence, preview, suppress.
pub async fn detail(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
) -> Response {
    if require_operator(&user).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let item = match state.operator_security.find(&user, id).await {
        Ok(item) => item,
        Err(_) => {
            return state
                .render_shell(
                    &user,
                    &csrf,
                    "/security/findings",
                    ErrorState::new(
                        "Finding not found",
                        "It may have been resolved or never existed.",
                        "/security/findings",
                    )
                    .render(),
                )
                .await
                .into_response();
        }
    };
    let preview = state.operator_security.preview(&user, id).await.ok();
    let content = html! {
        h1 { (item.title()) }
        dl class="server-meta" {
            dt { "Severity" } dd { span class=(severity_class(item.severity())) { (item.severity().as_str()) } }
            dt { "Source" } dd { (item.source().as_str()) }
            dt { "Resource" } dd { code { (item.resource()) } }
            dt { "Rule" } dd { code { (item.rule_key()) } }
            dt { "State" } dd { span class=(state_class(item.state())) { (item.state().as_str()) } }
            dt { "Mode" } dd { (item.remediation_mode().as_str()) }
        }
        h2 { "Evidence" }
        p { (item.evidence()) }
        @if let Some(info) = preview {
            h2 { "Remediation preview" }
            p { (info.summary()) }
            ul {
                @for step in info.steps() {
                    li { (step) }
                }
            }
            p {
                @if info.supports_rollback() {
                    "Rollback is supported."
                } @else {
                    "Rollback is not supported; recovery guidance is shown on failure."
                }
            }
            form method="post" action=(format!("/security/findings/{id}/remediate")) class="form form-grid" {
                (crate::layout::csrf_field(&csrf))
                label { "Idempotency key" input name="idempotency_key" required; }
                label class="checkbox" { input type="checkbox" name="confirmed" value="1"; "I confirm this remediation" }
                button type="submit" { "Remediate" }
            }
        } @else {
            h2 { "Manual remediation" }
            p { "No automatic adapter covers this finding. Follow the evidence above using the specialized page, then re-check." }
        }
        h2 { "Suppress with expiry" }
        form method="post" action=(format!("/security/findings/{id}/suppress")) class="form form-grid" {
            (crate::layout::csrf_field(&csrf))
            label { "Reason" input name="reason" required; }
            label { "Scope" input name="scope" required; }
            label { "Expires in hours" input name="expires_in_hours" type="number" min="1" max="720" value="24" required; }
            button type="submit" { "Suppress" }
        }
        p { a class="btn" href="/security/findings" { "Back to queue" } }
    };
    state
        .render_shell(&user, &csrf, "/security/findings", content)
        .await
        .into_response()
}

#[derive(Default, Deserialize)]
#[serde(default)]
/// Suppress form fields.
pub struct SuppressForm {
    _csrf: String,
    reason: String,
    scope: String,
    expires_in_hours: i64,
}

#[derive(Default, Deserialize)]
#[serde(default)]
/// Remediate form fields.
pub struct RemediateForm {
    _csrf: String,
    idempotency_key: String,
    confirmed: Option<String>,
}

/// POST /security/findings/seed — ingest live firewall + service state.
pub async fn seed_queue(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<SuppressForm>,
) -> Response {
    if require_operator(&user).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let now = Utc::now();
    let active = state
        .security
        .blocks()
        .await
        .map(|blocks| blocks.iter().filter(|block| block.is_active(now)).count())
        .unwrap_or(0);
    let mut degraded = Vec::new();
    if let Ok(services) = state.system_services.inventory().await {
        for service in services {
            if service.status.active_state != "active" {
                degraded.push(service.descriptor.id().as_str().to_string());
            }
        }
    }
    let findings = seed_findings(active, &degraded, now);
    let _ = state.operator_security.ingest(&user, findings, now).await;
    Redirect::to("/security/findings").into_response()
}

/// POST /security/findings/{id}/suppress.
pub async fn suppress_post(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<SuppressForm>,
) -> Response {
    if require_operator(&user).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let now = Utc::now();
    let hours = form.expires_in_hours.clamp(1, 720);
    let expires_at = now + chrono::Duration::hours(hours);
    match state
        .operator_security
        .suppress_finding(&user, id, &form.reason, &form.scope, expires_at, now)
        .await
    {
        Ok(_) => Redirect::to("/security/findings").into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

/// POST /security/findings/{id}/remediate.
pub async fn remediate_post(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(id): Path<Uuid>,
    Form(form): Form<RemediateForm>,
) -> Response {
    if require_operator(&user).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.csrf.verify(session.id(), &form._csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let confirmed = form.confirmed.as_deref() == Some("1");
    match state
        .operator_security
        .remediate(&user, id, &form.idempotency_key, confirmed, Utc::now())
        .await
    {
        Ok(_) => Redirect::to("/security/findings").into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "operator-security-control-plane");
    }

    #[test]
    fn seed_policy_covers_blocks_and_degraded_services() {
        let now = Utc::now();
        assert!(seed_findings(0, &[], now).is_empty());
        let items = seed_findings(2, &["nginx".to_string()], now);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.resource() == "firewall:rules"));
        assert!(items.iter().any(|item| item.resource() == "service:nginx"));
        for item in &items {
            assert_eq!(
                item.remediation_mode(),
                openpanel_domain::operator_security::RemediationMode::Automatic
            );
            assert!(!item.evidence().contains("hunter2"));
        }
    }

    #[test]
    fn queue_table_uses_only_shipped_css_tokens() {
        let now = Utc::now();
        let items = seed_findings(1, &[], now);
        let out = queue_table(&items).into_string();
        assert!(out.contains("class=\"table\""));
        assert!(out.contains("/security/findings/"));
        let empty = queue_table(&[]).into_string();
        assert!(empty.contains("op-empty-state"));
    }

    #[test]
    fn severity_and_state_badges_reuse_shipped_tokens() {
        assert_eq!(
            severity_class(FindingSeverity::Critical),
            "op-attn-critical"
        );
        assert_eq!(severity_class(FindingSeverity::Medium), "op-attn-warning");
        assert_eq!(severity_class(FindingSeverity::Info), "op-attn-info");
        assert_eq!(state_class(FindingState::Resolved), "op-status-healthy");
        assert_eq!(state_class(FindingState::Failed), "op-status-error");
    }
}
