# Progress: forge-publish-provider

Status: complete (2026-09-30) — implemented, gates green, ready to archive

## Goal

OpenPanel already owns application delivery, deployment adapters,
idempotency, audit, and rollback. This change exposes those
capabilities through the optional `forge-publish-provider/0.1.0`
contract so Forge can drive publish/verify/rollback without moving
those responsibilities into Forge and without making the provider
mandatory for OpenPanel startup.

## Reuse map (per `Agents.md` §8)

- **`openpanel-app` `ApplicationDeliveryService`** — the only
  application/environment/release lifecycle. The facade calls
  `build_release`, `promote`, `rollback`, `list_releases`, and
  `list_applications`; it never re-implements the release state
  machine or the rollback policy. Rollback target selection reuses
  the service's `Failed` / `Deploying` eligibility rule.
- **`openpanel-app` `DeploymentAdapterService`** — every rollout
  flows through `run(User, DeploymentPlan)` with `DeploymentAction::
  {Preflight, Deploy, Verify, Rollback}`, preserving the existing
  evidence schema and idempotency handling.
- **`openpanel-domain` `deployment_adapters`** — `DeploymentPlan`,
  `DeploymentEvidence`, `OperationKey`, `SecretRef`, `DeploymentState`
  reused verbatim; the provider passes secret-set *names* only.
- **`openpanel-core` `audit`** — every request fans out through the
  canonical `redact_metadata` allowlist with five new `ForgeProvider*`
  actions in `openpanel-core/src/audit/action.rs`.

## What shipped

- Domain wire contract:
  `crates/openpanel-domain/src/forge_publish_provider/`
  (`mod.rs`, `types.rs`, `logic.rs`, `tests.rs`) — request/response
  shapes, `deny_unknown_fields`, protocol/project/revision/operation
  validation, source-kind mapping, and `redact_diagnostic`.
- App facade + transport + lifecycle:
  `crates/openpanel-app/src/forge_publish_provider/`
  (`mod.rs`, `facade.rs`, `protocol.rs`, `lifecycle.rs`, `tests.rs`),
  registered in `crates/openpanel-app/src/lib.rs`.
- Integration suite `tests/integration/forge_publish_provider.rs`
  registered in `tests/integration/main.rs`, plus conformance frames
  under `tests/fixtures/forge-publish-provider/`.
- Five new audit actions in `crates/openpanel-core/src/audit/action.rs`.

## Optional-startup guarantee

`ForgeProviderProcessHandle` models start/stop from the
`OPENPANEL_FORGE_PUBLISH_PROVIDER` environment variable; a disabled,
stopped, or failed provider never affects the main service
(`main_service_operational()` is always `true`). No composition-root
wiring depends on the provider.

## Verification (2026-09-30)

- `cargo test -p openpanel-domain --lib forge_publish_provider` 36/36.
- `cargo test -p openpanel-app --lib forge_publish_provider` 19/19.
- `cargo test -p openpanel --test integration forge_publish_provider` 8/8.
- `scripts/check-tests.sh`, fmt, clippy (`-D warnings`), docs, audit,
  file-length, scan-literal, class-coverage, browser-ui-quality,
  portable-runtime, deployment-adapters, application-delivery,
  host-migration-interoperability, tasks-testing-first, reuse-strict,
  layering, spec-drift, agent-governance, governance-contract, maturity,
  test-gates (127/127), driftwatch-gate all green.
- `openspec validate forge-publish-provider --strict` valid.

## Notes / deferrals

- `reuse-strict` collision on the new `JsonRpcParseError::code`
  method was resolved by renaming it to `json_rpc_code` rather than
  growing the classified baseline.
- The stdio loop is header-driven: `protocol.rs` owns newline-framed
  decode/encode and error mapping; whoever owns the transport (Forge,
  a systemd unit, or a test) drives it. A supervised long-running
  process wrapper is out of scope for this change.
