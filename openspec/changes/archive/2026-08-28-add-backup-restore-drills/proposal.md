# Add Backup restore drills

## Why

Backup verification today is checksum-only plus a preflight
(`crates/openpanel-app/src/backups/service.rs:513-569`): an artifact
can hash correctly and still be unrestorable (corrupt dump, missing
table, broken docroot). KeyHelp alerts on backup-cleanup failures and
enterprise practice demands automated test-restores; no surveyed panel
ships them natively — a differentiator on top of the existing backups
and server-snapshot work.

## What Changes

- **Restore drill**: scheduled or manual full pipeline execution of a
  chosen backup into an isolated sandbox (temp dirs + throwaway
  database with random suffix), followed by automated assertions per
  resource kind.
- **Drill report**: pass/fail per assertion, duration, artifact id,
  sandbox paths wiped afterwards; retained history (default 20 runs).
- **Alerting**: failed drills route through notification channels.
- Surfaces: API `/api/v1/backups/{id}/drills`, CLI
  `openpanel backup drill …`, web Backups → Drills tab.

## Capabilities

### Modified Capabilities

- `backups`: add automated restore-drill verification on top of
  existing verify/preflight.

## Impact

- Domain: `RestoreDrill{backup_id, started_at, finished_at, outcome,
  assertions[]}`, `DrillAssertion{kind, passed, detail}`;
  `DrillError`.
- App: reuses existing restore machinery pointed at a sandbox context;
  new `SandboxContext` builder (temp docroot, suffixed DB name);
  assertions: SQL dump executes and row counts > 0; site archive
  extracts and contains manifest/index; SSL entry decrypts under the
  local master key.
- API/CLI/web as above; scheduler via cron capability.
- Security: sandbox DB user limited to the suffixed database; secrets
  never logged in reports; sandbox always torn down (even on failure).
- Coupling: backups, databases (provisioning shell path), cron,
  notifications.

## Non-goals

- No production cutover from a drill.
- No cross-host drill execution (local host only for now).
