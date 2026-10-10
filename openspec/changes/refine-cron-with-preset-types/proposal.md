# Proposal: refine-cron-with-preset-types

## WHY

Operators coming from Baota expect scheduled tasks as guided presets, not raw
executable + cron-expression strings. OpenPanel ships only that raw form:
`crates/openpanel-web/src/cron.rs` renders `new_form` with
Name/Schedule/Timezone/Executable/Arguments/Working-directory fields,
`create` hard-codes `kind: "command"`, and `router.rs` mounts only
`/cron*` list/detail/run/history routes. `backups.rs` likewise offers a
metadata-only plan form (name/schedule/retention). Baota's cron page instead
offers typed tasks (site backup, DB backup, log cut, directory backup, time
sync, RAM release, URL visit) with a human cycle picker, backup
destinations, retain-N, exclusions, failure reminders, execute-now, log
view, and chained task-scheduling. Without presets, operators hand-craft
shell lines where scope, quota, allow-list, and retention expectations are
invisible.

## WHAT

Refine `/cron` with 8 Baota preset types mapping onto existing services:
Shell, Backup-Site, Backup-DB, Cut-Log, Backup-Dir, Sync-Time, Free-RAM,
Access-URL. A human cycle picker (Daily / N-Days / Hourly / N-Hours /
N-Minutes / Weekly / Monthly / N-Seconds) compiles to the stored cron
expression. Backup presets add Backup-to (Local/FTP/S3/GDrive via existing
`BackupTargetKind` + offsite credentials), Retain-latest-N, and exclusion
rules. Every preset reuses `CronService` scope/quota/allow-list checks,
`BackupService` plans/runs, `LogService` rotation, and `NotificationService`
channels for failure reminders. Execute-now, per-run log view, and chained
Task-Scheduling (event-triggers-next) complete the surface.

## Capabilities

- **Modified:** `cron` — preset-type model, cycle-picker compilation,
  execute-now, log view, chained triggers (delta in this change).
- **Modified:** `backups` — backup-preset binding (destination, retain-N,
  exclusions) over existing plans/runs (delta in this change).
- Reused unchanged: `notifications` (failure-reminder channels only),
  `offsite-backup-targets` (credential/adapter contract),
  `logs` rotation, `web-ui-styling` tokens, identity/auth + `JobScope` RBAC.

## Non-goals

- No new scheduler engine; scheduling, overlap policy, and quota semantics
  stay in `CronService`.
- No new notification transport; reminders reuse registered SMTP/webhook
  channels only.
- No new backup storage backend or concrete S3/B2/rsync adapter; targets
  reuse the offsite contract.
- No sibling overlap: no `/terminal` surface (see
  `add-terminal-nav-and-saved-commands`), no `/migrate` wizard (see
  `add-migration-wizard-ui`), no `/ftp` or phpMyAdmin link (see
  `add-ftp-global-and-phpmyadmin-link`), no file-manager affordances (see
  `refine-files-with-baota-affordances`).
