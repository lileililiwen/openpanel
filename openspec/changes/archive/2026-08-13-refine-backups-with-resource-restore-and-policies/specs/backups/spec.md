## ADDED Requirements

### Requirement: Typed Restore Scope

Every restore request SHALL carry a `RestoreScope` discriminated union selecting exactly one resource and one timestamp. The kind-specific selector MUST be validated: `Site` requires a known `site_id`, `Database` a known `db_id`, `MailDomain` a syntactically valid hostname, `Mailbox` an email address with a known domain, `AuditLog` a `from` strictly earlier than `to`. A restore whose selector fails validation MUST be rejected with `BackupsError::InvalidRestoreScope` before any storage I/O.

#### Scenario: Site restore request

- **WHEN** an authorised caller submits a restore request with `scope=Site{site_id=<known>, at=<ts>}`
- **THEN** the request is staged as a plan preview; no restore actions run until `mode=execute` and a fresh `confirmed_at` is supplied.

#### Scenario: Unknown site id

- **WHEN** the `site_id` does not exist in the panel
- **THEN** the request is rejected with `InvalidRestoreScope` and no run is created.

#### Scenario: Audit-log range inverted

- **WHEN** `AuditLog{from, to}` has `from >= to`
- **THEN** the request is rejected with `InvalidRestoreScope{reason="audit_log_inverted_range"}`.

### Requirement: Backup Target Kind

Every plan SHALL record a `BackupTargetKind` of `Local | OffsiteS3 | OffsiteRsync | OffsiteB2 | OffsiteWasabi`. The kind-specific payload is rejected at plan creation if mandatory fields are missing (S3 bucket and region; rsync host, user, ssh_key_id; etc.). A plan MUST NOT change its `target_kind` after first run; changing target kind requires creating a new plan and either keeping or archiving the old one.

#### Scenario: S3 plan validated

- **WHEN** an Owner creates a plan with `target_kind=OffsiteS3` and a non-empty bucket and a known region
- **THEN** the plan is persisted and `target_kind='offsite_s3'` is recorded.

#### Scenario: Empty bucket name

- **WHEN** an Owner submits `target_kind=OffsiteS3` with an empty bucket
- **THEN** creation fails with `InvalidTargetKind{reason="empty_bucket"}`.

#### Scenario: Target kind is immutable per plan

- **WHEN** a plan is updated
- **THEN** any change to `target_kind` is rejected with `TargetKindChangeForbidden`; the existing plan rows keep their original target kind.

### Requirement: Per-Plan Schedule Constraints

Plans SHALL record the schedule (cron expression and timezone), retention (count or duration), and target in a single record. A plan whose schedule is invalid (cron syntax, timezone unknown) MUST be rejected at creation. The plan MUST be revisited when the cron expression changes; pre-existing runs are not retroactively re-evaluated.

#### Scenario: Invalid cron

- **WHEN** an Owner creates a plan with `schedule="0 25 * * *"` (invalid hour)
- **THEN** the request is rejected with `BackupsError::InvalidSchedule` and no plan is persisted.

#### Scenario: Unknown timezone

- **WHEN** an Owner submits a plan with `timezone="Mars/Olympus_Mons"`
- **THEN** the request is rejected with `BackupsError::UnknownTimezone`.
