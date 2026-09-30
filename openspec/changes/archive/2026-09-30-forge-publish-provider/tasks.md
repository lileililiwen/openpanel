# Tasks: forge-publish-provider

## 1. Testing

- [x] Add unit tests for Forge request parsing, translation, and redaction.
      — domain `crates/openpanel-domain/src/forge_publish_provider/tests.rs`
      (36 tests: parse/validate protocol, project, revision, operation
      identity, source-kind mapping, redaction, wire round-trips) and app
      `crates/openpanel-app/src/forge_publish_provider/tests.rs` (19 tests:
      capabilities, preflight, publish, verify, rollback, idempotency,
      conflict, health failure, redaction, lifecycle, protocol codec).
      `cargo test -p openpanel-domain --lib forge_publish_provider` 36/36,
      `cargo test -p openpanel-app --lib forge_publish_provider` 19/19
      (2026-09-30).
- [x] Add contract tests for capabilities, preflight, publish, verify, and
      rollback.
      — app contract tests cover all five operations against the real
      `ApplicationDeliveryService` + `DeploymentAdapterService`; integration
      `tests/integration/forge_publish_provider.rs` exercises the full
      capabilities → preflight → publish → verify cycle (2026-09-30).
- [x] Add integration tests for idempotency, health failure, and rollback.
      — `forge_publish_provider_duplicate_operation_returns_existing_evidence`,
      `forge_publish_provider_health_failure_is_reported`,
      `forge_publish_provider_rollback_restores_a_prior_release`; 8/8 green
      via `cargo test -p openpanel --test integration forge_publish_provider`
      (2026-09-30).
- [x] Add process lifecycle tests proving optional startup behavior.
      — `lifecycle_disabled_provider_never_starts`,
      `lifecycle_enabled_provider_starts_and_stops`,
      `lifecycle_failure_keeps_the_main_service_operational`,
      `lifecycle_enable_flag_is_strict` (app) plus
      `forge_publish_provider_independent_lifecycle_disabled_by_default`
      (integration); `ForgeProviderProcessStatus::main_service_operational`
      is always `true` (2026-09-30).

## 2. Implementation

- [x] Add provider facade reusing application-delivery and deployment-adapter
      services.
      — `crates/openpanel-app/src/forge_publish_provider/facade.rs`
      (`ForgePublishProvider`): every mutation delegates to
      `ApplicationDeliveryService` (`register_application` at setup,
      `build_release`, `promote`, `rollback`, `list_releases`,
      `list_applications`); every rollout flows through
      `DeploymentAdapterService::run` with a typed `DeploymentPlan`
      (`Preflight` / `Deploy` / `Verify` / `Rollback`). No second release
      or rollback state machine (2026-09-30).
- [x] Add stdio/JSON-RPC adapter and typed error mapping.
      — `crates/openpanel-app/src/forge_publish_provider/protocol.rs`
      (`JsonRpcCodec`, `decode_request` / `decode_envelope` /
      `encode_response` / `encode_error`, `JsonRpcParseError` with
      `json_rpc_code`) and domain typed errors
      (`ForgeProviderError` → wire `ForgeProviderStatus` in
      `map_error_to_status`); conformance frames under
      `tests/fixtures/forge-publish-provider/*.json` (2026-09-30).
- [x] Register provider composition without changing OpenPanel's main
      service.
      — `forge_publish_provider` is registered in
      `crates/openpanel-app/src/lib.rs` with re-exports; the provider is
      opt-in via `OPENPANEL_FORGE_PUBLISH_PROVIDER`
      (`ForgeProviderProcessHandle`), and no composition-root or main-service
      wiring depends on it (2026-09-30).

## 3. Regression

- [x] Prove existing OpenPanel API, CLI, plugin, and application-delivery tests
      remain unchanged and passing.
      — `scripts/check-tests.sh` (workspace unit + integration suites) green
      on 2026-09-30; new files only added modules/re-exports, no existing
      test or public API was modified (2026-09-30).
- [x] Prove secrets and source contents never appear in provider output.
      — `redact_diagnostic` collapses any diagnostic matching the
      sensitive-marker allowlist (`password`, `secret`, `token`, private-key
      headers, host paths, transport commands) to `[REDACTED]`; responses
      carry only secret-set names (`SecretRef`), never values, and never echo
      source `url` / `source_ref`. Proven by
      `secret_bearing_diagnostics_are_redacted` (app) and
      `forge_publish_provider_redacts_secret_diagnostics` (integration)
      (2026-09-30).

## 4. Verification

- [x] Run targeted Rust tests and workspace quality gates.
      — 2026-09-30: fmt, clippy (workspace, `-D warnings`), docs, audit,
      file-length, scan-literal, class-coverage, browser-ui-quality,
      portable-runtime, deployment-adapters, application-delivery,
      host-migration-interoperability, tasks-testing-first, reuse-strict,
      layering, spec-drift, agent-governance, governance-contract, maturity,
      test-gates (127/127), driftwatch-gate, and `scripts/check-tests.sh` all
      green (2026-09-30).
- [x] Run strict OpenSpec validation.
      — `openspec validate forge-publish-provider --strict` reports
      "Change 'forge-publish-provider' is valid" (2026-09-30).
- [x] Run Forge provider conformance fixtures against the OpenPanel provider.
      — `tests/fixtures/forge-publish-provider/{capabilities,preflight,publish,
      verify,rollback}.json` decode through `decode_request` and the
      read-only fixtures execute against `ForgePublishProvider`
      (`forge_publish_provider_conformance_fixtures_round_trip`)
      (2026-09-30).
