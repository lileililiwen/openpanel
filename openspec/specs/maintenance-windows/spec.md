# maintenance-windows Specification

## Purpose
TBD - created by archiving change 2026-08-13-add-scheduled-maintenance-windows. Update Purpose after archive.
## Requirements
### Requirement: MaintenanceWindow Lifecycle

The system SHALL let Owners create, list, update, and delete
`MaintenanceWindow` records. Each window has a typed
`affected_action_classes` list, an `allow_overrides` flag, an
`allow_reads` flag, and a `notification_recipient` (the user
or role to be warned before the window starts).

#### Scenario: Create a window

- **WHEN** an Owner submits `{ starts_at, ends_at,
        affected_action_classes: ["software-install","backup-restore"],
        allow_reads: true }`
- **THEN** the window is persisted, a pre-start notification is
        queued (default 30 minutes before), and audit
        `MaintenanceWindowCreated` is recorded.

#### Scenario: Overlapping windows

- **WHEN** an Owner tries to create a window that overlaps an
        existing one in the same action class
- **THEN** the request is rejected with `MaintenanceError::Overlap` and the offending window id is returned.

#### Scenario: Past window cannot be created

- **WHEN** `ends_at < now()`
- **THEN** the request is rejected with `MaintenanceError::WindowInPast`.

### Requirement: Enforcer Integration

`MaintenanceEnforcer::check(class, actor)` MUST be consulted by
every bounded context that issues a destructive operation
listed in `affected_action_classes`. The check fails closed
when the actor is not exempted and the current time is within
the window.

#### Scenario: Destructive blocked

- **WHEN** `BackupRestore` is in `affected_action_classes` and a restore is attempted during the window
- **THEN** the call fails with `MaintenanceError::InMaintenanceWindow{class, window_id}` and audit `MaintenanceBlocked{class, window_id}` is recorded.

#### Scenario: Reads allowed

- **WHEN** `allow_reads=true` and a read-only call is attempted
- **THEN** the enforcer permits the call.

### Requirement: Override

`POST /admin/maintenance/override` SHALL accept
`{ window_id, action_class, confirmed_at, reason }` and SHALL
return a single-use override token valid for ≤ 30 minutes
when `allow_overrides=true` for that window. The override
MUST be recorded on consumption and SHALL be consumed exactly
once.

#### Scenario: Override issued

- **WHEN** an Owner submits a fresh `override` body within the window and `allow_overrides=true`
- **THEN** a `MaintenanceOverrideToken{expires_at = now + 30min}` is returned and audit `MaintenanceOverrideIssued` is recorded.

#### Scenario: Replay refused

- **WHEN** the same token is consumed a second time
- **THEN** the request is rejected with `OverrideTokenConsumed`; audit `MaintenanceOverrideReplayAttempt` records the attempt.

#### Scenario: Override allowed = false

- **WHEN** `allow_overrides=false`
- **THEN** the override endpoint returns 403 with `override_disallowed`.

### Requirement: Notification Pre-Start

The system SHALL enqueue a pre-start notification to the
window's recipient 30 minutes before `starts_at` (configurable
per window). The notification SHALL use the
`notification-channels` capability if available; otherwise it
falls back to a panel audit event.

#### Scenario: Pre-start notification

- **WHEN** a window begins in 30 minutes and the channel list is configured
- **THEN** a notification is delivered with the window label,
        start time, and the action classes affected.

#### Scenario: Channel list absent

- **WHEN** no notification channel is configured
- **THEN** the pre-start event is recorded in audit only.

### Requirement: Audit Hygiene

All maintenance-related events MUST be audited. The `reason`
field in an override call MAY include user-supplied text but
MUST NOT include secret material; the panel SHALL refuse an
override whose `reason` contains a recognised secret pattern.

#### Scenario: Secret pattern detected

- **WHEN** an Owner submits `reason = "BEGIN PRIVATE KEY ----- …"`
- **THEN** the request is rejected with `SecretPatternInReason` and no override is issued.

#### Scenario: Reason recorded

- **WHEN** an override succeeds
- **THEN** the audit `MaintenanceOverrideConsumed` records the action class, window id, and the reason text verbatim (which is the caller's responsibility to redact).

