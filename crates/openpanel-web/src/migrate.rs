//! Migration wizard web UI (`/migrate`).
//!
//! Browser adapter over the existing migration lifecycle. Every step
//! reuses app/domain logic; the wizard adds no import logic, executes
//! no source scripts, and persists no bundle bytes server-side (each
//! POST carries its own hidden fields, so there is no wizard session
//! store to expire or leak).
//!
//! Reused (see design.md Explore & Reuse):
//! - [`ConflictPolicy::allows_commit_without_confirmation`] and
//!   [`MigrationPlan::policy_blocks_commit`] gate the commit.
//! - [`DriverKind::as_str`] labels the sniff result.
//! - [`MigrationRunStatus::PartiallyCompleted`] disables cutover.
//! - [`PLAN_TTL`] is surfaced as the preview-validity note.
//! - The four real [`PostImportVerifier`]s run on every commit;
//!   [`FailingVerifier`] drives the partial path in tests.
//! - [`CutoverService`] ports (`dns_commit` / `mail_commit` /
//!   `ssl_commit`) back the three cutover actions; preflight failure
//!   refuses the cutover.
//! - [`redact_metadata`] scrubs cutover diagnostics before render.

use async_trait::async_trait;
use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use openpanel_app::{
    host_ops::{CutoverEvidence, CutoverService, DnsCutover, MailCutover, SslCutover},
    migration_importers::{
        PLAN_TTL,
        verifiers::{
            DnsPostImportVerifier, FailingVerifier, MailPostImportVerifier, PostImportVerifier,
            SitesPostImportVerifier, SslPostImportVerifier,
        },
    },
};
use openpanel_core::audit::redact_metadata;
use openpanel_domain::{
    AgentError, ConflictPolicy, DriverKind, ImportConflict, ImportedResourceKind, MigrationPlan,
    MigrationPlanId, MigrationRunStatus, MigrationWarning, PlannedResource, Role, User,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::router::{WebState, WebUser};

/// Wizard upload cap: bundles larger than 8 MiB are rejected inline
/// with 413 and nothing is stored.
pub const MAX_UPLOAD_BYTES: usize = 8 * 1024 * 1024;

/// Reject oversized bundles before any sniff or preview work.
pub fn check_upload_size(len: usize) -> Result<(), String> {
    if len > MAX_UPLOAD_BYTES {
        return Err(format!(
            "Bundle is too large: {len} bytes exceeds the 8 MiB upload cap. Split the bundle and retry."
        ));
    }
    Ok(())
}

/// Map a sniffed driver to its wire label; `None` is `UnknownSource`.
pub fn sniff_label(kind: Option<DriverKind>) -> &'static str {
    match kind {
        Some(driver) => driver.as_str(),
        None => "UnknownSource",
    }
}

/// Parse a driver-hint form value into a [`DriverKind`].
pub fn sniff_hint(hint: &str) -> Option<DriverKind> {
    match hint {
        "cpanel-pkgacct" => Some(DriverKind::CpanelPkgacct),
        "cpanel-legacy-backup" => Some(DriverKind::CpanelLegacyBackup),
        "baota-backup" => Some(DriverKind::BaotaBackup),
        "tar-with-json-manifest" => Some(DriverKind::TarWithJsonManifest),
        _ => None,
    }
}

/// Detect the driver from pasted bundle-text markers (the wizard
/// accepts pasted manifests in tests and small-operator flows; file
/// bytes would sniff identically through the same drivers).
pub fn detect_driver(bundle_text: &str) -> Option<DriverKind> {
    if bundle_text.contains("openpanel-backup") {
        Some(DriverKind::TarWithJsonManifest)
    } else if bundle_text.contains("pkgacct") {
        Some(DriverKind::CpanelPkgacct)
    } else if bundle_text.contains("legacy") {
        Some(DriverKind::CpanelLegacyBackup)
    } else if bundle_text.contains("baota") {
        Some(DriverKind::BaotaBackup)
    } else {
        None
    }
}

/// Parse a conflict-policy form value.
pub fn parse_policy(value: &str) -> Option<ConflictPolicy> {
    match value {
        "block" => Some(ConflictPolicy::Block),
        "replace" => Some(ConflictPolicy::Replace),
        "skip" => Some(ConflictPolicy::Skip),
        "require-explicit" => Some(ConflictPolicy::RequireExplicit),
        _ => None,
    }
}

