# Tasks: Add a verified ecosystem service catalog

## 1. Testing

### BFS — Baseline and impact coverage

- [x] Map existing catalog, trust, installer, container, backup, job, and UI
  contracts. *(Captured in `design.md` §"Explore & Reuse" +
  §"Implementation Plan": software-center `CatalogEntry` /
  `ComponentKind` / `WebApplicationManifest` / `SoftwareVersion` /
  `SupportedPlatform` / `PackageId`, software-center-trust
  fail-closed digest states, `container_runtime` quotas +
  `RegistryCredential`, `backups` health/drill vocabulary,
  `deployment_adapters` P3 port/lifecycle pattern,
  `application_delivery` P4 service+audit pattern,
  `openpanel_core::audit::redact_metadata` allowlist, `AuditService`
  fan-out, and the `operator_security`/`monitoring_fleet`
  in-memory-projection precedent.)*

- [x] Add failing manifest, signature, compatibility, resource, and lifecycle
  fixtures. *(TDD red phase confirmed first: domain `tests.rs`
  written and compiled red (missing modules + proptest link error)
  before `types.rs`/`logic.rs` existed. Final fixture set: 27 domain
  lib tests in `crates/openpanel-domain/src/verified_service_catalog/tests.rs`
  — `manifest_signature_must_be_lowercase_hex`,
  `verify_manifest_signature_rejects_tampered_digest`,
  `check_target_compatibility_refuses_unsupported_target`,
  `plan_service_install_fails_on_disallowed_port_without_mutation`,
  `plan_service_install_fails_on_storage_and_resource_exhaustion`,
  `service_job_rejects_invalid_transitions`,
  `can_retry_respects_bounded_attempts`,
  `ensure_installable_refuses_revoked_even_when_cached`, + 2
  proptests (`prop_signature_is_deterministic_and_bound_to_inputs`,
  `prop_valid_source_digest_round_trips`).)*

### DFS — Requirement-by-requirement implementation

- [x] Add signed manifest schema, publisher/trust state, and compatibility
  validation. *(Domain `verified_service_catalog`:
  `manifest.rs` (`PublisherIdentity`, `SourceDigest` —
  `sha256:<64 hex>` enforced at the type level —
  `ServiceCompatibility` wildcard matrix, `ResourceRequest`,
  `ServicePermissions`, `ServiceHealthProbe`,
  `ServiceBackupPolicy`, `ServiceUpgradePolicy`,
  `ServiceManifest` — version grammar reuses
  `software_center::SoftwareVersion::parse`);
  `types.rs` (`TrustState` verified/community/revoked/blocked
  fail-closed states, `VerifiedServiceCatalogError`);
  `logic.rs` (`manifest_signature` HMAC-SHA256 reusing the
  domain's `hmac`+`sha2` primitives, `verify_manifest_signature`,
  `check_target_compatibility`).)*

- [x] Add install plan preview with resource/secret/data effects.
  *(`plan_service_install` in `logic.rs` + `ServicePlan` /
  `HostPolicy` / `PlanConflict` in `types.rs`: reveals effective
  ports, public-network flag, credential references (ids only,
  never values), data effects, and storage/CPU/memory ceilings;
  returns the first specific conflict (PortNotAllowed,
  StorageQuotaExceeded, CpuQuotaExceeded, MemoryQuotaExceeded) and
  is pure — no mutation path exists. App entry point
  `VerifiedServiceCatalogService::plan_install`.)*

- [x] Add install, upgrade, backup, rollback, health, and removal lifecycle.
  *(`ServiceJob` observable state machine
  (`Pending -> Running -> Succeeded|Failed`, `Failed -> Running`
  retry, `Failed -> RolledBack`) with bounded
  `can_retry`/`service_job_transition_allowed`; diagnostics redacted
  through the canonical `deployment_adapters::redact_diagnostic`;
  `CatalogProvisioner` port delegates install/backup/health/remove
  to the existing software-center / container-runtime services.
  App: `crates/openpanel-app/src/verified_service_catalog/service.rs`
  — `install_entry`, `upgrade_entry` (signature re-verified, backup
  preflight, health-gated, previous release restored + guidance on
  failure), `run_backup`, `verify_health`, `rollback`,
  `remove_entry`; migration
  `crates/openpanel-app/src/migrations/verified_service_catalog/V001__init.sql`
  registered in `migrations/mod.rs`.)*

