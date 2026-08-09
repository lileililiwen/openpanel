## Why

Recurring maintenance is a stated v0.1 follow-up and is necessary for dependable backups, log rotation, and operator jobs. OpenPanel needs a safe scheduler with observable execution rather than unrestricted edits to system crontabs.

## What Changes

- Add a `cron` bounded context for validated schedules and owned jobs.
- Support command and HTTP-request jobs, enable/disable, run-now, bounded history, timeout, and overlap policy.
- Add REST, CLI, web UI, audit, and background scheduling surfaces.
- Execute jobs without a shell by default and never return stored secrets.

## Capabilities

### New Capabilities

- `cron`: scheduled job lifecycle and execution history.

### Modified Capabilities

None.

## Impact

Adds domain/app/API/CLI/web modules, SQLite migrations, a scheduler background task, test-support mocks, and configuration for concurrency, timeout, and history retention.
