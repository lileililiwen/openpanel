# quality Specification

## Purpose

Quality gates that protect the OpenPanel delivery pipeline. The
application-delivery change adds a new gate to the canonical `make
check` chain so the application / environment / release contract is
enforced by the same self-test surface as the other governance
gates.

## Requirements

## ADDED Requirements

### Requirement: Application-Delivery Contract Gate

`make check` SHALL run `scripts/check-application-delivery.sh` as
part of the mandatory gate chain. The gate MUST be read-only and
MUST verify that:

- the live `application-delivery` spec exists at
  `openspec/specs/application-delivery/spec.md`,
- the domain module
  `crates/openpanel-domain/src/application_delivery/` exists and
  exports the `Application` aggregate, the `Environment` type, the
  `Release` type, the `ReleaseState` state machine, the
  `EnvironmentKind` discriminator, the `redact_diagnostic` helper,
  the `is_mutable_digest` helper, the `resolve_environment` helper,
  and the `can_promote` / `can_rollback` decision helpers,
- the application service
  `crates/openpanel-app/src/application_delivery/service.rs` exposes
  an `ApplicationDeliveryService` whose public surface includes
  `register_application`, `build_release`, `promote`, and `rollback`
  and that gates non-operator callers,
- no `use openpanel_app` import appears in any domain module
  (layering is enforced as a pre-existing requirement, but the
  gate re-asserts it for the new bounded context),
- the integration test file
  `tests/integration/application_delivery.rs` exists and references
  every scenario in the live spec.

A missing module, an absent type, a missing test file, or any
other violation MUST fail `make check` and print
`step: application-delivery status: failed` plus the offending
path. The gate MUST degrade to `status: skipped` only when the
`rg` binary is unavailable; all other failures MUST exit non-zero.

#### Scenario: All wiring is present

- **WHEN** the live `application-delivery` spec, the domain
  module, the app service, and the integration test file are
  all in place
- **THEN** `make application-delivery` exits zero and prints
  `step: application-delivery status: ok`.

#### Scenario: Missing integration test fails the gate

- **WHEN** `tests/integration/application_delivery.rs` is removed
- **THEN** `make application-delivery` exits non-zero and reports
  the missing test path.

#### Scenario: Domain depends on app code

- **WHEN** a file under
  `crates/openpanel-domain/src/application_delivery/` gains a
  `use openpanel_app::...` import
- **THEN** the gate exits non-zero and reports the layering
  violation.