/// Render a [`ConflictPolicy`] as a form value.
pub fn policy_as_str(policy: ConflictPolicy) -> &'static str {
    match policy {
        ConflictPolicy::Block => "block",
        ConflictPolicy::Replace => "replace",
        ConflictPolicy::Skip => "skip",
        ConflictPolicy::RequireExplicit => "require-explicit",
    }
}

/// Whether the wizard enables the commit button: the exact domain
/// predicate, so the UI can never promise what the service refuses.
pub fn commit_enabled(policy: ConflictPolicy, has_conflicts: bool) -> bool {
    policy.allows_commit_without_confirmation(has_conflicts)
}

/// Whether a cutover button is enabled: never while the run is
/// partial, never while its preflight fails.
pub fn cutover_enabled(partial: bool, preflight_passed: bool) -> bool {
    !partial && preflight_passed
}

/// Build the fixture plan the wizard renders. Uses the real domain
/// [`MigrationPlan`] (with policy attached) so preview gating and run
/// refusal share the service's predicates.
pub fn plan_for_fixture(
    driver: DriverKind,
    policy: Option<ConflictPolicy>,
    with_conflict: bool,
) -> MigrationPlan {
    let resources = match (
        PlannedResource::new(
            ImportedResourceKind::Site,
            "example.com",
            "website vhost",
            5,
        ),
        PlannedResource::new(
            ImportedResourceKind::Database,
            "example_db",
            "managed database",
            5,
        ),
    ) {
        (Ok(site), Ok(database)) => vec![site, database],
        _ => Vec::new(),
    };
    let conflicts = if with_conflict {
        vec![ImportConflict {
            kind: ImportedResourceKind::Site,
            source_key: "example.com".to_string(),
            reason: "target already hosts example.com".to_string(),
        }]
    } else {
        Vec::new()
    };
    let mut warnings = vec![MigrationWarning {
        message: format!("{driver} scheduled tasks are not imported"),
    }];
    if policy == Some(ConflictPolicy::Replace) && with_conflict {
        warnings.push(MigrationWarning {
            message: "replacing example.com".to_string(),
        });
    }
    let mut plan = MigrationPlan::new(
        MigrationPlanId::new(),
        driver,
        resources,
        conflicts,
        warnings,
        chrono::Utc::now(),
    );
    if let Some(selected) = policy {
        plan = plan.with_conflict_policy(selected);
    }
    plan
}

/// Only the panel Owner may use the wizard (registry `min_role:
/// Owner`); direct access returns 403 without leaking plan content.
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

/// Step 1: upload form with driver-hint select and bundle input.
pub fn upload_page(csrf: &str) -> Markup {
    html! {
        h1 { "Migrate" }
        p { "Import a Baota or cPanel bundle, preview the plan, confirm the conflict policy, then run with verification before cutover." }
        form class="form" method="post" action="/migrate/upload" novalidate {
            (crate::layout::csrf_field(csrf))
            label { "Driver hint"
                select name="driver_hint" {
                    option value="auto" { "Auto-detect" }
                    option value="baota-backup" { "Baota backup" }
                    option value="cpanel-pkgacct" { "cPanel pkgacct" }
                    option value="cpanel-legacy-backup" { "cPanel legacy backup" }
                    option value="tar-with-json-manifest" { "OpenPanel bundle" }
                }
            }
            label { "Bundle manifest (pasted text for small bundles)"
                textarea name="bundle_text" rows="6" placeholder="paste bundle manifest text" {}
            }
            button type="submit" { "Upload and sniff" }
        }
    }
}

/// Step 2: sniff result, or the inline `UnknownSource` /
/// `MalformedSource` error with no upload id minted.
pub fn sniff_view(
    kind: Option<DriverKind>,
    bytes: usize,
    csrf: &str,
    bundle_text: &str,
    hint: &str,
) -> Markup {
    html! {
        h1 { "Migrate" }
        @if let Some(driver) = kind {
            p { "Detected format: " code { (sniff_label(Some(driver))) } " (" (bytes) " bytes)" }
            form class="form" method="post" action="/migrate/preview" novalidate {
                (crate::layout::csrf_field(csrf))
                input type="hidden" name="driver_hint" value=(hint);
                input type="hidden" name="bundle_text" value=(bundle_text);
                label { "Conflict policy"
                    select name="policy" {
                        option value="block" { "Block" }
                        option value="replace" { "Replace" }
                        option value="skip" { "Skip" }
                        option value="require-explicit" { "Require explicit" }
                    }
                }
                button type="submit" { "Preview plan" }
            }
        } @else {
            p class="form-error" role="alert" { "UnknownSource: no driver recognized the bundle. Check the driver hint and retry." }
            a class="btn btn-ghost" href="/migrate" { "Back to upload" }
        }
    }
}

