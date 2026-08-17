# Add per-user resource quotas

## Why

cPanel's headline feature is per-account resource limits: disk,
bandwidth, inodes, CPU, and email quotas. Baota's reseller tier adds
the same. OpenPanel has RBAC but every account shares one big
filesystem and one big network pipe. A panel that supports multiple
reseller users without quotas will let any one of them fill the disk
or saturate the link. This change adds a `quotas` bounded context that
enforces limits at the kernel boundary (POSIX `quota` for disk,
`tc` for bandwidth, inotify/`fanotify` for inode counts) and exposes
usage + overage events through the existing monitoring/notification
pipeline.

## What Changes

- New `quotas` bounded context with a `QuotaPolicy` aggregate
  (per-user or per-site: disk MB, bandwidth MB/month, inodes, max
  file size, optional CPU shares).
- Enforcement adapter: `setquota`/`edquota` for disk; `tc` for
  bandwidth shaping per UID via `iptables`-or-`nftables` marks; an
  in-process inode counter for the home directory tree.
- Soft/hard limits with grace windows; soft limit triggers a
  warning event, hard limit blocks the write at the kernel layer.
- Usage is sampled periodically and surfaced through the existing
  monitoring module; overage fires an `AlertFired` that the
  notification channel pipeline can consume.
- REST, CLI, and `/users/{id}/quotas` web surface.

## Capabilities

### New Capabilities

- `quotas`: per-account disk / bandwidth / inode / CPU limits and
  enforcement.

### Modified Capabilities

- `monitoring`: usage metrics emitted for each quota dimension;
  alerts flow through the existing notification pipeline.

## Impact

- Domain: `QuotaPolicy` aggregate, `QuotaUsage` value object.
- App: `QuotaService`, `DiskEnforcer`, `BandwidthEnforcer`,
  `InodeCounter` adapters behind typed ports.
- API/CLI/web: `/api/v1/users/{id}/quotas`, `openpanel quota
  {set,get,usage}`, `/users/{id}/quotas` page.
- Dependencies: `nix` for `quotactl`, `rtnetlink` for `tc`, no
  shell-outs. Pure-Rust enforcement fits the project's "no script
  injection" stance.
