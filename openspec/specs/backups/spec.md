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

The refinement SHALL introduce the new types without changing the existing `BackupPlan` / `BackupRun` lifecycle. The follow-on `add-offsite-backup-targets` change wires `BackupTargetPolicy` into the storage layer and the remote adapters.

#### Scenario: Existing lifecycle unchanged

- **WHEN** a plan is created, run, verified, and restored after the
        refinement
- **THEN** the `BackupPlan` / `BackupRun` state transitions are
        identical to the pre-refinement behaviour.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `BackupTargetKindChanged` and `BackupRestoredScope` audit events. The bounded context as archived today owns the typed model and the validation rules.

#### Scenario: Restore scope is audited

- **WHEN** a scoped restore completes
- **THEN** an audit `BackupRestoredScope` event records the restored
        resource kinds without payload content.

### Requirement: Server Snapshot Bundle

The system SHALL produce a single server-snapshot artifact bundling
panel metadata, sites, databases, and SSL certificate material
described by a versioned JSON manifest with per-entry SHA-256 hashes.
Private keys SHALL remain AES-256-GCM ciphertext inside the bundle and
the master key SHALL never be included.

#### Scenario: Bundle created

- **WHEN** an Admin creates a full snapshot on a host with one site,
        one database, and one issued certificate
- **THEN** the artifact contains a schema-versioned manifest listing
          all four resource kinds with matching hashes.

#### Scenario: Keys stay encrypted

- **WHEN** any snapshot artifact is inspected
- **THEN** SSL key entries contain ciphertext in the documented
          layout, never PEM plaintext, and the manifest contains no
          secret material.

### Requirement: Restore Preflight

Restoring a snapshot SHALL require a preflight step that checks
manifest-schema compatibility, warns on panel-version skew and
same-host restores, lists name collisions, and issues a single-use
confirmation token; restore without that token SHALL be refused before
any mutation.

#### Scenario: Schema too new

- **WHEN** preflight sees a manifest schema newer than supported
- **THEN** it fails with `SnapshotError::SchemaTooNew` and offers no
          token.

#### Scenario: Confirmation gate

- **WHEN** restore is called without the token from a completed
          preflight
- **THEN** nothing is written to disk or database.

### Requirement: Ordered Restore

Restore SHALL verify each entry's hash, then apply resources in
dependency order (databases, sites, certificates, metadata) and record
an audit event per applied resource plus a completion event.

#### Scenario: Tampered entry aborts

- **WHEN** an entry's bytes do not match its manifest hash
- **THEN** restore fails before applying that resource and previously
          applied resources are reported in the result.

### Requirement: Host-to-Host Migration

The system SHALL support migrating by exporting a full snapshot to an
attached offsite target and importing it on a fresh host through the
migration-importer driver contract, emitting a bootstrap command for
the new host.

#### Scenario: Migrate round-trip

- **WHEN** an operator runs migrate against target T, then runs the
          bootstrap command on a fresh host attached to T
- **THEN** the fresh host reports the same sites, databases, and
          certificates as the source, and audit events exist on both
          hosts.

### Requirement: Scheduled Snapshots and Retention

Snapshots SHALL be schedulable through the existing cron capability
and retention SHALL follow the account's hosting plan; pruning beyond
retention SHALL be audited.

#### Scenario: Retention enforced

- **WHEN** a plan allows 5 snapshots and a scheduled run creates the
          6th
- **THEN** the oldest snapshot is pruned and a `SnapshotPruned` audit
          event records which.

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

