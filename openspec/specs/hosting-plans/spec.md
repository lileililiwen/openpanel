# hosting-plans Specification

## Purpose

Introduces hosting plans as the canonical bundle of feature
toggles and quota caps that govern what each user may do on the
panel. Plans compose with the `resource-quotas` and
`account-hierarchy` capabilities: a parent account can choose to
let its quota caps "float" with the sum of child caps, and a
User's effective cap is `min(role_default, plan_cap, parent_cap)`.

## Requirements

### Requirement: Hosting Plan Aggregate

The hosting-plans context SHALL model a `HostingPlan` aggregate carrying: a `PlanId` (ULID), a unique `name` (3..=64 chars), a `description`, an optional `prices: Vec<PlanPrice>` (display only — the panel never charges), a `features: BTreeMap<PlanFeature, PlanFeatureState>` map, a `PlanQuotas` structure, `allowed_apps: Vec<AppId>`, `allowed_php_runtimes: Vec<PhpRuntimeRef>`, a `PlanStatus` of `Active | Disabled`, `created_at`, `updated_at`, and `audit_meta`. `PlanQuotas` SHALL contain at minimum: `disk_bytes`, `bandwidth_bytes_per_month`, `max_sites`, `max_databases`, `max_mail_domains`, `max_mailboxes`, `max_cron_jobs`, `max_api_tokens`, `max_fleet_agents`. The aggregate MUST enforce unique names, MUST reject Disabled-then-re-enable of a plan whose name was reused, and MUST reject assignments when status is `Disabled`.

#### Scenario: Plan name uniqueness

- **WHEN** a user attempts to create a plan with `name="Basic"` while one already exists
- **THEN** creation fails with `HostingPlansError::DuplicateName`.

#### Scenario: Disabled plan refuses assignment

- **WHEN** an Owner submits an assignment for a plan whose status is `Disabled`
- **THEN** the assignment is rejected with `HostingPlansError::PlanDisabled`.

### Requirement: Plan Lifecycle

The system SHALL let Owners create, read, update, disable, clone, and delete plans. A plan whose `user_plan_assignments` table contains one or more rows MUST NOT be deletable; the response is `409 Conflict` with `plan_in_use`. Plan updates MUST NOT shrink quota caps below currently-assigned user consumption; the update is rejected with `HostingPlansError::WouldShrinkBelowUsage`.

#### Scenario: Delete a plan in use

- **WHEN** a plan has ≥ 1 active assignment
- **THEN** `DELETE /api/v1/hosting-plans/{id}` returns 409.

#### Scenario: Update shrinks below usage

- **WHEN** an Owner submits `quota_caps.disk_bytes = 1MB` and an assigned user has `disk_used = 100MB`
- **THEN** the update is rejected with `HostingPlansError::WouldShrinkBelowUsage`.

### Requirement: Plan Assignment

The system SHALL let Owners assign a `HostingPlan` to a single user via `POST /api/v1/hosting-plans/{id}/assign`, replacing any prior assignment. The operation is append-only; an audit event `UserAssignedToPlan{user_id, plan_id, assigned_by}` is recorded. An Owner MAY unassign via `POST /api/v1/hosting-plans/{id}/unassign`; the user then reverts to the role default.

#### Scenario: Assign replaces prior plan

- **WHEN** `assign` is called for a user already assigned to plan A with plan B
- **THEN** the prior assignment row is soft-deleted; a new row exists for plan B; audit `UserReassigned{from=A, to=B}` is recorded.

#### Scenario: Unassign reverts to role default

- **WHEN** an Owner calls `unassign` for a User-role principal
- **THEN** `User.hosting_plan_id` becomes `NULL` and the resolver returns the role default.

### Requirement: Plan Resolver

The `PlanResolver` SHALL return `EffectiveQuotas` for any user as `min(role_default, plan_cap, parent_cap)` along all axes. The resolver MUST be safe to call on every authenticated request and MUST cache results for at most 60 seconds, then re-read the source of truth.

#### Scenario: All three caps agree

- **WHEN** role default disk is 100GB, plan disk is 50GB, parent cap is 30GB
- **THEN** `EffectiveQuotas.disk_bytes == 30GB`.

#### Scenario: Role default governs when no plan

- **WHEN** `User.hosting_plan_id IS NULL` and the user is at root
- **THEN** `EffectiveQuotas` equals `role_default`.

### Requirement: Plan Audit and Event Surface

Every plan lifecycle mutation SHALL emit an audit event with `actor_id`, `plan_id`, the redacted diff, and an event kind from `{PlanCreated, PlanUpdated, PlanDisabled, PlanEnabled, PlanCloned, PlanAssigned, PlanReassigned, PlanUnassigned, PlanDeleteBlocked}`. The web UI SHALL provide a `/plans/{id}/history` page listing the events.

#### Scenario: History is paginated

- **WHEN** a plan has > 100 events
- **THEN** the history page shows the most recent 100 with a next-page cursor.
