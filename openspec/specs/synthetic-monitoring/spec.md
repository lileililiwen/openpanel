# synthetic-monitoring Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-synthetic-monitoring. Update Purpose after archive.
## Requirements
### Requirement: Define Synthetic Check

The system SHALL let an authorised caller create an external synthetic
check of kind HTTP(S) GET, TCP connect, or SSL certificate-expiry for a
site or host URL. The probe target MUST be operator-defined; no probe
SHALL execute caller-supplied commands. A created check SHALL be
schedulable by an interval.

#### Scenario: Create HTTP check

- **WHEN** an Admin posts `POST /monitoring/checks` with
        `kind=Http`, a `target` URL, and `expected_status=200`
- **THEN** a `SyntheticCheck` row exists, `enabled=true`, and an audit
        `SyntheticCheckCreated{kind, target}` is recorded.

#### Scenario: Reject malformed target

- **WHEN** the `target` is empty or not a valid URL/host:port
- **THEN** the request is rejected with
        `SyntheticCheckError::InvalidTarget` and no row is created.

### Requirement: Run Probe

`GET`/`POST /monitoring/checks/{id}/run` SHALL execute the check's
probe immediately and store a `CheckResult` with status, latency, and a
detail hash (never the full response body). A forced run SHALL be
throttled so duplicate runs within a short window are not created.

#### Scenario: Successful HTTP probe

- **WHEN** a run is forced on a reachable HTTP check returning 200
- **THEN** a `CheckResult{status=ok, latency_ms}` is stored and the
        check's `last_run_id` is updated.

#### Scenario: Throttled duplicate run

- **WHEN** a second forced run arrives within the throttle window
- **THEN** no new `CheckResult` is created and `429` is returned.

### Requirement: SSL Expiry Warning

An SSL-expiry check SHALL perform a TLS handshake, read the
certificate `not_after`, and compute remaining validity. When remaining
time is within `warn_before_secs`, the result status SHALL be `warn`;
when expired, `fail`.

#### Scenario: Certificate near expiry warns

- **WHEN** a certificate expires within the configured warning window
- **THEN** the `CheckResult` status is `warn` and an alert is emitted
        via notification-channels.

#### Scenario: Expired certificate fails

- **WHEN** the certificate `not_after` is in the past
- **THEN** the `CheckResult` status is `fail` and an alert is emitted
        via notification-channels.

### Requirement: Alert On Failure

When a scheduled or on-demand check transitions to `fail` or `warn`,
the system SHALL emit an alert through `notification-channels`. Alerts
SHALL carry the check label and status only, never response bodies.

#### Scenario: Failure alerts

- **WHEN** a TCP check cannot connect within `timeout_ms`
- **THEN** a `fail` result is stored and an alert is routed through
        notification-channels with the label and status.

