# Refine Databases with remote access enforcement

## Why

The archived `add-database-privilege-management` delta modelled a
`RemoteAccess` CIDR ACL and audit events
(`crates/openpanel-domain/src/db_privileges/mod.rs:147-187`,
`crates/openpanel-app/src/db_privileges/service.rs:94-153`) but nothing
applies it to MySQL: no `CREATE USER …@host`, no GRANT sync, no
surface, and the live spec still pins `db_host = localhost`. cPanel's
"Remote MySQL" is a baseline feature; without grant application the
ACL is dead state.

## What Changes

- **Grant application**: toggling remote access creates/modifies the
  matching MySQL accounts (`user@'<cidr-derived host>'`) and grants;
  disabling drops them, leaving only `user@localhost`.
- Lift the v0.1 `db_host = localhost` pin in the live aggregate and
  provisioning requirements (MODIFIED deltas).
- **Surfaces**: `GET/PUT /api/v1/databases/{id}/remote-access`, CLI
  `openpanel database remote-access …`, web Databases tab section.
- Reuse `DbRemoteAccessChanged` audit; add grant-failure surfacing.
- Depends on `refine-specs-with-drift-repair` merging the privilege
  delta into the live spec first.

## Capabilities

### Modified Capabilities

- `databases`: remote-access ACL becomes enforced state with surfaces;
  aggregate/provisioning requirements updated to allow non-local
  hosts.

## Impact

- App: extends `RemoteAccessController` with a `MySqlGrantPort`
  (shell-out via existing `mysql` CLI detection path); idempotent
  reconcile on boot (drift repair for manually deleted grants).
- Domain: no new aggregates; `RemoteAccess` gains derived host-pattern
  helper (pure).
- API/CLI/web as above; DTO never returns passwords.
- Security: wildcard hosts limited to CIDR-derived patterns; `0.0.0.0/0`
  stays behind the existing explicit opt-in gate; failures audited.
- Coupling: databases + db_privileges modules; drift-repair change is
  a prerequisite.

## Non-goals

- No SSL-enforced remote connections policy (server-level TLS config
  is out of scope here).
- No PostgreSQL support (engine still MySQL-only).
