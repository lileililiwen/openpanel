# Proposal: Add release and deployment governance

## Why

OpenPanel currently has source CI and an installer package, but no repository
evidence for reproducible server releases, signed artifacts, container
publishing, upgrade/rollback behavior, or a supported-platform matrix. A
server-management panel must make installation and recovery trustworthy before
it can compete with mature panels.

## What Changes

- Build release artifacts for documented Linux targets, including musl and ARM.
- Publish checksums, SBOM, provenance, and signature verification metadata.
- Add a reproducible container build and health/readiness checks.
- Add upgrade, migration, rollback, and incompatible-schema safeguards.
- Add release smoke tests for installer, binary, container, and supported OSes.

## Capabilities

### New Capabilities

- `release-deployment-governance`

### Modified Capabilities

- `quality` — gains a `Release Governance Gate` requirement that wires
  `scripts/check-release-governance.sh` into `make check` so a release
  cannot be advertised as supported unless every artifact ships with
  its checksum, SBOM, provenance, and signature sidecars.

## Non-goals

- No cloud hosting service.
- No Kubernetes operator.
- No change to product domain persistence rules.
- No automatic production deployment without an explicit release approval.
- No online signature key infrastructure (HSM / KMS); the v1 model is
  a minisign public-key fingerprint published in `docs/RELEASE.md`.

## Dependencies

Depends on `ratchet-quality-and-spec-maturity` for enforceable release checks.
