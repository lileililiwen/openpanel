# forge-publish-provider Specification

## Purpose
TBD - created by archiving change forge-publish-provider. Update Purpose after archive.
## Requirements
### Requirement: Forge provider facade

OpenPanel SHALL expose its existing application-delivery and deployment
adapters through the `forge-publish-provider/0.1.0` contract without duplicating
their lifecycle logic.

#### Scenario: Publish request

- **WHEN** the provider receives a valid publish request
- **THEN** it invokes the existing typed delivery path and returns redacted
  terminal evidence

#### Scenario: Invalid request

- **WHEN** a request omits its project, revision, operation id, or contract
- **THEN** the provider refuses before any mutation

### Requirement: Independent lifecycle

The provider SHALL be independently startable and stoppable, and its absence
or failure SHALL NOT prevent OpenPanel’s main service from starting.

#### Scenario: Provider disabled

- **WHEN** Forge disables the OpenPanel provider
- **THEN** no provider process is invoked and OpenPanel remains operational

### Requirement: Idempotency and rollback

The provider SHALL preserve OpenPanel’s existing idempotency and rollback
semantics for repeated operation ids and failed health checks.

#### Scenario: Duplicate operation

- **WHEN** the same operation id and release digest are submitted twice
- **THEN** the second request returns the existing terminal evidence

### Requirement: Secret and diagnostic redaction

The provider SHALL return secret references only and SHALL redact secrets,
source contents, credentials, and transport-specific sensitive diagnostics.

#### Scenario: Secret-bearing failure

- **WHEN** an underlying adapter reports a secret-bearing diagnostic
- **THEN** the provider response contains only the redacted diagnostic

