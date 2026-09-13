# Backup DR and Migration Runbook

Covers `backup-dr-operations`: backup health (RPO/RTO), restore
drills, scoped restore, and host-to-host migration.

## RPO and RTO

- **RPO** is the age of the last successful run
  (`BackupHealth::rpo_secs`). A plan whose success is older than its
  nominal interval is **stale**: inspect run logs, retry the plan,
  then check schedule and retention configuration.
- **RTO estimate** is 900 seconds (`ESTIMATED_RTO_SECS`) until measured
  recoveries exist. Record real drill durations alongside the drill
  report when committing to tighter objectives.
- A terminal **failed** run marks the plan failed: inspect the failing
  resource, fix storage or credentials, then retry. Failed runs never
  count as coverage.

## Restore drills

- Trigger on demand: `POST /api/v1/backups/runs/{id}/drills`, then
  `GET /api/v1/backups/runs/{id}/drills` for history or
  `GET .../drills/{drill_id}` for one report. CLI: `openpanel
  backups drill run --id <run>`, `... list --id <run>`,
  `... show --id <drill>`.
- Drills run the production restore pipeline in an isolated sandbox
  (temporary docroot plus a throwaway suffixed database) and always
  tear the sandbox down, including on failure.
- The most recent 20 reports per backup are retained; a failed drill
  dispatches a `DrillFailed` notification to subscribed channels.
- Schedule drills through the existing cron capability by creating a
  job that runs the drill command at the desired cadence.

## Scoped restore

- Every restore starts from `POST
  /api/v1/backups/runs/{id}/restore/preview`: compatibility,
  collisions, resource scope, and estimated impact.
- Execution requires the single-use confirmation token from preflight.
  Requests without a valid token change nothing.
- Server snapshots follow the same gate: `preflight` mints the token,
  `restore` consumes it exactly once and applies resources in
  dependency order (databases, sites, certificates, metadata).

## Host migration

1. Attach an offsite target to the plan and verify it
   (`verify_remote_roundtrip`: connectivity, credentials, write/read,
   listing). Do not migrate to an unverified target.
2. Export the snapshot bundle to the target key
   (`export_to_target`), then check readiness on the destination:
   schema compatibility, collision preview, and the bootstrap command
   (`assess_migration_readiness` / `migration_bootstrap_command`).
3. Run the printed bootstrap command on a fresh compatible host
   (`openpanel migrate import --target <key> --host <label>`), then
   verify the destination inventory (sites, databases, certificates)
   and confirm audit events exist on both hosts.
4. Resolve collisions explicitly: confirm overwrite scope before
   importing over existing names. Never force-import blindly.

## Remote targets

- Verify before trusting: a target that fails connectivity,
  credentials, write, read, or listing checks must not hold the only
  copy. The verification probe object is bounded and removed
  afterwards; reports never contain secret material.
- Keep retention aligned with the plan: the health surface reports
  the configured copy count next to staleness so under-retained
  plans are visible before they age out.
