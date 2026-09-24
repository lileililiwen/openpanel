# application-delivery Specification

## Purpose
TBD - created by archiving change add-git-application-delivery. Update Purpose after archive.
## Requirements
### Requirement: Immutable Application Release

Every deployment MUST resolve to an immutable source or image digest and MUST
persist the release identity before runtime mutation.

#### Scenario: Mutable tag rejected

- **WHEN** a production deployment references an unpinned mutable image tag
- **THEN** the deployment is rejected or resolves and records the immutable
  digest before deployment.

### Requirement: Isolated Environment Lifecycle

Production and preview environments MUST have separate resource and secret
references, and preview environments MUST be quota-limited and expirable.

#### Scenario: Preview cannot use production secret

- **WHEN** a preview requests a production-only secret
- **THEN** resolution is denied and the denial is audited without the secret.

### Requirement: Health-Gated Promotion

An application MUST NOT enter `Ready` or production promotion state until its
declared health checks pass within the configured deadline.

#### Scenario: Health timeout

- **WHEN** the deployment does not become healthy before its deadline
- **THEN** promotion is blocked, diagnostics are bounded and redacted, and the
  release remains recoverable.

### Requirement: Audited Rollback

An authorized operator MUST be able to roll back to a prior verified release;
the result MUST include release identity, health result, and rollback state.

#### Scenario: Rollback succeeds

- **WHEN** an operator confirms rollback to the previous healthy release
- **THEN** traffic returns to that release and one audit event records the
  transition without credentials or source payloads.

