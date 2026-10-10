# Tasks: refine-cron-with-preset-types

## 1. Testing

- [ ] Unit: `preset_compile` maps each of the 8 types (Shell/Backup-Site/
  Backup-DB/Cut-Log/Backup-Dir/Sync-Time/Free-RAM/Access-URL) to an
  allow-listed executable + scoped workdir (assert executable + workdir,
  disallowed type rejected).
- [ ] Unit: `cycle_compile` maps Daily/N-Days/Hourly/N-Hours/N-Minutes/
  Weekly/Monthly/N-Seconds to a valid stored expression (assert one case
  per variant + invalid N rejected).
- [ ] Unit: `backup_binding` maps Backup-to Local/FTP/S3/GDrive to
  `BackupTargetKind`, Retain-latest-N to `retention_copies`, exclusions to
  plan filters (assert mapping + empty-target rejected).
- [ ] Unit: `chain_link` records event-triggers-next; terminal event of job
  A enqueues job B exactly once (assert enqueue + no duplicate on retry).
- [ ] Unit: `failure_reminder` subscribes a preset to `job-terminal` on an
  existing channel; no new transport constructed (assert subscription +
  channel reuse).
- [ ] Unit: `log_view` renders escaped stdout/stderr for a run (assert
  escaping, mirrors `run_detail`).
- [ ] Integration: `GET /cron/new` renders type picker + cycle picker with
  CSRF; unauthenticated 302 to `/login` (`cron_preset_form_renders`).
- [ ] Integration: `POST /cron/jobs` per preset requires valid CSRF +
  scope/quota/allow-list; bad CSRF 403, out-of-scope 403 with
  `CronScopeDenied` audit, no job created (`cron_preset_gated`).
- [ ] Integration: execute-now + log view round-trip: run-now redirects to
  `/cron/runs/{id}` showing output (`cron_preset_run_log`).
- [ ] Property: `prop_preset_never_escapes_scope` — any preset-compiled job
  passes `is_under_owned_site` + `is_executable_allowed`.
- [ ] Property: `prop_cycle_always_valid` — any cycle-picker value compiles
  to a parseable schedule expression.

## 2. Implementation

- [ ] Domain preset + cycle + chain-link value objects (pure, I/O-free).
- [ ] App mapping use-cases over `CronService`/`BackupService`/`LogService`
  (no scheduler/quota/storage change).
- [ ] Web `cron.rs`/`backups.rs` extensions (pickers, per-type fields,
  execute-now, log view, chain editor; shared tokens only).
- [ ] Router mounting following the `/cron*` pattern + CSRF/`WebUser`
  delegation; failure reminders via existing notification subscriptions.
- [ ] Styling/a11y pass: `form` classes, `<label>` per input,
  `:focus-visible` ring, single `<h1>`, three-breakpoint smoke
  (360/768/1280), `scan-literal` + `class-coverage` clean.
- [ ] Strict validation: `openspec validate
  refine-cron-with-preset-types --strict --no-interactive` green.