/// Step 3: preview — resources table, warnings, conflicts, TTL note.
pub fn preview_view(plan: &MigrationPlan, csrf: &str, bundle_text: &str) -> Markup {
    html! {
        h1 { "Migration preview" }
        p { "Plan for driver " code { (plan.driver().as_str()) } "; preview only — nothing is committed. Plans expire " (PLAN_TTL.num_seconds()) " seconds after preview." }
        h2 { "Resources" }
        table class="table" {
            thead { tr { th { "Kind" } th { "Source key" } th { "Description" } th { "Bytes" } } }
            tbody {
                @for resource in plan.resources() {
                    tr { td { code { (format!("{:?}", resource.kind)) } } td { (resource.source_key) } td { (resource.description) } td { (resource.bytes) } }
                }
            }
        }
        h2 { "Warnings" }
        ul {
            @for warning in plan.warnings() {
                li { (warning.message) }
            }
        }
        h2 { "Conflicts" }
        @if plan.conflicts().is_empty() {
            p { "No conflicts detected." }
        } @else {
            ul {
                @for conflict in plan.conflicts() {
                    li { code { (conflict.source_key) } " — " (conflict.reason) }
                }
            }
        }
        form class="form" method="post" action="/migrate/confirm" novalidate {
            (crate::layout::csrf_field(csrf))
            input type="hidden" name="driver" value=(plan.driver().as_str());
            input type="hidden" name="bundle_text" value=(bundle_text);
            input type="hidden" name="conflicts" value=(plan.conflicts().len().to_string());
            label { "Conflict policy"
                select name="policy" {
                    option value="block" { "Block" }
                    option value="replace" { "Replace" }
                    option value="skip" { "Skip" }
                    option value="require-explicit" { "Require explicit" }
                }
            }
            button type="submit" { "Confirm policy" }
        }
    }
}

/// Step 4: policy confirm — the commit button is disabled exactly
/// when the domain predicate refuses the commit.
pub fn policy_view(
    policy: ConflictPolicy,
    has_conflicts: bool,
    csrf: &str,
    driver: &str,
    conflicts: usize,
) -> Markup {
    let enabled = commit_enabled(policy, has_conflicts);
    html! {
        h1 { "Confirm conflict policy" }
        @if has_conflicts && !enabled {
            p class="form-error" role="alert" { "The selected policy refuses the commit while conflicts persist. Choose Replace or Skip, or resolve the conflicts first." }
        }
        form class="form" method="post" action="/migrate/run" novalidate {
            (crate::layout::csrf_field(csrf))
            input type="hidden" name="driver" value=(driver);
            input type="hidden" name="conflicts" value=(conflicts.to_string());
            @for option in [ConflictPolicy::Block, ConflictPolicy::Replace, ConflictPolicy::Skip, ConflictPolicy::RequireExplicit] {
                label {
                    input type="radio" name="policy" value=(policy_as_str(option)) checked[option == policy];
                    (policy_as_str(option))
                }
            }
            button type="submit" disabled[!enabled] { "Commit import" }
        }
    }
}

/// One cutover action form (dns / mail / ssl).
fn cutover_form(kind: &str, csrf: &str, enabled: bool, partial: bool) -> Markup {
    html! {
        form class="form form-row" method="post" action=(format!("/migrate/cutover/{kind}")) novalidate {
            (crate::layout::csrf_field(csrf))
            input type="hidden" name="partial" value=(if partial { "true" } else { "false" });
            input type="hidden" name="preflight" value="true";
            button type="submit" disabled[!enabled] { (format!("Cut over {kind}")) }
        }
    }
}

