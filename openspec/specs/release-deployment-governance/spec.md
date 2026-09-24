# release-deployment-governance Specification

## Purpose
TBD - created by archiving change add-release-and-deployment-governance. Update Purpose after archive.
## Requirements
### Requirement: Reproducible Supported Artifacts

Every release MUST build documented Linux targets, including the supported
glibc, musl, and ARM targets, from a pinned toolchain and Cargo.lock.

#### Scenario: Repeated release build

- **WHEN** two clean jobs build the same commit and lockfile
- **THEN** their artifact content hashes match.

### Requirement: Artifact Integrity Metadata

Each published artifact MUST have a checksum, SBOM, provenance record, and
signature that can be verified without network access to the build job.

#### Scenario: Unsigned artifact

- **WHEN** a release item lacks a valid signature or checksum
- **THEN** publication fails and the artifact is not advertised as supported.

### Requirement: Container Runtime Contract

The official container MUST expose health and readiness endpoints, terminate
cleanly, persist data only through a declared volume, and run without an
unnecessary privileged mode.

#### Scenario: Healthy container

- **WHEN** the container starts with a fresh data volume
- **THEN** `/health` succeeds, readiness reports migration state, and shutdown
  completes without corrupting the store.

### Requirement: Safe Upgrade and Rollback

Upgrade MUST preflight version/schema compatibility, run migrations before
readiness, and refuse unsupported downgrade or irreversible rollback paths.

#### Scenario: Too-new schema

- **WHEN** an older binary sees a database schema newer than it supports
- **THEN** startup refuses mutation and reports an actionable recovery path.

### Requirement: Release Gate Is Blocking

No release workflow MAY publish artifacts unless the required quality and
platform smoke jobs MUST succeed.

#### Scenario: Quality failure

- **WHEN** `make check` fails in the release workflow
- **THEN** publication is blocked.

### Requirement: Publication Evidence Manifest

The release workflow MUST emit `dist/evidence-manifest.json` whose records
identify the build commit, target, timestamp, command, tool versions, scope,
and per-gate result state. The manifest MUST be validated by the
`make release-evidence` gate before any artifact is uploaded; a missing,
malformed, stale, or non-`PASS` record on any required gate (audit,
coverage, browser-ui-quality, release-governance, smoke) MUST block
publication with `BLOCKED` evidence. The detailed contract — required
record ids, field schema, and the `STALE` / `BLOCKED` / `FAIL`
classification — is owned by
`openspec/specs/release-evidence/spec.md`.

#### Scenario: Missing coverage record blocks upload

- **WHEN** `dist/evidence-manifest.json` is present but lacks the
  `coverage` record
- **THEN** the release-evidence gate fails and the upload step does not
  run.

#### Scenario: Stale evidence is rejected

- **WHEN** an evidence record was produced for a different commit or
  target than the one currently being published
- **THEN** the release-evidence gate rejects it as stale and publication
  is blocked.

