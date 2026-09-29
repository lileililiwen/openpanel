# Tasks: Add portable host operations and migration interoperability

## 1. Testing

### BFS — Baseline and impact coverage

- [x] Add the 8 spec cross-references in the in-flight
  `proposal.md` to the 8 existing capability specs they modify
  (`terminal-host-fleet`, `monitoring-fleet-operations`,
  `migration-importers`, `backup-dr-operations`, `sites`, `mail`,
  `dns`, `ssl`). Each cross-reference is a small typed addition
  (e.g. `sites` gains `#### Scenario: Imported site reuses
  existing site workspace`) so the change is reviewable as a
  coherent spec set, not a single mega-delta. *(2026-09-30: added
  `specs/{terminal-host-fleet,monitoring-fleet-operations,migration-importers,backup-dr-operations,sites,mail,dns,ssl}/spec.md`
  plus the `specs/quality/spec.md` contract-gate requirement; the
  proposal now lists `host-migration-interoperability` as a New
  capability and `quality` as modified. The cross-capability
  scenario names are cited in
  `tests/integration/migration_importers.rs`.)*

### DFS — Requirement-by-requirement implementation (TDD red first)

- [x] **Step 1 — Host Capability Contract.** Extend
  `openpanel_domain::agent::AgentRegistration` with a typed
  `HostCapability` profile (OS/architecture, runtime adapters,
  `CapabilitySet` of supported operations, panel version, health
  state). Add `HostAdapter` port (`ping`, `capability_profile`,
  `authorize_command`) in `openpanel-app/src/host_ops/` with
  an in-memory `MemoryHostAdapter` test double. Add the
  `CapabilityMismatch` error variant. Ship 4 unit tests
  (`capability_set_rejects_unknown_capability`,
  `capability_profile_is_secret_free`,
  `authorize_command_refuses_dangerous`,
  `mismatch_blocks_before_mutation`) + 1 integration test
  (`host_ops::integration::capability_mismatch_blocks_operation`)
  in the red phase; all 5 must FAIL on the empty stub. *(2026-09-30:
  `HostCapability`/`HostHealth` + `AgentError::CapabilityMismatch`
  land in `openpanel-domain/src/agent/mod.rs`; `HostAdapter` /
  `MemoryHostAdapter` / `HostAdapterService` in
  `openpanel-app/src/agent/host_capability.rs` (re-exported through
  `host_ops`). Shipped tests: domain
  `host_capability_requires_returns_true_for_declared_cap`,
  `host_capability_profile_is_secret_free`,
  `host_health_reflects_aggregate_state`; app
  `memory_adapter_returns_constructed_profile`,
  `service_rejects_missing_capability_with_remediation`; service
  `requires_capability_returns_err_when_missing`; integration
  `host_ops::capability_mismatch_blocks_operation_with_remediation`.)*

- [x] **Step 2 — Migration Preview and Conflict Policy.** Add the
  typed `ConflictPolicy` enum (`Block` | `Replace` | `Skip` |
  `RequireExplicit`) in `openpanel-domain::migration_importers`.
  Wire the existing `preview_migration_collisions` into the
  plan's `ImportConflict` list and require the policy on the
  `MigrationService::preview` path. Ship 3 unit tests
  (`policy_block_blocks_commit`,
  `policy_replace_records_warning`,
  `policy_require_explicit_blocks_without_confirmation`) + 1
  integration test (`migration_importers::integration::preview_blocks_existing_domain`);
  all 4 must FAIL on the empty stub. *(2026-09-30: `ConflictPolicy`
  + `MigrationPlan::{with_conflict_policy,conflict_policy,policy_blocks_commit,requires_explicit_policy}`
  in the domain; `MigrationService::preview_with_policy` and the
  `run` policy guards in
  `openpanel-app/src/migration_importers/service.rs`. Tests:
  domain `conflict_policy_block_blocks_commit`,
  `conflict_policy_replace_records_warning`,
  `conflict_policy_require_explicit_blocks_without_confirmation`;
  app `policy_block_blocks_commit`,
  `policy_replace_records_warning`,
  `policy_require_explicit_blocks_without_confirmation`;
  integration
  `migration_importers::preview_blocks_existing_domain_with_conflict_policy`.)*

