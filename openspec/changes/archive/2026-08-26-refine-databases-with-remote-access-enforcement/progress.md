# Progress — refine-databases-with-remote-access-enforcement

**Done (session 2026-08-26):**

- Domain: pure `mysql_host_pattern` (/32 exact, /24 a.b.c.%, IPv6
  /64 wildcard, broad-prefix rejection without opt-in),
  `desired_hosts`, `reconcile_diff`; units + 100-case property.
- App: `MySqlGrantPort` trait + `MySqlShellGrantPort` (mysql CLI) +
  `MemoryGrantPort` recorder; `RemoteAccessController::apply`
  all-or-nothing with reverse-order revert and GrantFailed{step};
  boot `reconcile_all` re-applies stored ACLs.
- REST: PUT/GET `/api/v1/databases/{id}/remote-access`; wildcard
  without opt-in -> 422 global_access_locked.
- CLI: `database remote-access {add,show}` with an in-memory port
  behind OPENPANEL__DATABASES__GRANT_PORT=memory for tests.
- Tests: all-or-nothing unit (recording mock), integrations
  apply/disable/wildcard-lock/audit/no-password-material, CLI E2E.

**Remaining:** 5.3 blocked environmentally (docs-gate OOM under
concurrent agent sessions); deferred web card + live-mysqld smoke;
archive.