- [x] Add catalog review, revocation, and contribution evidence surfaces.
  *(`submit_entry` keeps invalid-signature manifests with trust
  `Blocked` for review evidence; `entries()`/`entry()` project the
  trust/installed state without key material; `revoke_publisher` +
  `revoke_digest` (Owner-only, `require_owner`) demote cached
  entries and are consulted at request time via
  `ensure_installable` + `RevocationList::blocks_manifest`; every
  mutation, denial, and refusal is audited through
  `AuditService` with `redact_metadata`. No HTTP/CLI/web routes are
  added by this change (no api/cli/web spec deltas; same scope
  precedent as P4 `add-git-application-delivery` — the service
  facade is the shared choke point for every surface).)*

### BFS — Cross-surface regression and completeness

- [x] Verify catalog installs reuse existing quotas, backup, audit, and job
  state rather than parallel implementations.
  *(Design `Explore & Reuse` honored: audit via
  `openpanel_core::AuditService` + the canonical
  `redact_metadata` allowlist (no new redaction logic); job state
  reuses the P3/P4 lifecycle state-machine + audit-fan-out
  pattern; provisioning delegated behind `CatalogProvisioner`
  (software-center install wizard / container-runtime, wired by the
  composition root) — the context ships no second installer,
  quota, or backup implementation. `make reuse-strict` green with
  zero new baseline entries: four flagged cross-crate names were
  resolved by renames (`blocks_manifest`,
  `revoke_publisher_id`, `revoke_artifact_digest`,
  `install_entry`), matching the change-8/9 rename precedent.)*

- [x] Verify blocked/revoked entries cannot be installed through API, CLI, or
  UI. *(This change adds no API/CLI/UI routes, so the
  service facade is the single choke point:
  `VerifiedServiceCatalogService::plan_install`/`install_entry`
  call `ensure_installable` on every request, refusing `Blocked`
  and `Revoked` trust states and any `RevocationList` match even
  for cached entries. Proven by `ensure_installable_refuses_revoked_even_when_cached`,
  `invalid_signature_blocks_entry_and_refuses_planning`, and the
  integration tests
  `verified_service_catalog_invalid_signature_blocks_entry` +
  `verified_service_catalog_cached_revoked_entry_refused_and_audited`
  (refusal audited as Denied without credentials: asserted on the
  serialized `redact_metadata` payload).)*

- [x] Verify failed upgrades preserve recoverable data and prior release.
  *(`upgrade_entry` re-verifies the new signature before touching
  the install, requires the declared pre-upgrade backup
  (fail-closed), and on failed health verification restores the
  previous verified plan via the provisioner, records the job as
  `RolledBack` with operator guidance, and keeps
  `installed_digest` on the prior release. Proven by app test
  `failed_upgrade_restores_previous_release_with_guidance` and
  integration test
  `verified_service_catalog_upgrade_failure_preserves_previous_release`.)*

### Verification

- [x] Run manifest and lifecycle tests, including hostile fixtures.
  *(Hostile fixtures: tampered digest, non-hex / mismatched
  signatures, zero/duplicate ports, unbounded resource claims,
  invalid target strings, secret-bearing diagnostics (asserted
  redacted). `cargo test -p openpanel-domain --lib
  verified_service_catalog` 27/27;
  `cargo test -p openpanel-app --lib verified_service_catalog`
  12/12; `cargo test --test integration verified_service_catalog`
  6/6 — all green 2026-09-24.)*

- [x] Run browser/API/CLI tests and container smoke tests.
  *(P6 ships no HTTP/CLI/web surface (service-façade precedent), so
  the test-surface runs are the library + integration suites and the
  bound gate checks. `driftwatchdog gate` 2026-09-24: "Gate passed
  locally" — every check PASS, zero blocking failures, zero pending
  reviews. The five cold-cache pending reviews (build, docs,
  test_gates, tests, browser_ui_quality) were resolved by running
  their bound scripts to completion: `cargo build --workspace
  --all-targets` ok; `scripts/check-tests-gate.sh` 225 passed / 0
  failed; `scripts/check-docs.sh` status ok; `scripts/test-gates.sh`
  118/118; `scripts/check-browser-ui-quality.sh` status ok.)*

- [x] Run `openspec validate add-verified-service-catalog --strict`.
  *(`Change 'add-verified-service-catalog' is valid` 2026-09-24.)*
