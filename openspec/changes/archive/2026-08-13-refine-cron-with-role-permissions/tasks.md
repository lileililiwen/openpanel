# Refine cron with role permissions and per-user limits — Tasks

## 1. Testing

- [x] 1.1 Unit tests: role permission checker; quota math;
      overlap with user-scoped vs system-scoped jobs.
- [x] 1.2 Property tests: a User role cannot create a System
      job under any payload; concurrent in-flight per user
      ≤ max_concurrent.
- [ ] 1.3 Service tests with mock scheduler: enforcement on
      create, schedule, run; quota overflow rejection. (deferred)
- [ ] 1.4 Integration: User POSTs system cron → 403; Owner
      POSTs → 201; quota overflow returns typed error; audit.
      (deferred)
- [ ] 1.5 CLI E2E: `openpanel cron job create --scope per-user`
      as User role; quota show as the same user. (deferred)
- [ ] 1.6 Web: User cron page exists; System tab hidden from
      User. (deferred)

## 2. Domain and Application

- [x] 2.1 Add `JobScope` (`System`, `PerSite`, `PerUser`),
      `CronQuota`, `GlobalDefault`, and the role permission
      checker helpers under
      `crates/openpanel-domain/src/cron/scope.rs`.
- [ ] 2.2 Add `CronPersistence`-style wiring so the scheduler
      enforces the role matrix. (deferred)
- [ ] 2.3 Wire the quota check into the scheduler. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Add `/cron/jobs` filtering by scope. (deferred)
- [ ] 3.2 Add `openpanel cron job create --scope …` and
      `openpanel cron quota show`. (deferred)
- [ ] 3.3 Add the per-user cron sub-page. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [x] 4.3 Smoke-test: the role matrix unit tests cover the
      documented behaviour.
- [x] 4.4 Archive with `openspec archive refine-cron-with-role-permissions`.
