## Why

OpenPanel can provision production data but cannot recover it. Both comparison panels treat scheduled backup, retention, destinations, and granular restore as core operations; this is the highest data-safety gap after scheduling.

## What Changes

- Add backup plans, on-demand runs, retention, manifests, integrity verification, and restore jobs.
- Back up site files, managed databases, and OpenPanel metadata to local storage first.
- Add REST, CLI, web UI, audit, progress, and Cron integration.
- Make restore non-destructive by default and require explicit overwrite confirmation.

## Capabilities

### New Capabilities

- `backups`: consistent backup, retention, verification, and restore lifecycle.

### Modified Capabilities

None.

## Impact

Adds a bounded context, migrations, archive/database adapters, background jobs, API/CLI/web surfaces, and configuration for staging space and retention. Remote object storage is deferred behind a destination port.
