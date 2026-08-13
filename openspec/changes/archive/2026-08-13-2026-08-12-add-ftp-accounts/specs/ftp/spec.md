## ADDED Requirements

### Requirement: Per-Site FTP Account

The system SHALL let an Owner create FTP accounts scoped to exactly one site. Each account carries a username (unique within the site), an argon2id password hash, the canonicalised site root as the home, optional per-account bandwidth and connection limits, a read-only flag, and an enabled state.

#### Scenario: Create an FTP account

- **WHEN** an Owner posts `{ username, password }` to a site's FTP endpoint
- **THEN** the account is persisted, the plaintext password is shown to the creator exactly once, and the FTP server picks it up without restart.

#### Scenario: Reject duplicate username

- **WHEN** an Owner tries to create an FTP account whose username already exists for that site
- **THEN** creation fails with 409 and no account is persisted.

#### Scenario: Disable an account

- **WHEN** an Owner disables an FTP account
- **THEN** subsequent login attempts return 530 and an `FtpLoginDenied` audit event is recorded.

### Requirement: Authentication Using Identity Password Primitives

The FTP server SHALL hash and verify each site-scoped FTP credential with the same argon2id password primitive used by identity. An FTP credential MUST resolve only within its persisted site scope; it MUST NOT grant access to another site's root.

Because FTP supplies no separate site identifier and usernames are unique only within a site, the wire username SHALL be the unambiguous qualified form `<site-uuid>@<account-username>`.

#### Scenario: Account logs into its site

- **WHEN** a valid enabled FTP credential opens a session for its qualified site
- **THEN** the server returns 230 and records the login.

#### Scenario: Cross-site login denied

- **WHEN** a credential is presented with a different site qualifier
- **THEN** the server returns 530 and audits `FtpLoginDenied`.

### Requirement: Chrooted Filesystem Access

Every FTP file operation MUST be chrooted to the account's site root using the same canonicalize-once-per-request discipline the files module uses. Path traversal, absolute paths, and symlink escape MUST be denied with a redacted audit row.

#### Scenario: Path traversal

- **WHEN** an FTP client requests `../../etc/passwd`
- **THEN** the server returns 550 and the path is logged redacted.

#### Scenario: Symlink escape

- **WHEN** a symlink under the site root points to `/etc/shadow`
- **THEN** the server returns 550 and audits `FtpChrootEscape`.

### Requirement: Transport Security

The embedded FTP server SHALL support explicit `StartTls` (default) and `None`. `None` is rejected unless the bind address is loopback. `ImplicitTls` configuration SHALL fail closed with a validation error because the selected `libunftp` transport does not implement implicit FTPS. The server SHALL refuse any login that completes before TLS is negotiated when TLS is enabled.

#### Scenario: Plain login refused under StartTls

- **WHEN** a client sends `USER` and `PASS` over a control channel before `AUTH TLS`
- **THEN** the server closes the control channel and audits `FtpTlsRequired`.

### Requirement: Per-Account and Global Limits

The FTP server SHALL enforce per-account concurrent connection and per-session bandwidth limits, plus a global connection ceiling. Surplus attempts return 421 and are audited. Idle and control-channel timeouts are bounded.

#### Scenario: Concurrent limit

- **WHEN** an account has 4 active sessions and a 5th login is attempted
- **THEN** the server returns 421 and audits `FtpConcurrentLimit`.

### Requirement: FTP Server Supervisor

A supervisor background task SHALL start the FTP server only when an Owner enables it (`OPENPANEL__FTP__ENABLED=true`, default false). On bind failure the supervisor records the OS error and refuses to start. On a runtime panic the supervisor restarts with backoff up to a hard cap.

#### Scenario: Bind failure

- **WHEN** the configured port is already in use
- **THEN** the supervisor records `FtpBindFailed` with the OS error and exits without retrying the occupied listener.

#### Scenario: Supervisor restart

- **WHEN** the FTP server panics
- **THEN** the supervisor restarts it with backoff and records each attempt.

### Requirement: FTP Surfaces

REST, CLI, and `/sites/{id}/ftp` web surfaces SHALL support account CRUD, the password-shown-once contract, and a live sessions list. Browser mutations MUST enforce CSRF. Plaintext passwords SHALL never be returned by `GET` and SHALL NEVER appear in any log line, audit row, or API response.

#### Scenario: Manage an FTP account through supported surfaces

- **WHEN** an Owner creates, lists, disables, enables, and deletes an FTP account through REST, CLI, or the web surface
- **THEN** each surface enforces the same site ownership and secret-handling rules, browser mutations require valid CSRF, and only the create response indicates that the submitted password was shown once
