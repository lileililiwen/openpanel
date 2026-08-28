## ADDED Requirements

### Requirement: Restore Drill Execution

The system SHALL execute a restore drill by restoring a chosen backup
into an isolated sandbox (temporary docroot plus a throwaway
suffixed database) using the production restore pipeline, running
per-resource-kind assertions, and always tearing down the sandbox —
including on failure.

#### Scenario: Healthy backup passes

- **WHEN** a drill runs on a backup containing a site, a database, and
        certificate material
- **THEN** all assertions pass, the report is persisted, and no
          sandbox directories or databases remain.

#### Scenario: Failure tears down

- **WHEN** any assertion fails mid-drill
- **THEN** the sandbox is still torn down and the report records the
          failing kinds.

### Requirement: Drill Assertions

Assertions SHALL cover at minimum: database dumps apply and yield at
least one table; site archives extract to a non-empty docroot with a
parsable manifest; encrypted key material decrypts under the local
master key. Assertion details SHALL contain no secret material.

#### Scenario: Corrupt dump detected

- **WHEN** the stored SQL dump cannot be applied
- **THEN** the database assertion fails with a stable error code and
          the drill outcome is Failed.

### Requirement: Drill Scheduling, History, and Alerting

Drills SHALL be schedulable via the cron capability, SHALL retain the
most recent 20 reports per backup, and SHALL notify subscribed
channels when a drill fails.

#### Scenario: Scheduled failure alerts

- **WHEN** a scheduled drill fails
- **THEN** a `DrillFailed` notification is dispatched and the report
          is queryable from the history endpoint.

### Requirement: Drill Surfaces

Operators SHALL trigger drills on demand, inspect reports, and manage
schedules via API, CLI, and web; every drill start and completion
SHALL be audited.

#### Scenario: On-demand round-trip

- **WHEN** an operator POSTs a drill then GETs the latest report
- **THEN** the report shows outcome, per-assertion results, duration,
          and the artifact id.
