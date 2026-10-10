# Design: refine-cron-with-preset-types

## Explore & Reuse (repo-map evidence, 2026-10-10)

`scripts/repo-map.sh` plus targeted reads confirm the cron/backup core
already exists; this change adds preset mapping + web adapters only:

- `crates/openpanel-web/src/cron.rs` — current surface: `list` (Name/
  Schedule/Timezone/Status table + `/cron/new` CTA), `new_form` (raw
  Name/Schedule/Timezone/Executable/Arguments/Working-directory/Timeout),
  `create` (hard-codes `kind: "command"`, `overlap_policy: "skip"`),
  `detail` (run/disable/enable + delete-confirm layer), `run` (execute-now
  → redirect to `/cron/runs/{id}`), `runs`/`run_detail` (state list +
  escaped stdout/stderr). All presets reuse these handlers; `new_form`
  gains a type picker + cycle picker + per-type fields that compile down
  to the existing `CronInput`, never a second creation path.
- `openpanel-app` `CronService` (`list`/`create`/`get`/`set_enabled`/
  `delete`/`run_now`/`runs`/`get_run`) with `JobScope` role checks
  (`role_allows_scope`), `CronQuota` (`check_quota`), `is_under_owned_site`
  + `is_executable_allowed` guards. Presets are pure mapping: each type
  resolves to an allow-listed executable + scoped workdir + timeout, then
  calls the unchanged `create`. No scheduler, overlap, or quota change.
- `openpanel-app` `BackupService` (plans/runs, `BackupPlanInput`,
  retention) + `BackupTargetKind` (`Local|OffsiteS3|OffsiteRsync|OffsiteB2|
  OffsiteWasabi`) + `RemoteTargetConfig`/credential model from
  `offsite-backup-targets`. Backup-Site/Backup-DB/Backup-Dir presets build
  a `BackupPlanInput` with destination (Local/FTP/S3/GDrive mapped onto the
  existing kinds), Retain-latest-N → `retention_copies`, and exclusion
  rules as plan filters. No storage backend or adapter change.
- `LogService` rotation + policies router (`openpanel-api` `routes/logs`)
  for the Cut-Log preset (rotate + truncate scoped log paths only).
  Free-RAM (drop-caches/sync hint via allow-listed command) and Sync-Time
  (NTP sync command) are Shell specializations, not new daemons.
  Access-URL is the existing HTTP-job kind surfaced as a preset.
- `NotificationService` SMTP/webhook channels + subscriptions
  (`notifications` spec): failure reminders subscribe the preset to a
  `job-terminal` event on its channel. No new transport, no new payload.
- `crates/openpanel-web/src/router.rs:467-476` — `/cron*` mounting pattern
  (`WebUser` + CSRF verify + service delegation). New routes follow the
  same shape (`/cron/new` extended form, `/cron/jobs/{id}/log`,
  `/cron/jobs/{id}/chain`); `backups.rs` plan form gains preset fields.
- `tokens.css` + `app.css` (`form`/`form-row`/`table`/`btn`/`banner`
  vocabulary), `layout.rs` shell, `capability_registry.rs` `/cron` entry
  (unchanged). All forms use `form` classes, paired `<label>`s,
  token-only values, mobile-first `min-width` queries.

New code is limited to a `CronPreset` mapping module (type + cycle-picker
→ `CronInput`/`BackupPlanInput`), `cron.rs`/`backups.rs` template/handler
extensions, and a chain-link record (next-job id on terminal event).

## Reuse record (implementation evidence, 2026-10-10)

- Reused unchanged: `CronSchedule::parse` (cycle validation), `CronService::create/get/runs/get_run`
  (all web handlers delegate), `is_under_owned_site` + `is_executable_allowed`
  (preset_compile guards), `BackupTargetKind`/`BackupTargetPolicy` (destination mapping),
  `BackupPlanInput` + `BackupResource::PanelMetadata` (plan shape), `NotificationService`
  channels (failure reminder = `FailureReminder::subscribe` record only, no transport),
  `LogService` rotation (untouched; Cut-Log is an allow-listed command), `tokens.css`
  `form` vocabulary + `layout::csrf_field` + `WebUser` extractor + `/cron*` router pattern,
  `AuditAction::CronChanged` (denial/chain-link audit; no `CronScopeDenied` variant exists
  in `action.rs`, so denials record `operation: scope-denied` metadata on the existing action).
- New code only: `openpanel-domain::cron::preset` (pure VOs), `openpanel-app::cron::preset_mapping`
  (pure `CronInput`/`BackupPlanInput` compilers), web form/handler extensions + 2 routes.
  No new scheduler engine, no scheduler/quota/storage change, no new transport or backend.

## DDD layering

- **Domain** (`openpanel-domain`): preset type + cycle value objects and
  chain-link value; pure compile-to-expression/schedule helpers, I/O-free.
- **App** (`openpanel-app`): mapping use-cases delegating to `CronService` /
  `BackupService` / `LogService`; no new repository, no migration beyond
  the chain-link column.
- **Adapters** (`openpanel-web` + api/cli surfaces): handlers verify CSRF,
  resolve `WebUser`, enforce `JobScope` RBAC, render maud with shared
  tokens. Role gating is defense in depth (403/404 without leaking args).

## Page flow

1. `GET /cron/new` — preset-type picker (8 types) + cycle picker + per-type
   fields (backup target/exclusions/retain-N, URL+method, log path, dir).
2. `POST /cron/jobs` — validate scope/quota/allow-list, compile cycle to
   expression, create job (+ backup plan binding for backup types).
3. Detail — execute-now, enable/disable, chain editor (event-triggers-next),
   failure-reminder subscription, per-run log view (stdout/stderr).

Sibling non-overlap: `/terminal` + saved commands, `/migrate` wizard,
`/ftp` + phpMyAdmin, file-manager affordances are owned by the four named
siblings; this change touches none of those routes or registry entries.

---

*This design AWAITS human principal approval per Agents.md §8 item 10 — do NOT implement until approved.*
