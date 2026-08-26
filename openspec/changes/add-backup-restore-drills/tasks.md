# Add Backup restore drills — Tasks

## 1. Testing

- [x] 1.1 Unit: `RestoreDrill` state machine — a drill is `Running`
      until every assertion completes; outcome flips to
      `Passed`/`Failed` only on finish; finishing twice is rejected.
- [x] 1.3 Unit: sandbox naming — the suffixed database name and temp
      docroot are unique per drill and never equal production names.
- [ ] 1.2 Unit: assertion evaluation — SQL dump executes and row
      counts > 0 passes; site archive extracts and contains a
      manifest/index passes; SSL entry decrypts under the local
      master key passes; each failure records a non-empty detail.
- [ ] 1.4 Property: for arbitrary drill reports (≥100 cases) the
      serialized report contains no secret material (passwords,
      cipher text, connection strings) — only paths, ids, outcomes,
      and details.
- [ ] 1.5 Integration (`tests/integration/backup_drills.rs`) with a
      mocked restore pipeline: run drill over a verified backup →
      report Passed with one assertion per bundled resource kind;
      sandbox paths wiped afterwards; report retained in history.
- [ ] 1.6 Integration: failed assertion → outcome Failed, sandbox
      still torn down, notification dispatched through the existing
      channels port.
- [ ] 1.7 Integration: retention — the 21st completed drill prunes
      the oldest (default keep 20).
- [ ] 1.8 CLI E2E: `cli_backup_drill_run_then_show`.
- [ ] 1.9 Web: Backups → Drills tab at 360/768/1280 px; screenshots.

## 2. Domain

- [x] 2.1 Add `RestoreDrill`, `DrillAssertion`, `DrillOutcome`,
      `DrillError` + validation under
      `crates/openpanel-domain/src/backups/`.

## 3. Application

- [x] 3.1 `SandboxContext` builder (temp docroot + suffixed throwaway
      database) reusing the provisioning shell path; guaranteed
      teardown even on failure.
- [ ] 3.2 Drill runner: point the existing restore machinery at the
      sandbox context, evaluate assertions per resource kind, persist
      the report with retention pruning.
- [ ] 3.3 Notification dispatch on Failed outcome via the existing
      channels port; cron job type for scheduled drills.

## 4. Adapters and UI

- [ ] 4.1 REST routes `/api/v1/backups/{id}/drills[/{drill_id}]`
      (run + list + show).
- [ ] 4.2 CLI `openpanel backup drill {run,list,show}`.
- [ ] 4.3 Web Backups → Drills tab.

## 5. Validation

- [ ] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean.
- [ ] 5.3 Smoke-test: drill a real backup against a local mysqld,
      confirm pass, then corrupt a dump and confirm fail + alert.
- [ ] 5.4 Archive with `openspec archive add-backup-restore-drills`.
