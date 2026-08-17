# Add hosting plans

## Why

The follow-on `add-resource-quotas` change will compute per-user
quotas; without a **plan** that bundles features and quota caps,
operators must hand-write quota values per user and reseller
relationships become impossible. cPanel's Packages and Baota's
plan tiers are the canonical abstraction: a plan is a named,
reusable bundle of feature toggles and quota caps that can be
attached to users at creation, swapped at any time, and used by
parent accounts to define child-account limits. This change
introduces the `hosting-plans` capability as the source of truth
for "what does this account get".

## What Changes

- New `hosting-plans` bounded context with `HostingPlan`
  aggregate: name, description, prices list (display only),
  feature toggles, quota caps, allowed app list (from
  `web-application-installer`), allowed PHP runtimes.
- New service: `HostingPlansService` covering
  `create / update / list / disable / assign / unassign / clone`.
- New endpoints: `GET/POST/PUT/DELETE /api/v1/hosting-plans`,
  `POST /api/v1/hosting-plans/{id}/assign` (body: user id), web
  UI on `/plans`, CLI `openpanel plans {create,list,assign}`.
- Backed by a new SQLite table `hosting_plans` and a join
  `user_plan_assignments(user_id, plan_id, assigned_at, assigned_by)`.

## Capabilities

### New Capabilities

- `hosting-plans`: hosting-plan CRUD and assignment.

## Impact

- Domain: `HostingPlan`, `PlanFeature`, `PlanQuota`,
  `AssignmentId`.
- App: `HostingPlansService`, `PlanAssignService`,
  `PlanResolver` (used by `resource-quotas` and
  `cron-role-permissions`).
- API/CLI/web: `/api/v1/hosting-plans/*`; CLI subcommands; web
  `/plans` page.
- Coupling: `User.hosting_plan_id` (placeholder field from
  `refine-identity-with-hierarchy-and-plan-fields`) is now
  populated by `PlanAssignService`.
