## ADDED Requirements

### Requirement: Job Scope and Role Permissions

Every cron `Job` SHALL have a `scope` enum of `System | PerSite | PerUser`. The `cron` service SHALL enforce a role matrix: Owners and Admins may create jobs of any scope; Users may create only `PerSite` or `PerUser` jobs. A request that violates the matrix MUST be rejected with `CronError::PermissionDenied` and audited as `CronPermissionDenied{actor_id, requested_scope}`.

#### Scenario: User requests a System job

- **WHEN** a principal with role `User` calls `POST /cron/jobs` with `scope=System`
- **THEN** the creation returns 403 with `permission_denied{axis=scope}` and an audit `CronPermissionDenied` event is recorded.

#### Scenario: Admin schedules a Site-scoped job

- **WHEN** an Admin schedules a job with `scope=PerSite` whose working directory resolves beneath a site they administrate
- **THEN** the job is persisted and the scope tag is stored on the row.

#### Scenario: Schedule outside owned sites

- **WHEN** a User submits a `PerSite` job whose working directory is outside every site they own
- **THEN** the request is rejected with `path_outside_owned_sites` and no audit body is recorded.

### Requirement: Per-User Cron Quotas

The system SHALL persist a `CronQuota` per user with at minimum the axes `max_concurrent`, `max_due_per_minute`, and `max_total`. Effective values are `max(global_default, plan_override_for_user)`. A job creation that would push `max_total` over the effective value MUST be rejected with `CronError::QuotaExceeded{axis="total"}` and audited as `CronQuotaBlocked{axis="total"}`.

#### Scenario: Plan override raises the cap

- **WHEN** global default `max_total=10` and a plan override sets it to 50 for a User
- **THEN** the User may schedule up to 50 jobs; attempts beyond 50 are rejected and audited.

#### Scenario: Concurrent in-flight limit

- **WHEN** a User has 4 jobs already running and `max_concurrent=4` becomes due again
- **THEN** the next pending run applies the job's own `OverlapPolicy` (skip, queue, or parallel); the scheduler MUST NOT spawn a 5th process.

#### Scenario: Per-minute run cap

- **WHEN** more than `max_due_per_minute` jobs become due within the same minute
- **THEN** excess jobs are deferred by at least 60s and audit `CronDeferredForRateLimit` is emitted.

### Requirement: User-Cron Surface

The system SHALL expose a `PerUser` cron page visible to User-role principals, scoped to jobs they own and limited to `PerUser` and `PerSite` scopes. The page SHALL run under the same session/CSRF rules as the rest of the web UI.

#### Scenario: User opens the cron page

- **WHEN** a User navigates to `/cron` on the web UI
- **THEN** the page lists only their jobs; the System tab is not rendered; the create form restricts to `PerUser` and `PerSite`.

#### Scenario: Admin opens the unified admin page

- **WHEN** an Owner opens `/cron/admin`
- **THEN** the page lists all scopes with badges and exposes quota overrides per user.
