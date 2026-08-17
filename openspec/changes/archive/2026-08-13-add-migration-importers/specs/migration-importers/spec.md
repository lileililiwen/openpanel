## Purpose

Provides an import pipeline so that operators moving from
cPanel or Baota can bring their existing sites, databases,
mailboxes, DNS records, and users into OpenPanel without
rebuilding by hand. Drivers sniff the input format, preview
the migration, run it under typed confirmation, and support a
24-hour rollback window.

# migration-importers Specification

## Requirements

### Requirement: Driver Sniffing and Selection

The system SHALL ship migration drivers for `cpanel-pkgacct`,
`cpanel-legacy-backup`, `baota-backup`, and a panel-emitted
`tar-with-json-manifest`. The driver registry SHALL sniff
unknown inputs by inspecting the archive structure and SHALL
refuse to proceed if zero drivers match. A driver MAY be
forced via `driver_hint`; forced selections still MUST pass
the same sniff test.

#### Scenario: Sniff detects cPanel

- **WHEN** a tarball matches the cPanel `pkgacct` layout
- **THEN** the driver registry chooses `cpanel-pkgacct` and audit `MigrationDriverSelected` records the driver.

#### Scenario: Sniff no match

- **WHEN** an unknown input is uploaded
- **THEN** the response is `415 UnsupportedArchive` with a redacted hint identifying the offending bytes.

#### Scenario: Force hint rejected

- **WHEN** an operator forces a driver that does not match the sniff
- **THEN** the preview is rejected with `driver_hint_mismatch`.

### Requirement: Migration Preview

The preview SHALL produce a typed `MigrationPlan` containing
every resource the import would create (sites, dbs, mailboxes,
dns records, users), every conflict (resource-name collisions),
and every warning (e.g. an unmappable cron entry). The preview
MUST run without committing any state.

#### Scenario: Preview is read-only

- **WHEN** an Owner POSTs `preview`
- **THEN** the response is the plan; no DB rows are written; no subprocess runs.

#### Scenario: Conflicts highlighted

- **WHEN** the source contains a user whose username exists in the panel
- **THEN** the plan highlights `conflicts: [{ resource: "user", name: "alice", resolution: "rename_or_skip" }]`.

### Requirement: Confirmed Run and Idempotency

The run endpoint SHALL accept a fresh `confirmed_at` within
±60 seconds and the chosen plan id. The run SHALL commit
per-resource translators in a single transaction; idempotent
re-runs of the same plan SHALL produce the same resource set
or fail with `already_imported` and never duplicate.

#### Scenario: Successful run

- **WHEN** the operator confirms a plan within 60s
- **THEN** resources are created in dependency order
        (users → sites → databases → dbs → mail → dns), an
        audit `MigrationRunCompleted` is recorded, and the
        response carries an `imported[]` array of `ImportedResource`.

#### Scenario: Translation failure

- **WHEN** one resource translator fails
- **THEN** the transaction is rolled back to the most recent
        safe point, audit `MigrationTranslationFailed` records
        the resource kind and redacted reason, and the run is
        marked `Failed` with the `translation_log` produced.

#### Scenario: Re-run is idempotent

- **WHEN** the same plan is run a second time with a fresh
        `idempotency_key`
- **THEN** the response is `200` with the prior resource set
        unchanged; no new DB rows are written.

### Requirement: Rollback Within 24 Hours

The run SHALL record a `run_id` and the set of
`ImportedResource` refs (site_id, db_id, mailbox_id, …). A
`rollback` endpoint reverses each ref in reverse commit order
within 24 hours of completion; after 24 hours a different
confirmation path is required.

#### Scenario: Within 24h rollback

- **WHEN** an Owner POSTs `rollback` with `run_id` and `confirmed_at` within 24h
- **THEN** each `ImportedResource` is removed in reverse commit order and the run is marked `RolledBack`.

#### Scenario: Past 24h rollback requires archive access

- **WHEN** an Owner POSTs `rollback` more than 24h after completion
- **THEN** the request is rejected with `rollback_window_passed`.

### Requirement: Encrypted Staging

The import staging area on disk SHALL be encrypted under a
per-import KEK encrypted under the master key. The KEK is
exposed once on run completion as an export token so a
disaster-recovery operator can decrypt from a cold backup.

#### Scenario: Export token issued once

- **WHEN** an import run completes
- **THEN** the response includes `kek_export_token` shown exactly once.

#### Scenario: Cold restore decrypts

- **WHEN** the staging area is restored on a fresh host with the export token and the master-key fingerprint
- **THEN** the panel re-derives the KEK and decrypts staging.
