# Tasks: Unify capability navigation and site workspaces

## 1. Testing

- [x] Add registry tests for unique capability keys, routes, icons, labels, roles, and scope.
- [x] Add integration tests that every mounted first-class page is discoverable and every nav route is mounted.
- [x] Add role tests for Owner/Admin/User visibility and direct-route authorization.
- [x] Add site-workspace tests for Domains, Runtime, Logs, Backups, unavailable capability, and preserved context after errors.
- [x] Run the new tests red before implementation.

## 2. Implementation

- [x] Add typed route/capability metadata and derive shell navigation from it.
- [x] Replace duplicated capability/tab lists with registry queries.
- [x] Implement missing site-scoped Domains, Runtime, Logs, and Backups landing routes using existing services.
- [x] Add explicit unavailable and unauthorized UI states.
- [x] Update discoverability and site-workspace specifications.

## 3. Verification

- [x] Run focused navigation, site-workspace, and route integration tests.
- [x] Run `make check` (green except three pre-existing blockers, all evidenced at HEAD: environmental `audit` h2 RUSTSEC-2026-0258, `reuse-strict` `guidance` duplicate from change 5, `spec-test-drift-strict` `ssl-production-lifecycle` gap from change 3; this change adds zero new failures).
- [x] Run `openspec validate unify-capability-navigation-and-site-workspaces --strict`.
- [x] Verify all mounted capabilities are reachable from the correct role-filtered shell.
