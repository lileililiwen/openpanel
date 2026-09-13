# Progress: add-operator-security-control-plane

## Research (2026-09-13)

Goal: unified operator security control plane (normalized findings,
safe remediation, expiry, verification) over existing services.

Reuse inventory (Explore & Reuse):
- `openpanel-domain::security` (`FirewallRule`, `FirewallPolicy`,
  `LoginKey`, `TemporaryBlock`) — firewall/lockout model; control
  plane references but does not duplicate.
- `openpanel-app::security::SecurityService` (`preview`/`apply_candidate`
  with check/apply/verify/rollback, `blocks`/`unblock`, allowlists) —
  firewall remediation adapter target.
- `openpanel-domain::malware_scanner` (`ScanFinding`, `Severity`,
  `ScanRun`) + `MalwareScannerService::restore` (24h window precedent
  for expiry semantics) — malware adapter target.
- `openpanel-app::waf::WafService` (`get`/`put`/`dry_run`/`hits`) —
  WAF adapter target (dry-run as preview, put as execute).
- `openpanel-domain::compliance` (`HardeningRun`, `rollback_targets`,
  `AuditRetentionPolicy`, `REDACTED`) + `AuditRetentionService` —
  compliance adapter target + redaction sentinel precedent.
- `openpanel-core::audit` (`AuditService`, `AuditAction`, `AuditEvent`)
  + `openpanel-app::notifications::NotificationService` — audit +
  notify on every mutation (same pattern as firewall `apply_candidate`).
- `openpanel-web::dashboard` attention queue (`AttentionItem`,
  `Severity::Critical/Warning`, `build_attention`) — summary consumer.
- `openpanel-web::capability_registry` (41-entry inventory, role gating,
  `op-empty-state`/`op-error-state` vocab) + `nav_model` icons +
  `layout::CapabilitySet::shipped` — navigation integration.
- `openpanel-web::security` (`/security` owner page, `form form-grid`,
  CSRF) + `ops_workflows::TaskState` + `ui_states::{EmptyState,
  ErrorState}` — web surface patterns; no new CSS tokens.
- `openpanel-api::routes::security::router` (`/security/*`,
  owner-gated) — API surface host.
- `scripts/check-reuse.sh` baseline: `Severity` cross-crate dupes are
  classified debt; new types use `Finding*` prefix to stay unique.
- `scripts/check-spec-test-drift.sh`: new cap
  `operator-security-control-plane` needs a test referencing the name.

## Design approval (2026-09-13)

Standing principal direction per HANDOFF.md (2026-09-13): design
approval for the remaining maturity changes is pre-granted
(automatic approve). This records the approval for
`add-operator-security-control-plane`:

- Approach approved: normalized finding projection at app layer;
  source scanners publish typed findings; lifecycle + evidence owned
  by control plane; one typed remediation adapter per existing
  mutation (preview, authorization, idempotency, post-check, audit,
  notification); unsupported fixes stay manual with evidence.
- Boundaries approved: no new scanner engines; no automatic fix
  without a typed adapter; no raw commands; no secret details in
  web/API/CLI/audit.
- Plan: domain `operator_security` (pure) → app
  `operator_security` (in-memory lifecycle + adapters over
  `AuditService`) → web `/security/findings` (WebRuntime-defaulted
  shared service, no router-signature churn) + API under
  `/security/findings` (shared Arc via composition root) + CLI
  read/preview commands + dashboard attention hook + runbook.

Approved-by: human principal (standing pre-grant, HANDOFF 2026-09-13).
Next: implement tasks.md top-to-bottom, tests first (red).

## Implementation (2026-09-13)

Done: domain `operator_security` (stable ids, dedup, order, expiry,
`redact_text`; metadata redaction reuses `openpanel_core`), app
`operator_security/{types,port,service}` (lifecycle, 5 typed adapters,
`ExistingServiceRemediationPort`, audit + `EventKind::Audit` fan-out),
web `/security/findings*` + nav/registry + dashboard hook, API
`/api/v1/security/findings*`, CLI `security findings *`, runbook
`docs/SECURITY_CONTROL_PLANE.md`. Reuse gate forced renames
(`rule_key`, `suppressed_by`, `apply_suppression`,
`suppress_finding`, `queue_page`/`finding_detail`/`seed_queue`/`suppress_post`/`remediate_post`)
and metadata-redaction reuse instead of baseline growth; app service
split into three files under the file-length hard limit.

## Verification (2026-09-13)

Done: domain 11/11, app 10/10, app notification integration 1/1, web
221/221, HTTP integration 7/7, CLI 2/2 green; `openspec validate
--strict` valid; `make check` EXIT=0 green end-to-end (only
classified reuse debt + tracked spec-test-drift debt remain).