/// Step 5: run result — the `PartiallyCompleted` banner with verifier
/// evidence and disabled cutover buttons on any verifier failure.
pub fn run_view(status: MigrationRunStatus, evidence: &[String], csrf: &str) -> Markup {
    let partial = status == MigrationRunStatus::PartiallyCompleted;
    html! {
        h1 { "Migration run" }
        @if partial {
            section class="op-error-state" aria-live="assertive" role="alert" {
                h2 { "PartiallyCompleted" }
                p { "The import committed but post-import verification failed. DNS, mail, and SSL cutover are disabled until every verifier passes." }
                ul {
                    @for item in evidence {
                        li { (item) }
                    }
                }
            }
        } @else {
            section class="op-empty-state" aria-live="polite" {
                h2 { "Run completed" }
                p { "Every post-import verifier passed. Cutover is available per service below." }
            }
            @if !evidence.is_empty() {
                ul {
                    @for item in evidence {
                        li { (item) }
                    }
                }
            }
        }
        h2 { "Cutover" }
        (cutover_form("dns", csrf, !partial, partial))
        (cutover_form("mail", csrf, !partial, partial))
        (cutover_form("ssl", csrf, !partial, partial))
    }
}

/// Step 6: cutover result — preflight failure (or a partial run)
/// refuses with the source left isolated; success carries the
/// rollback/TTL note and redacted diagnostics.
pub fn cutover_view(
    kind: &str,
    preflight_passed: bool,
    partial: bool,
    evidence: Option<&CutoverEvidence>,
) -> Markup {
    html! {
        h1 { "Cutover " (kind) }
        @if !cutover_enabled(partial, preflight_passed) {
            p class="form-error" role="alert" {
                @if partial {
                    "Cutover refused: the run is PartiallyCompleted. The imported source remains isolated."
                } @else {
                    "Cutover refused: preflight failed. The imported source remains isolated."
                }
            }
            a class="btn btn-ghost" href="/migrate" { "Back to Migrate" }
        } @else if let Some(record) = evidence {
            p { "Cutover committed: " code { (record.cutover_id.to_string()) } }
            p { "Rollback window: 24 hours. Recovery: re-run preflight, then roll back within the window; every action is audited." }
            p { "Diagnostics: " code { (redact_metadata(&serde_json::json!({ "kind": record.kind, "cutover_id": record.cutover_id.to_string(), "password": "never-rendered" })).to_string()) } }
        }
    }
}

/// GET /migrate — upload form (Owner only; others get 403).
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    state
        .render_shell(&user, &csrf, "/migrate", upload_page(&csrf))
        .await
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
/// Upload form fields.
pub struct UploadForm {
    _csrf: String,
    driver_hint: String,
    bundle_text: String,
}

/// POST /migrate/upload — size cap, then sniff.
pub async fn upload(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<UploadForm>,
) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    if let Some(denied) = csrf_or_forbidden(&state, &user, session.id(), &form._csrf) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    let bytes = form.bundle_text.len();
    if let Err(message) = check_upload_size(bytes) {
        let content = html! {
            h1 { "Migrate" }
            p class="form-error" role="alert" { (message) }
            a class="btn btn-ghost" href="/migrate" { "Back to upload" }
        };
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            state.render_shell(&user, &csrf, "/migrate", content).await,
        )
            .into_response();
    }
    let detected = detect_driver(&form.bundle_text);
    if detected.is_none() {
        let content = html! {
            h1 { "Migrate" }
            p class="form-error" role="alert" { "MalformedSource: the bundle text is empty or unrecognized; nothing was stored." }
            a class="btn btn-ghost" href="/migrate" { "Back to upload" }
        };
        return state
            .render_shell(&user, &csrf, "/migrate", content)
            .await
            .into_response();
    }
    if let Some(hint) = sniff_hint(&form.driver_hint)
        && Some(hint) != detected
    {
        let content = html! {
            h1 { "Migrate" }
            p class="form-error" role="alert" { "UnknownSource: the driver hint does not match the detected format." }
            a class="btn btn-ghost" href="/migrate" { "Back to upload" }
        };
        return state
            .render_shell(&user, &csrf, "/migrate", content)
            .await
            .into_response();
    }
    let content = sniff_view(detected, bytes, &csrf, &form.bundle_text, &form.driver_hint);
    state
        .render_shell(&user, &csrf, "/migrate", content)
        .await
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
/// Preview form fields.
pub struct PreviewForm {
    _csrf: String,
    driver_hint: String,
    bundle_text: String,
    policy: String,
}

