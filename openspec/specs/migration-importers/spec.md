# migration-importers Specification

## Purpose

The migration-importers bounded context lets operators bring
existing cPanel (`pkgacct` / `legacy`), Baota (`/www/backup/`),
and OpenPanel's own `tar-with-json-manifest` backup bundles into
the panel through a dry-run preview, an atomic run, and a bounded
rollback window. This refinement ships the typed model, the
`MigrationDriver` contract, and the reference
`tar-with-json-manifest` driver with its preview / run / rollback
lifecycle; the cPanel and Baota native-tar parsers ship in the
follow-on driver changes.

## Requirements

### Requirement: Driver Contract

The bounded context SHALL model a `MigrationDriver` trait with an
associated `Source`, a synchronous `sniff` that returns the
recognized `DriverKind` (or `None`), an async `dry_run` that
produces a `MigrationPlan`, an async `run` that commits the plan
and returns the `ImportedResource` set, and an async `rollback`
that undoes a committed run. `DriverKind` has four values:
`cpanel-pkgacct`, `cpanel-legacy-backup`, `baota-backup`, and
`tar-with-json-manifest`.

#### Scenario: Sniff recognizes the format

- **WHEN** a `TarWithJsonManifestDriver` sniffs a bundle whose
  manifest validates
- **THEN** it returns `Some(DriverKind::TarWithJsonManifest)`.

### Requirement: Migration Plan

The bounded context SHALL model a `MigrationPlan` carrying a
`MigrationPlanId`, the driver, `Vec<PlannedResource>`,
`Vec<ImportConflict>`, `Vec<MigrationWarning>`, and a creation
timestamp. The plan exposes `total_bytes()`, `is_empty()`, and
`refuses_empty_run()`.

#### Scenario: Empty plan refused

- **WHEN** a plan has no resources
- **THEN** `refuses_empty_run()` returns `true`.

### Requirement: Imported Resource

The bounded context SHALL model an `ImportedResource` carrying the
run id, `ImportedResourceKind` (user, site, database, mail domain,
mailbox, DNS zone, cron job, SSL certificate), the source key, the
target reference id, and a rolled-back flag.

#### Scenario: Rollback marks resources

- **WHEN** a run is rolled back
- **THEN** each imported resource is marked `rolled_back` and the
  run header transitions to `RolledBack`.

### Requirement: Reference Tar-with-JSON-Manifest Driver

The bounded context SHALL implement the `tar-with-json-manifest`
driver: a bundle is a manifest (`format` MUST equal
`openpanel-backup`) plus per-resource payloads. The manifest
rejects unsafe payload paths (`..`, leading slash) and missing
payloads. The driver previews the plan, commits through a
`ManifestTranslator` hook (returning `Ok(None)` to skip, `Ok(Some(
ref_id))` to commit), and rolls back by delegating to the bounded
contexts.

#### Scenario: Malformed manifest rejected

- **WHEN** the manifest format is not `openpanel-backup`
- **THEN** `validate` returns `MigrationError::MalformedSource`.

#### Scenario: Missing payload rejected

- **WHEN** a declared payload path has no bundle entry
- **THEN** building the bundle returns `MigrationError::MalformedSource`.

### Requirement: Service Lifecycle

The application service SHALL provide `preview`, `run`, and
`rollback`. `preview` verifies the driver hint matches the sniffed
kind. `run` refuses empty plans, refuses expired plans (60s TTL),
and refuses a plan that was already imported. `rollback` is only
allowed for a `Completed` run within the 24h window.

#### Scenario: Idempotency guard

- **WHEN** a confirmed plan is run a second time
- **THEN** the service returns `MigrationError::AlreadyImported`
  with the prior run id and records `MigrationAlreadyImportedRejected`.

#### Scenario: Driver hint mismatch

- **WHEN** a hint disagrees with the sniffed kind
- **THEN** `preview` returns `MigrationError::UnknownSource`.

### Requirement: Audit and Event Surface

The service SHALL record `MigrationPreviewed`,
`MigrationRunCommitted`, `MigrationRunRolledBack`, and
`MigrationAlreadyImportedRejected` audit events.

#### Scenario: Committed run is audited

- **WHEN** an import run commits its planned resources
- **THEN** an audit `MigrationRunCommitted` event records the run id
        and driver without source payload content.

The cPanel /
Baota parser changes keep the same audit surface.