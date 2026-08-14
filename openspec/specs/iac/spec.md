# iac Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-terraform-provider-and-sdk. Update Purpose after archive.
## Requirements
### Requirement: Generated Client SDK

The system SHALL provide a typed client SDK generated from the OpenAPI
spec (`openapi.rs`) in at least Rust, Go, and TypeScript. The SDK SHALL
expose typed operations for sites, users, DNS, and backups and SHALL
authenticate with existing API tokens. The SDK SHALL be regenerated from
the single OpenAPI source so it cannot diverge from the live API.

#### Scenario: SDK covers core resources

- **WHEN** the SDK is generated from the current OpenAPI spec
- **THEN** typed client methods exist for sites, users, DNS, and
        backups and require a token to construct.

#### Scenario: SDK tracks the API

- **WHEN** the OpenAPI spec changes and the SDK is regenerated
- **THEN** the generated SDK reflects the changed operations and the
        drift check passes.

### Requirement: Terraform Provider

The system SHALL publish a Terraform provider generated from the same
OpenAPI spec exposing resources for sites, users, DNS zones, and
backups. Each resource SHALL map to a real API endpoint and SHALL
request only the scopes it needs.

#### Scenario: Provider resource maps to endpoint

- **WHEN** a `terraform plan` references `openpanel_site`
- **THEN** the provider resolves it to the site endpoint in the contract
        and plans a create/update/delete accordingly.

#### Scenario: Apply creates via API

- **WHEN** an operator applies a configuration with a site and a DNS zone
- **THEN** both resources are created through the live REST API and
        `destroy` removes them.

### Requirement: Sync Guarantee

The system SHALL run CI that regenerates the SDK and provider from
`openapi.rs` and SHALL fail the pipeline when the generated artifacts
drift from the committed ones or from the spec. This guarantees the
generated surfaces stay a faithful contract of the API.

#### Scenario: Drift fails CI

- **WHEN** a generated artifact differs from the committed artifact for
        an unchanged OpenAPI spec
- **THEN** the sync CI job fails and reports the drift.

