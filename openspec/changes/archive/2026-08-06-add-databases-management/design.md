# Design: Add Databases Management

## Context

The sites module provisions nginx vhosts but operators need MySQL
databases + DB users to run real apps. This change adds the `databases`
bounded context following the same pattern as sites: domain aggregate
→ service → repository → HTTP routes → CLI subcommands.

The `databases` module is independent of `sites` (a site may have zero
or many databases; a database may exist without a site). RBAC is the
same shape — Owner / Admin / User with the same access rules.

## Goals / Non-Goals

**Goals:**

- Provision a MySQL database + DB user from the panel.
- Store the DB password encrypted at rest (AES-256-GCM with a master key).
- Per-user RBAC mirroring the sites module.
- Detect when MySQL is missing and fail gracefully (no hard dep).
- Audit every mutation without leaking secrets.

**Non-Goals:**

- PostgreSQL — deferred to v0.2. Same shape, different binary.
- Remote MySQL hosts — v0.1 is `localhost` Unix-socket only.
- Per-database resource quotas — not in v0.1.
- Database backups — out of scope for v0.1.
- Connection pooling from panel to MySQL — the panel doesn't connect
  to MySQL itself; the panel orchestrates MySQL operations via the
  CLI.

## Decisions

### 1. Shell out to `mysql` instead of connecting directly

**Decision**: Use the `mysql` CLI via `std::process::Command` for
every mutation. Do not add `sqlx` MySQL features.

**Rationale**: Avoids adding a heavy native dep (MySQL client library),
works in dev environments without a running MySQL daemon (just
requires the `mysql` CLI for shell-out tests), and matches how
cPanel/Baota talk to MySQL.

**Alternatives considered**:

- `sqlx` with the `mysql` feature — clean but adds ~50 dependencies
  and a runtime client that requires the daemon to be up.
- `mysql_async` — same problem.

### 2. AES-256-GCM with master key in config

**Decision**: Use `aes-gcm` crate. Master key from `database.master_key`
(config) base64-decoded to 32 bytes. Per-record random 12-byte nonce
stored as `nonce || ciphertext` in the SQLite table.

**Rationale**: AES-GCM is the standard AEAD; the `aes-gcm` crate is
small (2 transitive deps). Master key out-of-band (env var or
`/etc/openpanel/openpanel.toml`) matches how Baota stores its master
key.

**Trade-off**: If the master key is lost, all DB passwords are
unrecoverable. The CLI prints a clear warning at first boot that the
master key must be backed up. v0.2 will add master-key rotation.

### 3. Auto-prefixed database names

**Decision**: Panel stores `db_name = "{owner_username}_{name}"` and
`db_user = "{owner_username}_{name}"` (same string). The user supplies
only the suffix.

**Rationale**: Guarantees uniqueness across users, makes `SHOW
DATABASES` in MySQL trivially attributable to a panel user, and
prevents users from squatting on common names like `app` or `wordpress`.

### 4. No transactional rollback on partial failure

**Decision**: If MySQL `CREATE DATABASE` succeeds but `CREATE USER`
fails, the orphan database is left in MySQL. The next
`change_password` / `delete_database` call cleans it up.

**Rationale**: MySQL has no multi-statement transaction wrapper that's
universally available; the panel runs as a separate process so a
nested transaction would require a stored procedure. Cleanup via the
next call is good enough for v0.1.

### 5. `localhost` only

**Decision**: v0.1 connects to MySQL via `localhost` (Unix socket).
Remote MySQL hosts (e.g. managed RDS) require TCP credentials, which
are out of scope.

**Rationale**: 99% of self-hosted panel deployments have MySQL on the
same host. v0.2 can add a `database.remote_url` config + TCP connect.

### 6. Password rotation returns plaintext once

**Decision**: `change_password` returns the new plaintext in the
response body. The panel does NOT keep it in memory after responding.

**Rationale**: The operator needs the new password to update the site
config. We have no other way to communicate it. Logging the plaintext
is forbidden.

## Risks / Trade-offs

- **Risk**: Master key leak via backup files → *Mitigation*: Document
  `OPENPANEL__DATABASE__MASTER_KEY` is sensitive; recommend
  filesystem permissions 0600 on `/etc/openpanel/openpanel.toml`.
- **Risk**: MySQL 5 vs 8 syntax differences (`CREATE USER ... IDENTIFIED BY`
  vs `CREATE USER ... IDENTIFIED WITH mysql_native_password`) →
  *Mitigation*: Use the `mysql` CLI which normalises the SQL across
  versions; the panel always calls `CREATE USER ... IDENTIFIED BY`
  which MySQL 5.7+ accepts.
- **Risk**: Concurrent `change_password` calls produce conflicting
  passwords → *Mitigation*: Service uses a per-DB write mutex (in-process).
- **Risk**: Passwords in process memory → *Mitigation*: zeroize after
  responding (drop the String from scope; v0.2 will use `zeroize` crate).

## Migration Plan

- No existing data to migrate.
- Operator must generate a master key before first boot:
  `openssl rand -base64 32` and set `OPENPANEL__DATABASE__MASTER_KEY`.
- First `openpanel migrate` applies the new `databases` table.
- First `openpanel database create` runs the MySQL shell-out.

## Open Questions

- Should the CLI accept `--password` to set a custom DB user password
  instead of generating one? → *Default: no* in v0.1, generate a
  strong random password. Operators can rotate it after.
- Should the module also support a `--no-password` flag for
  passwordless local-socket users? → *Default: no*; users always have
  a password in v0.1.