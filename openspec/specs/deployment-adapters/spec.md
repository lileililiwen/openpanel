# deployment-adapters Specification

## Purpose
TBD - created by archiving change add-portable-deployment-adapters. Update Purpose after archive.
## Requirements
### Requirement: Adapter Capability Declaration

Each adapter MUST declare its target identity, supported actions, runtime
contract version, health method, rollback support, and secret-reference
mechanism before it can receive a deployment plan.

#### Scenario: Unsupported action

- **WHEN** a caller requests an action not declared by the adapter
- **THEN** the plan is rejected before remote mutation with an actionable
  unsupported-capability result.

### Requirement: Idempotent Deployment Lifecycle

Mutating adapter actions MUST use an operation key and MUST return the same
terminal result for a replayed completed operation without duplicating work.

#### Scenario: Replay

- **WHEN** the same deploy operation key is submitted twice
- **THEN** the second request returns the original result and does not create a
  second release or duplicate audit event.

### Requirement: Provider-Neutral Evidence

Every deployment action MUST produce evidence containing target identity,
release digest, action, state, timestamps, and redacted diagnostics without
requiring a specific transport or host path.

#### Scenario: Mac adapter evidence

- **WHEN** the Mac/Jenkins adapter completes a deployment
- **THEN** it emits evidence conforming to the same schema as a generic Linux
  adapter, with no Mac-only field required by OpenPanel.

### Requirement: Safe Failure and Rollback

An adapter MUST distinguish preflight rejection, transport failure, health
failure, partial deployment, and rollback failure; it MUST NOT report success
when verification is incomplete.

#### Scenario: Health failure

- **WHEN** a deployment starts but the target fails its declared health check
- **THEN** the result is failed or rolled-back with evidence and never `Ready`.

