# Tasks: Add portable deployment adapters

## 1. Testing

### BFS — Baseline and impact coverage

- [x] Map existing fleet, agent, release, audit, command-safety, and Jenkins
  integration boundaries.
- [x] Add failing fake-adapter fixtures for every action and failure state.

### DFS — Requirement-by-requirement implementation

- [x] Add adapter manifest, capability model, and action result types.
- [x] Add idempotent dry-run, preflight, deploy, verify, status, logs, and
  rollback lifecycle.
- [x] Add secret references, audit-safe evidence, and authorization checks.

### BFS — Cross-surface regression and completeness

- [x] Verify adapters do not leak host paths, credentials, or provider names
  into product contracts.
- [x] Verify failed verification and unsupported rollback are distinct states.
- [x] Verify the Mac/Jenkins adapter passes without becoming a required CI
  environment.

### Verification

- [x] Run fake, OCI, generic Linux, and Mac adapter conformance suites.
- [x] Run audit, fleet, and release gates.
- [x] Run `openspec validate add-portable-deployment-adapters --strict`.
