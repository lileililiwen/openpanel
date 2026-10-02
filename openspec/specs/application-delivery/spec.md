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

### Requirement: A production docker build adapter SHALL resolve immutable digests (DCK-001)

`DockerBuildAdapter` (id `oci-docker-build-v1`) MUST implement the domain
`BuildAdapter` by building git sources through the existing docker service
and pushing to the configured registry, returning a digest-pinned
`ReleaseDigest`, and mapping daemon-unavailable and registry-auth failures
to the new typed `RuntimeUnavailable` and `RegistryUnauthorized` domain
errors with redacted fixed details. The application's build command MUST be
passed unchanged to the Docker build context.

#### Scenario: Git source becomes a pinned digest

- **WHEN** `resolve_digest` runs for a git application source at a valid
  ref against the docker-service double
- **THEN** the returned release digest is the registry-pushed immutable
  digest, not a mutable tag

#### Scenario: Registry authentication rejected

- **WHEN** the push step fails with an auth error
- **THEN** the adapter returns the chosen domain error variant and the
  detail string contains no credential material

### Requirement: A production docker runtime adapter SHALL converge and roll back environments (DCK-002)

`DockerRuntimeAdapter` (id `oci-docker-runtime-v1`) MUST implement
`RuntimeAdapter` `deploy`/`rollback` by pulling the pinned digest and
converging the environment container through the existing docker service,
labeling it with application/environment/release identity, returning a
terminal `ReleaseOutcome` with bounded diagnostics.

#### Scenario: Deploy converges one labeled container

- **WHEN** `deploy` runs for a release and environment
- **THEN** the docker double records one container creation carrying the
  three openpanel labels and the outcome is terminal

#### Scenario: Rollback target missing

- **WHEN** `rollback` names a prior release that no longer exists on the
  host
- **THEN** a typed domain error is returned and no container is mutated

### Requirement: The forge provider composition SHALL use production adapters (DCK-004)

The `forge-provider` entrypoint MUST construct delivery services from the
docker-backed adapters, and the fail-closed `Unavailable` composition
markers introduced with the stdio transport MUST be deleted rather than
retained as alternates.

#### Scenario: Publish path succeeds against the double

- **WHEN** a Preflight/Deploy frame exchange runs in the integration
  harness with the docker double
- **THEN** the facade returns success statuses where the Unavailable seam
  previously returned failures

