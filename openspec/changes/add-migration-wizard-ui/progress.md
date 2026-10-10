# Progress: add-migration-wizard-ui (2026-10-10)

Goal: `/migrate` web wizard over the existing migration lifecycle. Design
APPROVED by principal (directive "implement", 2026-10-10, Agents.md §8.10).

Reuse (no new import logic, per design):
- `crates/openpanel-app/src/migration_importers/service.rs` —
  `PLAN_TTL` surfaced in preview; `MigrationService::{preview_with_policy,
  run_with_verifiers}` semantics mirrored statelessly (no repo handle in
  `WebState`; SQLite repo construction is out of scope for the adapter).
- `migration_importers/verifiers` — the four real `PostImportVerifier`s
  are executed in `POST /migrate/run`; `FailingVerifier` drives the
  `PartiallyCompleted` path.
- Domain `ConflictPolicy::allows_commit_without_confirmation`,
  `MigrationPlan::policy_blocks_commit`, `MigrationRunStatus`,
  `DriverKind::as_str` — wizard gating predicates, zero new variants.
- `crates/openpanel-app/src/host_ops/cutover.rs` — `CutoverService` with
  one form-backed port per kind (`dns_commit`/`mail_commit`/`ssl_commit`;
  preflight failure refuses, TTL noted on success).
- `openpanel_core::audit::redact_metadata` — cutover diagnostics.
- Web: `layout::csrf_field` + `Shell` via `render_shell`, `audit::forbidden`,
  `form`/`table`/`btn`/`form-error`/`op-empty-state`/`op-error-state`
  tokens only, `REGISTRY` + `NAV_SECTIONS` + `CapabilitySet::shipped`.

Status: research done; implementing tests-first per tasks.md.
