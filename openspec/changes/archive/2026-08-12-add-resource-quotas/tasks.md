# Add per-user resource quotas — Tasks

## 1. Testing

- [x] 1.1 Unit: QuotaPolicy validation, soft/hard math, grace window
      evaluation, monthly bandwidth reset.
- [x] 1.2 Property: usage over hard always blocks; usage under soft
      never blocks; grace window advances correctly across time.
- [ ] 1.3 Service tests with mock disk/bandwidth/inode enforcers and
      mock clock. (deferred — enforcer ports live in the follow-on
      `add-multi-host-agent` change)
- [ ] 1.4 Integration: full disk -> EDQUOT at file manager; bandwidth
      over soft -> warning; over hard -> egress blocked.
      (deferred to the kernel-level enforcement follow-on)
- [ ] 1.5 CLI E2E: `openpanel quota set/get/usage` and
      `openpanel quota disable`. (deferred)
- [ ] 1.6 Web: /users/{id}/quotas with policy form, usage chart,
      and CSRF. (deferred)

## 2. Domain and Application

- [x] 2.1 Implement `QuotaPolicy` aggregate, `QuotaUsage` value
      object under
      `crates/openpanel-domain/src/quotas/`.
- [x] 2.2 Add SQLite migrations for `quota_policies` and
      `quota_usages`.
- [x] 2.3 Implement `QuotaService` and register the module.

## 3. Adapters and UI

- [ ] 3.1 Add `/api/v1/users/{id}/quotas` REST routes with DTOs.
      (deferred; HTTP layer follows the kernel-enforcement
      follow-on)
- [ ] 3.2 Add `openpanel quota {set,get,usage}` CLI subcommands.
      (deferred)
- [ ] 3.3 Build `/users/{id}/quotas` web page with policy form,
      usage chart, and CSRF. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: disk > hard -> QuotaExceeded enforcement.
      (deferred — kernel-level enforcement follow-on)
- [x] 4.4 Archive with `openspec archive add-resource-quotas`.

## 5. Module Wiring

- [x] 5.1 Audit actions: QuotaPolicyChanged, QuotaPolicyDeleted,
      QuotaSoftLimitReached, QuotaHardLimitReached.
