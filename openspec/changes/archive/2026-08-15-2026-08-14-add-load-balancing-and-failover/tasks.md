# Add load balancing and failover — Tasks

## 1. Testing

- [x] 1.1 Unit: probe decision logic (enable/disable at thresholds);
      sticky pin movement on member disable.
- [x] 1.2 Property: a disabled member never receives traffic; member
      weights are non-negative and sum to a sane total.
- [x] 1.3 Service: add member, simulate probe failure, confirm
      auto-removal from rotation; then recovery re-enables it.
- [x] 1.4 Integration: live balancer routes only to enabled members;
      killing a backend triggers failover.
- [ ] 1.5 CLI E2E: `openpanel lb list` -> `set` -> `members` -> `health`.
- [ ] 1.6 Web: Load Balancer tab (CSRF), member toggle, health badge.

## 2. Domain and Application

- [x] 2.1 Implement `Pool`, `Member`, `HealthCheck`, `LbStatus` under
      `crates/openpanel-domain/src/load_balancing/`.
- [x] 2.2 Add SQLite migration for `lb_pools`, `lb_members`.
- [x] 2.3 Implement `LbService`, `HealthProbe`, `MemberRotator`;
      register the module via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/lb/pools`, `/lb/pools/{id}/members`, `/lb/health` REST
      routes (admin/cluster authority required).
- [ ] 3.2 Add `openpanel lb {list,set,members}` CLI.
- [ ] 3.3 Build the Load Balancer tab (CSRF), member toggle, health
      badge.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: create a pool across two nodes, fail one backend,
      confirm traffic fails over; recover and confirm re-enable.
- [x] 4.4 Archive with `openspec archive add-load-balancing-and-failover`.
