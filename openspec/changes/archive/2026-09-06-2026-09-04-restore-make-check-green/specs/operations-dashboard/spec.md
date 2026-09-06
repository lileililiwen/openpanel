# Delta for Operations Dashboard

## ADDED Requirements

### Requirement: Dashboard View Is Split From Its Tests

The operations dashboard view SHALL live at
`crates/openpanel-web/src/dashboard/mod.rs`, with its unit tests in the
sibling `crates/openpanel-web/src/dashboard/tests.rs`. Neither file SHALL
exceed the `file-length` `hard_limit` configured in
`cargo-lint-extra.toml`.

#### Scenario: View and tests live in separate files

- **WHEN** a contributor edits how the dashboard composes its widgets
- **THEN** the implementation is in `dashboard/mod.rs` and its covering
  tests are in `dashboard/tests.rs`.

#### Scenario: The view grows past the hard limit

- **WHEN** `dashboard/mod.rs` exceeds the configured `hard_limit`
- **THEN** `make file-length` fails and names the file.
