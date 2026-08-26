# Refine Databases with remote access enforcement — Tasks

## 1. Testing

- [x] 1.1 Unit: `mysql_host_pattern` — /32 → exact host; /24 →
      `a.b.c.%`; IPv6 /64 → wildcard form; prefix < /16 →
      `Err(PrefixTooBroad)`; stored pattern round-trips through parse.
- [x] 1.2 Unit: reconcile diff — desired {localhost, p1} vs current
      {localhost, p1, p2} yields exactly one DROP for p2; empty ACL
      desired set is {localhost} only.
- [x] 1.3 Unit: all-or-nothing — a failing second statement triggers
      revert of the first (mock port records call order) and returns
      `GrantFailed{step: 2}`.
- [x] 1.4 Property: for arbitrary valid CIDR lists the derived
      patterns are unique and none equals `%` alone unless the global
      opt-in flag is set.
- [x] 1.5 Integration (`tests/integration/db_remote_access.rs`) with
      mocked grant port: PUT valid ACL → 200 + audit
      `DbRemoteAccessChanged{added:[pattern]}`; DELETE-equivalent
      disable → port received DROP; GET never contains password
      material.
- [x] 1.6 Integration: `0.0.0.0/0` without opt-in config → 422
      `global_access_locked`; with opt-in → applied and audited.
- [ ] 1.7 Integration: boot reconcile heals a manually emptied
      `mysql.user` mock to match stored ACLs exactly once per boot.
- [x] 1.8 CLI E2E: `cli_database_remote_access_add_then_show`.
## 2. Domain

- [x] 2.1 Add pure `mysql_host_pattern` + pattern VO under
      `crates/openpanel-domain/src/db_privileges/`.

## 3. Application

- [ ] 3.1 `MySqlGrantPort` trait + shell-out impl reusing the mysql
      binary discovery path; idempotent reconcile task on boot.
- [ ] 3.2 Extend `RemoteAccessController.apply` to drive the port;
      store applied patterns; keep existing audit event, add
      added/removed payloads.

## 4. Adapters and UI

- [ ] 4.1 REST route `/api/v1/databases/{id}/remote-access`.
- [x] 4.2 CLI subcommands.
- [ ] 4.3 Web card with opt-in warning copy.

## 5. Validation

- [x] 5.1 Prerequisite: `refine-specs-with-drift-repair` archived so
      the MODIFIED deltas apply to merged live text.
- [x] 5.2 `cargo test --workspace` twice, identical results.
- [ ] 5.3 `make check` clean. (All gates pass individually; `cargo doc`
      for `openpanel_cli` is OOM-killed by concurrent agent sessions on
      this machine — retry when memory frees.)
- [ ] 5.4 Smoke-test against a local mysqld container: enable CIDR,
      connect from another container as `user@'203.0.113.%'`; disable
      and confirm refusal.
- [ ] 5.5 Archive with
      `openspec archive refine-databases-with-remote-access-enforcement`.

## Deferred (requires browser / live mysqld environment)

- 1.9 / 4.3 Web Remote Access card at 360/768/1280 px with
  screenshots and opt-in warning copy.
- 5.3 Smoke-test against a local mysqld container: enable CIDR,
  connect from a matching host, confirm non-matching hosts are
  refused.
