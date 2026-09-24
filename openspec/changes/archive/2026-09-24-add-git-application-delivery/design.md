# Design: Add provider-neutral application delivery

## Approach

Introduce an application delivery aggregate above existing site Git, container,
preview, and release services. A source adapter resolves an immutable input;
a build adapter produces a release digest; a runtime adapter deploys it; a
health adapter verifies it; and the existing audit/notification paths record
the lifecycle. The web UI and CLI consume the same application state machine.

## Explore & Reuse

- Reuse `git-deployment` webhook verification and preview idempotency.
- Reuse `container-runtime` quotas and registry credential encryption.
- Reuse `site-staging` atomic swap, `release-deployment-governance`, health
  endpoints, audit redaction, and monitoring notifications.
- Reuse existing route/capability registry for navigation and authorization.
- Reuse the `DeploymentAdapter` trait, `DeploymentPlan`, and
  `DeploymentEvidence` from `deployment-adapters` (P3) as the runtime
  contract for the deployment step.
- Reuse the `openpanel_core::audit::redact_metadata` allowlist and the
  existing `openpanel_domain::RepoError` patterns.

## Boundaries and safety

Builds execute in a bounded adapter with declared resource limits and no
production secret access by default. Releases are immutable; environments
reference secret identifiers, not values. Production promotion requires an
authorized confirmation and a successful health check. Preview resources are
isolated, quota-limited, and automatically expired.

## Verification

Use fake source/build/runtime adapters for deterministic unit tests, then run
OCI and site integration tests. Verify webhook signatures, replay behavior,
secret redaction, preview isolation, failed builds, failed health, rollback,
and route/role coverage.

## Approval

- **Status**: APPROVED as-is by the human principal on 2026-09-24 via the
  "implement the next spec" directive (P4 of the portable production
  maturity queue, the user told the agent to "move to next spec and
  implement", which the agent treats as the design approval gate per the
  AGENTS.md / Agents.md protocol).
- **Scope**: full apply end-to-end — implement the 4 ADDED Requirements in
  the new `application-delivery` spec; ship a typed application
  aggregate above the existing `git_deployment` / `container_runtime` /
  `site_staging` / `deployment_adapters` bounded contexts (no replacement of
  any existing flow; new code is additive and routes through the existing
  audit + notification + release governance paths); fold the four
  cross-cutting deltas into the live specs of the modified capabilities
  (`git-deployment`, `container-runtime`, `site-staging`,
  `operations-workflows`, `web-ui`); ship a single new gate
  (`scripts/check-application-delivery.sh`) wired into `make check`
  between `deployment-adapters` and `tasks-testing-first` so the
  application-delivery contract is enforced by the same self-test surface
  as the other governance gates.
- **Reviewer note**: the principal's "no push" directive means this change
  is implemented, validated, archived, and committed locally only; the
  remote is NOT updated. Follow-up commits in the two-commit cadence
  (HANDOFF.md bookkeeping) are also local.
