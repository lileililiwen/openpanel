## ADDED Requirements

### Requirement: Scheduled Job Aggregate

The system SHALL persist named jobs with owner, five-field schedule, IANA timezone, job kind, enabled state, timeout, overlap policy, and next-run timestamp. Invalid expressions, unknown timezones, unsafe working directories, empty executables, and non-positive timeouts MUST be rejected.

#### Scenario: Create a valid job

- **WHEN** an authorized caller creates `/usr/bin/php` with argument `artisan`, schedule `0 2 * * *`, timezone `Asia/Shanghai`, and a site-owned working directory
- **THEN** the job is persisted with its next UTC run and no shell interpolation

#### Scenario: Reject traversal

- **WHEN** a working directory resolves outside every site owned by the job owner
- **THEN** creation fails and no job is persisted

### Requirement: Due Job Execution

The scheduler SHALL transactionally lease due jobs, execute argv without a shell, enforce timeout/output/concurrency limits, apply the overlap policy, record terminal status and timestamps, and recover expired leases as interrupted runs.

#### Scenario: Overlapping run is skipped

- **WHEN** a job with `skip` policy becomes due while its previous lease is active
- **THEN** no second process starts and a skipped run is recorded

#### Scenario: Timed-out process

- **WHEN** a process exceeds its configured timeout
- **THEN** the adapter terminates it and records `TimedOut` without blocking later schedules

### Requirement: Cron Authorization and Surfaces

REST, CLI, and web surfaces SHALL support create, list, get, update, enable, disable, delete, run-now, run-list, and run-detail. Users SHALL access only their own jobs; Admins SHALL manage User-owned jobs; only Owners SHALL manage Owner-owned jobs. Every mutation and manual run SHALL be audited without command arguments or output.

#### Scenario: User lists jobs

- **WHEN** a User requests `GET /api/v1/cron/jobs`
- **THEN** only that user's jobs are returned

#### Scenario: Manual execution

- **WHEN** an authorized caller invokes `openpanel cron run --id <id>`
- **THEN** one leased execution starts and the CLI returns its run ID and status

### Requirement: Execution History Retention

The system SHALL retain bounded run metadata and capped stdout/stderr, prune records older than the configured retention, escape output in HTML, and never include inherited environment values.

#### Scenario: View failed run

- **WHEN** an authorized caller opens a failed run detail
- **THEN** status, timestamps, exit code, and capped escaped output are returned

#### Scenario: Retention pruning

- **WHEN** cleanup runs with a 30-day policy
- **THEN** completed runs older than 30 days are deleted while active leases remain
