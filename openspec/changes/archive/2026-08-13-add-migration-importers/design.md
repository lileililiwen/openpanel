# Add migration importers — Design

## Driver shape

```rust
pub trait MigrationDriver: Send + Sync {
    type Source;
    fn sniff(&self, tar: &[u8]) -> Option<DriverKind>;
    async fn dry_run(&self, src: Self::Source) -> MigrationPlan;
    async fn run(&self, src: Self::Source, plan_id: PlanId,
                 confirmed_at: DateTime<Utc>)
                 -> Result<Vec<ImportedResource>, MigrationError>;
}
```

## Drivers

- `cpanel-pkgacct`: parses `pkgacct` tar format; users →
  identity; domains → sites; MySQL dumps → databases; mail →
  mail; DNS zones → dns. Migrates rpms/ssl export.
- `cpanel-legacy-backup`: same data via the older `legacy`
  format used before `pkgacct` shipped.
- `baota-backup`: parses `/www/backup/` bundles; panel-site
  records → sites; mail → mail; cron → cron.
- `tar-with-json-manifest`: deterministic format the panel
  itself can emit for hand-crafting or test fixtures.

## Stages

```
preview:
  decrypt → unpack staging → per-driver dry-run
    → MigrationPlan { resources: [...], conflicts: [...], warnings: [...] }

run:
  confirm within 60s
  → per-resource translator → IdempotencyRecord per resource
  → commit per ResourceKind in a single transaction; failed
    transaction → TranslationLog marked Failed; partial state
    is rolled back to the per-resource commit point.
  → emit ImportedResource[] with ref ids (site_id, db_id, …)

rollback:
  for each ImportedResource in reverse order:
    delete by ref id (where supported)
```

## Encryption

Staging is encrypted under a per-import KEK derived from a
random 256-bit key encrypted under the master key. The KEK is
returned once to the operator as an export token so a cold
restore of an import staging is possible.

## Endpoints

```
POST /api/v1/migration/import/preview  body: { source_url?, source_path?, source_bytes_b64?, driver_hint? }
                                        → 200 { plan_id, resources, conflicts, warnings }
POST /api/v1/migration/import/run       body: { plan_id, confirmed_at, target_owner_user_id }
                                        → 200 { run_id, imported[] }
POST /api/v1/migration/import/rollback  body: { run_id, confirmed_at }
                                        → 200 { rolled_back[] }
GET  /api/v1/migration/imports          list recent runs within retention
```

## CLI

```
openpanel migration preview <source> --driver <name>
openpanel migration run     <plan_id> --confirmed-at <ts> --target-owner <u>
openpanel migration rollback <run_id>  --confirmed-at <ts>
openpanel migration imports list
```

## Tests

```
1.1  Unit: each driver's sniff function detects the right
      format and ignores others.
1.2  Property: idempotent re-run of the same source with the
      same plan produces the same imported resource set or
      rejects with a typed `already_imported`.
1.3  Service tests with mock translators: preview / run /
      rollback and translation log correctness.
1.4  Integration: live import of a small cPanel pkgacct and a
      Baota bundle; rollback undo.
1.5  CLI E2E: full lifecycle.
1.6  Web: import wizard (CSRF), progress, rollback button.
```
