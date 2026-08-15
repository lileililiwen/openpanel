## ADDED Requirements

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
