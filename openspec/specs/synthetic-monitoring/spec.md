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

### Requirement: Public Status Page

The system SHALL publish an opt-in, unauthenticated status page at a
random slug URL showing the current state of published checks and a
90-day daily uptime history. A disabled or unknown slug SHALL render
an indistinguishable 404. The page SHALL NOT expose check target URLs
or internal identifiers beyond admin-chosen labels.

#### Scenario: Published check visible

- **WHEN** an Admin publishes check `c1` with label "Website" and a
        visitor opens `/status/<slug>`
- **THEN** the page shows "Website" with its current state badge and
          history bars, and contains no trace of `c1`'s target URL.

#### Scenario: Disabled page hidden

- **WHEN** the status page is disabled
- **THEN** `/status/<slug>` returns the same 404 body as an unknown
          slug.

### Requirement: Incident History

The page SHALL derive incidents from check-result transitions:
consecutive non-OK results form one incident that closes on the first
OK result; unresolved incidents are shown as ongoing.

#### Scenario: Transition forms one incident

- **WHEN** results go OK → Degraded → Down → Up for a check
- **THEN** exactly one incident is listed spanning from the first
          Degraded result to the first subsequent OK result.

### Requirement: Anonymous Access Guardrails

The public route SHALL be rate-limited per source IP and SHALL send
cache headers allowing short-lived shared caching; reads SHALL NOT be
written to the audit trail.

#### Scenario: Rate limited

- **WHEN** a client exceeds the configured request rate
- **THEN** further responses are 429 with a Retry-After header.

### Requirement: Email Subscription

Visitors MAY subscribe by email to incident notifications; the
subscription SHALL require double opt-in through the existing
notification dispatcher and SHALL be cancellable.

#### Scenario: Double opt-in

- **WHEN** an address subscribes but has not confirmed
- **THEN** no incident mail is sent until confirmation completes.

### Requirement: Policy Administration

Admins SHALL manage the page (enable/disable, slug regeneration,
per-check publish toggles and labels) via API, CLI, and web; every
policy mutation SHALL be audited.

#### Scenario: Slug regeneration

- **WHEN** an Admin regenerates the slug
- **THEN** the old URL returns 404 thereafter and the change is
          audited.

