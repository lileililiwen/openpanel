# quality Specification

## Requirements

## ADDED Requirements

### Requirement: Host-Migration Interoperability Contract Gate

`make check` SHALL run `scripts/check-host-migration-interoperability.sh` as
part of the mandatory gate chain. The gate MUST be read-only and MUST verify
that:

- the `host-migration-interoperability` spec exists at
  `openspec/specs/host-migration-interoperability/spec.md` (after archive) or
  as the in-flight change delta,
- the domain agent module `crates/openpanel-domain/src/agent/mod.rs` exports
  the `HostCapability` value object, the `HostHealth` enum, and the
  `CapabilityMismatch` error variant,
- the domain migration module
  `crates/openpanel-domain/src/migration_importers/mod.rs` exports the
  `ConflictPolicy` enum and the `PartiallyCompleted` run status,
- the host-adapter port
  `crates/openpanel-app/src/agent/host_capability.rs` exports the
  `HostAdapter` trait,
- the cutover façade `crates/openpanel-app/src/host_ops/` exports the
  `CutoverService`,
- the post-import port
  `crates/openpanel-app/src/migration_importers/verifiers/mod.rs` exports the
  `PostImportVerifier` trait,
- the cPanel source driver
  `crates/openpanel-app/src/migration_importers/drivers/cpanel/mod.rs` exports
  both `CpanelPkgacctDriver` and `CpanelLegacyBackupDriver`,
- the Baota source driver
  `crates/openpanel-app/src/migration_importers/drivers/baota/mod.rs` exports
  `BaotaBackupDriver`, and
- every scenario in the `host-migration-interoperability` spec is referenced by
  name in `tests/integration/host_ops.rs` or
  `tests/integration/migration_importers.rs`.

A missing module, an absent trait, a missing driver, or an uncovered scenario
MUST fail `make check` and print `step: host-migration-interoperability status:
failed` plus the offending path. The gate MUST degrade to `status: skipped` only
when the `rg` binary is unavailable; all other failures MUST exit non-zero.

#### Scenario: All wiring is present

- **WHEN** the spec, the domain capability and conflict types, the host-adapter
  port, the cutover façade, the verifier port, the cPanel and Baota drivers,
  and the integration test files are all in place
- **THEN** `make host-migration-interoperability` exits zero and prints
  `step: host-migration-interoperability status: ok`.

#### Scenario: Missing source driver fails the gate

- **WHEN** the cPanel or Baota driver module is removed or stops exporting its
  driver type
- **THEN** `make host-migration-interoperability` exits non-zero and names the
  offending driver path.

#### Scenario: Missing scenario coverage fails the gate

- **WHEN** a scenario in the `host-migration-interoperability` spec is no longer
  referenced by name in the host-ops or migration-importers integration tests
- **THEN** `make host-migration-interoperability` exits non-zero and reports the
  uncovered scenario.
