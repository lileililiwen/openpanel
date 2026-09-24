# Proposal: Add portable runtime packaging

## Why

OpenPanel's runtime package is currently optimized for Linux release artifacts
and a Dockerfile, while the deployment automation expects a Compose project.
The product must run on ordinary supported Linux hosts and OCI runtimes
without requiring a maintainer's desktop, Docker Desktop, Jenkins, or a
specific filesystem layout.

## What Changes

- Define a provider-neutral runtime contract for native Linux and OCI images.
- Publish a pinned image/configuration contract with persistent data,
  readiness, graceful shutdown, and upgrade/rollback semantics.
- Define supported target discovery and clear unsupported-platform behavior.
- Add portable deployment examples and conformance fixtures.

## BFS Impact Map

- Capabilities: `release-deployment-governance`, `container-runtime`, and
  `quality` are modified.
- Users: self-hosters, package maintainers, distribution packagers, and
  operators using Docker/Podman/Kubernetes-compatible runtimes.
- Contracts: OCI labels, image entrypoint, data volume, config/env mapping,
  health endpoint, signal handling, and native service unit.
- Integrations: registries and init systems are adapters; Mac/Jenkins is only
  a test adapter.
- Failure boundaries: unsupported OS, wrong architecture, missing volume,
  invalid config, failed migration, unhealthy process, and rollback.
- Unaffected: customer data model and web feature semantics.

## Capabilities

### Modified Capabilities

- `release-deployment-governance`
- `container-runtime`
- `quality`

## Non-goals

- No dependency on Docker Desktop, Jenkins, Cloudflare, or a developer laptop.
- No provider-specific cloud orchestrator.
- No automatic mutation of host firewall, DNS, or TLS configuration.
