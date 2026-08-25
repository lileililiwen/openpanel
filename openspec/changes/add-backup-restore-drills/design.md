# Add Backup restore drills — Design

## Explore & Reuse

- Restore pipeline: `crates/openpanel-app/src/backups/service.rs`
  (`restore_preview` preflight :539–569, restore writers) — the drill
  drives the same code against a `SandboxContext` instead of live
  paths; no second restore implementation.
- Database provisioning shell path (`openspec/specs/databases/spec.md`,
  "MySQL Provisioning via Shell") creates the throwaway
  `{name}_drill_<rand>` database/user, dropped afterwards.
- Cron scheduling (`openspec/specs/cron/spec.md`) and notifications
  dispatcher (`openspec/specs/notifications/spec.md`) reused as-is.
- Audit: `DrillStarted`, `DrillCompleted{outcome}`.

## Sandbox context

```rust
pub struct SandboxContext { docroot: TempDir, db_name: String, db_user: String }
impl SandboxContext {
    pub fn create(deps) -> Result<Self, DrillError>;   // random suffix, provisions db
    pub fn teardown(self, deps);                        // best-effort, audited on failure
}
```

## Assertions (pure where possible)

| Kind | Assertion |
|---|---|
| Database | dump applies via `mysql < dump`; ≥1 table exists |
| Site | archive extracts; manifest parses; index/docroot non-empty |
| SslKeys | ciphertext decrypts under local master key; PEM parses |
| PanelMetadata | manifest schema supported; users table non-empty |

Assertion outcomes are pure structs; execution glue lives in app.

## Flow

```
run_drill(backup_id):
   artifact = fetch(backup_id)            # offsite pull if needed
   ctx = SandboxContext::create()
   outcome = try { restore_into(ctx); run_assertions() } finally { ctx.teardown() }
   persist RestoreDrill + assertions; prune history to 20
   if failed: notify(DrillFailed{backup_id, failing[]})
```

## Endpoints / CLI / Web

```
POST /api/v1/backups/{id}/drills            (run now)
GET  /api/v1/backups/{id}/drills[/{did}]
PUT  /api/v1/backups/{id}/drill-schedule    {cron_expr | disabled}
CLI: openpanel backup drill {run,show,schedule}
Web: Backups → Drills tab (history, last report, schedule editor)
```

## Layering

Domain: pure types + assertion result model. App: sandbox builder,
assertion runners, service, repo. Adapters standard.
