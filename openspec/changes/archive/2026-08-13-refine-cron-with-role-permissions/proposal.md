# Refine cron with role permissions and per-user limits

## Why

`openspec/specs/cron/spec.md` defines scheduled jobs, leases,
overlap policy, and timeout enforcement. It does not pin **role
permissions** (which roles may create which kinds of jobs) or
**per-user concurrency / count limits**. cPanel has distinct user
cron and system cron queues with separate caps. OpenPanel currently
lacks that distinction, allowing an Owner to schedule as many jobs
as they wish. This refinement closes the gap without changing the
existing scheduler semantics.

## What Changes

- New `Job::owner_role` field recording the principal role under
  which the job was created; scheduler enforces role-specific
  quotas.
- New role matrix: `Owner`, `Admin` may schedule all job kinds;
  `User` may only schedule jobs whose working directory is
  beneath a site they own and whose executable is allow-listed.
- New `CronQuota` per-user value object: `max_concurrent`,
  `max_due_per_minute`, `max_total`. Defaults are layered (global
  → user).
- New "user cron" UI surface for end users to schedule trivial
  tasks under their site roots.

## Capabilities

### Modified Capabilities

- `cron`: role-based permissions; per-user quotas; user-cron
  surface.

## Impact

- Domain: `JobScope` (`System`, `PerSite`, `PerUser`),
  `CronQuota`.
- App: `CronPermissionChecker` (role matrix), `QuotaEnforcer`.
- API/CLI/web: `/cron/jobs` filters by `JobScope`; `User` role
  sees only `PerSite` / `PerUser` jobs; `Owner` sees all.
