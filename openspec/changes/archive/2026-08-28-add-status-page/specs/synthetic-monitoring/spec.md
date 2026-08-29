## ADDED Requirements

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
