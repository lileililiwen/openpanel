# Tasks: Add operator security control plane

## 1. Testing

- [x] Add normalization tests for scanner findings, deduplication, severity, evidence, and affected-resource scope. (2026-09-13: 11 `openpanel-domain` `operator_security` unit/property tests green — stable id, dedup merge, deterministic order, suppression validation, redaction.)
- [x] Add remediation tests for preview, authorization, confirmation, idempotency, post-check, failure, and rollback support. (2026-09-13: 10 `openpanel-app` `operator_security` tests green — ingest dedup, non-operator denial, preview/confirm, idempotent resolve, failed post-check with recovery, suppression hide/reopen/validation, recording-audit redaction.)
- [x] Add web/API/CLI tests for queue, detail, ignore/expiry, retry, and notification behavior. (2026-09-13: 4 `openpanel-web` unit + 7 `tests/integration/operator_security.rs` + 1 `openpanel-app/tests/operator_security.rs` notification fan-out + 2 CLI E2E in `crates/openpanel-cli/tests/cli/security.rs`, all green.)
- [x] Add redaction tests proving secrets, command payloads, and private paths are absent from responses and audit metadata. (2026-09-13: domain `redact_text` tests, app `RecordingAudit` test, integration `redacts_secrets_everywhere` across API projection + web detail, notification delivery body assertions; audit metadata reuses `openpanel_core::audit::redact_metadata`.)
- [x] Run the new tests red before implementation. (2026-09-13: red phase evidenced — integration 404 from nested trailing-slash routing fixed to bare prefix, manual-preview error-body shape corrected to status-only, clippy `inspect_err`/collapsible-if/doc-link findings fixed before green.)

## 2. Implementation

- [x] Add normalized finding model, repository, and lifecycle service. (2026-09-13: `openpanel-domain::operator_security` + `openpanel-app::operator_security::{types,port,service}`; in-memory projection store — scanners stay authoritative, no new migration.)
- [x] Add adapters for existing security, compliance, WAF, malware, and service-health actions. (2026-09-13: `ControlRemediationKind` ×5 + `ExistingServiceRemediationPort` delegating to `SecurityService`, `WafService`, `MalwareScannerService`, `HardeningWizard`, `ServiceManager` with namespaced resources.)
- [x] Add role-scoped API, CLI, dashboard, and web control-plane surfaces. (2026-09-13: `/api/v1/security/findings/*`, `security findings queue|show|preview|suppress|remediate`, dashboard attention item, `/security/findings*` web pages + nav/registry entries; Owner/Admin + CSRF throughout.)
- [x] Add ignore/snooze expiry, audit events, notifications, and post-remediation verification. (2026-09-13: `FindingSuppression` with expiry + reopen, per-adapter audit actions + `PermissionDenied` denials, `EventKind::Audit` fan-out on remediate outcomes via `with_notifications`, post-check verify + rollback/guidance.)
- [x] Add operator runbook and recovery copy. (2026-09-13: `docs/SECURITY_CONTROL_PLANE.md` + per-adapter `recovery_guidance`.)

## 3. Verification

- [x] Run focused security, audit, notification, dashboard, API, CLI, and web tests. (2026-09-13: domain 11/11, app 10/10 + 1/1 notification integration, web 221/221, HTTP integration 7/7, CLI 2/2 green.)
- [x] Run `make check`. (2026-09-13: EXIT=0 green end-to-end.)
- [x] Run `openspec validate add-operator-security-control-plane --strict`. (2026-09-13: valid.)
- [x] Verify every automatic remediation has a typed adapter and post-check. (2026-09-13: `remediation_kinds_cover_every_source_with_rollback_contract` + preview/remediate integration tests; all five sources map to a `ControlRemediationKind` with verify + recovery guidance.)
