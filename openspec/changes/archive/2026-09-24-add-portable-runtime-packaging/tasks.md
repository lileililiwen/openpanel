# Tasks: Add portable runtime packaging

## 1. Testing

### BFS — Baseline and impact coverage

- [x] Map binary, image, installer, config, migration, and service-unit
  consumers.
- [x] Add failing conformance fixtures for supported targets, missing data,
  bad config, health failure, and rollback.

### DFS — Requirement-by-requirement implementation

- [x] Implement the shared runtime contract and target manifest.
- [x] Complete OCI labels, pinned image usage, volume/config mapping, and
  graceful signal behavior.
- [x] Complete native Linux service install, upgrade, rollback, and uninstall
  boundaries.

### BFS — Cross-surface regression and completeness

- [x] Verify native and OCI adapters expose equivalent config and health
  semantics.
- [x] Verify no adapter references desktop-only paths or credentials.
- [x] Verify migrations and rollback refuse unsafe schema transitions.

### Verification

- [x] Run package and container conformance tests on a clean supported target.
- [x] Run release artifact and smoke checks.
- [x] Run `openspec validate add-portable-runtime-packaging --strict`.