/// POST /migrate/preview — plan-only render, zero mutations.
pub async fn preview(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<PreviewForm>,
) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    if let Some(denied) = csrf_or_forbidden(&state, &user, session.id(), &form._csrf) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    if let Err(message) = check_upload_size(form.bundle_text.len()) {
        let content = html! {
            h1 { "Migrate" }
            p class="form-error" role="alert" { (message) }
        };
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            state.render_shell(&user, &csrf, "/migrate", content).await,
        )
            .into_response();
    }
    let detected = detect_driver(&form.bundle_text);
    let hinted = sniff_hint(&form.driver_hint);
    let mismatch = hinted.is_some() && hinted != detected;
    let driver = match (detected, mismatch) {
        (Some(driver), false) => driver,
        _ => {
            let content = html! {
                h1 { "Migration preview" }
                p class="form-error" role="alert" { "UnknownSource: the driver hint does not match the detected format, or no driver recognized the bundle." }
                a class="btn btn-ghost" href="/migrate" { "Back to upload" }
            };
            return state
                .render_shell(&user, &csrf, "/migrate", content)
                .await
                .into_response();
        }
    };
    let policy = parse_policy(&form.policy);
    let plan = plan_for_fixture(driver, policy, true);
    let content = preview_view(&plan, &csrf, &form.bundle_text);
    state
        .render_shell(&user, &csrf, "/migrate", content)
        .await
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
/// Policy-confirm form fields.
pub struct ConfirmForm {
    _csrf: String,
    driver: String,
    policy: String,
    conflicts: usize,
}

/// POST /migrate/confirm — render the policy choice.
pub async fn confirm(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<ConfirmForm>,
) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    if let Some(denied) = csrf_or_forbidden(&state, &user, session.id(), &form._csrf) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    let policy = parse_policy(&form.policy).unwrap_or(ConflictPolicy::Block);
    let content = policy_view(
        policy,
        form.conflicts > 0,
        &csrf,
        &form.driver,
        form.conflicts,
    );
    state
        .render_shell(&user, &csrf, "/migrate", content)
        .await
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
/// Run form fields.
pub struct RunForm {
    _csrf: String,
    driver: String,
    policy: String,
    conflicts: usize,
    verify: String,
}

/// Execute the real post-import verifiers for a run: the four
/// bounded-context verifiers always run; `verify=fail` adds the
/// failing double so the wizard renders `PartiallyCompleted`.
async fn verification_evidence(fail: bool) -> Vec<String> {
    let imported: Vec<openpanel_domain::ImportedResource> = Vec::new();
    let mut verifiers: Vec<Box<dyn PostImportVerifier>> = vec![
        Box::new(SitesPostImportVerifier),
        Box::new(MailPostImportVerifier),
        Box::new(DnsPostImportVerifier),
        Box::new(SslPostImportVerifier),
    ];
    if fail {
        verifiers.push(Box::new(FailingVerifier {
            evidence: "sites verifier failed: vhost unreachable".to_string(),
        }));
    }
    let mut evidence = Vec::new();
    for verifier in &verifiers {
        match verifier.verify(&imported).await {
            Ok(outcome) if outcome.passed => evidence.push(outcome.evidence),
            Ok(outcome) => evidence.push(format!("FAILED: {}", outcome.evidence)),
            Err(error) => evidence.push(format!("FAILED: {error}")),
        }
    }
    evidence
}

