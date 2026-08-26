# app-runtimes Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-non-php-runtimes. Update Purpose after archive.
## Requirements
### Requirement: Select Runtime and Version

`PUT /sites/{id}/runtime` SHALL let an authorised caller select a
non-PHP runtime (`node`, `python`, `ruby`, `go`) for a site with a
pinned version from the allowed list. The runtime SHALL run inside the
site chroot under the site user, and the app port SHALL bind to
loopback only. An unsupported version pin SHALL be rejected.

#### Scenario: Runtime set succeeds

- **WHEN** an Owner puts `PUT /sites/{s1}/runtime` with
        `{ kind: "node", version: "20" }`
- **THEN** a `SiteRuntime` row exists, a per-user supervisor unit is
        written, nginx proxies to `127.0.0.1:APP_PORT`, and audit
        `RuntimeChanged{kind, version}` records kind + version only.

#### Scenario: Unsupported version rejected

- **WHEN** a version outside the allowed pin list is requested
- **THEN** the request is rejected with
        `RuntimeError::UnsupportedVersion` and no unit is written.

### Requirement: Read Runtime

`GET /sites/{id}/runtime` SHALL return the selected runtime kind, pinned
version, app port, and current status without exposing secrets.

#### Scenario: Read current runtime

- **WHEN** an Owner gets `GET /sites/{s1}/runtime`
- **THEN** the response contains kind, version, app port, and status,
        and contains no credentials or source paths.

### Requirement: Runtime Lifecycle Control

`GET /sites/{id}/runtime/logs` (with `action` `start|stop|restart`)
SHALL drive the per-user supervisor unit and synchronise
`RuntimeStatus`; `action=tail` SHALL stream captured logs (capped, no
secrets). A crash SHALL surface as `Crashed` after supervisor marks the
unit failed.

#### Scenario: Start brings app online

- **WHEN** an Owner issues `start` on a stopped site
- **THEN** the supervisor unit starts, status becomes `Running`, and
        nginx proxies live traffic to the app port.

#### Scenario: Stop takes app offline

- **WHEN** an Owner issues `stop`
- **THEN** the unit stops, status becomes `Stopped`, and nginx returns
        `502` until it is started again.

#### Scenario: Logs tail is capped and secret-free

- **WHEN** an Owner requests `action=tail`
- **THEN** returned log lines are capped in length and contain no
        environment secrets or private keys.

### Requirement: Runtime Environment Variables

An authorised caller SHALL manage an ordered set of environment
variables per runtime. Keys SHALL match `[A-Za-z_][A-Za-z0-9_]*`
(≤128 chars), reserved keys SHALL be rejected, and sets beyond 128
variables or 64 KiB total SHALL be rejected at validation.

#### Scenario: Values reach the process

- **WHEN** an Owner sets `LOG_LEVEL=debug` and restarts the runtime
- **THEN** the application process observes `LOG_LEVEL=debug` in its
        environment.

#### Scenario: Reserved key rejected

- **WHEN** a caller defines `PATH`
- **THEN** validation fails with `EnvError::ReservedKey` and nothing
        is persisted.

### Requirement: Secret Variables

Variables flagged as secrets SHALL be stored only as AES-256-GCM
ciphertext, SHALL be written into a 0600 environment file inside the
runtime's jail referenced by `EnvironmentFile=`, and SHALL never be
returned by any surface, written to logs, or embedded in the supervisor
unit text.

#### Scenario: Write-only secret

- **WHEN** a secret variable is stored and later listed via API, CLI,
        or web
- **THEN** responses contain its key and `secret: true` but never its
        value.

#### Scenario: Unit stays clean

- **WHEN** any runtime with secret variables is rendered
- **THEN** the supervisor unit contains no secret plaintext; secrets
        appear only in the jailed 0600 env file.

### Requirement: Pending-Restart Semantics

Environment changes SHALL take effect on the next runtime restart;
surfaces SHALL expose a pending-vs-applied indicator that clears when
the existing restart action completes.

#### Scenario: Pending cleared by restart

- **WHEN** an Owner updates the environment then restarts the runtime
- **THEN** the pending indicator is false afterwards and the process
        runs with the new set.

