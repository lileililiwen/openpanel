# quality Specification

## Requirements

## ADDED Requirements

### Requirement: Deployment-Adapter Contract Gate

`make check` SHALL run `scripts/check-deployment-adapters.sh` as part of
the mandatory gate chain. The gate MUST be read-only and MUST verify that:

- the live `deployment-adapters` spec exists at
  `openspec/specs/deployment-adapters/spec.md`,
- the domain module `crates/openpanel-domain/src/deployment_adapters/`
  exists and exports the `DeploymentAdapter` trait, the `AdapterManifest`
  type, the `DeploymentPlan` type, and the `DeploymentEvidence` type,
- the application service
  `crates/openpanel-app/src/deployment_adapters/service.rs` exposes a
  `DeploymentAdapterService` whose `run` method validates the plan,
  enforces idempotency, and rejects non-operator callers,
- no `use openpanel_app` import appears in any domain module
  (layering is enforced as a pre-existing requirement, but the gate
  re-asserts it for the new bounded context),
- the integration test file
  `tests/integration/deployment_adapters.rs` exists and references
  every scenario in the live spec.

A missing module, an absent trait, a missing test file, or any other
violation MUST fail `make check` and print `step: deployment-adapters
status: failed` plus the offending path. The gate MUST degrade to
`status: skipped` only when the `rg` binary is unavailable; all other
failures MUST exit non-zero.

#### Scenario: All wiring is present

- **WHEN** the live `deployment-adapters` spec, the domain module, the
  app service, and the integration test file are all in place
- **THEN** `make deployment-adapters` exits zero and prints
  `step: deployment-adapters status: ok`.

#### Scenario: Missing integration test fails the gate

- **WHEN** `tests/integration/deployment_adapters.rs` is removed
- **THEN** `make deployment-adapters` exits non-zero and reports the
  missing test path.

#### Scenario: Domain depends on app code

- **WHEN** a file under `crates/openpanel-domain/src/deployment_adapters/`
  gains a `use openpanel_app::...` import
- **THEN** `make deployment-adapters` exits non-zero and names the
  offending file; the import must be removed (the layering gate would
  also catch this, but the contract gate restates the invariant for
  the new bounded context).
