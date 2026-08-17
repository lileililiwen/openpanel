# Add migration importers

## Why

Operators moving from cPanel or Baota need a way to bring their
existing sites, databases, mailboxes, DNS records, and users
into OpenPanel without rebuilding by hand. Both upstream
panels ship machine-readable backup formats
(`pkgacct`/`backup-<date>.tar.gz` for cPanel, the standard
`/www/backup/` bundle for Baota). OpenPanel currently has no
importer, forcing operators to spend days manually translating
formats. This change adds import pipelines with dry-run
preview, atomic commit, and complete audit trails.

## What Changes

- New bounded context `migration-importers` with importer
  drivers for `cpanel-pkgacct`, `cpanel-legacy-backup`,
  `baota-backup`, and a generic `tar-with-json-manifest`.
- New domain objects: `MigrationPlan`, `ImportedResource`,
  `TranslationLog`.
- New endpoints: `POST /migration/import/preview`,
  `POST /migration/import/run`, `POST /migration/import/rollback`
  (rollback undo per-import-run for ≤ 24h window).
- Staging area: encrypted at rest under a per-import KEK,
  reused across drivers.

## Capabilities

### New Capabilities

- `migration-importers`: import pipeline for cPanel and Baota
  backup bundles with dry-run / run / rollback.

## Impact

- Domain: `MigrationDriver`, `MigrationPlan`, `ImportedResource`
  enum (per `ResourceKind`), `TranslationLog` with redacted
  diagnostics.
- App: `MigrationService`, drivers in
  `crates/openpanel-app/src/migration_importers/`.
- API/CLI/web: `/migration/import/*`; CLI
  `openpanel migration {preview,run,rollback}`; web wizard.
- Coupling: drivers talk to `sites`, `databases`, `mail`,
  `dns`, `identity`, `backups` bounded contexts.
