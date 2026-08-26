# Refine Logs with rotation policy

## Why

The logs capability reads, redacts, and downloads logs and *detects*
rotation (`openspec/specs/logs/spec.md`) but nothing configures it:
rotation policy for nginx site logs, managed-service logs, and panel
logs is unmanaged, so disks fill on busy hosts. cPanel, Plesk, and
Virtualmin all expose log-rotation controls; 1Panel ships scheduled
log-cleanup tasks.

## What Changes

- **Rotation policies** per source class (site access/error,
  managed service, panel): max age, max size, retained generations,
  compress flag.
- Policy application renders logrotate-style drop-in configs owned by
  the panel (managed block), applied atomically; drift detected on the
  existing rotation-detection path.
- **Manual rotation trigger** per source (rotate now).
- Disk-pressure guardrail: monitoring threshold event can suggest (not
  force) tightening.
- Surfaces: API `/api/v1/logs/policies`, CLI `openpanel logs policy …`,
  web Logs → Retention tab.

## Capabilities

### Modified Capabilities

- `logs`: add rotation-policy management to read/redact/download.

## Impact

- Domain: `RotationPolicy{source_class, max_age_days, max_size_mb,
  keep_generations, compress}`, validation bounds;
  `LogsError::Policy…`.
- App: drop-in renderer + writer following the atomic-write precedent;
  manual rotate shells out to the system logrotate binary (detected at
  startup like mysql); audit events.
- API/CLI/web as above.
- Security: policies never widen read permissions; compressed archives
  inherit 0640 root-group ownership; no log content in audit events.
- Coupling: logs module; monitoring threshold events (read-only hint);
  cron not required (logrotate handles schedule).

## Non-goals

- No central log shipping/aggregation (observability-export covers
  export).
- No per-file custom rotation scripts.
