# host-migration-interoperability Specification

## Purpose

Real-world adoption depends on operating more than one host and importing existing sites safely. Virtualmin and other panels make domain-level backup, restore, migration, quota, mail, and connectivity behavior explicit. OpenPanel has agent fleet, backup/DR, and migration foundations, but needs a portable operator workflow with dry-run plans, scoped changes, and source adapters.
## Requirements
### Requirement: Host Capability Contract

Each managed host MUST expose an authenticated capability profile containing
OS/architecture, runtime adapters, supported operations, version, and health
state without exposing credentials or command material.

#### Scenario: Capability mismatch

- **WHEN** an operation requires a capability absent from the host profile
- **THEN** the operation is rejected before mutation with a remediation.

### Requirement: Migration Preview and Conflict Policy

Every migration MUST produce a bounded plan listing resources, warnings,
conflicts, data effects, and rollback support before commit.

#### Scenario: Existing domain conflict

- **WHEN** a source domain maps to an existing target domain
- **THEN** the plan blocks or requires an explicit conflict policy and performs
  no destructive change during preview.

### Requirement: Scoped Verified Import

An import MUST commit only resources explicitly included in the confirmed plan,
MUST use existing authorization and bounded-context services, and MUST verify
post-import site, database, mail, DNS, and SSL state where applicable.

#### Scenario: Partial verification

- **WHEN** the site imports successfully but mail or SSL verification fails
- **THEN** the result is incomplete with actionable evidence and no automatic
  DNS cutover.

### Requirement: Reversible Cutover

DNS, mail, and SSL cutover MUST be separate authorized actions with preflight,
audit, and a declared rollback or recovery path.

#### Scenario: Cutover rejected

- **WHEN** external reachability or certificate preflight fails
- **THEN** cutover is refused and the imported source remains isolated.
