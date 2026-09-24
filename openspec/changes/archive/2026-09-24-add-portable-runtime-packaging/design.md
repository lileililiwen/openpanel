# Design: Add portable runtime packaging

## Approach

Define one runtime contract with two official adapters: a native Linux service
and an OCI image. Both use the same configuration schema, data directory,
health/readiness semantics, migration lifecycle, and rollback rules. Compose,
systemd, Podman, Kubernetes, and the Mac deployment repository consume the
contract without changing OpenPanel behavior.

## Explore & Reuse

- Reuse `packages/installer/install.sh`, `packages/installer/entrypoint.sh`,
  `packages/installer/smoke-container.sh`, `Dockerfile`, `/health`,
  `healthcheck`, and existing release provenance.
- Reuse the config schema and migration ceiling checks rather than adding a
  second configuration parser.
- Reuse existing graceful shutdown and non-root container behavior.

## Boundaries and compatibility

The product owns the binary, image, config, data, and service contract. A
deployment adapter owns process supervision, network publishing, secret
injection, and host-specific paths. The adapter MUST NOT require a developer
checkout or infer credentials from local shell state. Unsupported platforms
return an actionable compatibility result; they are not silently treated as
Linux.

## Verification

Run native install/upgrade/rollback fixtures, OCI smoke tests, architecture
matrix tests, config validation tests, and a clean-host acceptance test using
an isolated Linux VM or equivalent provider-neutral runner.

## Approval

- **Status**: APPROVED as-is by the human principal on 2026-09-24.
- **Scope**: full apply end-to-end — implement the 4 ADDED requirements in
  the new `portable-runtime` spec, modify `release-deployment-governance`,
  `container-runtime`, and `quality` per the proposal's "Modified
  Capabilities" list, and wire the conformance fixtures and
  spec-test-drift evidence required by the new requirements.
- **Reviewer note**: the two flags I raised (the `container-runtime` name
  collision and the thin design body) were acknowledged by the principal
  and explicitly de-scoped from this change. The proposal is to be
  followed verbatim.
