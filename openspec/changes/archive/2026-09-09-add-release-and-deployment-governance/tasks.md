# Tasks: Release and deployment governance

## 1. Testing

- [x] Add workflow-lint fixtures for missing target, unsigned artifact, missing SBOM, and non-blocking quality gate.
- [x] Add installer integration tests for clean install, upgrade, rollback refusal, and unsupported OS.
- [x] Add container smoke tests for `/health`, readiness, non-root policy, and signal shutdown.
- [x] Add migration compatibility tests for older supported schema and too-new schema.
- [x] Run all new tests red before implementation.

## 2. Implementation

- [x] Add release matrix workflow with pinned Rust/toolchain and target versions.
- [x] Add reproducible artifact packaging, checksums, SBOM, provenance, and signatures.
- [x] Add Dockerfile and container health/readiness configuration.
- [x] Add installer upgrade/rollback and migration preflight behavior.
- [x] Add supported-platform and release operator documentation.

## 3. Verification

- [x] Run workflow/configuration tests and package smoke tests.
- [x] Run `make check`.
- [x] Run `openspec validate add-release-and-deployment-governance --strict`.
- [x] Verify artifacts from two clean builds have identical content hashes.
