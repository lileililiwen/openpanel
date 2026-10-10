# cron Specification

## Purpose

The cron bounded context covers schedules, jobs, execution history
and overlap policy. After this refinement, it also owns the
`JobScope`, `CronQuota`, and role permission checker that
constrain which roles may create which kinds of jobs.
## Requirements
### Requirement: Job Scope

The cron bounded context SHALL model a `JobScope` (`System |
PerSite | PerUser`). The role permission checker MUST allow
each role a specific subset of scopes:

- `Owner` may create `System`, `PerSite`, and `PerUser` jobs.
- `Admin` may create `PerSite` and `PerUser` jobs.
- `User` may create `PerSite` and `PerUser` jobs but NOT `System` jobs.

#### Scenario: User cannot create system jobs

- **WHEN** `role_allows_scope(Role::User, JobScope::System)` is called
- **THEN** the function returns `false`.

#### Scenario: Owner can create any scope

- **WHEN** `role_allows_scope(Role::Owner, scope)` is called for any `scope`
- **THEN** the function returns `true`.

### Requirement: Per-User Quota

The cron bounded context SHALL model a `CronQuota` with `max_concurrent`, `max_due_per_minute`, and `max_total`. The `CronQuota::effective(global, user)` helper computes the per-axis maximum of a global default and a per-user override.

The `check_quota` helper validates the current state against the effective quota and returns `QuotaExceededTotal`, `QuotaExceededConcurrent`, or `QuotaExceededPerMinute` as appropriate.

#### Scenario: Total overflow

- **WHEN** `total_active >= max_total`
- **THEN** `check_quota` returns `CronScopeError::QuotaExceededTotal`.

#### Scenario: Concurrent overflow

- **WHEN** `concurrent_active >= max_concurrent`
- **THEN** `check_quota` returns `CronScopeError::QuotaExceededConcurrent`.

### Requirement: Working Directory and Executable Allow-List

The cron bounded context SHALL expose `is_under_owned_site(workdir, owned_roots)` and `is_executable_allowed(executable, allow_list)` helpers. Both are pure functions used by the scheduler when constructing a job.

#### Scenario: Working directory not under owned site

- **WHEN** `workdir` does not start with any of the owned roots
- **THEN** `is_under_owned_site` returns `false`.

#### Scenario: Executable not in allow-list

- **WHEN** `executable` is not in the `allow_list`
- **THEN** `is_executable_allowed` returns `false`.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `CronScopeDenied` audit events when `role_allows_scope` returns `false`.

#### Scenario: Out-of-scope job access is audited

- **WHEN** a caller whose role disallows the job's scope reads or
        mutates that job
- **THEN** an audit `CronScopeDenied` event records the caller and job
        id without the command arguments. The bounded context as archived today owns the typed model and the pure-function helpers.

### Requirement: Cron Preset Types

The cron surface MUST offer 8 Baota preset types — Shell, Backup-Site,
Backup-DB, Cut-Log, Backup-Dir, Sync-Time, Free-RAM, Access-URL — each
compiling to the existing `CronInput` through `CronService` scope, quota,
and executable allow-list checks. Presets MUST NOT bypass `JobScope` RBAC,
`check_quota`, `is_under_owned_site`, or `is_executable_allowed`.

#### Scenario: Preset compiles through guards

- **WHEN** an authorized user creates a Backup-DB preset with a valid cycle
- **THEN** the compiled job passes scope/quota/allow-list checks and is
  created with an allow-listed executable and scoped workdir.

#### Scenario: Out-of-scope preset refused

- **WHEN** a User creates a System-scoped preset
- **THEN** creation is refused with `CronScopeDenied` audit and no job is
  created.

### Requirement: Human Cycle Picker

The creation form MUST offer a human cycle picker — Daily, N-Days, Hourly,
N-Hours, N-Minutes, Weekly, Monthly, N-Seconds — compiling to the stored
schedule expression. Invalid N values MUST be rejected before creation.

#### Scenario: Cycle compiles

- **WHEN** a user picks Every-N-Hours with N=6
- **THEN** the stored expression encodes a 6-hour cadence and the job runs
  on that cadence.

### Requirement: Execute-Now And Log View

Every job MUST offer execute-now (immediate `run_now`) and a per-run log
view rendering escaped stdout/stderr. Log output MUST be safely escaped.

#### Scenario: Execute-now round-trip

- **WHEN** an authorized user triggers execute-now on a job
- **THEN** a run is created and its log view shows the escaped output.

### Requirement: Failure Reminders Over Existing Channels

Failure reminders MUST subscribe the job's terminal-failure event to an
already-registered notification channel (SMTP/webhook). The change MUST NOT
add a notification transport.

#### Scenario: Failure notifies

- **WHEN** a subscribed preset job ends in failure
- **THEN** a delivery is queued on the bound channel and audited.

### Requirement: Chained Task Scheduling

A job MUST optionally name a next job triggered on its terminal event
(event-triggers-next), enqueued exactly once per terminal event.

#### Scenario: Chain fires once

- **WHEN** job A completes with a chain link to job B
- **THEN** job B is enqueued once, with no duplicate on redelivery.