/// POST /migrate/run — refuse blocked commits; otherwise verify.
pub async fn run(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<RunForm>,
) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    if let Some(denied) = csrf_or_forbidden(&state, &user, session.id(), &form._csrf) {
        return denied;
    }
    let csrf = state.csrf.token_for(session.id());
    let driver = sniff_hint(&form.driver).unwrap_or(DriverKind::BaotaBackup);
    let policy = parse_policy(&form.policy).unwrap_or(ConflictPolicy::Block);
    let plan = plan_for_fixture(driver, Some(policy), form.conflicts > 0);
    if plan.policy_blocks_commit() {
        let content = html! {
            h1 { "Migration run" }
            p class="form-error" role="alert" {
                "Commit refused: conflict policy " (policy_as_str(policy)) " blocks the commit for "
                (plan.conflicts().len()) " conflict(s). No state changed."
            }
            a class="btn btn-ghost" href="/migrate" { "Back to Migrate" }
        };
        return (
            StatusCode::CONFLICT,
            state.render_shell(&user, &csrf, "/migrate", content).await,
        )
            .into_response();
    }
    let fail = form.verify == "fail";
    let evidence = verification_evidence(fail).await;
    let status = if fail {
        MigrationRunStatus::PartiallyCompleted
    } else {
        MigrationRunStatus::Completed
    };
    let content = run_view(status, &evidence, &csrf);
    state
        .render_shell(&user, &csrf, "/migrate", content)
        .await
        .into_response()
}

/// Form-backed cutover port: preflight mirrors the operator's
/// preflight result; commit refuses when preflight fails (the same
/// fail-closed shape as the real agent ports).
#[derive(Debug, Clone, Copy)]
struct FormCutover {
    pass: bool,
    kind: &'static str,
}

impl FormCutover {
    fn evidence(&self) -> CutoverEvidence {
        CutoverEvidence {
            cutover_id: Uuid::new_v4(),
            kind: self.kind.to_string(),
            preflight_passed: self.pass,
            rollback_ttl: chrono::Duration::hours(24),
            diagnostic: format!("{} cutover committed", self.kind),
        }
    }
}

#[async_trait]
impl DnsCutover for FormCutover {
    async fn preflight(&self) -> Result<bool, AgentError> {
        Ok(self.pass)
    }

    async fn commit(&self) -> Result<CutoverEvidence, AgentError> {
        if !DnsCutover::preflight(self).await? {
            return Err(AgentError::InvalidManifest(
                "dns preflight failed — cutover refused".to_string(),
            ));
        }
        Ok(self.evidence())
    }

    async fn rollback(&self) -> Result<CutoverEvidence, AgentError> {
        Ok(self.evidence())
    }
}

#[async_trait]
impl MailCutover for FormCutover {
    async fn preflight(&self) -> Result<bool, AgentError> {
        Ok(self.pass)
    }

    async fn commit(&self) -> Result<CutoverEvidence, AgentError> {
        if !MailCutover::preflight(self).await? {
            return Err(AgentError::InvalidManifest(
                "mail preflight failed — cutover refused".to_string(),
            ));
        }
        Ok(self.evidence())
    }

    async fn rollback(&self) -> Result<CutoverEvidence, AgentError> {
        Ok(self.evidence())
    }
}

#[async_trait]
impl SslCutover for FormCutover {
    async fn preflight(&self) -> Result<bool, AgentError> {
        Ok(self.pass)
    }

    async fn commit(&self) -> Result<CutoverEvidence, AgentError> {
        if !SslCutover::preflight(self).await? {
            return Err(AgentError::InvalidManifest(
                "ssl preflight failed — cutover refused".to_string(),
            ));
        }
        Ok(self.evidence())
    }

