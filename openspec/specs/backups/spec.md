# backups Specification

## Purpose

The backups bounded context covers plans, runs, manifests, integrity,
secret safety, and restore lifecycle. After this refinement, it also
owns the `ResourceKind`, `RestoreScope`, and `BackupTargetKind`
enums that the per-resource restore and the remote-target policy
depend on.

## Requirements

### Requirement: Resource Kind

The backups bounded context SHALL model a `ResourceKind` enum (`Site | Database | MailDomain | Mailbox | AuditLog | Configuration`). Every plan and every restore request carries a `ResourceKind`.

#### Scenario: Site restore

- **WHEN** `RestoreScope::new(ResourceKind::Site, json!({"site_id": "abc"}))` is called
- **THEN** the scope is constructed with `kind = Site`.

### Requirement: Restore Scope

The backups bounded context SHALL model a `RestoreScope { kind: ResourceKind, selector: serde_json::Value }`. The constructor rejects any non-object selector (the application layer validates per-kind fields).

#### Scenario: Non-object selector rejected

- **WHEN** `RestoreScope::new(ResourceKind::Site, json!("not-an-object"))` is called
- **THEN** the constructor returns `BackupRefineError::InvalidSelector`.

#### Scenario: Missing field

- **WHEN** `scope.require_str("site_id")` is called and the selector is empty
- **THEN** the call returns `BackupRefineError::MissingSelectorField`.

### Requirement: Backup Target Kind

The backups bounded context SHALL model a `BackupTargetKind` enum (`Local | OffsiteS3 | OffsiteRsync | OffsiteB2 | OffsiteWasabi`) and a `BackupTargetPolicy { kind, bucket?, rsync_url? }`. The S3 and rsync constructors reject empty payloads.

#### Scenario: S3 bucket must be non-empty

- **WHEN** `BackupTargetPolicy::s3("")` is called
- **THEN** the constructor returns `BackupRefineError::InvalidSelector`.

#### Scenario: rsync URL must be non-empty

- **WHEN** `BackupTargetPolicy::rsync("")` is called
- **THEN** the constructor returns `BackupRefineError::InvalidSelector`.

### Requirement: Behaviour Parity

The refinement introduces the new types without changing the existing `BackupPlan` / `BackupRun` lifecycle. The follow-on `add-offsite-backup-targets` change wires `BackupTargetPolicy` into the storage layer and the remote adapters.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `BackupTargetKindChanged` and `BackupRestoredScope` audit events. The bounded context as archived today owns the typed model and the validation rules.
