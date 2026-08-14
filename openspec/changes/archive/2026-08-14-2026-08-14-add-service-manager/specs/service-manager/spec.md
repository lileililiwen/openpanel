## ADDED Requirements

### Requirement: List Services

`GET /admin/services` SHALL return the allow-listed system services
with their current `status`, boot `enabled` flag, and recent journal
log lines. Listing SHALL be Admin-gated and MUST NOT mutate state.

#### Scenario: List allowed services

- **WHEN** an Admin fetches `/admin/services`
- **THEN** each returned `ServiceInfo` has `name`, `status`,
        `enabled`, and `recent_logs`; only allow-listed units appear.

#### Scenario: Non-admin denied

- **WHEN** a non-Admin caller requests the list
- **THEN** the response is `403` and no data is returned.

### Requirement: Perform Service Action

`POST /admin/services/{name}/{action}` SHALL perform `start`, `stop`,
`restart`, `enable`, or `disable` on an allow-listed service. The
action SHALL be Admin-gated and audited; the unit name MUST be
validated against the allow-list before any `systemctl` call.

#### Scenario: Restart a service

- **WHEN** an Admin posts `/admin/services/nginx/restart`
- **THEN** `systemctl restart nginx` runs, status is refreshed, and an
        audit `ServiceAction{name, action}` is recorded.

#### Scenario: Unknown unit rejected

- **WHEN** the `{name}` is not in the allow-list
- **THEN** the request is rejected with
        `ServiceError::NotAllowed` and no `systemctl` call occurs.

#### Scenario: Bad action verb rejected

- **WHEN** `{action}` is not one of the five allowed verbs
- **THEN** the request is rejected with
        `ServiceError::UnknownAction` and no command runs.

### Requirement: Recent Logs

The system SHALL surface the most recent journal lines for a listed
service so operators can diagnose failures without SSH.

#### Scenario: Logs available

- **WHEN** an Admin lists services
- **THEN** each `ServiceInfo.recent_logs` contains the last N journal
        lines for that unit (no secret material beyond standard output).
