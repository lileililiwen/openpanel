# Delta for Audit Activity

## ADDED Requirements

### Requirement: Audit Core Is Decomposed Behind A Stable Facade

The `openpanel_core::audit` module SHALL be decomposed into focused
submodules — `action` for the action taxonomy, `cursor` for query and
pagination types, `redaction` for secret scrubbing, and `sqlite` for the
repository — with the shared types and the `AuditService` trait retained
in `audit/mod.rs` and unit tests moved to `audit/tests.rs`.

`audit/mod.rs` SHALL re-export every moved item so that
`AuditAction`, `AuditQuery`, `AuditCursor`, `redact_metadata`, and
`SqliteAuditService` remain reachable at their existing
`openpanel_core::audit::*` paths. No file under
`crates/openpanel-core/src/audit/` SHALL exceed the `file-length`
`hard_limit` configured in `cargo-lint-extra.toml`.

#### Scenario: Callers import from the facade

- **WHEN** an outer crate imports `openpanel_core::audit::AuditAction`
- **THEN** the import resolves through the `mod.rs` re-export and the
  caller requires no change.

#### Scenario: A submodule grows past the hard limit

- **WHEN** any file under `crates/openpanel-core/src/audit/` exceeds the
  configured `hard_limit`
- **THEN** `make file-length` fails and names the offending file.

#### Scenario: A test double omits a trait method

- **WHEN** an implementation of `AuditService` does not implement every
  method the trait declares
- **THEN** `cargo check --workspace --all-targets` fails to compile
  rather than leaving an untested stub.
