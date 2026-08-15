# Add Database Privilege Management — Tasks

## 1. Testing

- [x] 1.1 Unit: grant-scope resolution rejects a table outside the
      database; remote-access empty-ACL rule; SSO token single-use and
      expiry.
- [x] 1.2 Property: a grant for `db1` can never confer access to
      `db2`; enabling remote access with an empty ACL and no opt-in is
      always rejected.
- [x] 1.3 Service: apply grants, revoke grants, toggle remote access,
      launch admin tool; audit records privilege names only.
- [x] 1.4 Integration: a `GRANT` applied via the API is visible in the
      engine; `0.0.0.0/0` is only reachable with explicit opt-in; SSO
      token opens the tool.
- [ ] 1.5 CLI E2E: `openpanel db user grants` → `remote` → `tool`.
- [ ] 1.6 Web: database user editor, remote-access toggle with an
      explicit wildcard opt-in warning, admin-tool launch button.

## 2. Domain and Application

- [x] 2.1 Extend `databases` domain with `DbGrant`, `GrantScope`,
      `RemoteAccess`, `RemoteAcl`, `AdminToolSession` under
      `crates/openpanel-domain/src/db_privileges/`.
- [x] 2.2 Add SQLite migration for `db_grants`, `remote_access`.
- [x] 2.3 Implement `PrivilegeService`, `RemoteAccessController`,
      `AdminToolSso`; register within the existing `databases` module.

## 3. Adapters and UI

- [ ] 3.1 Add `GET`/`PUT /databases/{id}/users/{uid}/privileges`,
      `PUT /databases/{id}/remote-access`,
      `POST /databases/{id}/admin-tool` REST routes.
- [ ] 3.2 Add `openpanel db user {grants,remote,tool}`.
- [ ] 3.3 Build the database user editor (grant scopes), remote-access
      toggle with opt-in warning, and admin-tool launch button (CSRF).

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: create a scoped user, toggle remote access
      (confirm empty ACL is rejected), launch phpMyAdmin via SSO.
- [x] 4.4 Archive with `openspec archive add-database-privilege-management`.