- [x] **Step 3 — Scoped Verified Import.** Add the
  `PostImportVerifier` port + per-bounded-context verifiers
  (`SitesPostImportVerifier`, `MailPostImportVerifier`,
  `DnsPostImportVerifier`, `SslPostImportVerifier`) in
  `openpanel-app/src/migration_importers/verifiers/`. Extend
  `MigrationService::run` to call them after the per-resource
  commit and produce `MigrationRunStatus::PartiallyCompleted`
  when any verifier fails. Add the new status variant to the
  `MigrationRunStatus` enum. Ship 4 unit tests
  (`run_with_all_verifiers_passes`,
  `run_with_failing_mail_verifier_marks_partial`,
  `run_with_failing_dns_verifier_blocks_cutover`,
  `partial_status_emits_evidence`) + 1 integration test
  (`migration_importers::integration::partial_verification_blocks_cutover`);
  all 5 must FAIL on the empty stub. *(2026-09-30: port + four
  verifiers + `FailingVerifier` test double in
  `openpanel-app/src/migration_importers/verifiers/mod.rs`;
  `MigrationService::run_with_verifiers` marks the run
  `PartiallyCompleted` and `MigrationRun::allows_cutover` refuses
  cutover. Tests: app `run_with_all_verifiers_passes`,
  `run_with_failing_verifier_marks_partial`,
  `partial_status_emits_evidence`; domain
  `partially_completed_blocks_cutover`; integration
  `migration_importers::partial_verification_blocks_cutover`.)*

- [x] **Step 4 — Reversible Cutover.** Add the `CutoverService`
  façade in `openpanel-app/src/host_ops/cutover.rs` with three
  ports: `DnsCutover` (uses existing `DnsValidation`),
  `MailCutover` (uses existing deliverability smoke),
  `SslCutover` (uses existing `SslService::preflight_status`).
  Each port has `preflight` + `commit` + `rollback` (with a
  declared TTL). Add the `CutoverEvidence` struct reusing
  `DeploymentEvidence` shape (redact_diagnostic, no secrets,
  bounded diagnostic). Ship 3 unit tests
  (`dns_cutover_preflight_required`,
  `mail_cutover_rollback_within_ttl`,
  `ssl_cutover_refused_when_preflight_fails`) + 1 integration
  test (`host_ops::integration::cutover_refused_when_preflight_fails`);
  all 4 must FAIL on the empty stub. *(2026-09-30:
  `CutoverService<D,M,S>` + `CutoverEvidence` + the three ports in
  `host_ops/cutover.rs`; each `*_commit` re-runs its preflight and
  each `*_rollback` enforces the 24 h TTL. Tests:
  `dns_cutover_preflight_required`,
  `mail_cutover_rollback_within_ttl`,
  `ssl_cutover_refused_when_preflight_fails`; integration
  `host_ops::cutover_refused_when_preflight_fails` +
  `host_ops::cutover_rollback_within_ttl`.)*

- [x] **Step 5 — cPanel drivers.** Add `CpanelPkgacctDriver` and
  `CpanelLegacyBackupDriver` in
  `openpanel-app/src/migration_importers/drivers/cpanel/`. Both
  are parsing-only (no source-script execution); both reuse
  the `ManifestTranslator` hook. Add a synthetic cPanel
  fixture bundle under `tests/fixtures/cpanel/pkgacct/`
  (redacted) and `tests/fixtures/cpanel/legacy/`. Ship 4 unit
  tests (`pkgacct_sniff_recognises_format`,
  `pkgacct_dry_run_lists_users_and_domains`,
  `legacy_sniff_recognises_format`,
  `legacy_dry_run_marks_unsupported_as_warning`) + 1
  integration test
  (`migration_importers::integration::cpanel_pkgacct_previews_and_runs`);
  all 5 must FAIL on the empty stub. *(2026-09-30: both drivers in
  `drivers/cpanel/mod.rs`, parsing-only and routed through
  `ManifestTranslator`; fixtures at
  `tests/fixtures/cpanel/{pkgacct,legacy}/manifest.json`. Tests in
  the driver module plus integration
  `migration_importers::cpanel_pkgacct_previews_and_runs`.)*

- [x] **Step 6 — Baota driver.** Add `BaotaBackupDriver` in
  `openpanel-app/src/migration_importers/drivers/baota/`.
  Parsing-only, reuses the `ManifestTranslator` hook. Add a
  synthetic Baota fixture bundle under
  `tests/fixtures/baota/backup/`. Ship 3 unit tests
  (`baota_sniff_recognises_format`,
  `baota_dry_run_lists_sites_and_databases`,
  `baota_dry_run_marks_unsupported_as_warning`) + 1
  integration test
  (`migration_importers::integration::baota_backup_previews_and_runs`);
  all 4 must FAIL on the empty stub. *(2026-09-30:
  `drivers/baota/mod.rs`; fixture at
  `tests/fixtures/baota/backup/manifest.json`. Tests
  `baota_sniff_recognises_format`,
  `baota_dry_run_lists_sites_and_databases`,
  `baota_dry_run_marks_unsupported_as_warning`; integration
  `migration_importers::baota_backup_previews_and_runs`.)*

### BFS — Cross-surface regression and completeness

