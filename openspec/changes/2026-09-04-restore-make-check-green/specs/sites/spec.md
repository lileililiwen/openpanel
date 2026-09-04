# Delta for Sites

## ADDED Requirements

### Requirement: Nginx Rendering Is Split From Its Tests

The nginx renderer for sites SHALL live at
`crates/openpanel-app/src/sites/nginx/mod.rs`, with its unit tests in the
sibling `crates/openpanel-app/src/sites/nginx/tests.rs`. Neither file
SHALL exceed the `file-length` `hard_limit` configured in
`cargo-lint-extra.toml`.

#### Scenario: Renderer and tests live in separate files

- **WHEN** a contributor edits nginx rendering behaviour for a site
- **THEN** the implementation is in `sites/nginx/mod.rs` and its
  covering tests are in `sites/nginx/tests.rs`.

#### Scenario: The renderer grows past the hard limit

- **WHEN** `sites/nginx/mod.rs` exceeds the configured `hard_limit`
- **THEN** `make file-length` fails and names the file.
