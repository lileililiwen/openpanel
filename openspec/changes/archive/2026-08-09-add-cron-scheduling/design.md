## Context

The core `JobSupervisor` can host background work, but there is no persisted user schedule or execution record. Jobs are privileged and must not turn HTTP input into shell syntax.

## Goals / Non-Goals

**Goals:** deterministic schedule calculation, per-owner RBAC, safe process execution, bounded concurrency, run history, and consistent API/CLI/web behavior.

**Non-Goals:** sub-second schedules, interactive processes, arbitrary environment inheritance, distributed scheduling, or backup implementation (backups consume this capability later).

## Decisions

1. Model `CronJob`, `CronSchedule`, `JobKind`, and `OverlapPolicy` in the domain. Accept five-field cron expressions in a configured IANA timezone and calculate `next_run_at` in UTC.
2. Represent commands as executable plus argument vector, not a shell string. Use an explicit working directory inside the owner's site roots, a minimal allowlisted environment, output caps, and a kill-on-timeout process adapter.
3. Claim due jobs transactionally with a lease. Default overlap policy is `skip`; global and per-owner concurrency limits prevent exhaustion. A stale lease is recoverable after timeout.
4. Persist metadata and truncated stdout/stderr separately with retention. API list responses expose metadata; output requires an authorized detail request and is escaped in the web UI.
5. Expose `/api/v1/cron/jobs`, `/runs`, `openpanel cron ...`, and `/cron`. All mutations audit job ID, owner, and action, never command secrets or output.

## Risks / Trade-offs

- Commands remain powerful -> Owner/Admin creation only, argv execution, path/env allowlists, timeouts, and audit.
- DST can duplicate or skip wall times -> define next occurrence using the timezone library and test both transitions.
- Crash during execution can leave a lease -> leases expire and runs become `Interrupted` on recovery.

## Migration Plan

Create empty cron tables and register a disabled-by-config scheduler before enabling it. Rollback stops the task; persisted jobs remain inert.