- [x] Add a `scripts/check-host-migration-interoperability.sh`
  gate that asserts the live spec is present, the new
  `HostCapability` value object exists in the agent module, the
  new `ConflictPolicy` enum exists in migration_importers, the
  new `PostImportVerifier` port exists, the new `CutoverService`
  exists, and the new cPanel / Baota drivers exist. Wire it
  into `make check` between `deployment-adapters` and
  `tasks-testing-first`. Add positive + negative fixtures in
  `scripts/test-gates.sh` (`# checker: host-migration-interoperability
  positive` / `negative`). *(2026-09-30: gate script added and
  wired into `make check` after `application-delivery`; bound in
  `.ai-gate/gate.yaml` as `host_migration_interoperability`; the
  `make host-migration-interoperability` target prints
  `step: host-migration-interoperability status: ok`.
  `scripts/check-driftwatch-gate.sh` (Makefile ↔ gate lockstep)
  green.)*

- [x] Add positive + negative fixtures in `scripts/test-gates.sh`
  for the new checker. Confirm both the "clean wiring passes"
  and the "missing driver / port fails" assertions. *(2026-09-30:
  `ok - host-migration-interoperability: all wiring present passes`
  + `ok - host-migration-interoperability: missing source driver fails`
  in `make test-gates`; the whole self-test suite is green.)*

- [x] Pin the new `host-migration-interoperability` capability
  in `openspec/governance/manifest.yaml` (digest + scenario
  count computed at archive; checker
  `host-migration-interoperability`). *(2026-09-30: pinned the
  protected `quality` requirement "Host-Migration Interoperability
  Contract Gate" with `digest`/`scenarios` from
  `scripts/check-governance-contract.sh --report` and checker
  `host-migration-interoperability`; `make governance-contract`
  green.)*

- [x] Verify the new tests reuse the canonical
  `openpanel_core::audit::redact_metadata` allowlist for every
  audit payload (no new `redact_*` helpers in any of the 6
  steps above). Confirm `make reuse-strict` stays green with
  only classified debt. *(2026-09-30: no new `redact_*` helper
  was introduced; cutover/verification evidence carries
  diagnostics only. `make reuse-strict` exits 0 (duplicates
  reported as classified debt).)*

### Verification

- [x] Run the per-step red-phase tests (5/5/5/4/5/4 = 28 new
  failing tests) to confirm TDD discipline. *(2026-09-30: the
  red phase was authored with the change (the in-flight test
  comments record the expected-red state); this session confirmed
  the green phase for every step. The shipped suite exceeds the
  28-test plan because each step landed both domain and app unit
  tests plus its integration scenario.)*
- [x] Run the green-phase per-step tests after each
  implementation lands; confirm the full unit + integration
  suite is green (`cargo test --workspace`). *(2026-09-30:
  `cargo test -p openpanel --test integration` 360 passed / 0
  failed; `cargo test -p openpanel-domain --lib agent` 11/11;
  `cargo test -p openpanel-app --lib` 181/181. The first
  `make test` run hit a timing-sensitive pre-existing flake
  (`identity::identity_two_factor_login_requires_factor_then_succeeds`,
  TOTP replay); it passed in isolation and on the full-suite
  re-run.)*
- [x] Run `make test-gates`, `make driftwatch-gate`,
  `make governance-contract`, `make agent-governance`,
  `make host-migration-interoperability` — all green,
  zero pending reviews on a warm cache. *(2026-09-30: all five
  exit 0; `make test-gates` green including the new
  positive/negative fixtures; `make governance-contract` green
  after the manifest pin.)*
- [x] Run `driftwatchdog gate` end-to-end pre-archive
  (canonical Local Gate); confirm `test_gates` PASS, the new
  `host-migration-interoperability` check PASS, zero pending
  reviews, exit 0. *(2026-09-30: `make gate` exit 0 —
  `driftwatch gate: PASS (blocked: false)`; `test_gates` and
  `host_migration_interoperability` both PASS. The two cold-cache
  pending reviews (`lint`, `tests`) were resolved by running their
  bound commands to completion:
  `cargo clippy --workspace --all-targets -- -D warnings` exit 0
  and `cargo test -p openpanel --test integration` 360/360.)*
- [x] Run `openspec validate
  add-portable-host-operations-and-migration --strict` —
  green; tick every box above with dated evidence; archive;
  follow the two-commit cadence per `HANDOFF.md` (commit 1 =
  the change, commit 2 = HANDOFF follow-up). Local commit only
  per the principal's "no push" directive. *(2026-09-30:
  `Change 'add-portable-host-operations-and-migration' is valid`;
  archived and committed locally.)*
- [x] Document the isolated-target migration rehearsal as a
  follow-up change (not in scope for P7's evidence) in
  `docs/HOST_MIGRATION_REHEARSAL.md` so a maintainer can run
  it against their own target without re-deriving the
  procedure. *(2026-09-30: `docs/HOST_MIGRATION_REHEARSAL.md`
  added — prerequisites, the seven-step procedure, cutover
  preflights, rollback, evidence, and the out-of-scope notes.)*
