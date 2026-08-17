# Refine cron with role permissions and per-user limits — Design

## Scope matrix

```
┌──────────────┬─────────────┬───────────────┬──────────────┐
│ Role         │ System cron │ Per-Site cron │ Per-User cron│
├──────────────┼─────────────┼───────────────┼──────────────┤
│ Owner        │ ✓           │ ✓             │ ✓            │
│ Admin        │ ✓ (own)     │ ✓             │ ✓            │
│ User         │ ✗           │ ✓ (own sites) │ ✓            │
└──────────────┴─────────────┴───────────────┴──────────────┘
```

System cron: working directory under `/etc/cron.*`, `/usr/local/sbin`,
or jobs whose executable allow-list includes system binaries and
which require a typed `confirmed_at` timestamp.

Per-Site cron: working directory must resolve beneath a site owned
by the principal.

Per-User cron: arbitrary argv but bounded by per-user allow-list
captured at user creation.

## Quota computation

```
effective = max(global_default, plan_override_for_user)
  max_concurrent = effective.max_concurrent  // default 4
  max_due_per_minute = effective.max_due_per_minute  // default 30
  max_total = effective.max_total  // default 100
```

When a new job would exceed `max_total`, creation returns
`CronError::QuotaExceeded{axis="total"}`. When two would run
simultaneously and exceed `max_concurrent`, the second applies the
job's own `OverlapPolicy` (`skip`, `queue`, `parallel`).

## Endpoints (added scope only)

```
POST /api/v1/cron/jobs              body: { ... }, now accepts scope
GET  /api/v1/cron/jobs              filterable by scope; Users never see System
GET  /api/v1/cron/quota             current per-user effective quota
```

CLI:
```
openpanel cron job create   --scope system|per-site|per-user …
openpanel cron quota show
```

Web: per-user cron sub-page visible only to Users; Owner sees a
unified cron admin page with scope badges.

## Tests

```
1.1  Unit: role permission checker; quota math; overlap with
     user-scoped vs system-scoped jobs.
1.2  Property: a User role cannot create a System job under any
     payload; concurrent in-flight per user ≤ max_concurrent.
1.3  Service tests with mock scheduler: enforcement on create,
     schedule, run; quota overflow rejection.
1.4  Integration: User POSTs system cron → 403; Owner POSTs → 201;
     quota overflow returns typed error; audit.
1.5  CLI E2E: `openpanel cron job create --scope per-user` as User
     role; quota show as the same user.
1.6  Web: User cron page exists; System tab hidden from User.
```