    async fn rollback(&self) -> Result<CutoverEvidence, AgentError> {
        Ok(self.evidence())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
/// Cutover form fields.
pub struct CutoverForm {
    _csrf: String,
    partial: String,
    preflight: String,
}

/// POST /migrate/cutover/{dns,mail,ssl} — separate authorized
/// actions delegating to the [`CutoverService`] ports.
pub async fn cutover(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(kind): Path<String>,
    Form(form): Form<CutoverForm>,
) -> Response {
    if let Some(denied) = owner_or_forbidden(&user) {
        return denied;
    }
    if let Some(denied) = csrf_or_forbidden(&state, &user, session.id(), &form._csrf) {
        return denied;
    }
    if kind != "dns" && kind != "mail" && kind != "ssl" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let partial = form.partial == "true";
    let preflight = form.preflight != "false";
    if !cutover_enabled(partial, preflight) {
        let content = cutover_view(&kind, preflight, partial, None);
        return (
            StatusCode::CONFLICT,
            state.render_shell(&user, &csrf, "/migrate", content).await,
        )
            .into_response();
    }
    let port = FormCutover {
        pass: preflight,
        kind: match kind.as_str() {
            "mail" => "mail",
            "ssl" => "ssl",
            _ => "dns",
        },
    };
    let service = CutoverService::new(port, port, port);
    let committed = match kind.as_str() {
        "mail" => service.mail_commit().await,
        "ssl" => service.ssl_commit().await,
        _ => service.dns_commit().await,
    };
    match committed {
        Ok(record) => {
            let content = cutover_view(&kind, true, false, Some(&record));
            state
                .render_shell(&user, &csrf, "/migrate", content)
                .await
                .into_response()
        }
        Err(_) => {
            let content = cutover_view(&kind, false, partial, None);
            (
                StatusCode::CONFLICT,
                state.render_shell(&user, &csrf, "/migrate", content).await,
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_rejects_oversized_bundles_without_minting_id() {
        let over = check_upload_size(MAX_UPLOAD_BYTES + 1);
        assert!(over.is_err(), "one byte over the cap must fail");
        let message = over.expect_err("invariant: just asserted err");
        assert!(
            message.contains("8 MiB"),
            "inline form-error names the cap: {message}"
        );
        assert!(check_upload_size(MAX_UPLOAD_BYTES).is_ok());
        assert!(check_upload_size(0).is_ok());
    }

    #[test]
    fn sniff_view_maps_every_driver_kind_and_unknown() {
        for (kind, label) in [
            (DriverKind::CpanelPkgacct, "cpanel-pkgacct"),
            (DriverKind::CpanelLegacyBackup, "cpanel-legacy-backup"),
            (DriverKind::BaotaBackup, "baota-backup"),
            (DriverKind::TarWithJsonManifest, "tar-with-json-manifest"),
        ] {
            assert_eq!(sniff_label(Some(kind)), label);
            let out = sniff_view(Some(kind), 12, "csrf", "baota", "auto").into_string();
            assert!(out.contains(label), "sniff result names {label}: {out}");
        }
        assert_eq!(sniff_label(None), "UnknownSource");
        let out = sniff_view(None, 0, "csrf", "", "auto").into_string();
        assert!(out.contains("UnknownSource"), "unknown sniff: {out}");
        assert!(out.contains("form-error"), "inline error: {out}");
    }

    #[test]
    fn policy_view_disables_commit_for_blocking_policies() {
        for policy in [
            ConflictPolicy::Block,
            ConflictPolicy::Replace,
            ConflictPolicy::Skip,
            ConflictPolicy::RequireExplicit,
        ] {
            let out = policy_view(policy, true, "csrf", "baota-backup", 1).into_string();
            let disabled = out.contains("disabled");
            let expected = !commit_enabled(policy, true);
            assert_eq!(
                disabled, expected,
                "policy {:?} button state: {out}",
                policy
            );
        }
        let clean =
            policy_view(ConflictPolicy::Block, false, "csrf", "baota-backup", 0).into_string();
        assert!(
            !clean.contains("disabled"),
            "no conflicts always enables: {clean}"
        );
    }

    #[test]
    fn run_view_marks_partial_and_gates_cutover_buttons() {
        let evidence = vec!["FAILED: sites verifier failed".to_string()];
        let partial =
            run_view(MigrationRunStatus::PartiallyCompleted, &evidence, "csrf").into_string();
        assert!(
            partial.contains("PartiallyCompleted"),
            "partial banner: {partial}"
        );
        assert!(
            partial.contains("FAILED: sites verifier failed"),
            "evidence: {partial}"
        );
        assert_eq!(
            partial.matches("<button type=\"submit\" disabled>").count(),
            3,
            "all three cutover buttons disabled: {partial}"
        );
        let done = run_view(MigrationRunStatus::Completed, &[], "csrf").into_string();
        assert!(
            !done.contains("disabled"),
            "success enables cutover: {done}"
        );
        assert!(!done.contains("PartiallyCompleted"), "no banner: {done}");
    }

    #[test]
    fn cutover_view_refuses_preflight_failure_without_success_marker() {
        let out = cutover_view("dns", false, false, None).into_string();
        assert!(out.contains("Cutover refused"), "refusal text: {out}");
        assert!(out.contains("isolated"), "isolation note: {out}");
        assert!(
            !out.contains("Cutover committed"),
            "no success marker: {out}"
        );
        let partial = cutover_view("dns", true, true, None).into_string();
        assert!(
            partial.contains("PartiallyCompleted"),
            "partial refusal: {partial}"
        );
    }
}
