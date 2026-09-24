# Proposal: Add provider-neutral application delivery

## Why

Coolify demonstrates that a modern panel must manage the full application
lifecycle—source or image, build, deploy, health, logs, domains, previews,
rollback—not only server resources. OpenPanel already has site Git deployment,
preview models, containers, and release contracts, but they are not one
coherent application delivery workflow.

## What Changes

- Define an application resource that can use Git, OCI, or Compose input.
- Add immutable release records, environment references, health checks,
  deployment history, logs, rollback, and preview lifecycle.
- Enforce production/preview isolation and auditable promotion.
- Keep build providers and registries behind adapters.

## BFS Impact Map

- Capabilities: `git-deployment`, `container-runtime`, `site-staging`,
  `operations-workflows`, `monitoring-fleet-operations`, and `web-ui` are
  modified.
- Users: site owners, operators, maintainers, and downstream application
  publishers.
- Contracts/data: application, environment, release, deployment, preview,
  health, log, secret reference, and rollback records.
- Integrations: Git providers, OCI registries, build runners, reverse proxies,
  and notification dispatchers.
- Failure boundaries: invalid source, build failure, health failure, quota,
  secret absence, replay, and rollback failure.
- Unaffected: host-specific ingress implementation and arbitrary shell scripts.

## Capabilities

### Modified Capabilities

- `git-deployment`
- `container-runtime`
- `site-staging`
- `operations-workflows`
- `web-ui`

## Non-goals

- No proprietary CI service requirement.
- No unbounded arbitrary code execution on the panel host.
- No forced Kubernetes dependency.
- No replacement of existing site-level Git behavior until parity is proven.
