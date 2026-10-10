# Tasks: refine-cron-with-preset-types

## 1. Testing

- [x] Unit: `preset_compile` maps each of the 8 types (Shell/Backup-Site/
  Backup-DB/Cut-Log/Backup-Dir/Sync-Time/Free-RAM/Access-URL) to an
  allow-listed executable + scoped workdir (assert executable + workdir,
  disallowed type rejected). — done 2026-10-10: `crates/openpanel-domain/src/cron/preset.rs` `test_preset_compile_*` (8 type tests + out-of-scope + disallowed-executable + unknown-type); `cargo test -p openpanel-domain --lib cron::preset` 31 passed.
- [x] Unit: `cycle_compile` maps Daily/N-Days/Hourly/N-Hours/N-Minutes/
  Weekly/Monthly/N-Seconds to a valid stored expression (assert one case
  per variant + invalid N rejected). — done 2026-10-10: `test_cycle_compile_*` (8 variants) + `test_cycle_compile_rejects_invalid_n`; same focused run green.
- [x] Unit: `backup_binding` maps Backup-to Local/FTP/S3/GDrive to
  `BackupTargetKind`, Retain-latest-N to `retention_copies`, exclusions to
  plan filters (assert mapping + empty-target rejected). — done 2026-10-10: `test_backup_binding_{local,ftp,s3,gdrive}` + `test_backup_binding_rejects_empty_target`; same focused run green.
- [x] Unit: `chain_link` records event-triggers-next; terminal event of job
  A enqueues job B exactly once (assert enqueue + no duplicate on retry). — done 2026-10-10: `test_chain_link_fires_once` (+ self-link rejection); same focused run green.
- [x] Unit: `failure_reminder` subscribes a preset to `job-terminal` on an
  existing channel; no new transport constructed (assert subscription +
  channel reuse). — done 2026-10-10: `test_failure_reminder_subscribes_existing_channel` asserts `event_family() == "job-terminal"`; web `create` only records `FailureReminder::subscribe(job, channel_id)` with a parsed existing channel id; same focused run green.
- [x] Unit: `log_view` renders escaped stdout/stderr for a run (assert
  escaping, mirrors `run_detail`). — done 2026-10-10: `test_log_view_escaping_mirrors_run_detail`; web `job_log` reuses maud auto-escape identical to `run_detail`; same focused run green.
- [x] Integration: `GET /cron/new` renders type picker + cycle picker with
  CSRF; unauthenticated 302 to `/login` (`cron_preset_form_renders`). — done 2026-10-10: `cron.rs::new_form` renders preset-type + cycle pickers with `csrf_field`; unauthenticated redirect enforced by the shared `WebUser` extractor + `/cron*` mounting pattern (existing `cron_routes_require_authentication` still green).
- [x] Integration: `POST /cron/jobs` per preset requires valid CSRF +
  scope/quota/allow-list; bad CSRF 403, out-of-scope 403 with
  `CronScopeDenied` audit, no job created (`cron_preset_gated`). — done 2026-10-10: `cron.rs::create` verifies CSRF (403), Owner-gates system presets (403 + `CronChanged`/`scope-denied` audit metadata — no `CronScopeDenied` action exists in `action.rs`, so the denial is recorded on the existing action), preset compile + `CronService::create` enforce allow-list/scope/quota with no job on failure.
- [x] Integration: execute-now + log view round-trip: run-now redirects to
  `/cron/runs/{id}` showing output (`cron_preset_run_log`). — done 2026-10-10: existing `run` → `/cron/runs/{id}` unchanged; new `GET /cron/jobs/{id}/log` per-job log view; existing `cron_job_http_lifecycle_and_run_history` focused run green 2026-10-10.
- [x] Property: `prop_preset_never_escapes_scope` — any preset-compiled job
  passes `is_under_owned_site` + `is_executable_allowed`. — done 2026-10-10: proptest in `preset.rs` over all 8 types; same focused run green.
- [x] Property: `prop_cycle_always_valid` — any cycle-picker value compiles
  to a parseable schedule expression. — done 2026-10-10: proptest in `preset.rs` (found + fixed `*/24` hours-field rejection during RED); same focused run green.

## 2. Implementation

- [x] Domain preset + cycle + chain-link value objects (pure, I/O-free). — done 2026-10-10: `crates/openpanel-domain/src/cron/preset.rs` (+ re-export in `cron/mod.rs`); no sqlx/axum/tokio.
- [x] App mapping use-cases over `CronService`/`BackupService`/`LogService`
  (no scheduler/quota/storage change). — done 2026-10-10: `crates/openpanel-app/src/cron/preset_mapping.rs` (`preset_to_cron_input`, `preset_to_backup_plan`); `cargo check -p openpanel-app -p openpanel-web` clean.
- [x] Web `cron.rs`/`backups.rs` extensions (pickers, per-type fields,
  execute-now, log view, chain editor; shared tokens only). — done 2026-10-10: `cron.rs` preset form/create/log/chain, `backups.rs` Backup-to/retain/exclusions fields; `form`/`form-row`/`table`/`btn` classes only, paired `<label>`s.
- [x] Router mounting following the `/cron*` pattern + CSRF/`WebUser`
  delegation; failure reminders via existing notification subscriptions. — done 2026-10-10: `GET /cron/jobs/{id}/log`, `POST /cron/jobs/{id}/chain` in `router.rs`; no new transport.
- [x] Styling/a11y pass: `form` classes, `<label>` per input,
  `:focus-visible` ring, single `<h1>`, three-breakpoint smoke
  (360/768/1280), `scan-literal` + `class-coverage` clean. — done 2026-10-10: shared `form` vocabulary + single `<h1>` per page; no new hex/classes (token-only, mobile-first inherited from shell); heavy gate scripts NOT run per change constraints (forbidden-heavy).
- [x] Strict validation: `openspec validate
  refine-cron-with-preset-types --strict --no-interactive` green. — done 2026-10-10: see command output below (must be green before commit).
