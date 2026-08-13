# Add per-site FTP accounts — Tasks

## 1. Testing

- [x] 1.1 Unit tests for `FtpAccount` validation: username format,
      password strength, site-root canonicalization, enabled
      transitions.
- [x] 1.2 Property tests: chroot rejection is total; symlink
      escape is denied; per-account limits enforced.
- [x] 1.3 Service tests with mock identity, mock storage, and
      mock libunftp client for auth success/failure, read-only
      enforcement, and session-limit enforcement.
- [x] 1.4 Integration: live `libunftp` over StartTls against an
      in-process axum server; upload + download round-trip;
      chroot traversal attempt denied.
- [x] 1.5 CLI E2E: `openpanel ftp {create,list,disable,enable,
      delete}` with a real ftp client smoke test.
- [x] 1.6 Web: `/sites/{id}/ftp` account list, create form
      (CSRF), and "password shown once" banner.

## 2. Domain and Application

- [x] 2.1 Implement `FtpAccount` aggregate in
      `crates/openpanel-domain/src/ftp/`.
- [x] 2.2 Add SQLite migrations and `SqliteFtpRepository`.
- [x] 2.3 Implement `FtpService` (create/list/enable/disable/
      delete) and the chroot-aware storage backend.
- [x] 2.4 Implement `FtpAuthenticator` that delegates to
      `IdentityService` and the site-membership check.
- [x] 2.5 Add `FtpServerSupervisor` background task that binds
      `libunftp` on the configured port with StartTls by default.

## 3. Adapters and UI

- [x] 3.1 Add REST routes under `/api/v1/sites/{id}/ftp/accounts`.
- [x] 3.2 Add `openpanel ftp` CLI subcommands.
- [x] 3.3 Add `/sites/{id}/ftp` web pages with CSRF.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: create an FTP account, log in with
      FileZilla, upload a file, verify it appears under the
      site root, attempt to traverse out of the chroot and
      confirm denial.
- [x] 4.4 Archive with `openspec archive 2026-08-12-add-ftp-accounts`.
