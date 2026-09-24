# verified-service-catalog Specification

## Purpose
TBD - created by archiving change add-verified-service-catalog. Update Purpose after archive.
## Requirements
### Requirement: Signed Declarative Manifest

Each installable service MUST have a versioned manifest containing publisher,
source digest, compatibility, permissions, resources, storage, health, backup,
and upgrade metadata, and MUST pass signature verification before planning.

#### Scenario: Invalid signature

- **WHEN** a manifest signature does not verify against a trusted publisher
- **THEN** the catalog marks the entry blocked and refuses planning or install.

### Requirement: Reviewable Install Plan

The panel MUST show the effective ports, storage, network access, resource
limits, credentials by identifier, and data effects before a service install is
confirmed.

#### Scenario: Resource conflict

- **WHEN** a service requests a port or resource outside the target policy
- **THEN** planning fails with a specific conflict and performs no mutation.

### Requirement: Safe Lifecycle

Install, upgrade, backup, health verification, rollback, and removal MUST be
represented as observable jobs with bounded retries, audit events, and explicit
failure states.

#### Scenario: Upgrade failure

- **WHEN** a service upgrade fails health verification
- **THEN** the previous verified release remains recoverable and the job is
  failed or rolled back with operator guidance.

### Requirement: Trust Revocation

The catalog MUST support publisher or artifact revocation, and a revoked
entry MUST NOT be installable even when it remains cached locally.

#### Scenario: Cached revoked entry

- **WHEN** an operator requests an entry whose publisher was revoked
- **THEN** the request is refused and the refusal is audited without exposing
  credentials or manifest secrets.

