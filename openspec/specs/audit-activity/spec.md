# audit-activity Specification

## Purpose
TBD - created by archiving change add-audit-activity-center. Update Purpose after archive.
## Requirements
### Requirement: Searchable audit history

The system MUST provide an owner-only audit page and API with actor, action, target, outcome, time-range, and cursor filters.

#### Scenario: Owner filters events

- **WHEN** an owner submits valid filters
- **THEN** results are ordered newest-first and include a deterministic next cursor

### Requirement: Audit data is redacted

The system MUST exclude passwords, tokens, private keys, request bodies, and non-allowlisted metadata from every audit response.

#### Scenario: Secret appears in stored metadata

- **WHEN** an event contains secret-shaped metadata
- **THEN** the HTML and JSON representations omit or redact it

### Requirement: Audit UI states are explicit

The audit page MUST render distinct loading, empty, no-results, error, and success states.

#### Scenario: No event matches filter

- **WHEN** a valid filter returns no events
- **THEN** the page shows a no-results state with a clear-filters action

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

