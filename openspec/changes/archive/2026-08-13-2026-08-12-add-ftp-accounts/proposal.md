# Add per-site FTP accounts

## Why

Baota, cPanel, and DirectAdmin all expose per-site FTP. A web owner
who uploads through FileZilla or `curlftpfs` is the canonical
panel workflow. OpenPanel's files module is HTTP-only and chrooted
through the panel; nothing in the codebase currently serves FTP.
This change adds a first-class `ftp` bounded context that integrates
with a vetted pure-Rust FTP server (`libunftp`), chroots each account
to its site's document root, and authenticates against the same
identity store as the panel — no separate password DB.

## What Changes

- New `ftp` bounded context with an `FtpAccount` aggregate per site
  (username, home = site doc root, password, read/write/list quota,
  enabled state).
- A supervised FTP server (`libunftp`) bound to a configurable port
  (default 21) with TLS optional (default opportunistic via
  `OPENPANEL__FTP__TLS_MODE=StartTls`).
- Authenticator delegates to the existing `IdentityService` and a
  site-membership check: an account can only log into the site it
  was created for.
- All file operations are chrooted to the site root via the same
  canonicalize-once-per-request discipline the files module uses.
- Per-account bandwidth and connection limits.
- REST, CLI, and `/sites/{id}/ftp` web surface.

## Capabilities

### New Capabilities

- `ftp`: per-site FTP account lifecycle and supervised FTP server.

### Modified Capabilities

- `files`: file operations gain an FTP transport with the same
  chroot invariants.

## Impact

- Domain: `FtpAccount` aggregate.
- App: `FtpService`, `FtpAuthenticator` (delegates to
  `IdentityService`), `FtpServerSupervisor` (background task).
- API/CLI/web: `POST/GET/DELETE /api/v1/sites/{id}/ftp/accounts`,
  `openpanel ftp {create,list,disable,enable,delete}`,
  `/sites/{id}/ftp` page.
- Dependency: `libunftp` (Rust async FTP server, MIT).
- The supervisor only starts when an Owner enables FTP globally
  (`OPENPANEL__FTP__ENABLED=true`, default false).
