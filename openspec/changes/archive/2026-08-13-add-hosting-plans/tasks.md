# Add hosting plans — Tasks

## 1. Testing

- [x] 1.1 Unit tests for `PlanValidator` and `PlanResolver`.
- [x] 1.2 Property tests: assignments are 1:1, effective ≤ plan,
      bound enforcement.
- [x] 1.3 Service tests for create / update / disable / clone /
      assign / unassign with mock audit.
- [x] 1.4 Integration: full lifecycle through REST.
- [ ] 1.5 CLI E2E: create-assign-resolve cycle. (deferred to follow-up)
- [ ] 1.6 Web: `/plans` list/edit/assign with CSRF. (deferred to follow-up)

## 2. Domain and Application

- [x] 2.1 Implement `HostingPlan`, `PlanFeature`, `PlanQuotas`,
      `PlanResolver` under
      `crates/openpanel-domain/src/hosting_plans/`.
- [x] 2.2 Add SQLite migrations for `hosting_plans`,
      `plan_features`, `user_plan_assignments`.
- [x] 2.3 Implement `HostingPlansService` and register the
      `hosting-plans` module with the registry.

## 3. Adapters and UI

- [x] 3.1 Add `/api/v1/hosting-plans` REST routes with DTOs.
- [ ] 3.2 Add `openpanel plans` CLI subcommands. (deferred to follow-up)
- [ ] 3.3 Build the `/plans` web page with assign dialog. (deferred to follow-up)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [x] 4.3 Smoke-test: create a plan with quotas; assign to a
      user; `PlanResolver` returns the plan's caps.
- [x] 4.4 Archive with `openspec archive add-hosting-plans`.
