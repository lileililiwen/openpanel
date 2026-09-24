# Proposal: Add portable deployment adapters

## Why

The current Mac deployment repository is useful operational infrastructure,
but it cannot become part of OpenPanel's product contract. OpenPanel needs a
small adapter boundary so operators can deploy it through Compose, systemd,
Podman, Kubernetes, or another remote host workflow while keeping host paths,
credentials, DNS, and ingress outside the application.

## What Changes

- Define a deployment adapter interface and capability declaration.
- Define prepare, deploy, verify, restart, rollback, status, and logs actions.
- Require dry-run plans, idempotency, secret references, and observable
  evidence.
- Add a Mac/Jenkins adapter conformance fixture without coupling product code
  to it.

## BFS Impact Map

- Capabilities: `release-deployment-governance`, `monitoring-fleet-operations`,
  `terminal-host-fleet`, and `quality` are modified.
- Users: operators deploying to any supported remote host or orchestrator.
- Contracts: adapter manifest, action lifecycle, evidence record, health
  checks, and rollback result.
- Integrations: systemd, Compose/Podman, Kubernetes, SSH, and Jenkins are
  replaceable adapters.
- Failure boundaries: unavailable host, unauthorized action, partial deploy,
  stale plan, failed health, and rollback failure.
- Unaffected: product authentication, domain services, and provider-specific
  credentials.

## Capabilities

### Modified Capabilities

- `release-deployment-governance`
- `monitoring-fleet-operations`
- `terminal-host-fleet`
- `quality`

## Non-goals

- No requirement for Jenkins or a desktop machine.
- No automatic SSH key generation or credential discovery.
- No remote command execution outside an explicit adapter policy.
- No public ingress/DNS automation.
