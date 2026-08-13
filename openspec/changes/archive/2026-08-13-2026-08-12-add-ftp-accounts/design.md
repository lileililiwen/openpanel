# Add per-site FTP accounts — Design

## Domain model

```
FtpAccount
  { id, site_id, username (unique within site),
    home_abs (canonicalised site root),
    password_hash (argon2id, same envelope as identity),
    bandwidth_kb_per_session?, max_concurrent_conns?,
    read_only: bool,
    enabled: bool,
    last_login_at?, last_login_ip?,
    created_at, disabled_at? }
```

## Authentication

The FTP wire username is `<site-uuid>@<account-username>`. FTP does not
carry a site identifier separately, while account usernames are only
unique within a site; requiring the qualified form makes lookup
unambiguous without weakening the per-site uniqueness contract. Panel
surfaces continue to display the account username separately.

```
USER <username>
PASS <password>
  → FtpAuthenticator::authenticate
       → look up FtpAccount by (site_id, username)
       → verify enabled
       → verify password via identity's shared Password primitive
       → keep lookup and filesystem root bound to account.site_id
       → audit FtpLogin (success or failure)
  → 230 Welcome
  → or 530 Login incorrect
```

- The panel never accepts a "global" FTP account; every account is
  scoped to exactly one site.
- An Owner can create accounts on any site; Admins and Users only
  on sites they own.

## Filesystem virtualization

- `libunftp` StorageBackend is wrapped to canonicalize every path
  before any syscall; the chroot check is identical to the files
  module's `Path::within(site_root)`.
- `..` and absolute paths are rejected; symlinks that escape the
  site root are rejected (canonicalize-then-check).
- File size caps mirror the file manager cap (50 MB read, 200 MB
  upload).

## TLS

- `OPENPANEL__FTP__TLS_MODE` ∈ { `StartTls`, `None` }. Default
  `StartTls`; `None` is refused unless the bind address is loopback.
  `ImplicitTls` is parsed only to fail closed with a clear validation
  error: the selected `libunftp` version supports explicit FTPS but
  does not expose an implicit-FTPS listener.
- The panel reuses the existing `SslPaths` envelope: a single cert
  can be selected for FTP via `OPENPANEL__FTP__TLS_CERT_DOMAIN`.

## Limits

- Per-account: max concurrent connections (default 4) and per-session
  bandwidth (default 1 GiB).
- Global: max total connections (default 256), max login attempts
  per minute per IP (default 5; surplus returns 421 and audits).
- Idle session timeout: 5 minutes; control channel: 2 minutes.

## Failure modes

- Port already bound: supervisor records a `BindFailed` audit
  event with the OS error and refuses to start.
- `libunftp` panic: supervisor restarts with backoff and a hard
  cap (3 attempts / 5 min) before giving up.
- Storage backend I/O error: 550 returned to the client with a
  redacted audit row.

## Endpoints

```
GET    /api/v1/sites/{id}/ftp/accounts
POST   /api/v1/sites/{id}/ftp/accounts
       { username, password, read_only?, bandwidth_kb_per_session?,
         max_concurrent_conns? }
       → 201 { id, plaintext_password_shown_once: true }
PATCH  /api/v1/sites/{id}/ftp/accounts/{acc_id}
       { enabled?, password?, ... }
DELETE /api/v1/sites/{id}/ftp/accounts/{acc_id}
GET    /api/v1/sites/{id}/ftp/accounts/{acc_id}/sessions
```

The plaintext password is shown to the creator exactly once; only
the argon2id hash is persisted.

## Tests

```
1.1  Unit: FtpAccount validation (username format, password
     strength, site-root canonicalization).
1.2  Property: chroot rejection is total — any path that
     canonicalises outside the site root is denied; symlink
     escape is denied.
1.3  Service tests with mock identity, mock storage, and mock
     libunftp client covering auth success/failure, read-only
     enforcement, and session-limit enforcement.
1.4  Integration: live `libunftp` over StartTls against an in-proc
     axum test server; upload + download round-trip; chroot
     traversal attempt denied.
1.5  CLI E2E: `openpanel ftp {create,list,disable,enable,delete}`.
1.6  Web: /sites/{id}/ftp with account list, create form (CSRF),
     and "password shown once" banner.
```
