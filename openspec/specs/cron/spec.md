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

The follow-on implementation SHALL emit `CronScopeDenied` audit events when `role_allows_scope` returns `false`. The bounded context as archived today owns the typed model and the pure-function helpers.
